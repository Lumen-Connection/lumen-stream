use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ui::i18n::Lang;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::Dark
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum ConvertEngine {
    Auto,
    Rust,
    LibreOffice,
    MsOffice,
}

impl Default for ConvertEngine {
    fn default() -> Self {
        ConvertEngine::Auto
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub default_download_dir: PathBuf,
    pub music_format: String,
    pub video_format: String,
    pub quality: String,
    pub max_history: usize,
    #[serde(default)]
    pub lang: Lang,
    #[serde(default)]
    pub subtitles: bool,
    #[serde(default = "default_sub_langs")]
    pub sub_langs: String,
    #[serde(default)]
    pub last_ytdlp_update: i64,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default = "default_true")]
    pub notify_on_complete: bool,
    #[serde(default = "default_win_w")]
    pub win_w: f32,
    #[serde(default = "default_win_h")]
    pub win_h: f32,
    #[serde(default)]
    pub rate_limit: String,
    #[serde(default = "default_fragments")]
    pub concurrent_fragments: u32,
    #[serde(default = "default_organize")]
    pub organize_by: String,
    #[serde(default)]
    pub cloud_folder: String,
    #[serde(default)]
    pub copy_to_cloud: bool,
    #[serde(default)]
    pub history_grid: bool,
    #[serde(default = "default_scale")]
    pub ui_scale: f32,
    #[serde(default)]
    pub high_contrast: bool,
    #[serde(default)]
    pub compact_ui: bool,
    #[serde(default)]
    pub transcribe_translate: bool,
    #[serde(default = "default_template")]
    pub filename_template: String,
    #[serde(default = "default_true")]
    pub smart_rename: bool,
    #[serde(default = "default_home_cards")]
    pub home_cards: Vec<String>,
    #[serde(default)]
    pub home_pinned: Vec<String>,
    #[serde(default)]
    pub last_tab: String,
    #[serde(default)]
    pub confirm_delete: bool,
    #[serde(default)]
    pub onboarded: bool,
    #[serde(default = "default_true")]
    pub auto_retry: bool,
    #[serde(default = "default_img_format")]
    pub image_format: String,
    #[serde(default)]
    pub image_max_width: u32,
    #[serde(default = "default_img_quality")]
    pub image_quality: u32,
    #[serde(default)]
    pub watermark_path: String,
    #[serde(default = "default_wm_pos")]
    pub watermark_pos: String,
    #[serde(default = "default_wm_scale")]
    pub watermark_scale: u32,
    #[serde(default = "default_wm_opacity")]
    pub watermark_opacity: f32,
    #[serde(default)]
    pub convert_engine: ConvertEngine,
}

