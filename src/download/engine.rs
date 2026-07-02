use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::config::settings::ConvertEngine;

use printpdf::{
    BuiltinFont, ColorBits, ColorSpace, Image, ImageTransform, ImageXObject, Mm, PdfDocument, Px,
};
use yt_dlp::client::deps::Libraries;
use yt_dlp::VideoSelection;

pub fn sanitize_filename(name: &str) -> String {
    let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\0'];
    let mut sanitized: String = name
        .chars()
        .filter(|c| !invalid_chars.contains(c) && !c.is_control())
        .collect();
    sanitized.truncate(200);
    if sanitized.trim().is_empty() {
        sanitized = "download".to_string();
    }
    sanitized
}

#[derive(Clone, Default)]
pub struct AudioTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: String,
    pub genre: String,
    pub track: String,
    pub bpm: String,
    pub key: String,
}

pub fn read_audio_tags(path: &str) -> AudioTags {
    use lofty::prelude::{Accessor, ItemKey, TaggedFileExt};
    let mut t = AudioTags::default();
    let Ok(tagged) = lofty::read_from_path(path) else {
        return t;
    };
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return t;
    };
    t.title = tag.title().map(|c| c.to_string()).unwrap_or_default();
    t.artist = tag.artist().map(|c| c.to_string()).unwrap_or_default();
    t.album = tag.album().map(|c| c.to_string()).unwrap_or_default();
    t.year = tag
        .get_string(ItemKey::Year)
        .or_else(|| tag.get_string(ItemKey::RecordingDate))
        .unwrap_or_default()
        .to_string();
    t.genre = tag.genre().map(|c| c.to_string()).unwrap_or_default();
    t.track = tag.track().map(|n| n.to_string()).unwrap_or_default();
    t.bpm = tag
        .get_string(ItemKey::IntegerBpm)
        .or_else(|| tag.get_string(ItemKey::Bpm))
        .unwrap_or_default()
        .to_string();
    t.key = tag.get_string(ItemKey::InitialKey).unwrap_or_default().to_string();
    t
}

