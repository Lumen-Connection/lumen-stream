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
    #[serde(default = "default_auto_ui_scale")]
    pub auto_ui_scale: bool,
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

fn default_auto_ui_scale() -> bool {
    true
}

/// Escala da UI a partir da altura do monitor em pontos (já dividida pelo
/// scale factor do SO). Âncoras: 720p→0.9, 1080p→1.0, 1440p→1.25, 4K→1.6.
/// Interpolação linear entre elas; clamp final 0.7..=2.0.
pub fn auto_scale_for_monitor(monitor_h_points: f32) -> f32 {
    const ANCHORS: [(f32, f32); 4] = [
        (720.0, 0.9),
        (1080.0, 1.0),
        (1440.0, 1.25),
        (2160.0, 1.6),
    ];
    let raw = if monitor_h_points <= ANCHORS[0].0 {
        // Abaixo de 720p: 240p → 0.7, 720p → 0.9.
        let t = (monitor_h_points - 240.0) / (720.0 - 240.0);
        0.7 + t * 0.2
    } else {
        let mut i = 0;
        while i + 1 < ANCHORS.len() && monitor_h_points > ANCHORS[i + 1].0 {
            i += 1;
        }
        if i + 1 < ANCHORS.len() {
            let (x0, y0) = ANCHORS[i];
            let (x1, y1) = ANCHORS[i + 1];
            let t = (monitor_h_points - x0) / (x1 - x0);
            y0 + t * (y1 - y0)
        } else {
            let (x0, y0) = ANCHORS[ANCHORS.len() - 2];
            let (x1, y1) = ANCHORS[ANCHORS.len() - 1];
            let t = (monitor_h_points - x0) / (x1 - x0);
            y0 + t * (y1 - y0)
        }
    };
    raw.clamp(0.7, 2.0)
}

/// Ajusta a janela salva à área do monitor: encolhe para ≤85% se passar do
/// monitor; amplia proporcionalmente se ficou <45% da largura (caso típico
/// de abrir numa TV 4K); respeita o mínimo 700×450 quando o monitor comporta.
pub fn fit_window_to_monitor(win_w: f32, win_h: f32, mon_w: f32, mon_h: f32) -> (f32, f32) {
    const MIN_W: f32 = 700.0;
    const MIN_H: f32 = 450.0;
    const MAX_FRAC: f32 = 0.85;
    const MIN_WIDTH_FRAC: f32 = 0.45;

    if mon_w <= 1.0 || mon_h <= 1.0 {
        return (win_w, win_h);
    }

    let max_w = mon_w * MAX_FRAC;
    let max_h = mon_h * MAX_FRAC;
    let too_big = win_w > max_w || win_h > max_h;
    let too_small = win_w < mon_w * MIN_WIDTH_FRAC;
    let below_min = win_w < MIN_W.min(max_w) || win_h < MIN_H.min(max_h);
    if !too_big && !too_small && !below_min {
        return (win_w, win_h);
    }

    let mut w = win_w;
    let mut h = win_h;

    if too_big {
        let scale = (max_w / w).min(max_h / h).min(1.0);
        w *= scale;
        h *= scale;
    }

    if w < mon_w * MIN_WIDTH_FRAC {
        let scale = (mon_w * MIN_WIDTH_FRAC) / w;
        w *= scale;
        h *= scale;
        if w > max_w || h > max_h {
            let scale = (max_w / w).min(max_h / h);
            w *= scale;
            h *= scale;
        }
    }

    w = w.max(MIN_W.min(max_w));
    h = h.max(MIN_H.min(max_h));
    (w, h)
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
            auto_ui_scale: true,
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
        assert!(c.auto_ui_scale);
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
        assert!(c.auto_ui_scale);
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
        c.auto_ui_scale = false;
        let json = serde_json::to_string(&c).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.music_format, "flac");
        assert!(back.theme == Theme::Light);
        assert_eq!(back.convert_engine, ConvertEngine::LibreOffice);
        assert_eq!(back.home_pinned, vec!["music"]);
        assert!(!back.auto_ui_scale);
    }

    #[test]
    fn auto_scale_hits_resolution_anchors() {
        assert_eq!(auto_scale_for_monitor(720.0), 0.9);
        assert_eq!(auto_scale_for_monitor(1080.0), 1.0);
        assert_eq!(auto_scale_for_monitor(1440.0), 1.25);
        assert_eq!(auto_scale_for_monitor(2160.0), 1.6);
    }

    #[test]
    fn auto_scale_interpolates_and_clamps() {
        assert!((auto_scale_for_monitor(900.0) - 0.95).abs() < 1e-5);
        assert_eq!(auto_scale_for_monitor(240.0), 0.7);
        assert_eq!(auto_scale_for_monitor(5000.0), 2.0);
        assert_eq!(auto_scale_for_monitor(0.0), 0.7);
    }

    #[test]
    fn fit_window_shrinks_to_85_percent_of_monitor() {
        let (w, h) = fit_window_to_monitor(1920.0, 1080.0, 1280.0, 720.0);
        assert!((w - 1280.0 * 0.85).abs() < 0.5);
        assert!((h - 720.0 * 0.85).abs() < 0.5);
        assert!(w <= 1280.0 * 0.85 + 0.01);
        assert!(h <= 720.0 * 0.85 + 0.01);
    }

    #[test]
    fn fit_window_leaves_valid_size_unchanged() {
        assert_eq!(
            fit_window_to_monitor(960.0, 640.0, 1920.0, 1080.0),
            (960.0, 640.0)
        );
    }

    #[test]
    fn fit_window_respects_min_when_monitor_allows() {
        let (w, h) = fit_window_to_monitor(500.0, 300.0, 1920.0, 1080.0);
        // 500 < 45% de 1920 (864) → amplia; depois o mínimo 700×450.
        assert!(w >= 700.0);
        assert!(h >= 450.0);
        assert!(w <= 1920.0 * 0.85);
        assert!(h <= 1080.0 * 0.85);
    }

    #[test]
    fn fit_window_enlarges_tiny_window_on_large_monitor() {
        let (w, h) = fit_window_to_monitor(960.0, 640.0, 3840.0, 2160.0);
        assert!((w - 3840.0 * 0.45).abs() < 1.0);
        let expected_h = 640.0 * (3840.0 * 0.45 / 960.0);
        assert!((h - expected_h).abs() < 1.0);
    }
}
