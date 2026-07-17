use std::path::{Path, PathBuf};
use std::time::Duration;

use yt_dlp::client::deps::Libraries;

use super::fs_utils::{
    binary_path, cleanup_partials, concat_frag_groups, find_output, frag_bytes, part_bytes,
};
use super::models::{format_duration, DownloadOptions, FormatRow, Progress, VideoPreview};
use super::net::download_thumbnail;
use super::ytdlp_util::{
    friendly_error, looks_like_url, parse_ytdlp_eta, parse_ytdlp_percent,
    parse_ytdlp_size, parse_ytdlp_speed, ytdlp_error,
};
use super::{kill_tree, wait_for_stop, DownloadEngine};

impl DownloadEngine {
    pub async fn resolve_source(&self, url: &str) -> String {
        let u = url.trim();
        if u.contains("spotify.com/track") {
            let api = format!("https://open.spotify.com/oembed?url={}", u);
            if let Ok(resp) = reqwest::get(&api).await {
                if let Ok(text) = resp.text().await {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(title) = json.get("title").and_then(|t| t.as_str()) {
                            if !title.trim().is_empty() {
                                return format!("ytsearch1:{}", title.trim());
                            }
                        }
                    }
                }
            }
        }
        u.to_string()
    }

    pub async fn fetch_info(
        &self,
        url: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.ytdlp_title(url).await
    }

    pub async fn fetch_preview(
        &self,
        url: &str,
    ) -> Result<VideoPreview, Box<dyn std::error::Error>> {
        if let Some(cached) = self.preview_cache.lock().unwrap().get(url).cloned() {
            return Ok(cached);
        }
        // Sempre pela invocação própria do yt-dlp: `ytdlp_preview` usa
        // `cmd.output()`, que drena stdout/stderr concorrentemente. Antes o
        // YouTube ia por `Downloader::fetch_video_infos` (crate yt-dlp), que foi
        // observado pendurar — o processo yt-dlp não retornava e a UI ficava
        // presa em "Buscando informações do link...". O download em si já usava
        // a invocação própria (`ytdlp_download`); isto unifica a busca de info.
        let preview = self.ytdlp_preview(url).await?;
        self.preview_cache
            .lock()
            .unwrap()
            .insert(url.to_string(), preview.clone());
        Ok(preview)
    }

    pub async fn fetch_and_download<F>(
        &self,
        url: &str,
        output_path: &str,
        opts: DownloadOptions,
        on_progress: F,
    ) -> Result<PathBuf, Box<dyn std::error::Error>>
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        if !looks_like_url(url) {
            return Err("URL inválida. Cole um link válido (ex.: https://...)".into());
        }
        let mut out = PathBuf::from(output_path);
        out.set_extension(&opts.format);
        self.ytdlp_download(url, &out, &opts, on_progress).await
    }

    pub(super) fn ytdlp_path(&self) -> PathBuf {
        binary_path(&self.libs_dir, "yt-dlp")
    }

    async fn ytdlp_title(&self, url: &str) -> Result<String, Box<dyn std::error::Error>> {
        let mut cmd = tokio::process::Command::new(self.ytdlp_path());
        cmd.arg("--no-warnings")
            .arg("--skip-download")
            .arg("--no-playlist")
            .arg("--print")
            .arg("%(title)s")
            .arg(url);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let output = cmd.output().await?;
        if !output.status.success() {
            return Err(ytdlp_error(&output.stderr).into());
        }
        let title = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if title.is_empty() {
            Ok("download".to_string())
        } else {
            Ok(title)
        }
    }

    async fn ytdlp_preview(&self, url: &str) -> Result<VideoPreview, Box<dyn std::error::Error>> {
        let mut cmd = tokio::process::Command::new(self.ytdlp_path());
        cmd.arg("--no-warnings")
            .arg("--skip-download")
            .arg("--no-playlist")
            .arg("--dump-single-json")
            .arg(url);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let output = cmd.output().await?;
        if !output.status.success() {
            return Err(ytdlp_error(&output.stderr).into());
        }

        let json: YtJson = serde_json::from_slice(&output.stdout)?;

        let mut resolutions: Vec<u32> = json
            .formats
            .iter()
            .filter_map(|f| f.height)
            .filter(|h| *h > 0)
            .collect();
        resolutions.sort_unstable();
        resolutions.dedup();
        resolutions.reverse();

        let est = json
            .formats
            .iter()
            .filter_map(|f| f.filesize.or(f.filesize_approx))
            .max();

        let channel = json.channel.or(json.uploader).unwrap_or_default();
        let duration = json
            .duration
            .map(|d| format_duration(d as i64))
            .unwrap_or_default();
        let thumbnail = match &json.thumbnail {
            Some(url) => download_thumbnail(url).await,
            None => None,
        };

        let is_live = json.is_live || json.live_status.as_deref() == Some("is_live");
        Ok(VideoPreview {
            title: json.title.unwrap_or_else(|| "download".to_string()),
            channel,
            duration,
            resolutions,
            est_size_video: est,
            est_size_audio: est,
            thumbnail,
            is_live,
        })
    }

    async fn ytdlp_download<F>(
        &self,
        url: &str,
        out: &Path,
        opts: &DownloadOptions,
        on_progress: F,
    ) -> Result<PathBuf, Box<dyn std::error::Error>>
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        use std::process::Stdio;
        use tokio::io::AsyncBufReadExt;

        let folder = out.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        let stem = out
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "download".to_string());
        let template = folder.join(format!("{}.%(ext)s", stem));
        let final_ext = opts.format.clone();

        crate::applog::info(&format!(
            "download: url={} format={} audio={} quality={} max_height={:?} rate={:?} frags={} clip={:?}",
            url,
            opts.format,
            opts.is_audio,
            opts.quality,
            opts.max_height,
            opts.rate_limit,
            opts.concurrent_fragments,
            opts.clip,
        ));

        const MAX_ATTEMPTS: u32 = 2;
        // Watchdog de estagnação: se os .part não crescerem pelo menos
        // STALL_MIN_GROWTH dentro de STALL_WINDOW, a tentativa é abortada.
        // Sem isso, uma fonte que estrangula a conexão a conta-gotas (ex.: X/
        // Twitter com mídia bloqueada) deixa o yt-dlp pendurado para sempre.
        const STALL_WINDOW: Duration = Duration::from_secs(180);
        const STALL_MIN_GROWTH: u64 = 256 * 1024;
        let mut last_err = String::new();
        let mut last_stalled = false;

        for attempt in 0..MAX_ATTEMPTS {
            let mut cmd = tokio::process::Command::new(self.ytdlp_path());
            cmd.arg("--no-warnings")
                .arg("--no-playlist")
                .arg("--newline")
                .arg("--continue")
                .arg("--retries")
                .arg("10")
                .arg("--fragment-retries")
                .arg("10")
                .arg("--ffmpeg-location")
                .arg(&self.ffmpeg_path)
                .arg("--embed-metadata")
                .arg("-o")
                .arg(&template);

            if opts.concurrent_fragments > 1 {
                cmd.arg("--concurrent-fragments")
                    .arg(opts.concurrent_fragments.to_string());
            }
            if let Some(rl) = &opts.rate_limit {
                if !rl.trim().is_empty() {
                    cmd.arg("--limit-rate").arg(rl.trim());
                }
            }
            if opts.live_from_start {
                cmd.arg("--live-from-start");
            }
            if opts.is_live {
                // Contêiner MPEG-TS: tolera parada/kill sem corromper (sem moov),
                // permitindo remuxar o que já foi gravado.
                cmd.arg("--hls-use-mpegts");
            }

            if let Some((start, end)) = &opts.clip {
                let start = start.trim();
                let end = end.trim();
                if !start.is_empty() || !end.is_empty() {
                    let section = format!(
                        "*{}-{}",
                        if start.is_empty() { "0" } else { start },
                        if end.is_empty() { "inf" } else { end },
                    );
                    cmd.arg("--download-sections")
                        .arg(section)
                        .arg("--force-keyframes-at-cuts");
                }
            }

            if opts.is_audio {
                let audio_format = if opts.format == "ogg" { "vorbis" } else { &opts.format };
                cmd.arg("-x")
                    .arg("--audio-format")
                    .arg(audio_format)
                    .arg("--audio-quality")
                    .arg("0")
                    .arg("--embed-thumbnail")
                    .arg("--convert-thumbnails")
                    .arg("jpg");
            } else {
                let selector = match opts.format.as_str() {
                    "mp4" => "bv*[vcodec^=avc1]+ba[acodec^=mp4a]/bv*+ba/b",
                    "webm" => "bv*[vcodec^=vp9]+ba/bv*+ba/b",
                    _ => "bv*+ba/b",
                };
                cmd.arg("-f")
                    .arg(selector)
                    .arg("--merge-output-format")
                    .arg(&opts.format);

                if let Some(h) = opts.max_height {
                    cmd.arg("-S").arg(format!("res:{}", h));
                } else {
                    match opts.quality.as_str() {
                        "medium" => {
                            cmd.arg("-S").arg("res:720");
                        }
                        "high" => {
                            cmd.arg("-S").arg("res:1080");
                        }
                        _ => {}
                    }
                }

                // Legendas em live falham (fragmentos de .vtt inexistentes) e abortam
                // a gravação — só baixamos legendas em vídeos normais.
                if !opts.is_live {
                    if let Some(langs) = &opts.subtitle_langs {
                        if !langs.trim().is_empty() {
                            cmd.arg("--write-subs")
                                .arg("--write-auto-subs")
                                .arg("--sub-langs")
                                .arg(langs)
                                .arg("--convert-subs")
                                .arg("srt");
                        }
                    }
                }
            }
            cmd.arg(url);
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            cmd.kill_on_drop(true);

            #[cfg(windows)]
            cmd.creation_flags(0x08000000);

            // No Unix, o yt-dlp vira líder do próprio process group: assim o
            // kill_tree consegue matar o grupo inteiro (yt-dlp + ffmpeg filho),
            // como o taskkill /T faz no Windows.
            #[cfg(unix)]
            cmd.process_group(0);

            let mut child = cmd.spawn()?;
            let child_pid = child.id();
            if let Some(pid) = child_pid {
                self.dl_pids.lock().unwrap().push(pid);
            }
            let stdout = child.stdout.take().ok_or("falha ao capturar saída do yt-dlp")?;
            let stderr = child.stderr.take().ok_or("falha ao capturar erros do yt-dlp")?;

            let stderr_task = tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let mut buf = String::new();
                let mut reader = tokio::io::BufReader::new(stderr);
                let _ = reader.read_to_string(&mut buf).await;
                buf
            });

            {
                let mut n = self.net.lock().unwrap();
                n.current = 0.0;
                n.history.clear();
            }
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            let (mut last_frac, mut last_speed, mut last_eta, mut last_bytes) =
                (0.0f64, 0.0f64, 0u64, 0u64);
            let stop = opts.stop.clone();
            let mut stopped = false;
            let mut stalled = false;
            let mut stall_mark = 0u64;
            let mut stall_at = std::time::Instant::now();
            // Em live, o yt-dlp entrega ao ffmpeg (progresso vai pro stderr, não pra
            // cá) — então lemos o tamanho direto dos .part no disco, num tick.
            let mut tick = tokio::time::interval(Duration::from_millis(500));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    line = lines.next_line() => {
                        let Ok(Some(line)) = line else { break };
                        let mut changed = false;
                        if let Some(p) = parse_ytdlp_percent(&line) {
                            last_frac = p;
                            changed = true;
                        }
                        if let Some(spd) = parse_ytdlp_speed(&line) {
                            last_speed = spd;
                            changed = true;
                            let mut n = self.net.lock().unwrap();
                            n.current = spd as f32;
                            n.history.push(spd as f32);
                            if n.history.len() > 160 {
                                n.history.remove(0);
                            }
                        }
                        if let Some(eta) = parse_ytdlp_eta(&line) {
                            last_eta = eta;
                            changed = true;
                        }
                        if let Some(sz) = parse_ytdlp_size(&line) {
                            last_bytes = sz;
                            changed = true;
                        }
                        if changed {
                            on_progress(Progress {
                                fraction: last_frac,
                                speed_bps: last_speed,
                                eta_secs: last_eta,
                                downloaded_bytes: last_bytes,
                            });
                        }
                    }
                    _ = tick.tick() => {
                        let bytes = part_bytes(&folder, &stem);
                        // Watchdog: não vale para lives (pausas do streamer são
                        // normais) nem para o pós-processamento (com o download
                        // completo, os bytes param de crescer enquanto o ffmpeg
                        // faz merge/extração).
                        if opts.is_live || last_frac >= 0.99 || bytes >= stall_mark + STALL_MIN_GROWTH {
                            stall_mark = stall_mark.max(bytes);
                            stall_at = std::time::Instant::now();
                        } else if stall_at.elapsed() >= STALL_WINDOW {
                            stalled = true;
                            // Mata a árvore (yt-dlp + ffmpeg filho) como no cancelamento.
                            if let Some(pid) = child_pid {
                                kill_tree(pid);
                            }
                            let _ = child.start_kill();
                            break;
                        }
                        if bytes != last_bytes {
                            let delta = bytes.saturating_sub(last_bytes) as f64 / 0.5;
                            last_bytes = bytes;
                            last_speed = delta;
                            {
                                let mut n = self.net.lock().unwrap();
                                n.current = delta as f32;
                                n.history.push(delta as f32);
                                if n.history.len() > 160 {
                                    n.history.remove(0);
                                }
                            }
                            on_progress(Progress {
                                fraction: last_frac,
                                speed_bps: last_speed,
                                eta_secs: last_eta,
                                downloaded_bytes: last_bytes,
                            });
                        }
                    }
                    _ = wait_for_stop(&stop) => {
                        stopped = true;
                        // Mata a árvore (yt-dlp + ffmpeg filho), senão o ffmpeg fica órfão.
                        if let Some(pid) = child_pid {
                            kill_tree(pid);
                        }
                        let _ = child.start_kill();
                        break;
                    }
                }
            }
            {
                self.net.lock().unwrap().current = 0.0;
            }
            if let Some(pid) = child_pid {
                self.dl_pids.lock().unwrap().retain(|p| *p != pid);
            }

            // Parada graciosa de gravação: mata o yt-dlp e remuxa o que já baixou.
            if stopped {
                let _ = child.wait().await;
                if let Some(p) = self.finalize_live_partials(&folder, &stem, &final_ext).await {
                    on_progress(Progress { fraction: 1.0, ..Default::default() });
                    return Ok(p);
                }
                return Err("Nada foi gravado antes de parar.".into());
            }

            let status = child.wait().await?;
            let stderr_text = stderr_task.await.unwrap_or_default();

            if status.success() {
                on_progress(Progress { fraction: 1.0, ..Default::default() });
                let expected = folder.join(format!("{}.{}", stem, final_ext));
                let result = if expected.exists() {
                    Some(expected)
                } else {
                    find_output(&folder, &stem)
                };
                return result.ok_or_else(|| {
                    format!("Arquivo de saída não encontrado para \"{}\"", stem).into()
                });
            }

            last_err = stderr_text;
            last_stalled = stalled;
            if stalled {
                crate::applog::error(&format!(
                    "download estagnou (sem crescimento por {}s) — tentativa {} abortada",
                    STALL_WINDOW.as_secs(),
                    attempt + 1
                ));
            } else {
                crate::applog::error(&format!("download falhou (tentativa {}): {}", attempt + 1, last_err.lines().last().unwrap_or("")));
            }

            // Live que caiu no meio da gravação: em vez de re-tentar (e arriscar
            // sobrescrever horas já gravadas), finaliza o que está no disco.
            if opts.is_live && part_bytes(&folder, &stem) > 0 {
                if let Some(p) = self.finalize_live_partials(&folder, &stem, &final_ext).await {
                    crate::applog::info("live caiu; gravação parcial finalizada");
                    on_progress(Progress { fraction: 1.0, ..Default::default() });
                    return Ok(p);
                }
            }

            if attempt + 1 < MAX_ATTEMPTS {
                tokio::time::sleep(Duration::from_secs(2 * (attempt as u64 + 1))).await;
            }
        }

        cleanup_partials(&folder, &stem);
        if last_stalled {
            return Err("O download estagnou: nenhum dado novo por 3 minutos. \
                        A fonte pode estar limitando ou bloqueando a conexão — \
                        tente novamente mais tarde ou em outra qualidade."
                .into());
        }
        Err(friendly_error(&last_err).into())
    }

    pub async fn fetch_playlist(
        &self,
        playlist_id: &str,
    ) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
        // Invocação própria do yt-dlp, drenada via `cmd.output()`. `--flat-playlist`
        // apenas enumera os itens (id/título) sem buscar a info completa de cada
        // vídeo — rápido. Antes usava `youtube_extractor().fetch_playlist_paginated`
        // do crate yt-dlp, que podia pendurar/falhar silenciosamente, deixando a
        // fila sem itens. `--print "%(id)s|%(title)s"` dá uma linha por item; o id
        // (11 chars, sem `|`) fica antes do primeiro `|`, então split_once basta.
        let url = format!("https://www.youtube.com/playlist?list={}", playlist_id);
        let mut cmd = tokio::process::Command::new(self.ytdlp_path());
        cmd.arg("--no-warnings")
            .arg("--flat-playlist")
            .arg("--print")
            .arg("%(id)s|%(title)s")
            .arg(&url);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let output = cmd.output().await?;
        if !output.status.success() {
            return Err(ytdlp_error(&output.stderr).into());
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let items = text
            .lines()
            .filter_map(|line| {
                let (id, title) = line.split_once('|')?;
                let id = id.trim();
                if id.is_empty() || id == "NA" {
                    return None;
                }
                let url = format!("https://www.youtube.com/watch?v={}", id);
                Some((url, title.trim().to_string()))
            })
            .collect();
        Ok(items)
    }

    pub async fn update_ytdlp(&self) -> Result<String, Box<dyn std::error::Error>> {
        let yt = binary_path(&self.libs_dir, "yt-dlp");
        let _ = std::fs::remove_file(&yt);
        let libraries = Libraries::new(yt.clone(), self.ffmpeg_path.clone());
        let path = libraries.install_youtube().await?;
        Ok(path.to_string_lossy().to_string())
    }

    /// Finaliza uma gravação de live interrompida: remuxa os `.part` de vídeo/áudio
    /// já baixados em um arquivo tocável e limpa os temporários.
    async fn finalize_live_partials(
        &self,
        folder: &Path,
        stem: &str,
        ext: &str,
    ) -> Option<PathBuf> {
        // Lista os .part principais (vídeo/áudio), ignorando fragmentos -Frag.
        let all_parts: Vec<(String, PathBuf)> = std::fs::read_dir(folder)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter_map(|p| {
                let n = p.file_name()?.to_str()?.to_string();
                if n.ends_with(".part") && !n.contains("-Frag") {
                    Some((n, p))
                } else {
                    None
                }
            })
            .collect();

        // Casa por prefixo do stem; se não achar, cai para o prefixo curto (sanitização
        // do yt-dlp pode divergir um pouco do nome esperado).
        let prefix = format!("{}.", stem);
        let short: String = stem.chars().take(24).collect();
        let mut parts: Vec<PathBuf> = all_parts
            .iter()
            .filter(|(n, _)| n.starts_with(&prefix) || n.starts_with(stem))
            .map(|(_, p)| p.clone())
            .collect();
        if parts.is_empty() {
            parts = all_parts
                .iter()
                .filter(|(n, _)| n.starts_with(&short))
                .map(|(_, p)| p.clone())
                .collect();
        }

        crate::applog::info(&format!(
            "finalize live: stem=\"{}\" parts_casadas={} parts_no_dir=[{}]",
            stem,
            parts.len(),
            all_parts
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));

        // Ignora .part vazios (placeholders) e compara com o total dos fragmentos
        // ".part-FragN" do modo DVR — usa o caminho que realmente contém os dados
        // (um placeholder de 0 bytes não pode vencer horas de fragmentos).
        parts.retain(|p| std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false));
        let main_total: u64 = parts
            .iter()
            .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
            .sum();
        if frag_bytes(folder, stem) > main_total {
            let frag_parts = concat_frag_groups(folder, stem).await;
            if !frag_parts.is_empty() {
                parts = frag_parts;
            }
        }
        if parts.is_empty() {
            return None;
        }
        // Maior primeiro (o vídeo costuma ser maior que o áudio).
        parts.sort_by_key(|p| {
            std::cmp::Reverse(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        });

        let out = folder.join(format!("{}.{}", stem, ext));
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-y");
        for p in &parts {
            cmd.arg("-i").arg(p);
        }
        cmd.arg("-c").arg("copy").arg(&out);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let muxed = cmd.output().await.map(|o| o.status.success()).unwrap_or(false)
            && std::fs::metadata(&out).map(|m| m.len() > 0).unwrap_or(false);

        let result = if muxed {
            Some(out.clone())
        } else {
            // Fallback: renomeia o maior .part para um arquivo utilizável.
            let _ = std::fs::remove_file(&out);
            let raw = folder.join(format!("{}.{}", stem, ext));
            if std::fs::rename(&parts[0], &raw).is_ok() {
                Some(raw)
            } else {
                None
            }
        };

        // Sucesso exige arquivo com conteúdo — nunca reportar um 0-byte como salvo.
        let result = result
            .filter(|p| std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false));

        // Só limpa os temporários após sucesso real; numa falha eles são a única
        // cópia dos dados e permitem recuperação manual.
        if result.is_some() {
            cleanup_partials(folder, stem);
        }
        result
    }

    pub async fn download_thumbnail_file(
        &self,
        url: &str,
        folder: &Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(folder).ok();
        let before: std::collections::HashSet<PathBuf> = std::fs::read_dir(folder)
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();

        let template = folder.join("%(title)s.%(ext)s");
        let mut cmd = tokio::process::Command::new(self.ytdlp_path());
        cmd.arg("--no-warnings")
            .arg("--no-playlist")
            .arg("--skip-download")
            .arg("--write-thumbnail")
            .arg("--convert-thumbnails")
            .arg("jpg")
            .arg("--ffmpeg-location")
            .arg(&self.ffmpeg_path)
            .arg("-o")
            .arg(&template)
            .arg(url);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let output = cmd.output().await?;
        if !output.status.success() {
            return Err(ytdlp_error(&output.stderr).into());
        }

        let is_img = |p: &Path| {
            matches!(
                p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(),
                Some("jpg") | Some("jpeg") | Some("png") | Some("webp")
            )
        };
        let new_file = std::fs::read_dir(folder)
            .ok()
            .and_then(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .find(|p| !before.contains(p) && is_img(p))
            });
        new_file.ok_or_else(|| "Miniatura não encontrada.".into())
    }

    pub async fn list_formats(
        &self,
        url: &str,
    ) -> Result<Vec<FormatRow>, Box<dyn std::error::Error>> {
        let mut cmd = tokio::process::Command::new(self.ytdlp_path());
        cmd.arg("--no-warnings")
            .arg("--no-playlist")
            .arg("--skip-download")
            .arg("--dump-single-json")
            .arg(url);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let output = cmd.output().await?;
        if !output.status.success() {
            return Err(ytdlp_error(&output.stderr).into());
        }
        let json: YtJson = serde_json::from_slice(&output.stdout)?;

        let mut rows: Vec<FormatRow> = Vec::new();
        for f in json.formats {
            let has_v = f.vcodec.as_deref().map(|c| c != "none").unwrap_or(false);
            let has_a = f.acodec.as_deref().map(|c| c != "none").unwrap_or(false);
            if !has_v && !has_a {
                continue;
            }
            let kind = if has_v && has_a {
                "Vídeo+Áudio"
            } else if has_v {
                "Vídeo"
            } else {
                "Áudio"
            };
            let resolution = match f.height {
                Some(h) if h > 0 => format!("{}p", h),
                _ if has_a && !has_v => "—".to_string(),
                _ => f.format_note.clone().unwrap_or_else(|| "—".to_string()),
            };
            let codec = {
                let v = f.vcodec.as_deref().filter(|c| *c != "none").unwrap_or("");
                let a = f.acodec.as_deref().filter(|c| *c != "none").unwrap_or("");
                [v, a].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("/")
            };
            rows.push(FormatRow {
                id: f.format_id.unwrap_or_default(),
                ext: f.ext.unwrap_or_default(),
                resolution,
                fps: f.fps.map(|x| format!("{:.0}", x)).unwrap_or_default(),
                codec,
                kind: kind.to_string(),
                bitrate: f.tbr.map(|x| format!("{:.0}k", x)).unwrap_or_default(),
                size: f.filesize.or(f.filesize_approx),
            });
        }
        rows.sort_by(|a, b| b.size.unwrap_or(0).cmp(&a.size.unwrap_or(0)));
        Ok(rows)
    }
}

#[derive(serde::Deserialize)]
struct YtJson {
    title: Option<String>,
    duration: Option<f64>,
    uploader: Option<String>,
    channel: Option<String>,
    thumbnail: Option<String>,
    #[serde(default)]
    is_live: bool,
    #[serde(default)]
    live_status: Option<String>,
    #[serde(default)]
    formats: Vec<YtFormat>,
}

#[derive(serde::Deserialize)]
struct YtFormat {
    format_id: Option<String>,
    ext: Option<String>,
    height: Option<u32>,
    fps: Option<f64>,
    vcodec: Option<String>,
    acodec: Option<String>,
    tbr: Option<f64>,
    filesize: Option<i64>,
    filesize_approx: Option<i64>,
    format_note: Option<String>,
}