pub fn write_audio_tags(path: &str, t: &AudioTags) -> Result<(), Box<dyn std::error::Error>> {
    use lofty::config::WriteOptions;
    use lofty::prelude::{Accessor, ItemKey, TagExt, TaggedFileExt};
    use lofty::tag::Tag;

    let mut tagged = lofty::read_from_path(path)?;
    let tag_type = tagged.primary_tag_type();
    if tagged.primary_tag_mut().is_none() {
        tagged.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged
        .primary_tag_mut()
        .ok_or("não foi possível criar a tag")?;

    if t.title.trim().is_empty() {
        tag.remove_title();
    } else {
        tag.set_title(t.title.clone());
    }
    if t.artist.trim().is_empty() {
        tag.remove_artist();
    } else {
        tag.set_artist(t.artist.clone());
    }
    if t.album.trim().is_empty() {
        tag.remove_album();
    } else {
        tag.set_album(t.album.clone());
    }
    if !t.year.trim().is_empty() {
        tag.insert_text(ItemKey::Year, t.year.trim().to_string());
    }
    if t.genre.trim().is_empty() {
        tag.remove_genre();
    } else {
        tag.set_genre(t.genre.clone());
    }
    if let Ok(n) = t.track.trim().parse::<u32>() {
        tag.set_track(n);
    }
    if !t.bpm.trim().is_empty() {
        tag.insert_text(ItemKey::IntegerBpm, t.bpm.trim().to_string());
    }
    if !t.key.trim().is_empty() {
        tag.insert_text(ItemKey::InitialKey, t.key.trim().to_string());
    }

    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

pub fn smart_clean_name(title: &str) -> String {
    const JUNK: &[&str] = &[
        "official", "oficial", "video", "vídeo", "audio", "áudio", "lyric", "letra",
        "lyrics", "hd", "4k", "8k", "mv", "m/v", "clipe", "visualizer", "remaster",
        "remastered", "explicit", "full album", "hq",
    ];
    let chars: Vec<char> = title.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let close = match chars[i] {
            '[' => Some(']'),
            '(' => Some(')'),
            '{' => Some('}'),
            _ => None,
        };
        if let Some(cl) = close {
            if let Some(j) = (i + 1..chars.len()).find(|&k| chars[k] == cl) {
                let inner: String = chars[i + 1..j].iter().collect::<String>().to_lowercase();
                if JUNK.iter().any(|k| inner.contains(k)) {
                    i = j + 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    let out = out.replace(" - Topic", "").replace("- Topic", "");
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed
        .trim()
        .trim_matches(|c| c == '-' || c == '|' || c == '·' || c == '_')
        .trim()
        .to_string();
    if trimmed.is_empty() {
        title.trim().to_string()
    } else {
        trimmed
    }
}

pub fn apply_template(template: &str, title: &str, channel: &str) -> String {
    let mut s = template.replace("%(title)s", title);
    s = s.replace("%(uploader)s", channel).replace("%(channel)s", channel);
    if s.trim().is_empty() {
        s = title.to_string();
    }
    s
}

#[derive(Clone, Copy, Default)]
pub struct Progress {
    pub fraction: f64,
    pub speed_bps: f64,
    pub eta_secs: u64,
    pub downloaded_bytes: u64,
}

#[derive(Default)]
pub struct NetStats {
    pub current: f32,
    pub history: Vec<f32>,
}

pub struct DownloadEngine {
    downloader: yt_dlp::Downloader,
    ffmpeg_path: PathBuf,
    libs_dir: PathBuf,
    preview_cache: Mutex<HashMap<String, VideoPreview>>,
    net: Mutex<NetStats>,
}

fn binary_path(dir: &PathBuf, name: &str) -> PathBuf {
    if cfg!(windows) {
        dir.join(format!("{}.exe", name))
    } else {
        dir.join(name)
    }
}

impl DownloadEngine {
    pub async fn new(output_dir: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LumenDownloader");
        let libs_dir = data_dir.join("libs");
        std::fs::create_dir_all(&libs_dir)?;
        std::fs::create_dir_all(&output_dir)?;
        cleanup_temp_dir(&output_dir);

        let libraries = Libraries::new(
            binary_path(&libs_dir, "yt-dlp"),
            binary_path(&libs_dir, "ffmpeg"),
        );

        let libraries = libraries.install_dependencies().await?;
        let ffmpeg_path = libraries.ffmpeg.clone();

        let downloader = yt_dlp::Downloader::builder(libraries, &output_dir)
            .with_timeout(Duration::from_secs(1800))
            .build()
            .await?;

        Ok(DownloadEngine {
            downloader,
            ffmpeg_path,
            libs_dir,
            preview_cache: Mutex::new(HashMap::new()),
            net: Mutex::new(NetStats::default()),
        })
    }

    pub fn net_stats(&self) -> (f32, Vec<f32>) {
        let n = self.net.lock().unwrap();
        (n.current, n.history.clone())
    }

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
        if !is_youtube(url) {
            return self.ytdlp_title(url).await;
        }
        let video = self.downloader.fetch_video_infos(url).await?;
        Ok(video.title.clone())
    }

    pub async fn fetch_preview(
        &self,
        url: &str,
    ) -> Result<VideoPreview, Box<dyn std::error::Error>> {
        if let Some(cached) = self.preview_cache.lock().unwrap().get(url).cloned() {
            return Ok(cached);
        }
        if !is_youtube(url) {
            let p = self.ytdlp_preview(url).await?;
            self.preview_cache
                .lock()
                .unwrap()
                .insert(url.to_string(), p.clone());
            return Ok(p);
        }
        let video = self.downloader.fetch_video_infos(url).await?;

        let mut resolutions: Vec<u32> = video
            .formats
            .iter()
            .filter_map(|f| f.video_resolution.height)
            .collect();
        resolutions.sort_unstable();
        resolutions.dedup();
        resolutions.reverse();

        let best_video = video
            .best_video_format()
            .and_then(|f| f.file_info.filesize.or(f.file_info.filesize_approx));
        let best_audio = video
            .best_audio_format()
            .and_then(|f| f.file_info.filesize.or(f.file_info.filesize_approx));
        let best_combined = video
            .formats
            .iter()
            .filter(|f| f.format_type().is_audio_and_video())
            .max_by_key(|f| f.video_resolution.height.unwrap_or(0))
            .and_then(|f| f.file_info.filesize.or(f.file_info.filesize_approx));
        let est_size_video = match (best_video, best_audio) {
            (Some(v), Some(a)) => Some(v + a),
            (Some(v), None) => Some(v),
            (None, Some(a)) => Some(a),
            (None, None) => best_combined,
        };
        let best_audio = best_audio.or(best_combined);

        let channel = video.channel.clone().or(video.uploader.clone()).unwrap_or_default();
        let duration = video
            .duration_string
            .clone()
            .or_else(|| video.duration.map(format_duration))
            .unwrap_or_default();

        let thumbnail = match &video.thumbnail {
            Some(url) => download_thumbnail(url).await,
            None => None,
        };

        let preview = VideoPreview {
            title: video.title.clone(),
            channel,
            duration,
            resolutions,
            est_size_video,
            est_size_audio: best_audio,
            thumbnail,
            is_live: video.is_live.unwrap_or(false) || video.live_status == "is_live",
        };
        self.preview_cache
            .lock()
            .unwrap()
            .insert(url.to_string(), preview.clone());
        Ok(preview)
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

    fn ytdlp_path(&self) -> PathBuf {
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
        let mut last_err = String::new();

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

            let mut child = cmd.spawn()?;
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
                    _ = wait_for_stop(&stop) => {
                        stopped = true;
                        let _ = child.start_kill();
                        break;
                    }
                }
            }
            {
                self.net.lock().unwrap().current = 0.0;
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
            crate::applog::error(&format!("download falhou (tentativa {}): {}", attempt + 1, last_err.lines().last().unwrap_or("")));
            if attempt + 1 < MAX_ATTEMPTS {
                tokio::time::sleep(Duration::from_secs(2 * (attempt as u64 + 1))).await;
            }
        }

        cleanup_partials(&folder, &stem);
        Err(friendly_error(&last_err).into())
    }

    pub async fn fetch_playlist(
        &self,
        playlist_id: &str,
    ) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
        let playlist = self
            .downloader
            .youtube_extractor()
            .fetch_playlist_paginated(playlist_id, 1, 1000)
            .await?;

        let items = playlist
            .entries
            .into_iter()
            .map(|e| {
                let url = if e.url.starts_with("http") {
                    e.url
                } else {
                    format!("https://www.youtube.com/watch?v={}", e.id)
                };
                (url, e.title)
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

    async fn transcode_audio(
        &self,
        input: &Path,
        output: &Path,
        format: &str,
        meta: &AudioMeta,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(input)
            .arg("-vn")
            .arg("-map_metadata")
            .arg("0");

        if let Some(t) = &meta.title {
            cmd.arg("-metadata").arg(format!("title={}", t));
        }
        if let Some(a) = &meta.artist {
            cmd.arg("-metadata").arg(format!("artist={}", a));
        }
        if let Some(al) = &meta.album {
            cmd.arg("-metadata").arg(format!("album={}", al));
        }

        match format {
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
        cmd.arg(output);

        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let result = cmd.output().await?;
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            return Err(format!("ffmpeg falhou ao converter para {}: {}", format, last).into());
        }
        Ok(())
    }

    pub async fn convert_file(
        &self,
        input: &str,
        output_path: &str,
        format: &str,
        preset: &str,
        engine: ConvertEngine,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let input_path = PathBuf::from(input);
        let mut out = PathBuf::from(output_path);
        out.set_extension(format);

        if out == input_path {
            let stem = out
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "convertido".to_string());
            out.set_file_name(format!("{}_convertido.{}", stem, format));
        }

        match categorize(&input_path) {
            FileCategory::Document => {
                if format == "txt" {
                    return self.pdf_to_text(&input_path, &out).await;
                }
                return self.pdf_to_images(&input_path, &out, format).await;
            }
            FileCategory::Office => {
                return self.office_convert(&input_path, &out, format, engine).await;
            }
            _ => {}
        }

        if format == "pdf" {
            return self.image_to_pdf(&input_path, &out).await;
        }

        if is_audio_format(format) {
            self.transcode_audio(&input_path, &out, format, &AudioMeta::default())
                .await?;
        } else {
            self.transcode_media(&input_path, &out, preset).await?;
        }
        Ok(out)
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

        cleanup_partials(folder, stem);
        result
    }

    async fn office_convert(
        &self,
        input: &Path,
        out: &Path,
        format: &str,
        engine: ConvertEngine,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        // TXT é sempre extração nativa (rápida e sem dependência).
        // Para PDF, respeita a escolha do usuário; Auto pega o melhor disponível.
        let chosen = if format == "txt" {
            ConvertEngine::Rust
        } else {
            match engine {
                ConvertEngine::Auto => auto_pick_engine(),
                other => other,
            }
        };

        match chosen {
            ConvertEngine::MsOffice => self.office_via_msoffice(input, out, format).await,
            ConvertEngine::LibreOffice => self.office_via_libreoffice(input, out, format).await,
            _ => self.office_via_native(input, out, format).await,
        }
    }

    async fn office_via_native(
        &self,
        input: &Path,
        out: &Path,
        format: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let input = input.to_path_buf();
        let out_path = out.to_path_buf();
        let out_ret = out_path.clone();
        let format = format.to_string();
        let title = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Documento".to_string());

        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let text = office_extract_text(&input)?;
            match format.as_str() {
                "txt" => std::fs::write(&out_path, text).map_err(|e| e.to_string()),
                "pdf" => render_text_pdf(&text, &out_path, &title),
                other => Err(format!(
                    "Conversão nativa para \"{}\" não suportada. Use PDF ou TXT.",
                    other
                )),
            }
        })
        .await
        .map_err(|e| e.to_string())??;

        Ok(out_ret)
    }

    async fn office_via_libreoffice(
        &self,
        input: &Path,
        out: &Path,
        format: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let soffice = libreoffice_path()
            .ok_or("LibreOffice não encontrado. Instale-o ou escolha outro motor.")?;
        let outdir = out.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        let input_stem = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "doc".to_string());

        let mut cmd = tokio::process::Command::new(&soffice);
        cmd.arg("--headless")
            .arg("--convert-to")
            .arg(format)
            .arg("--outdir")
            .arg(&outdir)
            .arg(input);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let result = cmd.output().await.map_err(|e| e.to_string())?;
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!("LibreOffice falhou: {}", stderr.trim()).into());
        }

        let produced = outdir.join(format!("{}.{}", input_stem, format));
        if produced.exists() {
            if produced != out {
                let _ = std::fs::rename(&produced, out);
                if out.exists() {
                    return Ok(out.to_path_buf());
                }
                return Ok(produced);
            }
            return Ok(produced);
        }
        Err("Arquivo convertido não encontrado.".into())
    }

    async fn office_via_msoffice(
        &self,
        input: &Path,
        out: &Path,
        format: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        if format != "pdf" {
            return Err("MS Office só é usado aqui para gerar PDF.".into());
        }
        #[cfg(not(windows))]
        {
            let _ = (input, out);
            return Err("MS Office só está disponível no Windows.".into());
        }
        #[cfg(windows)]
        {
            let input = input.to_path_buf();
            let out_path = out.to_path_buf();
            let out_ret = out_path.clone();
            tokio::task::spawn_blocking(move || msoffice_to_pdf(&input, &out_path))
                .await
                .map_err(|e| e.to_string())??;
            Ok(out_ret)
        }
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

    pub async fn batch_convert_images(
        &self,
        inputs: Vec<PathBuf>,
        out_dir: PathBuf,
        format: String,
        max_width: u32,
        quality: u32,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(&out_dir)?;
        let mut ok = 0usize;
        let mut last_err = String::new();
        for inp in &inputs {
            let stem = inp
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "imagem".to_string());
            let out = out_dir.join(format!("{}.{}", stem, format));

            let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
            cmd.arg("-y").arg("-i").arg(inp);
            if max_width > 0 {
                cmd.arg("-vf").arg(format!("scale='min({},iw)':-2", max_width));
            }
            match format.as_str() {
                "jpg" | "jpeg" => {
                    let qv = 2 + ((100u32.saturating_sub(quality.min(100))) * 29 / 100);
                    cmd.arg("-q:v").arg(qv.to_string());
                }
                "webp" => {
                    cmd.arg("-quality").arg(quality.min(100).to_string());
                }
                "png" => {
                    cmd.arg("-compression_level").arg("9");
                }
                _ => {}
            }
            cmd.arg(&out);
            #[cfg(windows)]
            cmd.creation_flags(0x08000000);

            let res = cmd.output().await?;
            if res.status.success() && out.exists() {
                ok += 1;
            } else {
                let stderr = String::from_utf8_lossy(&res.stderr);
                last_err = stderr
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .to_string();
            }
        }
        if ok == 0 {
            return Err(format!("nenhuma imagem convertida: {}", last_err).into());
        }
        Ok(out_dir)
    }

    pub async fn transcribe(
        &self,
        input: &str,
        lang: &str,
        translate: bool,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let (exe, model) = self.ensure_whisper().await?;

        let input_path = PathBuf::from(input);
        let folder = input_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = input_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "audio".to_string());

        let wav = folder.join(format!("{}.whisper.wav", stem));
        let mut conv = tokio::process::Command::new(&self.ffmpeg_path);
        conv.arg("-y")
            .arg("-i")
            .arg(&input_path)
            .arg("-ar")
            .arg("16000")
            .arg("-ac")
            .arg("1")
            .arg("-c:a")
            .arg("pcm_s16le")
            .arg(&wav);
        #[cfg(windows)]
        conv.creation_flags(0x08000000);
        let conv_res = conv.output().await?;
        if !conv_res.status.success() {
            let stderr = String::from_utf8_lossy(&conv_res.stderr);
            let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            return Err(format!("ffmpeg falhou ao preparar o áudio: {}", last).into());
        }

        let out_base = folder.join(&stem);
        let mut cmd = tokio::process::Command::new(&exe);
        if let Some(exe_dir) = exe.parent() {
            cmd.current_dir(exe_dir);
        }
        cmd.arg("-m")
            .arg(&model)
            .arg("-f")
            .arg(&wav)
            .arg("-otxt")
            .arg("-of")
            .arg(&out_base)
            .arg("-l")
            .arg(if lang.trim().is_empty() { "auto" } else { lang.trim() });
        if translate {
            cmd.arg("-tr");
        }
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let result = cmd.output().await?;
        let _ = std::fs::remove_file(&wav);
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            let stdout = String::from_utf8_lossy(&result.stdout);
            let combined = format!("{}\n{}", stderr, stdout);
            let last = combined
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| {
                    format!(
                        "o binário não iniciou (código {:?}). Tente novamente; pode faltar uma DLL do whisper.cpp.",
                        result.status.code()
                    )
                });
            crate::applog::error(&format!("whisper falhou: {}", last));
            return Err(format!("Whisper falhou: {}", last).into());
        }

        let out = folder.join(format!("{}.txt", stem));
        if out.exists() {
            Ok(out)
        } else {
            find_output(&folder, &stem).ok_or_else(|| "Transcrição não encontrada.".into())
        }
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

        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(input)
            .arg("-i")
            .arg(watermark)
            .arg("-filter_complex")
            .arg(&filter)
            .arg("-c:a")
            .arg("copy")
            .arg(&out);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let output = cmd.output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            return Err(format!("ffmpeg falhou ao aplicar marca d'água: {}", last).into());
        }
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

    async fn ensure_whisper(&self) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
        let dir = self.libs_dir.join("whisper");
        std::fs::create_dir_all(&dir)?;
        let model = dir.join("ggml-base.bin");

        if find_whisper_exe(&dir).is_none() {
            #[cfg(windows)]
            {
                crate::applog::info("baixando whisper.cpp (binário)");
                let url = "https://github.com/ggerganov/whisper.cpp/releases/latest/download/whisper-bin-x64.zip";
                let bytes = reqwest::get(url).await?.error_for_status()?.bytes().await?;
                let dir2 = dir.clone();
                tokio::task::spawn_blocking(move || -> Result<(), String> {
                    let reader = std::io::Cursor::new(bytes.as_ref());
                    let mut archive = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;
                    for i in 0..archive.len() {
                        let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
                        let outpath = match f.enclosed_name() {
                            Some(p) => dir2.join(p),
                            None => continue,
                        };
                        if f.is_dir() {
                            std::fs::create_dir_all(&outpath).ok();
                        } else {
                            if let Some(p) = outpath.parent() {
                                std::fs::create_dir_all(p).ok();
                            }
                            let mut out = std::fs::File::create(&outpath).map_err(|e| e.to_string())?;
                            std::io::copy(&mut f, &mut out).map_err(|e| e.to_string())?;
                        }
                    }
                    Ok(())
                })
                .await
                .map_err(|e| e.to_string())??;
            }
            #[cfg(not(windows))]
            {
                return Err("Transcrição automática disponível apenas no Windows por enquanto.".into());
            }
        }

        let exe = find_whisper_exe(&dir)
            .ok_or("binário do whisper.cpp não encontrado após o download")?;

        let model_ok = std::fs::metadata(&model)
            .map(|m| m.len() > 1_000_000)
            .unwrap_or(false);
        if !model_ok {
            let _ = std::fs::remove_file(&model);
            crate::applog::info("baixando modelo do whisper (ggml-base)");
            let url =
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin?download=true";
            let bytes = reqwest::get(url).await?.error_for_status()?.bytes().await?;
            if bytes.len() < 1_000_000 {
                return Err("download do modelo do Whisper falhou (arquivo muito pequeno).".into());
            }
            let model2 = model.clone();
            tokio::task::spawn_blocking(move || std::fs::write(&model2, &bytes))
                .await
                .map_err(|e| e.to_string())??;
        }

        Ok((exe, model))
    }

    async fn ensure_pdfium(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        use pdfium_render::prelude::Pdfium;

        let lib_name = Pdfium::pdfium_platform_library_name();
        let lib_path = self.libs_dir.join(&lib_name);
        if lib_path.exists() {
            return Ok(lib_path);
        }

        let asset = if cfg!(windows) {
            "pdfium-win-x64.tgz"
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "pdfium-mac-arm64.tgz"
            } else {
                "pdfium-mac-x64.tgz"
            }
        } else {
            "pdfium-linux-x64.tgz"
        };
        let url = format!(
            "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/{}",
            asset
        );

        let bytes = reqwest::get(&url).await?.error_for_status()?.bytes().await?;

        let lib_path2 = lib_path.clone();
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(&bytes[..]));
            let mut archive = tar::Archive::new(gz);
            for entry in archive.entries().map_err(|e| e.to_string())? {
                let mut entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path().map_err(|e| e.to_string())?.into_owned();
                if path.file_name() == Some(lib_name.as_os_str()) {
                    entry.unpack(&lib_path2).map_err(|e| e.to_string())?;
                    return Ok(());
                }
            }
            Err("pdfium não encontrado no pacote baixado".to_string())
        })
        .await
        .map_err(|e| e.to_string())??;

        Ok(lib_path)
    }

    async fn pdf_to_images(
        &self,
        input: &Path,
        out_base: &Path,
        format: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let pdfium_path = self.ensure_pdfium().await?;

        let input = input.to_path_buf();
        let folder = out_base
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = out_base
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "pagina".to_string());
        let ext = format.to_string();

        let first = tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
            use pdfium_render::prelude::*;

            let bindings = Pdfium::bind_to_library(&pdfium_path)
                .map_err(|e| format!("falha ao carregar pdfium: {}", e))?;
            let pdfium = Pdfium::new(bindings);
            let document = pdfium
                .load_pdf_from_file(&input, None)
                .map_err(|e| format!("falha ao abrir o PDF: {}", e))?;

            let config = PdfRenderConfig::new().scale_page_by_factor(2.0);
            let pages = document.pages();
            let total = pages.len();
            if total == 0 {
                return Err("o PDF não tem páginas".to_string());
            }

            let mut first_path: Option<PathBuf> = None;
            for (index, page) in pages.iter().enumerate() {
                let image = page
                    .render_with_config(&config)
                    .map_err(|e| e.to_string())?
                    .as_image();

                let name = if total == 1 {
                    format!("{}.{}", stem, ext)
                } else {
                    format!("{}_pagina_{}.{}", stem, index + 1, ext)
                };
                let path = folder.join(name);

                let result = if ext == "jpg" || ext == "jpeg" {
                    image.into_rgb8().save(&path)
                } else {
                    image.save(&path)
                };
                result.map_err(|e| e.to_string())?;

                if first_path.is_none() {
                    first_path = Some(path);
                }
            }
            Ok(first_path.unwrap())
        })
        .await
        .map_err(|e| e.to_string())??;

        Ok(first)
    }

    pub async fn merge_pdfs(
        &self,
        inputs: Vec<PathBuf>,
        out: &Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let pdfium_path = self.ensure_pdfium().await?;
        let out2 = out.to_path_buf();
        tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
            use pdfium_render::prelude::*;
            let bindings = Pdfium::bind_to_library(&pdfium_path).map_err(|e| e.to_string())?;
            let pdfium = Pdfium::new(bindings);
            let mut dest = pdfium.create_new_pdf().map_err(|e| e.to_string())?;
            for inp in &inputs {
                let src = pdfium
                    .load_pdf_from_file(inp, None)
                    .map_err(|e| format!("abrir {}: {}", inp.display(), e))?;
                let n = src.pages().len();
                if n == 0 {
                    continue;
                }
                let idx = dest.pages().len();
                dest.pages_mut()
                    .copy_pages_from_document(&src, &format!("1-{}", n), idx)
                    .map_err(|e| e.to_string())?;
            }
            dest.save_to_file(&out2).map_err(|e| e.to_string())?;
            Ok(out2)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.into())
    }

    pub async fn split_pdf(
        &self,
        input: &Path,
        out_folder: &Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let pdfium_path = self.ensure_pdfium().await?;
        let input = input.to_path_buf();
        let folder = out_folder.to_path_buf();
        let stem = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "pdf".to_string());
        tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
            use pdfium_render::prelude::*;
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let bindings = Pdfium::bind_to_library(&pdfium_path).map_err(|e| e.to_string())?;
            let pdfium = Pdfium::new(bindings);
            let src = pdfium
                .load_pdf_from_file(&input, None)
                .map_err(|e| e.to_string())?;
            let total = src.pages().len();
            if total == 0 {
                return Err("o PDF não tem páginas".to_string());
            }
            for i in 0..total {
                let mut dest = pdfium.create_new_pdf().map_err(|e| e.to_string())?;
                dest.pages_mut()
                    .copy_pages_from_document(&src, &format!("{}", i + 1), 0)
                    .map_err(|e| e.to_string())?;
                let path = folder.join(format!("{}_pagina_{}.pdf", stem, i + 1));
                dest.save_to_file(&path).map_err(|e| e.to_string())?;
            }
            Ok(folder)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.into())
    }

    pub async fn rotate_pdf(
        &self,
        input: &Path,
        out: &Path,
        degrees: i32,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let pdfium_path = self.ensure_pdfium().await?;
        let input = input.to_path_buf();
        let out2 = out.to_path_buf();
        tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
            use pdfium_render::prelude::*;
            let bindings = Pdfium::bind_to_library(&pdfium_path).map_err(|e| e.to_string())?;
            let pdfium = Pdfium::new(bindings);
            let doc = pdfium
                .load_pdf_from_file(&input, None)
                .map_err(|e| e.to_string())?;
            let rot = match ((degrees % 360) + 360) % 360 {
                90 => PdfPageRenderRotation::Degrees90,
                180 => PdfPageRenderRotation::Degrees180,
                270 => PdfPageRenderRotation::Degrees270,
                _ => PdfPageRenderRotation::None,
            };
            for mut page in doc.pages().iter() {
                page.set_rotation(rot);
            }
            doc.save_to_file(&out2).map_err(|e| e.to_string())?;
            Ok(out2)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.into())
    }

    pub async fn reorder_pdf(
        &self,
        input: &Path,
        out: &Path,
        order: String,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let pdfium_path = self.ensure_pdfium().await?;
        let input = input.to_path_buf();
        let out2 = out.to_path_buf();
        tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
            use pdfium_render::prelude::*;
            let order = order.trim();
            if order.is_empty() {
                return Err("informe a nova ordem das páginas (ex.: 3,1,2)".to_string());
            }
            let bindings = Pdfium::bind_to_library(&pdfium_path).map_err(|e| e.to_string())?;
            let pdfium = Pdfium::new(bindings);
            let src = pdfium.load_pdf_from_file(&input, None).map_err(|e| e.to_string())?;
            let mut dest = pdfium.create_new_pdf().map_err(|e| e.to_string())?;
            dest.pages_mut()
                .copy_pages_from_document(&src, order, 0)
                .map_err(|e| e.to_string())?;
            dest.save_to_file(&out2).map_err(|e| e.to_string())?;
            Ok(out2)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.into())
    }

    pub async fn compress_pdf(
        &self,
        input: &Path,
        out: &Path,
        dpi: f32,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let pdfium_path = self.ensure_pdfium().await?;
        let input = input.to_path_buf();
        let out2 = out.to_path_buf();
        let out_ret = out2.clone();
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            use pdfium_render::prelude::*;
            let bindings = Pdfium::bind_to_library(&pdfium_path).map_err(|e| e.to_string())?;
            let pdfium = Pdfium::new(bindings);
            let doc = pdfium.load_pdf_from_file(&input, None).map_err(|e| e.to_string())?;
            let total = doc.pages().len();
            if total == 0 {
                return Err("o PDF não tem páginas".to_string());
            }
            let config = PdfRenderConfig::new().scale_page_by_factor((dpi / 72.0) as f32);

            let mut pdoc: Option<printpdf::PdfDocumentReference> = None;
            for (i, page) in doc.pages().iter().enumerate() {
                let bitmap = page.render_with_config(&config).map_err(|e| e.to_string())?;
                let rgb = bitmap.as_image().to_rgb8();
                let (w, h) = rgb.dimensions();
                let xobj = ImageXObject {
                    width: Px(w as usize),
                    height: Px(h as usize),
                    color_space: ColorSpace::Rgb,
                    bits_per_component: ColorBits::Bit8,
                    interpolate: false,
                    image_data: rgb.into_raw(),
                    image_filter: None,
                    smask: None,
                    clipping_bbox: None,
                };
                let wmm = Mm(w as f32 / dpi * 25.4);
                let hmm = Mm(h as f32 / dpi * 25.4);
                if pdoc.is_none() {
                    let (d, pg, layer) =
                        printpdf::PdfDocument::new("Lumen Converter", wmm, hmm, "1");
                    let lr = d.get_page(pg).get_layer(layer);
                    printpdf::Image::from(xobj).add_to_layer(
                        lr,
                        ImageTransform { dpi: Some(dpi), ..Default::default() },
                    );
                    pdoc = Some(d);
                } else {
                    let d = pdoc.as_ref().unwrap();
                    let (pg, layer) = d.add_page(wmm, hmm, format!("{}", i + 1));
                    let lr = d.get_page(pg).get_layer(layer);
                    printpdf::Image::from(xobj).add_to_layer(
                        lr,
                        ImageTransform { dpi: Some(dpi), ..Default::default() },
                    );
                }
            }
            let d = pdoc.ok_or("falha ao gerar o PDF")?;
            let file = std::fs::File::create(&out2).map_err(|e| e.to_string())?;
            let mut writer = std::io::BufWriter::new(file);
            d.save(&mut writer).map_err(|e| e.to_string())?;
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;
        Ok(out_ret)
    }

    pub async fn dependency_status(&self) -> Vec<(String, String)> {
        async fn version(cmd_path: &Path, args: &[&str]) -> Option<String> {
            let mut cmd = tokio::process::Command::new(cmd_path);
            cmd.args(args);
            #[cfg(windows)]
            cmd.creation_flags(0x08000000);
            let out = cmd.output().await.ok()?;
            let s = String::from_utf8_lossy(&out.stdout);
            s.lines().next().map(|l| l.trim().to_string()).filter(|l| !l.is_empty())
        }

        let missing_or_corrupt = |path: &Path| -> String {
            if path.exists() {
                "⚠ corrompido".to_string()
            } else {
                "não instalado".to_string()
            }
        };

        let mut rows = Vec::new();
        let yt = version(&self.ytdlp_path(), &["--version"])
            .await
            .unwrap_or_else(|| missing_or_corrupt(&self.ytdlp_path()));
        rows.push(("yt-dlp".to_string(), yt));

        let ff = version(&self.ffmpeg_path, &["-version"])
            .await
            .map(|l| l.replace("ffmpeg version ", "").split(' ').next().unwrap_or("").to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| missing_or_corrupt(&self.ffmpeg_path));
        rows.push(("ffmpeg".to_string(), ff));

        let pdfium = {
            use pdfium_render::prelude::Pdfium;
            let p = self.libs_dir.join(Pdfium::pdfium_platform_library_name());
            if p.exists() { "instalado".to_string() } else { "não baixado".to_string() }
        };
        rows.push(("pdfium".to_string(), pdfium));

        let whisper = {
            let dir = self.libs_dir.join("whisper");
            let has_exe = find_whisper_exe(&dir).is_some();
            let has_model = dir.join("ggml-base.bin").exists();
            match (has_exe, has_model) {
                (true, true) => "instalado (base)".to_string(),
                (true, false) => "binário ok, sem modelo".to_string(),
                _ => "não baixado".to_string(),
            }
        };
        rows.push(("whisper.cpp".to_string(), whisper));

        rows
    }

    async fn image_to_pdf(
        &self,
        input: &Path,
        out: &Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let input = input.to_path_buf();
        let out_path = out.to_path_buf();
        let out_ret = out_path.clone();

        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let dynamic = image::open(&input).map_err(|e| e.to_string())?;
            let rgb = dynamic.to_rgb8();
            let (width, height) = rgb.dimensions();

            let xobject = ImageXObject {
                width: Px(width as usize),
                height: Px(height as usize),
                color_space: ColorSpace::Rgb,
                bits_per_component: ColorBits::Bit8,
                interpolate: false,
                image_data: rgb.into_raw(),
                image_filter: None,
                smask: None,
                clipping_bbox: None,
            };

            let dpi = 96.0;
            let width_mm = Mm(width as f32 / dpi * 25.4);
            let height_mm = Mm(height as f32 / dpi * 25.4);

            let (doc, page, layer) =
                PdfDocument::new("Lumen Converter", width_mm, height_mm, "Imagem");
            let layer_ref = doc.get_page(page).get_layer(layer);
            Image::from(xobject).add_to_layer(
                layer_ref,
                ImageTransform {
                    dpi: Some(dpi),
                    ..Default::default()
                },
            );

            let file = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
            let mut writer = std::io::BufWriter::new(file);
            doc.save(&mut writer).map_err(|e| e.to_string())?;
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;

        Ok(out_ret)
    }

    pub async fn images_to_pdf_multi(
        &self,
        inputs: Vec<PathBuf>,
        out: &Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let out_path = out.to_path_buf();
        let out_ret = out_path.clone();

        tokio::task::spawn_blocking(move || -> Result<(), String> {
            if inputs.is_empty() {
                return Err("nenhuma imagem selecionada".to_string());
            }
            let dpi = 96.0;
            let make_xobject = |path: &Path| -> Result<(ImageXObject, u32, u32), String> {
                let dynamic = image::open(path).map_err(|e| e.to_string())?;
                let rgb = dynamic.to_rgb8();
                let (w, h) = rgb.dimensions();
                Ok((
                    ImageXObject {
                        width: Px(w as usize),
                        height: Px(h as usize),
                        color_space: ColorSpace::Rgb,
                        bits_per_component: ColorBits::Bit8,
                        interpolate: false,
                        image_data: rgb.into_raw(),
                        image_filter: None,
                        smask: None,
                        clipping_bbox: None,
                    },
                    w,
                    h,
                ))
            };

            let (first_xobj, fw, fh) = make_xobject(&inputs[0])?;
            let (doc, page, layer) = PdfDocument::new(
                "Lumen Converter",
                Mm(fw as f32 / dpi * 25.4),
                Mm(fh as f32 / dpi * 25.4),
                "Imagem 1",
            );
            let layer_ref = doc.get_page(page).get_layer(layer);
            Image::from(first_xobj).add_to_layer(
                layer_ref,
                ImageTransform {
                    dpi: Some(dpi),
                    ..Default::default()
                },
            );

            for (i, path) in inputs.iter().enumerate().skip(1) {
                let (xobj, w, h) = make_xobject(path)?;
                let (page, layer) = doc.add_page(
                    Mm(w as f32 / dpi * 25.4),
                    Mm(h as f32 / dpi * 25.4),
                    format!("Imagem {}", i + 1),
                );
                let layer_ref = doc.get_page(page).get_layer(layer);
                Image::from(xobj).add_to_layer(
                    layer_ref,
                    ImageTransform {
                        dpi: Some(dpi),
                        ..Default::default()
                    },
                );
            }

            let file = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
            let mut writer = std::io::BufWriter::new(file);
            doc.save(&mut writer).map_err(|e| e.to_string())?;
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;

        Ok(out_ret)
    }

    async fn pdf_to_text(
        &self,
        input: &Path,
        out: &Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let input = input.to_path_buf();
        let out_path = out.to_path_buf();
        let out_ret = out_path.clone();

        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let doc = printpdf::lopdf::Document::load(&input).map_err(|e| e.to_string())?;
            let pages = doc.get_pages();
            let page_numbers: Vec<u32> = pages.keys().copied().collect();
            let text = doc
                .extract_text(&page_numbers)
                .map_err(|e| e.to_string())?;
            if text.trim().is_empty() {
                return Err(
                    "Nenhum texto encontrado (o PDF pode ser apenas imagens escaneadas)."
                        .to_string(),
                );
            }
            std::fs::write(&out_path, text).map_err(|e| e.to_string())?;
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;

        Ok(out_ret)
    }

    async fn transcode_media(
        &self,
        input: &Path,
        output: &Path,
        preset: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);

        let is_gif = output
            .extension()
            .map(|e| e.eq_ignore_ascii_case("gif"))
            .unwrap_or(false);
        if is_gif {
            cmd.arg("-y")
                .arg("-i")
                .arg(input)
                .arg("-vf")
                .arg("fps=12,scale=480:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse")
                .arg(output);
            #[cfg(windows)]
            cmd.creation_flags(0x08000000);
            let result = cmd.output().await?;
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
                return Err(format!("ffmpeg falhou ao gerar GIF: {}", last).into());
            }
            return Ok(());
        }

        cmd.arg("-y").arg("-i").arg(input).arg("-map_metadata").arg("0");

        match preset {
            "compress" => {
                cmd.arg("-c:v").arg("libx264").arg("-crf").arg("28")
                    .arg("-preset").arg("medium").arg("-c:a").arg("aac").arg("-b:a").arg("128k");
            }
            "1080" => {
                cmd.arg("-vf").arg("scale=-2:1080").arg("-c:v").arg("libx264")
                    .arg("-crf").arg("22").arg("-c:a").arg("aac").arg("-b:a").arg("160k");
            }
            "720" => {
                cmd.arg("-vf").arg("scale=-2:720").arg("-c:v").arg("libx264")
                    .arg("-crf").arg("23").arg("-c:a").arg("aac").arg("-b:a").arg("160k");
            }
            "480" => {
                cmd.arg("-vf").arg("scale=-2:480").arg("-c:v").arg("libx264")
                    .arg("-crf").arg("24").arg("-c:a").arg("aac").arg("-b:a").arg("128k");
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
                cmd.arg("-c:a").arg("aac").arg("-b:a").arg(g(4).unwrap_or("160k"));
            }
            _ => {}
        }
        cmd.arg(output);

        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let result = cmd.output().await?;
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            return Err(format!("ffmpeg falhou ao converter: {}", last).into());
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct DownloadOptions {
    pub is_audio: bool,
    pub format: String,
    pub quality: String,
    pub max_height: Option<u32>,
    pub subtitle_langs: Option<String>,
    pub clip: Option<(String, String)>,
    pub rate_limit: Option<String>,
    pub concurrent_fragments: u32,
    pub live_from_start: bool,
    /// Gravação de live: usa contêiner MPEG-TS (robusto a interrupção) para o
    /// arquivo sobreviver a uma parada forçada e poder ser remuxado.
    pub is_live: bool,
    /// Sinaliza uma parada graciosa de gravação de live (finaliza o que já baixou).
    pub stop: Option<Arc<AtomicBool>>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        DownloadOptions {
            is_audio: false,
            format: "mp4".to_string(),
            quality: "best".to_string(),
            max_height: None,
            subtitle_langs: None,
            clip: None,
            rate_limit: None,
            concurrent_fragments: 4,
            live_from_start: false,
            is_live: false,
            stop: None,
        }
    }
}

