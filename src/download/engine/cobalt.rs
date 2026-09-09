use super::models::AudioMeta;
use super::routing::{DownloadFailure, FailureKind};
use super::{DownloadEngine, DownloadOptions, EnginePreference, Progress, Stage};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::io::AsyncWriteExt;
type Result<T> = std::result::Result<T, DownloadFailure>;
fn failure(kind: FailureKind, message: impl Into<String>) -> DownloadFailure {
    DownloadFailure::new(kind, message)
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Request<'a> {
    url: &'a str,
    download_mode: &'static str,
    audio_format: &'static str,
    video_quality: String,
    youtube_video_codec: &'static str,
    local_processing: &'static str,
    filename_style: &'static str,
}
impl<'a> Request<'a> {
    fn new(url: &'a str, o: &DownloadOptions) -> Self {
        let quality = if let Some(height) = o.max_height {
            [144, 240, 360, 480, 720, 1080, 1440, 2160, 4320]
                .into_iter()
                .filter(|h| *h <= height)
                .last()
                .unwrap_or(144)
                .to_string()
        } else {
            match o.quality.as_str() {
                "medium" => "720",
                "high" => "1080",
                _ => "max",
            }
            .into()
        };
        Self {
            url,
            download_mode: if o.is_audio { "audio" } else { "auto" },
            audio_format: "best",
            video_quality: quality,
            youtube_video_codec: match o.format.as_str() {
                "mkv" => "av1",
                "webm" => "vp9",
                _ => "h264",
            },
            local_processing: "disabled",
            filename_style: "pretty",
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(tag = "status")]
enum Response {
    #[serde(rename = "tunnel")]
    Tunnel { url: String, filename: String },
    #[serde(rename = "redirect")]
    Redirect { url: String, filename: String },
    #[serde(rename = "error")]
    Error { error: ApiError },
    #[serde(rename = "picker")]
    Picker,
    #[serde(rename = "local-processing")]
    LocalProcessing,
    #[serde(other)]
    Unknown,
}
#[derive(Debug, Deserialize)]
struct ApiError {
    code: String,
}
impl Response {
    fn media(self) -> Result<(String, String)> {
        match self {
            Self::Tunnel { url, filename } | Self::Redirect { url, filename } => {
                valid_media_url(&url)?;
                Ok((url, filename))
            }
            Self::Error { error } => {
                let code = error.code;
                let logical = code.strip_prefix("error.api.").unwrap_or(&code);
                let kind = if logical.starts_with("content.") {
                    FailureKind::Restricted
                } else if logical.starts_with("api.auth") || logical.starts_with("auth.") {
                    FailureKind::Dependency
                } else if logical.contains("rate")
                    || matches!(logical, "fetch.fail" | "fetch.critical")
                {
                    FailureKind::Transport
                } else if code.contains("unsupported") {
                    FailureKind::Unsupported
                } else {
                    FailureKind::Extraction
                };
                // Codes only: never propagate arbitrary context containing signed URLs.
                let safe: String = code
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
                    .take(100)
                    .collect();
                Err(failure(kind, format!("Cobalt: {safe}")))
            }
            Self::Picker => Err(failure(
                FailureKind::Unsupported,
                "Multi-item posts require item selection, unavailable in Cobalt v1. / Seleção de itens indisponível no Cobalt v1.",
            )),
            Self::LocalProcessing => Err(failure(
                FailureKind::Unsupported,
                "Cobalt returned an unsupported local-processing response.",
            )),
            Self::Unknown => Err(failure(
                FailureKind::Unsupported,
                "Unsupported Cobalt API response; repair the companion.",
            )),
        }
    }
}
fn valid_media_url(s: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(s)
        .map_err(|_| failure(FailureKind::Unsupported, "Invalid media URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(failure(FailureKind::Unsupported, "Unsafe media URL"));
    }
    Ok(url)
}
fn safe_filename(name: &str) -> String {
    // Treat both separator conventions identically even in Linux fixture tests.
    let base = name.rsplit(['/', '\\']).next().unwrap_or("download");
    let safe = super::sanitize_filename(base)
        .trim_matches(['.', ' '])
        .to_string();
    let stem = safe.split('.').next().unwrap_or("").to_ascii_uppercase();
    if safe.is_empty()
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.len() == 4
    {
        format!("download-{safe}")
    } else {
        safe.chars().take(180).collect()
    }
}
fn transport(_: reqwest::Error) -> DownloadFailure {
    failure(
        FailureKind::Transport,
        "Cobalt network request failed or timed out / Falha de rede ou tempo esgotado no Cobalt",
    )
}
async fn resolve(
    endpoint: &super::companion::Endpoint,
    url: &str,
    opts: &DownloadOptions,
) -> Result<(String, String)> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(transport)?;
    let response = client
        .post(&endpoint.url)
        .header("Accept", "application/json")
        .header("Authorization", format!("Api-Key {}", endpoint.key))
        .json(&Request::new(url, opts))
        .send()
        .await
        .map_err(transport)?;
    let status = response.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return Err(failure(
            FailureKind::Transport,
            format!("Cobalt HTTP {}", status.as_u16()),
        ));
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(failure(
            FailureKind::Dependency,
            "Cobalt authentication failed; repair companion",
        ));
    }
    let mut response = response;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        if body.len() + chunk.len() > 1024 * 1024 {
            return Err(failure(
                FailureKind::Extraction,
                "Cobalt response too large",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    let parsed: Response = serde_json::from_slice(&body)
        .map_err(|_| failure(FailureKind::Extraction, "Malformed Cobalt JSON response"))?;
    if !status.is_success() && !matches!(parsed, Response::Error { .. }) {
        return Err(failure(
            FailureKind::Extraction,
            "Cobalt API request failed",
        ));
    }
    parsed.media()
}
async fn transfer<F: Fn(Progress)>(url: &str, path: &Path, progress: &F) -> Result<()> {
    valid_media_url(url)?;
    // Separate client: it never knows the API key, including same-origin redirects.
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5
                || !matches!(attempt.url().scheme(), "http" | "https")
                || !attempt.url().username().is_empty()
                || attempt.url().password().is_some()
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(transport)?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(transport)?
        .error_for_status()
        .map_err(transport)?;
    if !response.status().is_success() {
        return Err(failure(
            FailureKind::Transport,
            "Cobalt media redirect rejected",
        ));
    }
    let length = response.content_length();
    let start = Instant::now();
    let mut bytes = 0u64;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await
        .map_err(DownloadFailure::io)?;
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        file.write_all(&chunk).await.map_err(DownloadFailure::io)?;
        bytes += chunk.len() as u64;
        let speed = bytes as f64 / start.elapsed().as_secs_f64().max(0.01);
        progress(Progress {
            engine: Some(EnginePreference::Cobalt),
            indeterminate: length.is_none(),
            downloaded_bytes: bytes,
            speed_bps: speed,
            fraction: length
                .filter(|l| *l > 0)
                .map(|l| bytes as f64 / l as f64)
                .unwrap_or(0.0),
            eta_secs: length
                .map(|l| (l.saturating_sub(bytes) as f64 / speed.max(1.0)) as u64)
                .unwrap_or(0),
            ..Default::default()
        });
    }
    file.sync_all().await.map_err(DownloadFailure::io)?;
    if bytes == 0 || length.is_some_and(|l| l != bytes) {
        return Err(failure(
            FailureKind::Transport,
            "Cobalt returned an empty or incomplete file",
        ));
    }
    Ok(())
}
/// Windows rename without REPLACE_EXISTING also works on FAT/exFAT destinations.
pub(super) fn publish_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let from: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 0)
        } == 0
        {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::hard_link(source, destination)
    }
}

