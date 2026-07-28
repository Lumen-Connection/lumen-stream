use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncReadExt};

use super::models::{AudioMeta, Progress, Stage, VideoProfile};
use super::DownloadEngine;

/// Codecs normalizados lidos do `ffmpeg -i`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamCodecs {
    pub video: Option<String>,
    pub audio: Option<String>,
}

/// Como finalizar o arquivo baixado no perfil pedido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizeMode {
    /// Já está no codec certo → só remux (`-c copy`), quase instantâneo.
    RemuxCopy,
    /// Vídeo ok, áudio não → re-encode só do áudio.
    RecodeAudio,
    /// Precisa re-encode completo (lento).
    FullEncode,
}

/// Extrai `Duration: HH:MM:SS.xx` do stderr do ffmpeg.
pub(super) fn parse_ffmpeg_duration(text: &str) -> Option<f64> {
    for line in text.lines() {
        let t = line.trim();
        let rest = t.strip_prefix("Duration:")?.trim();
        let token = rest.split(',').next()?.trim();
        let mut parts = token.split(':');
        let h: f64 = parts.next()?.parse().ok()?;
        let m: f64 = parts.next()?.parse().ok()?;
        let s: f64 = parts.next()?.parse().ok()?;
        return Some(h * 3600.0 + m * 60.0 + s);
    }
    None
}

/// Converte linhas de progresso do ffmpeg em segundos.
/// Aceita `out_time_us=` (µs) e `out_time_ms=` (ms).
pub(super) fn parse_out_time_us(line: &str) -> Option<f64> {
    let t = line.trim();
    if let Some(v) = t.strip_prefix("out_time_us=") {
        return v.parse::<f64>().ok().map(|us| us / 1_000_000.0);
    }
    if let Some(v) = t.strip_prefix("out_time_ms=") {
        return v.parse::<f64>().ok().map(|ms| ms / 1_000.0);
    }
    None
}

/// Fração de progresso, limitada a 1.0.
pub(super) fn progress_fraction(out_time_secs: f64, total_secs: f64) -> f64 {
    if total_secs <= 0.0 {
        return 0.0;
    }
    (out_time_secs / total_secs).clamp(0.0, 1.0)
}

/// Extrai codecs de vídeo/áudio do stderr do `ffmpeg -i`.
pub(super) fn parse_ffmpeg_stream_codecs(text: &str) -> StreamCodecs {
    let mut out = StreamCodecs::default();
    for line in text.lines() {
        let t = line.trim();
        // Ex.: Stream #0:0(und): Video: h264 (avc1 / 0x31637661), ...
        if let Some(rest) = t.split_once("Video:").map(|(_, r)| r.trim()) {
            if out.video.is_none() {
                let token = rest.split_whitespace().next().unwrap_or("").trim_matches(',');
                out.video = Some(normalize_video_codec(token));
            }
        } else if let Some(rest) = t.split_once("Audio:").map(|(_, r)| r.trim()) {
            if out.audio.is_none() {
                let token = rest.split_whitespace().next().unwrap_or("").trim_matches(',');
                out.audio = Some(normalize_audio_codec(token));
            }
        }
    }
    out
}

pub(super) fn normalize_video_codec(raw: &str) -> String {
    let r = raw.to_ascii_lowercase();
    if r.starts_with("h264") || r.starts_with("avc") {
        "h264".into()
    } else if r.starts_with("hevc") || r.starts_with("h265") {
        "hevc".into()
    } else if r.starts_with("av1") || r.starts_with("av01") {
        "av1".into()
    } else if r.starts_with("vp9") || r.starts_with("vp09") {
        "vp9".into()
    } else if r.starts_with("vp8") {
        "vp8".into()
    } else {
        r
    }
}

pub(super) fn normalize_audio_codec(raw: &str) -> String {
    let r = raw.to_ascii_lowercase();
    if r.starts_with("aac") || r.starts_with("mp4a") {
        "aac".into()
    } else if r.starts_with("opus") {
        "opus".into()
    } else if r.starts_with("vorbis") {
        "vorbis".into()
    } else if r.starts_with("flac") {
        "flac".into()
    } else if r.starts_with("mp3") || r.starts_with("libmp3") {
        "mp3".into()
    } else {
        r
    }
}