pub fn is_valid_url(url: &str) -> bool {
    let u = url.trim();
    u.starts_with("http://") || u.starts_with("https://")
}

pub fn looks_like_url(url: &str) -> bool {
    let u = url.trim();
    !u.is_empty() && !u.contains(char::is_whitespace) && u.contains('.')
}

pub fn friendly_error(stderr: &str) -> String {
    let low = stderr.to_lowercase();
    let known = if low.contains("private video") || low.contains("sign in to confirm") {
        Some("Vídeo privado ou que exige login.")
    } else if low.contains("confirm your age") || low.contains("age-restricted") || low.contains("age restricted") {
        Some("Conteúdo com restrição de idade (requer login/cookies).")
    } else if low.contains("video unavailable") || low.contains("this video is not available") {
        Some("Vídeo indisponível.")
    } else if low.contains("requested format is not available") {
        Some("Formato/resolução indisponível para este vídeo.")
    } else if low.contains("unsupported url") || low.contains("is not a valid url") {
        Some("Link não suportado.")
    } else if low.contains("http error 403") || low.contains("403 forbidden") {
        Some("Acesso negado (403). Tente atualizar o yt-dlp em Configurações.")
    } else if low.contains("http error 404") {
        Some("Conteúdo não encontrado (404).")
    } else if low.contains("getaddrinfo")
        || low.contains("failed to resolve")
        || low.contains("unable to download webpage")
        || low.contains("temporary failure in name resolution")
        || low.contains("connection")
    {
        Some("Falha de conexão. Verifique sua internet.")
    } else {
        None
    };

    match known {
        Some(msg) => msg.to_string(),
        None => {
            let last = stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("erro desconhecido");
            format!("Falha no download: {}", last.trim())
        }
    }
}