pub(super) fn publish(staged: &Path, requested: &Path) -> Result<PathBuf> {
    let parent = requested.parent().unwrap_or(Path::new("."));
    let stem = requested.file_stem().unwrap_or_default().to_string_lossy();
    let ext = requested.extension().unwrap_or_default().to_string_lossy();
    for n in 0..10000 {
        let dest = if n == 0 {
            requested.to_path_buf()
        } else {
            parent.join(format!("{stem} ({n}).{ext}"))
        };
        match publish_file(staged, &dest) {
            Ok(()) => return Ok(dest),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(DownloadFailure::io(e)),
        }
    }
    Err(failure(
        FailureKind::Filesystem,
        "Cannot choose an unused output filename",
    ))
}
impl DownloadEngine {
    pub(super) async fn cobalt_download<F: Fn(Progress) + Send + Sync>(
        &self,
        url: &str,
        output: &str,
        opts: &DownloadOptions,
        progress: &F,
    ) -> Result<PathBuf> {
        progress(Progress {
            engine: Some(EnginePreference::Cobalt),
            indeterminate: true,
            ..Default::default()
        });
        let endpoint = self.companion.lock().await.endpoint(&self.libs_dir).await?;
        let (media, filename) = resolve(&endpoint, url, opts).await?;
        let requested = Path::new(output);
        let folder = requested.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(folder).map_err(DownloadFailure::io)?;
        let staging = tempfile::Builder::new()
            .prefix(".lumen-cobalt-")
            .tempdir_in(folder)
            .map_err(DownloadFailure::io)?;
        let source = staging.path().join(safe_filename(&filename));
        transfer(&media, &source, progress).await?;
        self.ensure_ffmpeg().await.map_err(|_| {
            failure(
                FailureKind::Dependency,
                "Cannot install FFmpeg for Cobalt finalization",
            )
        })?;
        let finalized = staging.path().join(format!("final.{}", opts.format));
        // Avoid a server filename colliding with our private finalization filename.
        let source = if source == finalized {
            let moved = staging.path().join("source-media");
            std::fs::rename(&source, &moved).map_err(DownloadFailure::io)?;
            moved
        } else {
            source
        };
        let on_final = |p: Progress| {
            progress(Progress {
                engine: Some(EnginePreference::Cobalt),
                stage: if opts.is_audio {
                    Stage::Transcoding
                } else {
                    p.stage
                },
                ..p
            })
        };
        if opts.is_audio {self.transcode_audio(&source,&finalized,&opts.format,&AudioMeta::default(),Some(&on_final)).await}
        else {self.transcode_video_profile(&source,&finalized,&opts.format,Some(&on_final)).await}
            .map_err(|_|failure(FailureKind::Finalization,"FFmpeg failed to finalize Cobalt media / FFmpeg falhou ao finalizar mídia do Cobalt"))?;
        if std::fs::metadata(&finalized)
            .map_err(DownloadFailure::io)?
            .len()
            == 0
        {
            return Err(failure(FailureKind::Finalization, "Empty finalized file"));
        }
        let name = opts
            .custom_filename
            .as_deref()
            .map(safe_filename)
            .unwrap_or_else(|| safe_filename(&filename));
        let mut destination = folder.join(name);
        destination.set_extension(&opts.format);
        publish(&finalized, &destination)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapping_and_protocol() {
        let mut o = DownloadOptions::default();
        assert_eq!(Request::new("x", &o).video_quality, "max");
        o.max_height = Some(1000);
        assert_eq!(Request::new("x", &o).video_quality, "720");
        for json in [
            r#"{"status":"picker"}"#,
            r#"{"status":"local-processing"}"#,
            r#"{"status":"future"}"#,
            r#"{"status":"error","error":{"code":"content.video.private"}}"#,
        ] {
            assert!(
                serde_json::from_str::<Response>(json)
                    .unwrap()
                    .media()
                    .is_err()
            );
        }
        assert!(serde_json::from_str::<Response>("invalid").is_err());
        assert!(valid_media_url("file:///secret").is_err());
        assert_eq!(safe_filename("../../outside.mp4"), "outside.mp4");
        assert_eq!(safe_filename("C:\\foo\\CON.mp4"), "download-CON.mp4");
    }
    #[test]
    fn publish_preserves_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("video.mp4");
        std::fs::write(&source, b"new").unwrap();
        std::fs::write(&target, b"old").unwrap();
        let result = publish(&source, &target).unwrap();
        assert_ne!(result, target);
        assert_eq!(std::fs::read(target).unwrap(), b"old");
        assert_eq!(std::fs::read(result).unwrap(), b"new");
    }
    #[tokio::test]
    async fn stream_unknown_length_has_no_credentials() {
        use tokio::io::AsyncReadExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut b = [0u8; 4096];
            let n = socket.read(&mut b).await.unwrap();
            assert!(
                !String::from_utf8_lossy(&b[..n])
                    .to_lowercase()
                    .contains("authorization")
            );
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nfixture")
                .await
                .unwrap();
        });
        let temp = tempfile::tempdir().unwrap();
        let unknown = std::sync::atomic::AtomicBool::new(false);
        transfer(
            &format!("http://{address}/"),
            &temp.path().join("out"),
            &|p| {
                unknown.store(p.indeterminate, std::sync::atomic::Ordering::Relaxed);
            },
        )
        .await
        .unwrap();
        task.await.unwrap();
        assert!(unknown.load(std::sync::atomic::Ordering::Relaxed));
    }
}