/// Decide se dá para só remuxar (rápido) ou precisa re-encodar.
pub(super) fn finalize_mode(profile: &VideoProfile, codecs: &StreamCodecs) -> FinalizeMode {
    let v = codecs.video.as_deref();
    let a = codecs.audio.as_deref();
    match profile.extension {
        "mp4" => {
            // Perfil H.264 (MP4): YouTube costuma entregar avc1+mp4a → remux.
            let v_ok = v == Some("h264");
            let a_ok = matches!(a, Some("aac") | None);
            if v_ok && a_ok {
                FinalizeMode::RemuxCopy
            } else if v_ok {
                FinalizeMode::RecodeAudio
            } else {
                FinalizeMode::FullEncode
            }
        }
        "webm" => {
            let v_ok = matches!(v, Some("vp9") | Some("vp8"));
            let a_ok = matches!(a, Some("opus") | Some("vorbis") | None);
            if v_ok && a_ok {
                FinalizeMode::RemuxCopy
            } else if v_ok {
                FinalizeMode::RecodeAudio
            } else {
                FinalizeMode::FullEncode
            }
        }
        "mkv" => {
            // Perfil AV1: só remux se já for AV1 (áudio livre no MKV).
            if v == Some("av1") {
                FinalizeMode::RemuxCopy
            } else {
                FinalizeMode::FullEncode
            }
        }
        _ => FinalizeMode::FullEncode,
    }
}

impl DownloadEngine {
    /// Duração do arquivo via `ffmpeg -i` (stderr).
    async fn probe_duration_secs(&self, input: &Path) -> Option<f64> {
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-hide_banner")
            .arg("-nostdin")
            .arg("-i")
            .arg(input);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let output = cmd.output().await.ok()?;
        parse_ffmpeg_duration(&String::from_utf8_lossy(&output.stderr))
    }