pub fn cleanup_partials(folder: &Path, stem: &str) {
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let is_temp_ext = matches!(ext.as_str(), "part" | "ytdl" | "temp" | "rawaudio");
            if name.starts_with(stem) && is_temp_ext {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

pub fn cleanup_temp_dir(folder: &Path) {
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if matches!(ext.as_str(), "part" | "ytdl" | "rawaudio")
                || name.starts_with("temp_audio_")
                || name.starts_with("temp_video_")
            {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

#[derive(Default)]
pub struct AudioMeta {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

#[derive(Clone)]
pub struct ThumbImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Default)]
pub struct VideoPreview {
    pub title: String,
    pub channel: String,
    pub duration: String,
    pub resolutions: Vec<u32>,
    pub est_size_video: Option<i64>,
    pub est_size_audio: Option<i64>,
    pub thumbnail: Option<ThumbImage>,
    pub is_live: bool,
}

pub async fn download_thumbnail(url: &str) -> Option<ThumbImage> {
    let bytes = reqwest::get(url).await.ok()?.bytes().await.ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let img = if img.width() > 360 {
        img.resize(
            360,
            (img.height() * 360 / img.width().max(1)).max(1),
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(ThumbImage {
        width: w as usize,
        height: h as usize,
        rgba: rgba.into_raw(),
    })
}

pub fn format_duration(secs: i64) -> String {
    let secs = secs.max(0);
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{}:{:02}", m, s)
    }
}

pub fn format_size(bytes: i64) -> String {
    let b = bytes as f64;
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn parse_ytdlp_percent(line: &str) -> Option<f64> {
    let l = line.trim_start();
    if !l.starts_with("[download]") {
        return None;
    }
    let token = l.split_whitespace().find(|t| t.ends_with('%'))?;
    token
        .trim_end_matches('%')
        .parse::<f64>()
        .ok()
        .map(|v| (v / 100.0).clamp(0.0, 1.0))
}

fn parse_ytdlp_speed(line: &str) -> Option<f64> {
    let l = line.trim_start();
    if !l.starts_with("[download]") {
        return None;
    }
    let tok = l.split_whitespace().find(|t| t.ends_with("/s"))?;
    let body = tok.trim_end_matches("/s");
    let split = body.find(|c: char| c.is_alphabetic()).unwrap_or(body.len());
    let (num, unit) = body.split_at(split);
    let value: f64 = num.parse().ok()?;
    let mult = match unit {
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "KiB" => 1024.0,
        "GB" => 1_000_000_000.0,
        "MB" => 1_000_000.0,
        "KB" | "kB" => 1000.0,
        "B" | "" => 1.0,
        _ => return None,
    };
    Some(value * mult)
}

fn parse_ytdlp_eta(line: &str) -> Option<u64> {
    let l = line.trim_start();
    if !l.starts_with("[download]") {
        return None;
    }
    let mut it = l.split_whitespace();
    let tok = loop {
        match it.next() {
            Some("ETA") => break it.next()?,
            Some(_) => continue,
            None => return None,
        }
    };
    let mut secs = 0u64;
    for p in tok.split(':') {
        secs = secs * 60 + p.parse::<u64>().ok()?;
    }
    Some(secs)
}

/// Extrai o total baixado de uma linha do yt-dlp (ex.: "[download]  30.56MiB at ...").
fn parse_ytdlp_size(line: &str) -> Option<u64> {
    let l = line.trim_start();
    if !l.starts_with("[download]") {
        return None;
    }
    for tok in l.split_whitespace() {
        let unit_mul = if let Some(n) = tok.strip_suffix("GiB") {
            Some((n, 1024.0 * 1024.0 * 1024.0))
        } else if let Some(n) = tok.strip_suffix("MiB") {
            Some((n, 1024.0 * 1024.0))
        } else if let Some(n) = tok.strip_suffix("KiB") {
            Some((n, 1024.0))
        } else {
            None
        };
        if let Some((num, mul)) = unit_mul {
            if let Ok(v) = num.parse::<f64>() {
                return Some((v * mul) as u64);
            }
        }
    }
    None
}

/// Aguarda até a flag de parada ser acionada; se não houver flag, nunca resolve.
async fn wait_for_stop(stop: &Option<Arc<AtomicBool>>) {
    match stop {
        Some(s) => loop {
            if s.load(Ordering::Relaxed) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        },
        None => std::future::pending::<()>().await,
    }
}

fn is_youtube(url: &str) -> bool {
    let u = url.to_lowercase();
    u.contains("youtube.com") || u.contains("youtu.be")
}

fn ytdlp_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let last = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("erro desconhecido");
    format!("yt-dlp: {}", last)
}

fn find_output(folder: &Path, stem: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(folder).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let matches_stem = path
            .file_stem()
            .map(|s| s.to_string_lossy() == stem)
            .unwrap_or(false);
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if matches_stem && !matches!(ext.as_str(), "srt" | "vtt" | "part" | "ytdl") {
            return Some(path);
        }
    }
    None
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

#[derive(Clone)]
pub struct FormatRow {
    pub id: String,
    pub ext: String,
    pub resolution: String,
    pub fps: String,
    pub codec: String,
    pub kind: String,
    pub bitrate: String,
    pub size: Option<i64>,
}

pub fn organize_subfolder(organize_by: &str, media_type: &str, channel: &str) -> Option<String> {
    match organize_by {
        "type" => Some(
            match media_type {
                "music" => "Música",
                "video" => "Vídeo",
                "convert" => "Convertidos",
                _ => "Outros",
            }
            .to_string(),
        ),
        "date" => Some(chrono::Local::now().format("%Y-%m-%d").to_string()),
        "channel" => {
            let c = sanitize_filename(channel);
            if c.trim().is_empty() || c == "download" {
                None
            } else {
                Some(c)
            }
        }
        _ => None,
    }
}

pub fn is_audio_format(format: &str) -> bool {
    matches!(
        format,
        "mp3" | "m4a" | "aac" | "opus" | "ogg" | "wav" | "flac"
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileCategory {
    Audio,
    Video,
    Image,
    Document,
    Office,
    Unknown,
}

pub fn categorize(path: &Path) -> FileCategory {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "mp3" | "m4a" | "aac" | "opus" | "ogg" | "oga" | "wav" | "flac" | "wma" | "aiff"
        | "alac" => FileCategory::Audio,
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "flv" | "wmv" | "m4v" | "mpeg" | "mpg"
        | "3gp" | "ts" | "m2ts" => FileCategory::Video,
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "tiff" | "tif" | "gif" | "heic" | "ico" => {
            FileCategory::Image
        }
        "pdf" => FileCategory::Document,
        "doc" | "docx" | "odt" | "rtf" | "txt" | "ppt" | "pptx" | "odp" | "xls" | "xlsx"
        | "ods" | "csv" | "epub" => FileCategory::Office,
        _ => FileCategory::Unknown,
    }
}

fn find_whisper_exe(dir: &Path) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["whisper-cli.exe", "main.exe"]
    } else {
        &["whisper-cli", "main"]
    };
    for name in names {
        if let Some(found) = find_named(dir, name, 3) {
            return Some(found);
        }
    }
    None
}

fn find_named(dir: &Path, target: &str, depth: u32) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if target.eq_ignore_ascii_case(name) {
                return Some(path);
            }
        }
    }
    if depth > 0 {
        for sub in subdirs {
            if let Some(found) = find_named(&sub, target, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

pub struct EngineStatus {
    pub msoffice: bool,
    pub msoffice_detail: String,
    pub libreoffice: bool,
}

static ENGINE_STATUS: OnceLock<EngineStatus> = OnceLock::new();

/// Detecta (uma vez por sessão) os motores externos disponíveis.
pub fn engine_status() -> &'static EngineStatus {
    ENGINE_STATUS.get_or_init(|| {
        let mut apps: Vec<&str> = Vec::new();
        if msoffice_exe("WINWORD.EXE").is_some() {
            apps.push("Word");
        }
        if msoffice_exe("EXCEL.EXE").is_some() {
            apps.push("Excel");
        }
        if msoffice_exe("POWERPNT.EXE").is_some() {
            apps.push("PowerPoint");
        }
        EngineStatus {
            msoffice: !apps.is_empty(),
            msoffice_detail: apps.join(", "),
            libreoffice: libreoffice_path().is_some(),
        }
    })
}

/// No modo Automático, escolhe o motor de maior fidelidade disponível.
fn auto_pick_engine() -> ConvertEngine {
    let st = engine_status();
    if st.msoffice {
        ConvertEngine::MsOffice
    } else if st.libreoffice {
        ConvertEngine::LibreOffice
    } else {
        ConvertEngine::Rust
    }
}

pub fn libreoffice_path() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        "/usr/bin/soffice",
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    ];
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

fn msoffice_exe(name: &str) -> Option<PathBuf> {
    let roots = [
        r"C:\Program Files\Microsoft Office",
        r"C:\Program Files (x86)\Microsoft Office",
    ];
    for r in roots {
        let root = Path::new(r);
        if root.exists() {
            if let Some(found) = find_named(root, name, 4) {
                return Some(found);
            }
        }
    }
    None
}

#[cfg(windows)]
fn msoffice_to_pdf(input: &Path, out: &Path) -> Result<(), String> {
    let ext = input
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let inp = input.to_string_lossy().replace('\'', "''");
    let outp = out.to_string_lossy().replace('\'', "''");

    let script = match ext.as_str() {
        "doc" | "docx" | "odt" | "rtf" | "txt" => format!(
            "$w = New-Object -ComObject Word.Application; $w.Visible = $false; \
             try {{ $d = $w.Documents.Open('{inp}'); $d.SaveAs([ref]'{outp}', [ref]17); $d.Close($false) }} \
             finally {{ $w.Quit() }}",
        ),
        "xls" | "xlsx" | "ods" | "csv" => format!(
            "$x = New-Object -ComObject Excel.Application; $x.Visible = $false; $x.DisplayAlerts = $false; \
             try {{ $wb = $x.Workbooks.Open('{inp}'); $wb.ExportAsFixedFormat(0, '{outp}'); $wb.Close($false) }} \
             finally {{ $x.Quit() }}",
        ),
        "ppt" | "pptx" | "odp" => format!(
            "$p = New-Object -ComObject PowerPoint.Application; \
             try {{ $pr = $p.Presentations.Open('{inp}', $true, $false, $false); $pr.SaveAs('{outp}', 32); $pr.Close() }} \
             finally {{ $p.Quit() }}",
        ),
        other => {
            return Err(format!(
                "MS Office não suporta \"{}\" para PDF aqui.",
                other
            ))
        }
    };

    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| e.to_string())?;

    if out.exists() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "MS Office não gerou o PDF. {}",
        stderr.trim()
    ))
}

