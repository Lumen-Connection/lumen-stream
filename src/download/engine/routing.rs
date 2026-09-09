use super::{DownloadEngine, DownloadOptions, Progress, VideoPreview};
use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnginePreference {
    #[default]
    Auto,
    YtDlp,
    Cobalt,
}
impl EnginePreference {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto (yt-dlp → Cobalt)",
            Self::YtDlp => "yt-dlp",
            Self::Cobalt => "Cobalt",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    Extraction,
    Transport,
    Unsupported,
    Restricted,
    Dependency,
    Filesystem,
    Cancelled,
    Finalization,
}
#[derive(Debug)]
pub struct DownloadFailure {
    pub kind: FailureKind,
    pub message: String,
}
impl DownloadFailure {
    pub fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    pub fn io(e: std::io::Error) -> Self {
        Self::new(FailureKind::Filesystem, e.to_string())
    }
    pub fn fallback_eligible(&self) -> bool {
        matches!(
            self.kind,
            FailureKind::Extraction | FailureKind::Transport | FailureKind::Dependency
        )
    }
    pub fn classify(message: impl Into<String>) -> Self {
        let message = message.into();
        let m = message.to_lowercase();
        let contains = |words: &[&str]| words.iter().any(|w| m.contains(w));
        let kind = if contains(&["cancel", "interrompido"]) {
            FailureKind::Cancelled
        } else if contains(&[
            "private",
            "deleted",
            "removed",
            "age-restrict",
            "not available in your country",
            "privado",
            "removido",
            "restrição",
        ]) {
            FailureKind::Restricted
        } else if contains(&["ffmpeg", "postprocess", "transcod", "remux"]) {
            FailureKind::Finalization
        } else if contains(&[
            "no space",
            "permission denied",
            "access is denied",
            "read-only",
            "disk",
            "permissão",
        ]) {
            FailureKind::Filesystem
        } else if contains(&[
            "no such file",
            "not found",
            "não encontrado",
            "dependency",
            "dependência",
        ]) {
            FailureKind::Dependency
        } else if contains(&[
            "timeout",
            "timed out",
            "network",
            "connection",
            "http error 5",
            "429",
            "fetch.rate",
        ]) {
            FailureKind::Transport
        } else {
            FailureKind::Extraction
        };
        Self { kind, message }
    }
}
impl fmt::Display for DownloadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for DownloadFailure {}

