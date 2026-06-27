use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ui::i18n::Lang;

/// Perfil de download salvo (preset de formato + qualidade por tipo).
#[derive(Clone, Serialize, Deserialize)]
pub struct DownloadProfile {
    pub name: String,
    pub media_type: String, // "music" | "video"
    pub format: String,
    pub quality: String,
}

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
    // Fragmentos baixados em paralelo (acelera). 0/1 = padrão do yt-dlp.
    #[serde(default = "default_fragments")]
    pub concurrent_fragments: u32,
    // Organização automática em subpastas: "none" | "type" | "date" | "channel".
    #[serde(default = "default_organize")]
    pub organize_by: String,
    // Pasta de nuvem (Drive/Dropbox/OneDrive sincronizado) para cópia automática.
    #[serde(default)]
    pub cloud_folder: String,
    #[serde(default)]
    pub copy_to_cloud: bool,
    /// Exibir histórico em grade (true) ou em lista (false).
    #[serde(default)]
    pub history_grid: bool,
    /// Escala da interface (1.0 = padrão).
    #[serde(default = "default_scale")]
    pub ui_scale: f32,
    /// Modo de alto contraste (acessibilidade).
    #[serde(default)]
    pub high_contrast: bool,
    /// Densidade compacta da interface.
    #[serde(default)]
    pub compact_ui: bool,
    /// Traduzir a transcrição para inglês (whisper --translate).
    #[serde(default)]
    pub transcribe_translate: bool,
    /// Template do nome do arquivo (tokens: %(title)s, %(uploader)s).
    #[serde(default = "default_template")]
    pub filename_template: String,
    /// Limpar automaticamente o nome (remover [Official Video], - Topic, etc.).
    #[serde(default = "default_true")]
    pub smart_rename: bool,
    /// Cards de atalho da Home (ids ordenados) e os fixados (favoritos).
    #[serde(default = "default_home_cards")]
    pub home_cards: Vec<String>,
    #[serde(default)]
    pub home_pinned: Vec<String>,
    /// Última aba aberta (id) e confirmação ao limpar histórico.
    #[serde(default)]
    pub last_tab: String,
    #[serde(default)]
    pub confirm_delete: bool,
    /// Boas-vindas já vistas (onboarding).
    #[serde(default)]
    pub onboarded: bool,
    /// Perfis de download salvos.
    #[serde(default)]
    pub profiles: Vec<DownloadProfile>,
    /// Re-tentar automaticamente downloads que falharem por rede.
    #[serde(default = "default_true")]
    pub auto_retry: bool,
    /// Conversão de imagens em lote: formato, largura máx (0=original) e qualidade.
    #[serde(default = "default_img_format")]
    pub image_format: String,
    #[serde(default)]
    pub image_max_width: u32,
    #[serde(default = "default_img_quality")]
    pub image_quality: u32,
    /// Marca d'água do usuário (caminho da imagem) e parâmetros.
    #[serde(default)]
    pub watermark_path: String,
    #[serde(default = "default_wm_pos")]
    pub watermark_pos: String,
    #[serde(default = "default_wm_scale")]
    pub watermark_scale: u32,
    #[serde(default = "default_wm_opacity")]
    pub watermark_opacity: f32,
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
            .join("LumenDownloader");

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
            profiles: Vec::new(),
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
        }
    }
}

impl Config {
    fn data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LumenDownloader")
    }

    pub fn config_path() -> PathBuf {
        Self::data_dir().join("config.json")
    }

    /// Carrega as configurações do disco, ou usa os padrões na primeira execução.
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    /// Salva as configurações no disco.
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