fn office_extract_text(path: &Path) -> Result<String, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "txt" | "md" | "markdown" | "log" => {
            std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|e| e.to_string())
        }
        "csv" => std::fs::read(path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| e.to_string()),
        "html" | "htm" => {
            let raw = std::fs::read(path).map_err(|e| e.to_string())?;
            Ok(strip_markup(&String::from_utf8_lossy(&raw)))
        }
        "rtf" => {
            let raw = std::fs::read(path).map_err(|e| e.to_string())?;
            Ok(rtf_to_text(&String::from_utf8_lossy(&raw)))
        }
        "docx" => office_xml_to_text(&read_zip_entry(path, "word/document.xml")?),
        "odt" | "odp" => office_xml_to_text(&read_zip_entry(path, "content.xml")?),
        "pptx" => {
            let slides = read_zip_entries(path, "ppt/slides/slide", ".xml")?;
            office_xml_to_text(&slides)
        }
        "epub" => {
            let pages = read_zip_entries_by_suffix(path, &[".xhtml", ".html", ".htm"])?;
            Ok(strip_markup(&pages))
        }
        "xlsx" | "xls" | "ods" => spreadsheet_to_text(path),
        other => Err(format!(
            "Formato \"{}\" não é suportado na conversão nativa.",
            other
        )),
    }
}