fn default_template() -> String {
    "%(title)s".to_string()
}
fn default_img_format() -> String {
    "jpg".to_string()
}
fn default_img_quality() -> u32 {
    85
}
fn default_home_cards() -> Vec<String> {
    ["music", "video", "transcribe", "converter"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}
fn default_wm_pos() -> String {
    "br".to_string()
}
fn default_wm_scale() -> u32 {
    100
}
fn default_wm_opacity() -> f32 {
    0.8
}

fn default_scale() -> f32 {
    1.0
}

fn default_organize() -> String {
    "none".to_string()
}

fn default_fragments() -> u32 {
    4
}

fn default_true() -> bool {
    true
}
fn default_win_w() -> f32 {
    960.0
}
fn default_win_h() -> f32 {
    640.0
}

fn default_sub_langs() -> String {
    "pt,en".to_string()
}

impl Default for Config {
    fn default() -> Self {
        let downloads_dir = dirs::download_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LumenStream");

        Config {
            default_download_dir: downloads_dir,
            music_format: "mp3".to_string(),
            video_format: "mp4".to_string(),
            quality: "best".to_string(),
            max_history: 50,
            lang: Lang::default(),
            subtitles: false,
            sub_langs: default_sub_langs(),
            last_ytdlp_update: 0,
            theme: Theme::default(),
            notify_on_complete: true,
            rate_limit: String::new(),
            concurrent_fragments: default_fragments(),
            organize_by: default_organize(),
            cloud_folder: String::new(),
            copy_to_cloud: false,
            history_grid: false,
            ui_scale: 1.0,
            high_contrast: false,
            compact_ui: false,
            transcribe_translate: false,
            filename_template: default_template(),
            smart_rename: true,
            home_cards: default_home_cards(),
            home_pinned: Vec::new(),
            last_tab: String::new(),
            confirm_delete: false,
            onboarded: false,
            auto_retry: true,
            image_format: default_img_format(),
            image_max_width: 0,
            image_quality: default_img_quality(),
            watermark_path: String::new(),
            watermark_pos: default_wm_pos(),
            watermark_scale: default_wm_scale(),
            watermark_opacity: default_wm_opacity(),
            win_w: default_win_w(),
            win_h: default_win_h(),
            convert_engine: ConvertEngine::default(),
        }
    }
}

impl Config {
    fn data_dir() -> PathBuf {
        crate::paths::data_dir()
    }

    pub fn config_path() -> PathBuf {
        Self::data_dir().join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            std::fs::write(&path, json).ok();
        }
    }

    pub fn db_path(&self) -> PathBuf {
        Self::data_dir().join("lumen.db")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert_eq!(c.music_format, "mp3");
        assert_eq!(c.video_format, "mp4");
        assert_eq!(c.quality, "best");
        assert_eq!(c.max_history, 50);
        assert_eq!(c.sub_langs, "pt,en");
        assert_eq!(c.concurrent_fragments, 4);
        assert_eq!(c.organize_by, "none");
        assert_eq!(c.filename_template, "%(title)s");
        assert_eq!(c.image_format, "jpg");
        assert_eq!(c.image_quality, 85);
        assert_eq!(c.watermark_pos, "br");
        assert_eq!(c.watermark_scale, 100);
        assert_eq!(c.ui_scale, 1.0);
        assert_eq!((c.win_w, c.win_h), (960.0, 640.0));
        assert!(c.notify_on_complete && c.smart_rename && c.auto_retry);
        assert!(!c.subtitles && !c.high_contrast && !c.onboarded);
        assert!(c.theme == Theme::Dark);
        assert_eq!(c.convert_engine, ConvertEngine::Auto);
        assert_eq!(c.home_cards, vec!["music", "video", "transcribe", "converter"]);
        assert_eq!(
            c.default_download_dir.file_name().and_then(|n| n.to_str()),
            Some("LumenStream")
        );
    }

    // Config antiga no disco (sem os campos novos) deve carregar com os
    // defaults preenchidos — é o contrato dos #[serde(default)].
    #[test]
    fn old_config_json_gains_defaults_for_new_fields() {
        let minimal = r#"{
            "default_download_dir": "C:/dl",
            "music_format": "opus",
            "video_format": "mkv",
            "quality": "1080",
            "max_history": 10
        }"#;
        let c: Config = serde_json::from_str(minimal).expect("json mínimo deve carregar");
        assert_eq!(c.music_format, "opus");
        assert_eq!(c.max_history, 10);
        assert_eq!(c.sub_langs, "pt,en");
        assert_eq!(c.concurrent_fragments, 4);
        assert!(c.notify_on_complete && c.auto_retry);
        assert_eq!(c.watermark_opacity, 0.8);
        assert!(c.theme == Theme::Dark);
    }

    #[test]
    fn serde_roundtrip_preserves_fields() {
        let mut c = Config::default();
        c.music_format = "flac".into();
        c.theme = Theme::Light;
        c.convert_engine = ConvertEngine::LibreOffice;
        c.home_pinned = vec!["music".into()];
        let json = serde_json::to_string(&c).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.music_format, "flac");
        assert!(back.theme == Theme::Light);
        assert_eq!(back.convert_engine, ConvertEngine::LibreOffice);
        assert_eq!(back.home_pinned, vec!["music"]);
    }
}