#[cfg(test)]
mod acceptance {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncReadExt;

    // Required by Windows CI/release after packaging; manually runnable with LUMEN_TEST_FFMPEG.
    #[tokio::test]
    #[ignore]
    async fn cobalt_gate_independence_interactive_and_queue() {
        let ffmpeg = PathBuf::from(
            std::env::var_os("LUMEN_TEST_FFMPEG").expect("packaging gate must supply FFmpeg"),
        );
        let temp = tempfile::tempdir().unwrap();
        let fixture = temp.path().join("fixture.mp4");
        let output = tokio::process::Command::new(&ffmpeg)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x64:r=10:d=0.2",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.2",
                "-c:v",
                "libx264",
                "-c:a",
                "aac",
                "-shortest",
            ])
            .arg(&fixture)
            .output()
            .await
            .unwrap();
        assert!(output.status.success());
        let bytes = Arc::new(std::fs::read(&fixture).unwrap());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let server_url = url.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let bytes = bytes.clone();
                let url = server_url.clone();
                tokio::spawn(async move {
                    let mut data = Vec::new();
                    let mut buf = [0; 4096];
                    loop {
                        let n = socket.read(&mut buf).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        data.extend_from_slice(&buf[..n]);
                        if data.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let header = String::from_utf8_lossy(&data).to_lowercase();
                    let header_end = data.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                    let length = header
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    while data.len() < header_end + length {
                        let n = socket.read(&mut buf).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        data.extend_from_slice(&buf[..n]);
                    }
                    let body = if header.starts_with("post ") {
                        assert!(header.contains("authorization: api-key fixture-key"));
                        serde_json::json!({"status":"redirect","url":format!("{url}media"),"filename":"fixture.mp4"}).to_string().into_bytes()
                    } else {
                        assert!(!header.contains("authorization:"));
                        bytes.to_vec()
                    };
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
                    socket.write_all(&body).await.unwrap();
                });
            }
        });
        let libs = temp.path().join("libs");
        std::fs::create_dir(&libs).unwrap();
        let broken = super::super::fs_utils::binary_path(&libs, "yt-dlp");
        std::fs::write(&broken, b"deliberately invalid executable").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&broken, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let engine = Arc::new(DownloadEngine {
            ffmpeg_path: ffmpeg,
            libs_dir: libs,
            dependency_lock: Default::default(),
            preview_cache: Mutex::new(Default::default()),
            net: Mutex::new(Default::default()),
            dl_pids: Mutex::new(Vec::new()),
            companion: tokio::sync::Mutex::new(super::super::companion::Companion::fixture(
                super::super::companion::Endpoint {
                    url,
                    key: "fixture-key".into(),
                },
            )),
        });
        let source = "https://streamable.com/lumen1";
        assert!(
            engine
                .preview_with_engine(source, EnginePreference::Auto)
                .await
                .unwrap()
                .title
                .is_empty()
        );
        for preference in [EnginePreference::Auto, EnginePreference::Cobalt] {
            let output = engine
                .fetch_and_download(
                    source,
                    &temp.path().join("requested.mp3").to_string_lossy(),
                    DownloadOptions {
                        engine: preference,
                        is_audio: true,
                        format: "mp3".into(),
                        ..Default::default()
                    },
                    |_| {},
                )
                .await
                .unwrap();
            assert!(output.is_file());
            assert!(std::fs::metadata(output).unwrap().len() > 0);
        }
        for format in [
            "mp3", "m4a", "aac", "opus", "ogg", "wav", "flac", "mp4", "mkv", "webm",
        ] {
            let audio = super::super::models::is_audio_format(format);
            let output = engine
                .fetch_and_download(
                    source,
                    &temp
                        .path()
                        .join(format!("profile.{format}"))
                        .to_string_lossy(),
                    DownloadOptions {
                        engine: EnginePreference::Cobalt,
                        is_audio: audio,
                        format: format.into(),
                        ..Default::default()
                    },
                    |_| {},
                )
                .await
                .unwrap();
            let probe = tokio::process::Command::new(&engine.ffmpeg_path)
                .args(["-hide_banner", "-i"])
                .arg(&output)
                .output()
                .await
                .unwrap();
            let codecs = super::super::media::parse_ffmpeg_stream_codecs(&String::from_utf8_lossy(
                &probe.stderr,
            ));
            let expected_audio = match format {
                "mp3" => "mp3",
                "m4a" | "aac" | "mp4" => "aac",
                "opus" | "webm" => "opus",
                "ogg" => "vorbis",
                "flac" | "mkv" => "flac",
                _ => "pcm_s16le",
            };
            assert_eq!(codecs.audio.as_deref(), Some(expected_audio), "{format}");
            if !audio {
                assert_eq!(
                    codecs.video.as_deref(),
                    Some(match format {
                        "mp4" => "h264",
                        "mkv" => "av1",
                        _ => "vp9",
                    })
                );
            }
        }
        let mut queue = crate::queue::Queue::new();
        queue.add(
            source.into(),
            String::new(),
            crate::app::MediaType::Music,
            "mp3".into(),
            "best".into(),
            temp.path().to_path_buf(),
            EnginePreference::Auto,
        );
        let db = temp.path().join("history.db");
        queue.pump(
            engine,
            db.clone(),
            None,
            false,
            String::new(),
            None,
            4,
            "none".into(),
            None,
            true,
        );
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let status = queue.jobs.lock().unwrap()[0].status.clone();
                match status {
                    crate::queue::JobStatus::Completed(_) => break,
                    crate::queue::JobStatus::Failed(e) => panic!("{e}"),
                    _ => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(
            crate::db::database::Database::open(&db)
                .all_active_history()
                .len(),
            1
        );
        server.abort();
    }
}