fn read_zip_entry(path: &Path, name: &str) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut entry = archive
        .by_name(name)
        .map_err(|_| format!("entrada \"{}\" não encontrada no arquivo", name))?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

fn read_zip_entries(path: &Path, prefix: &str, suffix: &str) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|n| n.starts_with(prefix) && n.ends_with(suffix))
        .collect();
    names.sort();
    let mut out = String::new();
    for name in names {
        if let Ok(mut entry) = archive.by_name(&name) {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                out.push_str(&buf);
                out.push('\n');
            }
        }
    }
    Ok(out)
}

fn read_zip_entries_by_suffix(path: &Path, suffixes: &[&str]) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|n| suffixes.iter().any(|s| n.to_lowercase().ends_with(s)))
        .collect();
    names.sort();
    let mut out = String::new();
    for name in names {
        if let Ok(mut entry) = archive.by_name(&name) {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                out.push_str(&buf);
                out.push('\n');
            }
        }
    }
    Ok(out)
}

fn spreadsheet_to_text(path: &Path) -> Result<String, String> {
    use calamine::{open_workbook_auto, Data, Reader};
    let mut wb = open_workbook_auto(path).map_err(|e| e.to_string())?;
    let names: Vec<String> = wb.sheet_names().to_vec();
    let mut out = String::new();
    for name in names {
        if let Ok(range) = wb.worksheet_range(&name) {
            if range.is_empty() {
                continue;
            }
            out.push_str(&format!("# {}\n", name));
            for row in range.rows() {
                let cells: Vec<String> = row
                    .iter()
                    .map(|c| match c {
                        Data::Empty => String::new(),
                        Data::String(s) => s.clone(),
                        Data::Float(f) => format!("{}", f),
                        Data::Int(i) => format!("{}", i),
                        Data::Bool(b) => format!("{}", b),
                        Data::DateTime(d) => format!("{}", d),
                        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
                        Data::Error(e) => format!("{:?}", e),
                    })
                    .collect();
                out.push_str(&cells.join(" | "));
                out.push('\n');
            }
            out.push('\n');
        }
    }
    Ok(out)
}