    /// Roda o ffmpeg lendo `-progress pipe:1` e reportando fração real.
    /// Registra o PID em `dl_pids` para cancelamento matar a árvore.
    ///
    /// `configure` deve acrescentar `-i`, codecs e o **arquivo de saída** —
    /// as flags globais (`-progress`, `-nostdin`, …) já vêm **antes**. Se
    /// `-progress pipe:1` for colocado depois do output, o ffmpeg trata
    /// `pipe:1` como segundo destino e a conversão trava sem terminar.
    async fn run_ffmpeg_progress<F, C>(
        &self,
        total_secs: Option<f64>,
        on_progress: Option<&F>,
        configure: C,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: Fn(Progress) + Send + Sync,
        C: FnOnce(&mut tokio::process::Command),
    {
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        cmd.arg("-hide_banner")
            .arg("-y")
            .arg("-nostdin")
            .arg("-progress")
            .arg("pipe:1")
            .arg("-nostats");
        configure(&mut cmd);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn()?;
        let pid = child.id();
        if let Some(pid) = pid {
            self.dl_pids.lock().unwrap().push(pid);
        }

        let stdout = child.stdout.take();
        let stderr_task = {
            let mut stderr = child.stderr.take();
            tokio::spawn(async move {
                let mut stderr_buf = Vec::new();
                if let Some(ref mut s) = stderr {
                    let _ = s.read_to_end(&mut stderr_buf).await;
                }
                stderr_buf
            })
        };

        // Lê progresso em paralelo com o wait: se o processo morrer sem
        // `progress=end`, o EOF do stdout encerra o loop.
        if let Some(stdout) = stdout {
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let t = line.trim();
                        if t == "progress=end" {
                            if let Some(cb) = on_progress {
                                cb(Progress {
                                    fraction: 1.0,
                                    ..Default::default()
                                });
                            }
                            break;
                        }
                        if let (Some(total), Some(secs)) = (total_secs, parse_out_time_us(t)) {
                            if let Some(cb) = on_progress {
                                cb(Progress {
                                    fraction: progress_fraction(secs, total),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                    Ok(None) => break, // EOF: processo fechou o pipe
                    Err(_) => break,
                }
            }
        }

        let status = child.wait().await?;
        if let Some(pid) = pid {
            self.dl_pids.lock().unwrap().retain(|running| *running != pid);
        }
        let stderr = stderr_task.await.unwrap_or_default();
        if !status.success() {
            let text = String::from_utf8_lossy(&stderr);
            let last = text
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("");
            return Err(format!("ffmpeg falhou: {}", last).into());
        }
        if let Some(cb) = on_progress {
            cb(Progress {
                fraction: 1.0,
                ..Default::default()
            });
        }
        Ok(())
    }

    async fn probe_stream_codecs(&self, input: &Path) -> StreamCodecs {
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-hide_banner")
            .arg("-nostdin")
            .arg("-i")
            .arg(input);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let output = match cmd.output().await {
            Ok(o) => o,
            Err(_) => return StreamCodecs::default(),
        };
        parse_ffmpeg_stream_codecs(&String::from_utf8_lossy(&output.stderr))
    }

    /// Finaliza o vídeo baixado no perfil (MP4/H.264, MKV/AV1, WebM/VP9).
    ///
    /// Preferência: **remux com `-c copy`** quando os codecs já batem (quase
    /// instantâneo). Re-encode completo só quando o vídeo de origem não é
    /// compatível — era o caminho lento que parecia "conversão infinita".
    pub(super) async fn transcode_video_profile<F>(
        &self,
        input: &Path,
        output: &Path,
        format: &str,
        on_progress: Option<&F>,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: Fn(Progress) + Send + Sync,
    {
        let profile = super::video_profile(format)
            .ok_or_else(|| format!("perfil de vídeo desconhecido: {}", format))?;
        let temp_output = profile_temp_output(output);
        let _ = std::fs::remove_file(&temp_output);

        let codecs = self.probe_stream_codecs(input).await;
        let mode = finalize_mode(profile, &codecs);
        crate::applog::info(&format!(
            "finalize vídeo: perfil={} v={:?} a={:?} modo={:?}",
            profile.extension, codecs.video, codecs.audio, mode
        ));

        let input = input.to_path_buf();
        match mode {
            FinalizeMode::RemuxCopy => {
                // Quase instantâneo: só remuxa streams, sem re-encode.
                let on_fin = |pr: Progress| {
                    if let Some(cb) = on_progress {
                        cb(Progress {
                            fraction: pr.fraction,
                            stage: Stage::Finalizing,
                            ..Default::default()
                        });
                    }
                };
                on_fin(Progress {
                    fraction: 0.0,
                    stage: Stage::Finalizing,
                    ..Default::default()
                });
                self.run_ffmpeg_progress(None, Some(&on_fin), |cmd| {
                    cmd.arg("-i")
                        .arg(&input)
                        .arg("-map")
                        .arg("0:v:0")
                        .arg("-map")
                        .arg("0:a?")
                        .arg("-map_metadata")
                        .arg("0")
                        .arg("-c")
                        .arg("copy");
                    if profile.extension == "mp4" {
                        cmd.arg("-movflags").arg("+faststart");
                    }
                    cmd.arg(&temp_output);
                })
                .await
                .map_err(|e| {
                    let _ = std::fs::remove_file(&temp_output);
                    e
                })?;
            }
            FinalizeMode::RecodeAudio => {
                // Vídeo copiado; só o áudio é re-encodado (bem mais rápido).
                let on_fin = |pr: Progress| {
                    if let Some(cb) = on_progress {
                        cb(Progress {
                            fraction: pr.fraction,
                            stage: Stage::Finalizing,
                            ..Default::default()
                        });
                    }
                };
                on_fin(Progress {
                    fraction: 0.0,
                    stage: Stage::Finalizing,
                    ..Default::default()
                });
                let total = self.probe_duration_secs(&input).await;
                let audio_enc = profile.audio_encoder;
                self.run_ffmpeg_progress(total, Some(&on_fin), |cmd| {
                    cmd.arg("-i")
                        .arg(&input)
                        .arg("-map")
                        .arg("0:v:0")
                        .arg("-map")
                        .arg("0:a?")
                        .arg("-map_metadata")
                        .arg("0")
                        .arg("-c:v")
                        .arg("copy")
                        .arg("-c:a")
                        .arg(audio_enc);
                    if audio_enc == "aac" {
                        cmd.arg("-b:a").arg("192k");
                    } else if audio_enc == "libopus" {
                        cmd.arg("-b:a").arg("160k");
                    }
                    if profile.extension == "mp4" {
                        cmd.arg("-movflags").arg("+faststart");
                    }
                    cmd.arg(&temp_output);
                })
                .await
                .map_err(|e| {
                    let _ = std::fs::remove_file(&temp_output);
                    e
                })?;
            }
            FinalizeMode::FullEncode => {
                // Último recurso: re-encode completo (ex.: AV1 pedido e fonte é H.264).
                let on_tc = |pr: Progress| {
                    if let Some(cb) = on_progress {
                        cb(Progress {
                            fraction: pr.fraction,
                            stage: Stage::Transcoding,
                            ..Default::default()
                        });
                    }
                };
                on_tc(Progress {
                    fraction: 0.0,
                    stage: Stage::Transcoding,
                    ..Default::default()
                });
                let total = self.probe_duration_secs(&input).await;
                let enc_args: Vec<&'static str> = video_profile_ffmpeg_args(profile);
                self.run_ffmpeg_progress(total, Some(&on_tc), |cmd| {
                    cmd.arg("-i")
                        .arg(&input)
                        .arg("-map")
                        .arg("0:v:0")
                        .arg("-map")
                        .arg("0:a?")
                        .arg("-map_metadata")
                        .arg("0")
                        .args(&enc_args)
                        .arg(&temp_output);
                })
                .await
                .map_err(|e| {
                    let _ = std::fs::remove_file(&temp_output);
                    e
                })?;
            }
        }

        if std::fs::metadata(&temp_output)
            .map(|metadata| metadata.len() == 0)
            .unwrap_or(true)
        {
            let _ = std::fs::remove_file(&temp_output);
            return Err(format!("ffmpeg falhou ao gerar {}: saída vazia", profile.label).into());
        }

        // Never expose a partial final file. Replace an existing target only
        // after FFmpeg successfully produced a non-empty temporary output.
        let _ = std::fs::remove_file(output);
        std::fs::rename(&temp_output, output)?;
        Ok(())
    }

    pub async fn generate_thumbnail(
        &self,
        video: &str,
        out: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(video)
            .arg("-vf")
            .arg("thumbnail,scale=160:-1")
            .arg("-frames:v")
            .arg("1")
            .arg(out);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let output = cmd.output().await?;
        if !output.status.success() || !out.exists() {
            return Err("falha ao gerar miniatura".into());
        }
        Ok(())
    }

    pub async fn detect_bpm(&self, file: &str) -> Result<u32, Box<dyn std::error::Error>> {
        const SR: usize = 11025;
        const FRAME: usize = 512;
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-i")
            .arg(file)
            .arg("-t")
            .arg("90")
            .arg("-ac")
            .arg("1")
            .arg("-ar")
            .arg(SR.to_string())
            .arg("-f")
            .arg("s16le")
            .arg("-");
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let output = cmd.output().await?;
        if !output.status.success() && output.stdout.is_empty() {
            return Err("não foi possível ler o áudio".into());
        }

        let samples: Vec<i16> = output
            .stdout
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        if samples.len() < SR {
            return Err("áudio muito curto".into());
        }
        let mut energy: Vec<f32> = Vec::new();
        for frame in samples.chunks(FRAME) {
            let e: f64 = frame.iter().map(|&s| (s as f64) * (s as f64)).sum();
            energy.push((e / frame.len() as f64) as f32);
        }
        let onset: Vec<f32> = energy
            .windows(2)
            .map(|w| (w[1] - w[0]).max(0.0))
            .collect();
        if onset.len() < 32 {
            return Err("áudio insuficiente para estimar BPM".into());
        }

        let frame_period = FRAME as f32 / SR as f32;
        let lag_for = |bpm: f32| ((60.0 / bpm) / frame_period).round() as usize;
        let (lag_min, lag_max) = (lag_for(180.0).max(1), lag_for(60.0));
        let mut best_lag = lag_min;
        let mut best_val = f32::MIN;
        for lag in lag_min..=lag_max.min(onset.len() / 2) {
            let mut sum = 0.0f32;
            for i in lag..onset.len() {
                sum += onset[i] * onset[i - lag];
            }
            if sum > best_val {
                best_val = sum;
                best_lag = lag;
            }
        }
        let bpm = (60.0 / (best_lag as f32 * frame_period)).round() as u32;
        Ok(bpm.clamp(40, 220))
    }

    pub async fn probe_metadata(&self, file: &str) -> Result<String, Box<dyn std::error::Error>> {
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-hide_banner").arg("-i").arg(file);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let output = cmd.output().await?;
        let text = String::from_utf8_lossy(&output.stderr);
        // Reusa o parser de duração (testável) — só filtra as linhas úteis.
        let _ = parse_ffmpeg_duration(&text);
        let mut lines: Vec<String> = Vec::new();
        for l in text.lines() {
            let t = l.trim();
            if t.starts_with("Input #")
                || t.starts_with("Duration")
                || t.starts_with("Stream #")
                || t.starts_with("Metadata")
                || t.starts_with("title")
                || t.starts_with("artist")
                || t.starts_with("album")
                || t.starts_with("encoder")
                || t.starts_with("major_brand")
            {
                lines.push(t.to_string());
            }
        }
        if lines.is_empty() {
            return Err("não foi possível ler os metadados".into());
        }
        Ok(lines.join("\n"))
    }

    pub(super) async fn transcode_audio<F>(
        &self,
        input: &Path,
        output: &Path,
        format: &str,
        meta: &AudioMeta,
        on_progress: Option<&F>,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: Fn(Progress) + Send + Sync,
    {
        let total = self.probe_duration_secs(input).await;
        let input = input.to_path_buf();
        let output = output.to_path_buf();
        let format_owned = format.to_string();
        let title = meta.title.clone();
        let artist = meta.artist.clone();
        let album = meta.album.clone();
        self.run_ffmpeg_progress(total, on_progress, |cmd| {
            cmd.arg("-i")
                .arg(&input)
                .arg("-vn")
                .arg("-map_metadata")
                .arg("0");
            if let Some(t) = &title {
                cmd.arg("-metadata").arg(format!("title={}", t));
            }
            if let Some(a) = &artist {
                cmd.arg("-metadata").arg(format!("artist={}", a));
            }
            if let Some(al) = &album {
                cmd.arg("-metadata").arg(format!("album={}", al));
            }
            match format_owned.as_str() {
                "mp3" => {
                    cmd.arg("-c:a").arg("libmp3lame").arg("-q:a").arg("2");
                }
                "m4a" | "aac" => {
                    cmd.arg("-c:a").arg("aac").arg("-b:a").arg("192k");
                }
                "opus" => {
                    cmd.arg("-c:a").arg("libopus").arg("-b:a").arg("160k");
                }
                "ogg" => {
                    cmd.arg("-c:a").arg("libvorbis").arg("-q:a").arg("5");
                }
                "wav" => {
                    cmd.arg("-c:a").arg("pcm_s16le");
                }
                "flac" => {
                    cmd.arg("-c:a").arg("flac");
                }
                _ => {}
            }
            cmd.arg(&output);
        })
        .await
        .map_err(|e| format!("ffmpeg falhou ao converter para {}: {}", format, e).into())
    }

    pub async fn extract_frames(
        &self,
        input: &str,
        fps: u32,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let input_path = PathBuf::from(input);
        let folder = input_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = input_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "video".to_string());
        let out_dir = folder.join(format!("{}_frames", stem));
        std::fs::create_dir_all(&out_dir)?;
        let pattern = out_dir.join("frame_%04d.png");

        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(&input_path)
            .arg("-vf")
            .arg(format!("fps={}", fps.max(1)))
            .arg(&pattern);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let result = cmd.output().await?;
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            return Err(format!("ffmpeg falhou ao extrair frames: {}", last).into());
        }
        Ok(out_dir)
    }

    /// Converte imagens em lote. `on_progress(done, total)` a cada item.
    /// Retorna `(pasta, nomes que falharam)`.
    pub async fn batch_convert_images<F>(
        &self,
        inputs: Vec<PathBuf>,
        out_dir: PathBuf,
        format: String,
        max_width: u32,
        quality: u32,
        on_progress: F,
    ) -> Result<(PathBuf, Vec<String>), Box<dyn std::error::Error>>
    where
        F: Fn(usize, usize) + Send + Sync,
    {
        std::fs::create_dir_all(&out_dir)?;
        let total = inputs.len();
        let mut ok = 0usize;
        let mut failed: Vec<String> = Vec::new();
        let mut last_err = String::new();
        for (i, inp) in inputs.iter().enumerate() {
            on_progress(i, total);
            let stem = inp
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "imagem".to_string());
            let name = inp
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| stem.clone());
            let out = out_dir.join(format!("{}.{}", stem, format));

            let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
            cmd.arg("-y").arg("-i").arg(inp);
            if max_width > 0 {
                cmd.arg("-vf").arg(format!("scale='min({},iw)':-2", max_width));
            }
            for (k, v) in image_format_ffmpeg_args(&format, quality) {
                cmd.arg(k).arg(v);
            }
            cmd.arg(&out);
            #[cfg(windows)]
            cmd.creation_flags(0x08000000);

            let res = cmd.output().await?;
            if res.status.success() && out.exists() {
                ok += 1;
            } else {
                failed.push(name);
                let stderr = String::from_utf8_lossy(&res.stderr);
                last_err = stderr
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .to_string();
            }
        }
        on_progress(total, total);
        if ok == 0 {
            return Err(format!("nenhuma imagem convertida: {}", last_err).into());
        }
        Ok((out_dir, failed))
    }

    pub async fn verify_integrity(&self, file: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-v")
            .arg("error")
            .arg("-i")
            .arg(file)
            .arg("-f")
            .arg("null")
            .arg("-");
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let output = cmd.output().await?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() && stderr.trim().is_empty() {
            Ok(())
        } else {
            let last = stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("arquivo possivelmente corrompido");
            Err(last.trim().to_string().into())
        }
    }

    pub async fn watermark_video(
        &self,
        input: &str,
        watermark: &str,
        output: &str,
        position: &str,
        scale_pct: u32,
        opacity: f32,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let out = PathBuf::from(output);
        let margin = 16;
        let overlay_pos = match position {
            "tl" => format!("{m}:{m}", m = margin),
            "tr" => format!("W-w-{m}:{m}", m = margin),
            "bl" => format!("{m}:H-h-{m}", m = margin),
            "center" => "(W-w)/2:(H-h)/2".to_string(),
            _ => format!("W-w-{m}:H-h-{m}", m = margin),
        };
        let scale = (scale_pct.clamp(5, 400) as f32) / 100.0;
        let opacity = opacity.clamp(0.0, 1.0);
        let filter = format!(
            "[1:v]format=rgba,colorchannelmixer=aa={op},scale=iw*{sc}:-1[wm];\
             [0:v][wm]overlay={pos}",
            op = opacity,
            sc = scale,
            pos = overlay_pos
        );

        let total = self.probe_duration_secs(Path::new(input)).await;
        let input = input.to_string();
        let watermark = watermark.to_string();
        let out_clone = out.clone();
        let nop = |_: Progress| {};
        self.run_ffmpeg_progress(total, Some(&nop), |cmd| {
            cmd.arg("-i")
                .arg(&input)
                .arg("-i")
                .arg(&watermark)
                .arg("-filter_complex")
                .arg(&filter)
                .arg("-c:a")
                .arg("copy")
                .arg(&out_clone);
        })
        .await
        .map_err(|e| format!("ffmpeg falhou ao aplicar marca d'água: {}", e))?;
        Ok(out)
    }

    pub async fn watermark_preview(
        &self,
        video: &str,
        watermark: &str,
        out: &Path,
        position: &str,
        scale_pct: u32,
        opacity: f32,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let out = out.to_path_buf();
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let margin = 16;
        let overlay_pos = match position {
            "tl" => format!("{m}:{m}", m = margin),
            "tr" => format!("W-w-{m}:{m}", m = margin),
            "bl" => format!("{m}:H-h-{m}", m = margin),
            "center" => "(W-w)/2:(H-h)/2".to_string(),
            _ => format!("W-w-{m}:H-h-{m}", m = margin),
        };
        let scale = (scale_pct.clamp(5, 400) as f32) / 100.0;
        let opacity = opacity.clamp(0.0, 1.0);
        let filter = format!(
            "[0:v]thumbnail[b];\
             [1:v]format=rgba,colorchannelmixer=aa={op},scale=iw*{sc}:-1[wm];\
             [b][wm]overlay={pos},scale='min(640,iw)':-1[o]",
            op = opacity,
            sc = scale,
            pos = overlay_pos
        );

        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(video)
            .arg("-i")
            .arg(watermark)
            .arg("-filter_complex")
            .arg(&filter)
            .arg("-map")
            .arg("[o]")
            .arg("-frames:v")
            .arg("1")
            .arg(&out);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let output = cmd.output().await?;
        if !output.status.success() || !out.exists() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            return Err(format!("falha ao gerar pré-visualização: {}", last).into());
        }
        Ok(out)
    }