#[cfg(test)]
mod http_failures {
    use super::*;
    use tokio::io::AsyncReadExt;
    async fn serve_once(reply: &'static [u8]) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 8192];
            let _ = socket.read(&mut buffer).await;
            socket.write_all(reply).await.unwrap();
        });
        (url, task)
    }
    #[tokio::test]
    async fn incomplete_empty_and_expired_streams_fail() {
        for reply in [
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".as_slice(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort".as_slice(),
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".as_slice(),
            b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n".as_slice(),
        ] {
            let (url, task) = serve_once(reply).await;
            let temp = tempfile::tempdir().unwrap();
            let error = transfer(&url, &temp.path().join("out"), &|_| {})
                .await
                .unwrap_err();
            assert_eq!(error.kind, FailureKind::Transport);
            task.await.unwrap();
        }
    }
    #[test]
    fn both_single_media_statuses_and_error_classes() {
        for status in ["redirect", "tunnel"] {
            let response:Response=serde_json::from_value(serde_json::json!({"status":status,"url":"https://example.com/media","filename":"x.mp4"})).unwrap();
            assert_eq!(response.media().unwrap().1, "x.mp4");
        }
        for (code, kind) in [
            ("content.video.private", FailureKind::Restricted),
            ("error.api.content.video.private", FailureKind::Restricted),
            ("error.api.fetch.fail", FailureKind::Transport),
            ("api.auth.key.missing", FailureKind::Dependency),
            ("fetch.rate", FailureKind::Transport),
        ] {
            let error = Response::Error {
                error: ApiError { code: code.into() },
            }
            .media()
            .unwrap_err();
            assert_eq!(error.kind, kind);
        }
    }
    #[tokio::test]
    async fn cancel_closes_stream_and_removes_staging() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let (ready, started) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n")
                .await
                .unwrap();
            let _ = ready.send(());
            let n = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut buffer))
                .await
                .unwrap();
            assert!(matches!(n, Ok(0) | Err(_)));
        });
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_path_buf();
        let task = tokio::spawn(async move {
            let result = transfer(&url, &temp.path().join("out"), &|_| {}).await;
            drop(temp);
            result
        });
        started.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        server.await.unwrap();
        assert!(!path.exists());
    }
}