fn office_xml_to_text(xml: &str) -> Result<String, String> {
    let mut s = xml.to_string();
    for para in ["</w:p>", "</text:p>", "</text:h>", "</a:p>", "</p>"] {
        s = s.replace(para, "\n");
    }
    for tab in ["<w:tab/>", "<w:tab />", "<text:tab/>", "<text:tab/>"] {
        s = s.replace(tab, "\t");
    }
    for br in ["<w:br/>", "<w:br />", "<text:line-break/>", "<br/>", "<br />", "<br>"] {
        s = s.replace(br, "\n");
    }
    Ok(unescape_entities(&strip_tags(&s)))
}

fn strip_markup(html: &str) -> String {
    let mut s = html.to_string();
    for br in [
        "</p>", "</div>", "</li>", "</tr>", "</h1>", "</h2>", "</h3>", "</h4>", "<br/>", "<br />",
        "<br>",
    ] {
        s = s.replace(br, "\n");
    }
    let no_tags = strip_tags(&s);
    let text = unescape_entities(&no_tags);
    let mut blank = 0;
    let mut out = String::new();
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(trimmed);
        out.push('\n');
    }
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn unescape_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        let mut ent = String::new();
        while let Some(&nc) = chars.peek() {
            if nc == ';' {
                chars.next();
                break;
            }
            if ent.len() > 10 {
                break;
            }
            ent.push(nc);
            chars.next();
        }
        match ent.as_str() {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            "nbsp" => out.push(' '),
            _ => {
                if let Some(rest) = ent.strip_prefix('#') {
                    let code = if let Some(hex) = rest.strip_prefix('x').or_else(|| rest.strip_prefix('X')) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        rest.parse::<u32>().ok()
                    };
                    if let Some(ch) = code.and_then(char::from_u32) {
                        out.push(ch);
                    }
                } else {
                    out.push('&');
                    out.push_str(&ent);
                    out.push(';');
                }
            }
        }
    }
    out
}