pub fn cobalt_unsupported(url: &str, opts: &DownloadOptions) -> Option<&'static str> {
    if (opts.is_audio && !super::models::is_audio_format(&opts.format))
        || (!opts.is_audio && super::video_profile(&opts.format).is_none())
    {
        return Some("Unsupported output format / Formato de saída não suportado");
    }
    if opts.max_height.is_some_and(|h| h < 144) {
        return Some("Cobalt minimum quality is 144p / Qualidade mínima do Cobalt: 144p");
    }
    let Ok(url) = reqwest::Url::parse(url) else {
        return Some(
            "Cobalt requires a direct HTTP(S) media page URL. / Cobalt requer um link HTTP(S) direto.",
        );
    };
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Some("Cobalt requires a direct HTTP(S) URL.");
    }
    let host = url.host_str().unwrap();
    if host == "spotify.com" || host.ends_with(".spotify.com") || url.path() == "/playlist" {
        return Some(
            "Spotify, search and playlist discovery require yt-dlp. / Spotify, busca e playlists requerem yt-dlp.",
        );
    }
    if opts.is_live || opts.live_from_start {
        return Some("Live recording requires yt-dlp. / Gravação de live requer yt-dlp.");
    }
    if opts.clip.is_some() {
        return Some("Clips require yt-dlp. / Recortes requerem yt-dlp.");
    }
    if opts
        .subtitle_langs
        .as_ref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        return Some("Subtitles require yt-dlp. / Legendas requerem yt-dlp.");
    }
    if opts
        .rate_limit
        .as_ref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        return Some(
            "Download rate limits require yt-dlp. / Limites de velocidade requerem yt-dlp.",
        );
    }
    None
}
impl DownloadEngine {
    pub async fn preview_with_engine(
        &self,
        url: &str,
        preference: EnginePreference,
    ) -> Result<VideoPreview, Box<dyn std::error::Error>> {
        let eligible = cobalt_unsupported(url, &DownloadOptions::default()).is_none();
        if preference == EnginePreference::Cobalt {
            if !eligible {
                return Err(DownloadFailure::new(
                    FailureKind::Unsupported,
                    "Cobalt requires a direct media link / Cobalt requer um link direto",
                )
                .into());
            }
            return Ok(VideoPreview::default());
        }
        let preview =
            match tokio::time::timeout(std::time::Duration::from_secs(45), self.fetch_preview(url))
                .await
            {
                Ok(result) => result,
                Err(_) => Err(DownloadFailure::new(
                    FailureKind::Transport,
                    "yt-dlp metadata timeout / Tempo esgotado ao obter metadados",
                )
                .into()),
            };
        match preview {
            Ok(p) => Ok(p),
            Err(e)
                if preference == EnginePreference::Auto
                    && eligible
                    && DownloadFailure::classify(e.to_string()).fallback_eligible() =>
            {
                Ok(VideoPreview::default())
            }
            Err(e) => Err(e),
        }
    }
    pub async fn fetch_and_download<F>(
        &self,
        url: &str,
        output: &str,
        opts: DownloadOptions,
        on_progress: F,
    ) -> Result<PathBuf, Box<dyn std::error::Error>>
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        if opts.engine == EnginePreference::Cobalt {
            if let Some(reason) = cobalt_unsupported(url, &opts) {
                return Err(DownloadFailure::new(FailureKind::Unsupported, reason).into());
            }
            return self
                .cobalt_download(url, output, &opts, &on_progress)
                .await
                .map_err(|e| Box::new(e) as _);
        }
        on_progress(Progress {
            engine: Some(EnginePreference::YtDlp),
            indeterminate: true,
            ..Default::default()
        });
        let progress = |p: Progress| {
            on_progress(Progress {
                engine: Some(EnginePreference::YtDlp),
                ..p
            })
        };
        let attempt = async {
            if opts.is_live {
                return self
                    .ytdlp_fetch_and_download(url, output, opts.clone(), progress)
                    .await;
            }
            let requested = std::path::Path::new(output);
            let folder = requested.parent().unwrap_or(std::path::Path::new("."));
            std::fs::create_dir_all(folder).map_err(DownloadFailure::io)?;
            let temporary;
            let staging_path = if let Some(id) = opts
                .staging_id
                .as_ref()
                .and_then(|id| uuid::Uuid::parse_str(id).ok())
            {
                let path = folder.join(format!(".lumen-ytdlp-{id}"));
                std::fs::create_dir_all(&path).map_err(DownloadFailure::io)?;
                path
            } else {
                temporary = tempfile::Builder::new()
                    .prefix(".lumen-ytdlp-")
                    .tempdir_in(folder)
                    .map_err(DownloadFailure::io)?;
                temporary.path().to_path_buf()
            };
            let staged = staging_path.join(requested.file_name().ok_or_else(|| {
                DownloadFailure::new(FailureKind::Filesystem, "Missing filename")
            })?);
            let path = self
                .ytdlp_fetch_and_download(url, &staged.to_string_lossy(), opts.clone(), progress)
                .await?;
            let mut destination = requested.to_path_buf();
            destination.set_extension(&opts.format);
            let output = super::cobalt::publish(&path, &destination)?;
            super::download::move_subtitle_sidecars(&path, &output);
            let _ = std::fs::remove_dir_all(&staging_path);
            Ok(output)
        };
        let result = attempt
            .await
            .map_err(|error| match error.downcast::<DownloadFailure>() {
                Ok(e) => *e,
                Err(e) => DownloadFailure::classify(e.to_string()),
            });
        match result {
            Ok(path) => Ok(path),
            Err(error) => {
                if opts.engine != EnginePreference::Auto
                    || !error.fallback_eligible()
                    || cobalt_unsupported(url, &opts).is_some()
                {
                    return Err(error.into());
                }
                on_progress(Progress {
                    engine: Some(EnginePreference::Cobalt),
                    fallback: true,
                    indeterminate: true,
                    ..Default::default()
                });
                let progress = |p: Progress| {
                    on_progress(Progress {
                        fallback: true,
                        ..p
                    })
                };
                let result = self.cobalt_download(url, output, &opts, &progress).await;
                if result.is_ok() {
                    if let Some(id) = opts
                        .staging_id
                        .as_ref()
                        .and_then(|id| uuid::Uuid::parse_str(id).ok())
                    {
                        let folder = std::path::Path::new(output)
                            .parent()
                            .unwrap_or(std::path::Path::new("."));
                        let _ = std::fs::remove_dir_all(folder.join(format!(".lumen-ytdlp-{id}")));
                    }
                }
                result.map_err(|second| {
                    Box::new(DownloadFailure::new(
                        second.kind,
                        format!("yt-dlp: {}; Cobalt: {}", error, second),
                    )) as _
                })
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capabilities_do_not_drop_options() {
        let mut opts = DownloadOptions::default();
        let url = "https://youtu.be/example";
        assert!(cobalt_unsupported(url, &opts).is_none());
        opts.clip = Some(("0".into(), "1".into()));
        assert!(cobalt_unsupported(url, &opts).is_some());
        opts.clip = None;
        opts.rate_limit = Some("1M".into());
        assert!(cobalt_unsupported(url, &opts).is_some());
        assert!(cobalt_unsupported("ytsearch1:track", &DownloadOptions::default()).is_some());
    }
    #[test]
    fn errors_do_not_switch_for_local_or_content_failures() {
        for e in [
            "private video",
            "No space left on device",
            "ffmpeg failed",
            "cancelled",
        ] {
            assert!(!DownloadFailure::classify(e).fallback_eligible(), "{e}");
        }
        for e in [
            "HTTP Error 403",
            "extractor broken",
            "connection reset",
            "No such file",
        ] {
            assert!(DownloadFailure::classify(e).fallback_eligible(), "{e}");
        }
    }
}
