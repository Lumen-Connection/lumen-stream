use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use super::text_utils::sanitize_filename;

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

/// A video-download profile. The extension is intentionally kept as the
/// persisted value so existing config, queue and history records remain valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoProfile {
    pub extension: &'static str,
    pub label: &'static str,
    pub video_encoder: &'static str,
    pub audio_encoder: &'static str,
}

const VIDEO_PROFILES: [VideoProfile; 3] = [
    VideoProfile {
        extension: "mp4",
        label: "H.264 (MP4)",
        video_encoder: "libx264",
        audio_encoder: "aac",
    },
    VideoProfile {
        extension: "mkv",
        label: "AV1 (MKV)",
        video_encoder: "libaom-av1",
        audio_encoder: "flac",
    },
    VideoProfile {
        extension: "webm",
        label: "VP9 (WebM)",
        video_encoder: "libvpx-vp9",
        audio_encoder: "libopus",
    },
];

pub fn video_profiles() -> &'static [VideoProfile] {
    &VIDEO_PROFILES
}

pub fn video_profile(extension: &str) -> Option<&'static VideoProfile> {
    VIDEO_PROFILES.iter().find(|profile| profile.extension == extension)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn duration_formats_minutes_and_hours() {
        assert_eq!(format_duration(5), "0:05");
        assert_eq!(format_duration(62), "1:02");
        assert_eq!(format_duration(3600), "1:00:00");
        assert_eq!(format_duration(3723), "1:02:03");
    }

    #[test]
    fn duration_clamps_negative_to_zero() {
        assert_eq!(format_duration(-10), "0:00");
    }

    #[test]
    fn size_uses_correct_units() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(2048), "2 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.00 GB");
    }

    #[test]
    fn organize_by_type_maps_media() {
        assert_eq!(organize_subfolder("type", "music", ""), Some("Música".into()));
        assert_eq!(organize_subfolder("type", "video", ""), Some("Vídeo".into()));
        assert_eq!(organize_subfolder("type", "convert", ""), Some("Convertidos".into()));
        assert_eq!(organize_subfolder("type", "other", ""), Some("Outros".into()));
    }

    #[test]
    fn organize_by_date_is_iso_day() {
        let d = organize_subfolder("date", "music", "").unwrap();
        assert_eq!(d.len(), 10, "esperado YYYY-MM-DD, veio {d}");
        assert_eq!(d.as_bytes()[4], b'-');
        assert_eq!(d.as_bytes()[7], b'-');
    }

    #[test]
    fn organize_by_channel_sanitizes_and_skips_empty() {
        assert_eq!(
            organize_subfolder("channel", "music", "Canal/Legal"),
            Some("CanalLegal".into())
        );
        assert_eq!(organize_subfolder("channel", "music", ""), None);
        assert_eq!(organize_subfolder("channel", "music", "???"), None);
    }

    #[test]
    fn organize_none_returns_none() {
        assert_eq!(organize_subfolder("none", "music", "c"), None);
    }

    #[test]
    fn audio_format_detection() {
        for f in ["mp3", "m4a", "aac", "opus", "ogg", "wav", "flac"] {
            assert!(is_audio_format(f), "{f} deveria ser áudio");
        }
        assert!(!is_audio_format("mp4"));
        assert!(!is_audio_format(""));
    }

    #[test]
    fn categorize_by_extension() {
        let cases = [
            ("a.mp3", FileCategory::Audio),
            ("a.FLAC", FileCategory::Audio),
            ("a.mp4", FileCategory::Video),
            ("a.mkv", FileCategory::Video),
            ("a.png", FileCategory::Image),
            ("a.pdf", FileCategory::Document),
            ("a.docx", FileCategory::Office),
            ("a.xlsx", FileCategory::Office),
            ("a.xyz", FileCategory::Unknown),
            ("sem_extensao", FileCategory::Unknown),
        ];
        for (name, expected) in cases {
            assert!(
                categorize(&PathBuf::from(name)) == expected,
                "categoria errada para {name}"
            );
        }
    }

    #[test]
    fn output_formats_cover_categories() {
        assert!(output_formats(FileCategory::Audio).contains(&"mp3"));
        assert!(output_formats(FileCategory::Video).contains(&"mp4"));
        assert!(output_formats(FileCategory::Image).contains(&"png"));
        assert!(output_formats(FileCategory::Office).contains(&"pdf"));
        assert!(output_formats(FileCategory::Unknown).is_empty());
    }

    #[test]
    fn download_options_defaults() {
        let o = DownloadOptions::default();
        assert!(!o.is_audio);
        assert_eq!(o.format, "mp4");
        assert_eq!(o.quality, "best");
        assert_eq!(o.concurrent_fragments, 4);
        assert!(!o.is_live);
        assert!(o.stop.is_none());
    }

    #[test]
    fn video_profiles_keep_stable_extensions_and_descriptive_labels() {
        let profiles = video_profiles();
        assert_eq!(
            profiles
                .iter()
                .map(|profile| profile.extension)
                .collect::<Vec<_>>(),
            vec!["mp4", "mkv", "webm"]
        );
        assert_eq!(video_profile("mp4").unwrap().label, "H.264 (MP4)");
        assert_eq!(video_profile("mkv").unwrap().label, "AV1 (MKV)");
        assert_eq!(video_profile("webm").unwrap().label, "VP9 (WebM)");
    }
}