fn rtf_to_text(rtf: &str) -> String {
    let mut out = String::new();
    let mut chars = rtf.chars().peekable();
    let mut depth = 0i32;
    while let Some(c) = chars.next() {
        match c {
            '{' => depth += 1,
            '}' => depth = (depth - 1).max(0),
            '\\' => {
                let mut word = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc.is_ascii_alphabetic() {
                        word.push(nc);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let mut num = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc.is_ascii_digit() || (num.is_empty() && nc == '-') {
                        num.push(nc);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if let Some(&' ') = chars.peek() {
                    chars.next();
                }
                match word.as_str() {
                    "par" | "line" => out.push('\n'),
                    "tab" => out.push('\t'),
                    _ => {}
                }
            }
            _ if depth >= 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn render_text_pdf(text: &str, out: &Path, title: &str) -> Result<(), String> {
    const PAGE_W: f32 = 210.0;
    const PAGE_H: f32 = 297.0;
    const MARGIN: f32 = 20.0;
    const FONT_SIZE: f32 = 11.0;
    const LINE_H: f32 = 5.0;
    const MAX_CHARS: usize = 92;

    let (doc, page1, layer1) =
        PdfDocument::new(title, Mm(PAGE_W), Mm(PAGE_H), "Texto");
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| e.to_string())?;
    let mut layer = doc.get_page(page1).get_layer(layer1);
    let mut y = PAGE_H - MARGIN;

    let emit = |layer: &mut printpdf::PdfLayerReference,
                y: &mut f32,
                doc: &printpdf::PdfDocumentReference,
                line: &str| {
        if *y < MARGIN {
            let (p, l) = doc.add_page(Mm(PAGE_W), Mm(PAGE_H), "Texto");
            *layer = doc.get_page(p).get_layer(l);
            *y = PAGE_H - MARGIN;
        }
        layer.use_text(line, FONT_SIZE, Mm(MARGIN), Mm(*y), &font);
        *y -= LINE_H;
    };

    for raw_line in text.replace('\t', "    ").lines() {
        let line = raw_line.trim_end();
        if line.is_empty() {
            y -= LINE_H;
            if y < MARGIN {
                let (p, l) = doc.add_page(Mm(PAGE_W), Mm(PAGE_H), "Texto");
                layer = doc.get_page(p).get_layer(l);
                y = PAGE_H - MARGIN;
            }
            continue;
        }
        for wrapped in wrap_line(line, MAX_CHARS) {
            emit(&mut layer, &mut y, &doc, &wrapped);
        }
    }

    let file = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut writer = std::io::BufWriter::new(file);
    doc.save(&mut writer).map_err(|e| e.to_string())?;
    Ok(())
}

fn wrap_line(line: &str, max: usize) -> Vec<String> {
    if line.chars().count() <= max {
        return vec![line.to_string()];
    }
    let mut out = Vec::new();
    let mut current = String::new();
    for word in line.split(' ') {
        if word.chars().count() > max {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            let mut chunk = String::new();
            for c in word.chars() {
                chunk.push(c);
                if chunk.chars().count() >= max {
                    out.push(std::mem::take(&mut chunk));
                }
            }
            if !chunk.is_empty() {
                current = chunk;
            }
            continue;
        }
        let extra = if current.is_empty() { 0 } else { 1 };
        if current.chars().count() + extra + word.chars().count() > max {
            out.push(std::mem::take(&mut current));
            current.push_str(word);
        } else {
            if extra == 1 {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

pub fn output_formats(category: FileCategory) -> Vec<&'static str> {
    match category {
        FileCategory::Audio => vec!["mp3", "m4a", "aac", "opus", "ogg", "wav", "flac"],
        FileCategory::Video => vec![
            "mp4", "mkv", "webm", "avi", "mov", "gif", "mp3", "m4a", "wav",
        ],
        FileCategory::Image => vec!["jpg", "png", "webp", "bmp", "tiff", "gif", "pdf"],
        FileCategory::Document => vec!["png", "jpg", "txt"],
        FileCategory::Office => vec!["pdf", "txt"],
        FileCategory::Unknown => vec![],
    }
}