    pub(super) async fn transcode_media<F>(
        &self,
        input: &Path,
        output: &Path,
        preset: &str,
        on_progress: Option<&F>,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: Fn(Progress) + Send + Sync,
    {
        let total = self.probe_duration_secs(input).await;
        let input = input.to_path_buf();
        let output = output.to_path_buf();
        let is_gif = output
            .extension()
            .map(|e| e.eq_ignore_ascii_case("gif"))
            .unwrap_or(false);
        if is_gif {
            return self
                .run_ffmpeg_progress(total, on_progress, |cmd| {
                    cmd.arg("-i")
                        .arg(&input)
                        .arg("-vf")
                        .arg(
                            "fps=12,scale=480:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse",
                        )
                        .arg(&output);
                })
                .await
                .map_err(|e| format!("ffmpeg falhou ao gerar GIF: {}", e).into());
        }

        let preset = preset.to_string();
        self.run_ffmpeg_progress(total, on_progress, |cmd| {
            cmd.arg("-i")
                .arg(&input)
                .arg("-map_metadata")
                .arg("0");
            match preset.as_str() {
                "compress" => {
                    cmd.arg("-c:v")
                        .arg("libx264")
                        .arg("-crf")
                        .arg("28")
                        .arg("-preset")
                        .arg("medium")
                        .arg("-c:a")
                        .arg("aac")
                        .arg("-b:a")
                        .arg("128k");
                }
                "1080" => {
                    cmd.arg("-vf")
                        .arg("scale=-2:1080")
                        .arg("-c:v")
                        .arg("libx264")
                        .arg("-crf")
                        .arg("22")
                        .arg("-c:a")
                        .arg("aac")
                        .arg("-b:a")
                        .arg("160k");
                }
                "720" => {
                    cmd.arg("-vf")
                        .arg("scale=-2:720")
                        .arg("-c:v")
                        .arg("libx264")
                        .arg("-crf")
                        .arg("23")
                        .arg("-c:a")
                        .arg("aac")
                        .arg("-b:a")
                        .arg("160k");
                }
                "480" => {
                    cmd.arg("-vf")
                        .arg("scale=-2:480")
                        .arg("-c:v")
                        .arg("libx264")
                        .arg("-crf")
                        .arg("24")
                        .arg("-c:a")
                        .arg("aac")
                        .arg("-b:a")
                        .arg("128k");
                }
                p if p.starts_with("manual:") => {
                    let parts: Vec<&str> = p.splitn(5, ':').collect();
                    let g = |i: usize| parts.get(i).map(|s| s.trim()).filter(|s| !s.is_empty());
                    cmd.arg("-c:v").arg("libx264");
                    if let Some(h) = g(1) {
                        cmd.arg("-vf").arg(format!("scale=-2:{}", h));
                    }
                    if let Some(fps) = g(2) {
                        cmd.arg("-r").arg(fps);
                    }
                    match g(3) {
                        Some(vb) => {
                            cmd.arg("-b:v").arg(vb);
                        }
                        None => {
                            cmd.arg("-crf").arg("23");
                        }
                    }
                    cmd.arg("-c:a")
                        .arg("aac")
                        .arg("-b:a")
                        .arg(g(4).unwrap_or("160k"));
                }
                _ => {}
            }
            cmd.arg(&output);
        })
        .await
        .map_err(|e| format!("ffmpeg falhou ao converter: {}", e).into())
    }
}

/// Args de qualidade do ffmpeg por formato de imagem.
/// PNG é sem perdas — não recebe parâmetro de qualidade.
pub fn image_format_ffmpeg_args(format: &str, quality: u32) -> Vec<(String, String)> {
    match format {
        "jpg" | "jpeg" => {
            let qv = 2 + ((100u32.saturating_sub(quality.min(100))) * 29 / 100);
            vec![("-q:v".into(), qv.to_string())]
        }
        "webp" => vec![("-quality".into(), quality.min(100).to_string())],
        "png" => vec![("-compression_level".into(), "9".into())],
        _ => Vec::new(),
    }
}

/// Resumo de falhas de lote de imagens (texto honesto para a UI).
pub fn image_batch_summary(ok: usize, failed: usize, pt: bool) -> String {
    if failed == 0 {
        if pt {
            format!("{} de {} convertidas", ok, ok)
        } else {
            format!("{} of {} converted", ok, ok)
        }
    } else if ok == 0 {
        if pt {
            format!("Nenhuma convertida — {} falharam", failed)
        } else {
            format!("None converted — {} failed", failed)
        }
    } else {
        let total = ok + failed;
        if pt {
            format!("{} de {} convertidas — {} falharam", ok, total, failed)
        } else {
            format!("{} of {} converted — {} failed", ok, total, failed)
        }
    }
}

fn profile_temp_output(output: &Path) -> PathBuf {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let stem = output
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    let ext = output.extension().and_then(|value| value.to_str()).unwrap_or("mkv");
    parent.join(format!("{}.lumen-transcoding.{}", stem, ext))
}

fn video_profile_ffmpeg_args(profile: &super::VideoProfile) -> Vec<&'static str> {
    match profile.extension {
        "mp4" => vec![
            "-c:v", profile.video_encoder, "-crf", "23", "-preset", "medium", "-pix_fmt",
            "yuv420p", "-c:a", profile.audio_encoder, "-b:a", "192k", "-movflags", "+faststart",
        ],
        "mkv" => vec![
            "-c:v", profile.video_encoder, "-crf", "30", "-b:v", "0", "-cpu-used", "6", "-c:a",
            profile.audio_encoder,
        ],
        "webm" => vec![
            "-c:v", profile.video_encoder, "-crf", "31", "-b:v", "0", "-row-mt", "1", "-c:a",
            profile.audio_encoder, "-b:a", "160k",
        ],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_profiles_have_the_promised_ffmpeg_encoders() {
        let mp4 = video_profile_ffmpeg_args(super::super::video_profile("mp4").unwrap());
        assert!(mp4.windows(2).any(|args| args == ["-c:v", "libx264"]));
        assert!(mp4.windows(2).any(|args| args == ["-c:a", "aac"]));

        let mkv = video_profile_ffmpeg_args(super::super::video_profile("mkv").unwrap());
        assert!(mkv.windows(2).any(|args| args == ["-c:v", "libaom-av1"]));
        assert!(mkv.windows(2).any(|args| args == ["-c:a", "flac"]));

        let webm = video_profile_ffmpeg_args(super::super::video_profile("webm").unwrap());
        assert!(webm.windows(2).any(|args| args == ["-c:v", "libvpx-vp9"]));
        assert!(webm.windows(2).any(|args| args == ["-c:a", "libopus"]));
    }

    #[test]
    fn parse_ffmpeg_duration_hh_mm_ss() {
        assert_eq!(
            parse_ffmpeg_duration("  Duration: 00:01:23.45, start: 0.0, bitrate: 1000 kb/s"),
            Some(83.45)
        );
        assert_eq!(
            parse_ffmpeg_duration("Duration: 01:00:00.00, start: 0.000000"),
            Some(3600.0)
        );
    }

    #[test]
    fn parse_ffmpeg_duration_missing() {
        assert_eq!(parse_ffmpeg_duration("Stream #0:0: Video"), None);
        assert_eq!(parse_ffmpeg_duration(""), None);
    }

    #[test]
    fn parse_out_time_us_to_seconds() {
        assert_eq!(parse_out_time_us("out_time_us=1500000"), Some(1.5));
        assert_eq!(parse_out_time_us("out_time_us=0"), Some(0.0));
        assert_eq!(parse_out_time_us("out_time_ms=2500"), Some(2.5));
        assert_eq!(parse_out_time_us("progress=continue"), None);
    }

    #[test]
    fn parse_stream_codecs_from_ffmpeg_probe() {
        let stderr = r#"
  Duration: 00:01:00.00, start: 0.000000
  Stream #0:0(und): Video: h264 (avc1 / 0x31637661), yuv420p, 1920x1080
  Stream #0:1(eng): Audio: aac (mp4a / 0x6134706D), 48000 Hz, stereo
"#;
        let c = parse_ffmpeg_stream_codecs(stderr);
        assert_eq!(c.video.as_deref(), Some("h264"));
        assert_eq!(c.audio.as_deref(), Some("aac"));
    }

    #[test]
    fn finalize_mode_prefers_remux_for_matching_mp4() {
        let mp4 = super::super::video_profile("mp4").unwrap();
        let ok = StreamCodecs {
            video: Some("h264".into()),
            audio: Some("aac".into()),
        };
        assert_eq!(finalize_mode(mp4, &ok), FinalizeMode::RemuxCopy);

        let bad_v = StreamCodecs {
            video: Some("vp9".into()),
            audio: Some("aac".into()),
        };
        assert_eq!(finalize_mode(mp4, &bad_v), FinalizeMode::FullEncode);

        let bad_a = StreamCodecs {
            video: Some("h264".into()),
            audio: Some("opus".into()),
        };
        assert_eq!(finalize_mode(mp4, &bad_a), FinalizeMode::RecodeAudio);
    }

    #[test]
    fn finalize_mode_webm_and_mkv() {
        let webm = super::super::video_profile("webm").unwrap();
        assert_eq!(
            finalize_mode(
                webm,
                &StreamCodecs {
                    video: Some("vp9".into()),
                    audio: Some("opus".into()),
                }
            ),
            FinalizeMode::RemuxCopy
        );
        let mkv = super::super::video_profile("mkv").unwrap();
        assert_eq!(
            finalize_mode(
                mkv,
                &StreamCodecs {
                    video: Some("h264".into()),
                    audio: Some("aac".into()),
                }
            ),
            FinalizeMode::FullEncode
        );
        assert_eq!(
            finalize_mode(
                mkv,
                &StreamCodecs {
                    video: Some("av1".into()),
                    audio: Some("opus".into()),
                }
            ),
            FinalizeMode::RemuxCopy
        );
    }

    #[test]
    fn progress_fraction_clamps_above_one() {
        assert_eq!(progress_fraction(150.0, 100.0), 1.0);
        assert!((progress_fraction(50.0, 100.0) - 0.5).abs() < 1e-9);
        assert_eq!(progress_fraction(10.0, 0.0), 0.0);
    }

    #[test]
    fn image_format_args_quality_per_format() {
        let jpg = image_format_ffmpeg_args("jpg", 90);
        assert!(jpg.iter().any(|(k, _)| k == "-q:v"));
        let webp = image_format_ffmpeg_args("webp", 80);
        assert!(webp.iter().any(|(k, v)| k == "-quality" && v == "80"));
        let png = image_format_ffmpeg_args("png", 50);
        assert!(!png.iter().any(|(k, _)| k == "-q:v" || k == "-quality"));
        assert!(png.iter().any(|(k, _)| k == "-compression_level"));
    }

    #[test]
    fn image_batch_summary_three_cases() {
        assert!(image_batch_summary(12, 0, true).contains("12 de 12"));
        assert!(image_batch_summary(8, 4, true).contains("8 de 12"));
        assert!(image_batch_summary(8, 4, true).contains("4 falharam"));
        assert!(image_batch_summary(0, 5, true).contains("Nenhuma"));
        assert!(image_batch_summary(3, 0, false).contains("3 of 3"));
    }
}
