use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::settings::Config;
use crate::db::database::Database;

/// Estado do editor de tags ID3.
pub struct TagEditState {
    pub path: String,
    pub t: crate::download::engine::AudioTags,
    pub detecting: bool,
}
use crate::download::engine::{DownloadEngine, VideoPreview};
use crate::queue::Queue;
use crate::ui::dashboard;

pub struct App {
    pub active_tab: Tab,
    pub music_url: String,
    pub video_url: String,
    pub operation: Arc<Mutex<DownloadOperation>>,
    pub db: Database,
    pub config: Config,
    pub engine: Option<Arc<DownloadEngine>>,
    pub download_task: Option<tokio::task::JoinHandle<()>>,
    /// Estado da atualização do yt-dlp exibido na tela de Configurações.
    pub update_status: Arc<Mutex<UpdateStatus>>,
    /// Fila de downloads em lote / playlists.
    pub queue: Queue,
    pub batch_input: String,
    pub batch_media_type: MediaType,
    pub batch_format: String,
    pub batch_quality: String,
    pub history_search: String,
    /// Filtro de formato no histórico ("" = todos).
    pub history_format_filter: String,
    /// Textura da miniatura de pré-visualização (cache).
    pub thumb_texture: Option<egui::TextureHandle>,
    pub thumb_key: Option<String>,
    /// Edição de nome de pasta em andamento (id, novo nome).
    pub folder_edit: Option<(i64, String)>,
    /// Clipboard watcher: último texto visto e sugestão de link a baixar.
    pub clip_suggest: Option<String>,
    clip_seen: String,
    clip_last_check: std::time::Instant,
    /// Conversão em lote: arquivos selecionados e formato de saída escolhido.
    pub batch_convert: Vec<std::path::PathBuf>,
    pub batch_convert_format: String,
    /// Notificações in-app (toasts) e controle de duplicação.
    pub toasts: Vec<Toast>,
    last_signaled: Option<String>,
    /// Fila de toasts vinda de tarefas em segundo plano (texto, é_erro).
    pub toast_queue: Arc<Mutex<Vec<(String, bool)>>>,
    /// Estado do inspetor de formatos (janela popup).
    pub inspector: Arc<Mutex<InspectorState>>,
    /// Janela genérica de informação (título, corpo) — ex.: metadados de arquivo.
    pub info_window: Arc<Mutex<Option<(String, String)>>>,
    /// Janela com QR code do link de origem (url, textura).
    pub qr_window: Option<(String, egui::TextureHandle)>,
    /// Diálogo de reordenação de PDF: (arquivo, ordem digitada).
    pub pdf_reorder: Option<(PathBuf, String)>,
    /// Editor de tags ID3 (janela).
    pub tag_editor: Option<TagEditState>,
    bpm_result: Arc<Mutex<Option<u32>>>,
    /// Filtro "só favoritos" no histórico e diálogo de tags/categorias (id, texto).
    pub history_fav_only: bool,
    pub history_tag_edit: Option<(i64, String)>,
    /// Modo de edição dos cards de atalho da Home.
    pub home_edit: bool,
    /// Itens do histórico selecionados (ações em massa).
    pub selected: std::collections::HashSet<i64>,
    /// Lista de arquivos órfãos encontrados (janela).
    pub orphans: Option<Vec<PathBuf>>,
    /// Rascunho do novo perfil de download (na tela de Configurações).
    pub profile_draft: crate::config::settings::DownloadProfile,
    /// Tela cheia (modo quiosque).
    pub fullscreen: bool,
    /// Último download (url, tipo) para "repetir".
    pub last_download: Option<(String, MediaType)>,
    /// Confirmação pendente de "limpar histórico" (media_type).
    pub pending_clear: Option<String>,
    /// Assinatura da fila para detectar mudanças e persistir.
    queue_sig: u64,
    /// Abas destacadas em janelas próprias (multi-janela).
    pub detached: Vec<Tab>,
    /// Pré-visualização da marca d'água: vídeo de amostra, textura e controle.
    pub wm_preview_video: Option<PathBuf>,
    pub wm_preview_tex: Option<egui::TextureHandle>,
    pub wm_preview_sig: String,
    pub wm_preview_busy: bool,
    wm_preview_ready: Arc<Mutex<Option<PathBuf>>>,
    /// Status/versões das dependências (yt-dlp, ffmpeg, pdfium, whisper).
    pub deps_status: Arc<Mutex<Vec<(String, String)>>>,
    pub deps_requested: bool,
    /// Paleta de comandos (Ctrl+K).
    pub cmd_palette_open: bool,
    pub cmd_query: String,
    /// Sinaliza reaplicar o tema/estilo no próximo frame.
    pub restyle: bool,
    /// Logo (logo + nome) exibido na sidebar — carregado uma vez.
    pub brand_texture: Option<egui::TextureHandle>,
    /// Cache de texturas da galeria de miniaturas (por caminho).
    pub gallery_textures: HashMap<PathBuf, egui::TextureHandle>,
    /// Miniaturas de vídeos do histórico: texturas carregadas e geração em background.
    pub thumb_textures: HashMap<String, egui::TextureHandle>,
    thumb_ready: Arc<Mutex<Vec<(String, PathBuf)>>>,
    thumb_inflight: std::collections::HashSet<String>,
    engine_holder: Arc<Mutex<Option<DownloadEngine>>>,
    engine_spawned: Arc<AtomicBool>,
    style_set: bool,
    update_checked: bool,
    win_dirty: bool,
}

/// Uma notificação efêmera exibida no canto da tela.
pub struct Toast {
    pub text: String,
    pub error: bool,
    pub created: std::time::Instant,
    /// Se presente, mostra um botão "Desfazer" que restaura este item do histórico.
    pub undo: Option<i64>,
}

/// Estado do inspetor de formatos.
#[derive(Default)]
pub struct InspectorState {
    pub open: bool,
    pub loading: bool,
    pub url: String,
    pub rows: Vec<crate::download::engine::FormatRow>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tab {
    Home,
    Music,
    Video,
    Converter,
    Queue,
    Folders,
    Gallery,
    Cloud,
    Stats,
    Achievements,
    Settings,
    Help,
}

impl Tab {
    pub fn id(&self) -> &'static str {
        match self {
            Tab::Home => "home",
            Tab::Music => "music",
            Tab::Video => "video",
            Tab::Converter => "converter",
            Tab::Queue => "queue",
            Tab::Folders => "folders",
            Tab::Gallery => "gallery",
            Tab::Cloud => "cloud",
            Tab::Stats => "stats",
            Tab::Achievements => "achievements",
            Tab::Settings => "settings",
            Tab::Help => "help",
        }
    }
    pub fn from_id(s: &str) -> Tab {
        match s {
            "music" => Tab::Music,
            "video" => Tab::Video,
            "converter" => Tab::Converter,
            "queue" => Tab::Queue,
            "folders" => Tab::Folders,
            "gallery" => Tab::Gallery,
            "cloud" => Tab::Cloud,
            "stats" => Tab::Stats,
            "achievements" => Tab::Achievements,
            "settings" => Tab::Settings,
            "help" => Tab::Help,
            _ => Tab::Home,
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum UpdateStatus {
    Idle,
    Running,
    Done(String),
    Error(String),
}

pub struct DownloadOperation {
    pub phase: DownloadPhase,
    pub url: String,
    pub title: String,
    pub file_name: String,
    pub folder_path: PathBuf,
    pub subfolder_name: String,
    pub create_subfolder: bool,
    pub media_type: MediaType,
    pub output_format: String,
    pub quality: String,
    pub source_file: PathBuf,
    /// Progresso do download: `Some(0.0..=1.0)` ou `None` quando indeterminado.
    pub progress: Option<f32>,
    /// Pré-visualização (canal, duração, resoluções, tamanho estimado).
    pub preview: Option<VideoPreview>,
    /// Recorte por tempo (trim).
    pub clip_enabled: bool,
    pub clip_start: String,
    pub clip_end: String,
    /// Resolução máxima escolhida (None = melhor).
    pub max_height: Option<u32>,
    /// Preset de conversão de vídeo ("" = original, "compress", "720", ...).
    pub convert_preset: String,
    /// Gravar live do início (yt-dlp --live-from-start).
    pub live_from_start: bool,
}

#[derive(Clone, PartialEq)]
pub enum DownloadPhase {
    Idle,
    Fetching,
    Configuring,
    Downloading(String),
    Completed(String),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MediaType {
    Music,
    Video,
    Convert,
}

impl App {
    pub fn new() -> Self {
        let config = Config::load();
        let db = Database::open(&config.db_path());
        // Auto-limpeza: esvazia itens da lixeira com mais de 30 dias.
        db.purge_old_trash(30);

        let engine_holder: Arc<Mutex<Option<DownloadEngine>>> = Arc::new(Mutex::new(None));

        let operation = Arc::new(Mutex::new(DownloadOperation {
            phase: DownloadPhase::Idle,
            url: String::new(),
            title: String::new(),
            file_name: String::new(),
            folder_path: config.default_download_dir.clone(),
            subfolder_name: String::new(),
            create_subfolder: false,
            media_type: MediaType::Music,
            output_format: String::new(),
            quality: String::from("best"),
            source_file: PathBuf::new(),
            progress: None,
            preview: None,
            clip_enabled: false,
            clip_start: String::new(),
            clip_end: String::new(),
            max_height: None,
            convert_preset: String::new(),
            live_from_start: false,
        }));

        let batch_format = config.video_format.clone();
        let batch_quality = config.quality.clone();

        // Retoma a fila salva da sessão anterior.
        let mut queue = Queue::new();
        queue.load(&Self::queue_path());

        App {
            active_tab: Tab::from_id(&config.last_tab),
            music_url: String::new(),
            video_url: String::new(),
            operation,
            db,
            config,
            engine: None,
            download_task: None,
            update_status: Arc::new(Mutex::new(UpdateStatus::Idle)),
            queue,
            batch_input: String::new(),
            batch_media_type: MediaType::Video,
            batch_format,
            batch_quality,
            history_search: String::new(),
            history_format_filter: String::new(),
            thumb_texture: None,
            thumb_key: None,
            folder_edit: None,
            clip_suggest: None,
            clip_seen: String::new(),
            clip_last_check: std::time::Instant::now(),
            batch_convert: Vec::new(),
            batch_convert_format: String::new(),
            toasts: Vec::new(),
            last_signaled: None,
            toast_queue: Arc::new(Mutex::new(Vec::new())),
            inspector: Arc::new(Mutex::new(InspectorState::default())),
            info_window: Arc::new(Mutex::new(None)),
            qr_window: None,
            brand_texture: None,
            pdf_reorder: None,
            tag_editor: None,
            bpm_result: Arc::new(Mutex::new(None)),
            history_fav_only: false,
            history_tag_edit: None,
            home_edit: false,
            selected: std::collections::HashSet::new(),
            orphans: None,
            profile_draft: crate::config::settings::DownloadProfile {
                name: String::new(),
                media_type: "video".to_string(),
                format: "mp4".to_string(),
                quality: "best".to_string(),
            },
            fullscreen: false,
            last_download: None,
            pending_clear: None,
            queue_sig: 0,
            detached: Vec::new(),
            wm_preview_video: None,
            wm_preview_tex: None,
            wm_preview_sig: String::new(),
            wm_preview_busy: false,
            wm_preview_ready: Arc::new(Mutex::new(None)),
            deps_status: Arc::new(Mutex::new(Vec::new())),
            deps_requested: false,
            cmd_palette_open: false,
            cmd_query: String::new(),
            restyle: false,
            gallery_textures: HashMap::new(),
            thumb_textures: HashMap::new(),
            thumb_ready: Arc::new(Mutex::new(Vec::new())),
            thumb_inflight: std::collections::HashSet::new(),
            engine_holder,
            engine_spawned: Arc::new(AtomicBool::new(false)),
            style_set: false,
            update_checked: false,
            win_dirty: false,
        }
    }

    /// Remove arquivos temporários da pasta de download. Retorna a quantidade.
    pub fn clear_temp_files(&self) -> usize {
        let dir = &self.config.default_download_dir;
        let mut count = 0;
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_lowercase();
                let is_temp = name.ends_with(".part")
                    || name.ends_with(".ytdl")
                    || name.ends_with(".rawaudio")
                    || name.ends_with(".whisper.wav")
                    || name.contains(".transcribe.")
                    || name.starts_with("temp_audio_")
                    || name.starts_with("temp_video_");
                if is_temp && path.is_file() && std::fs::remove_file(&path).is_ok() {
                    count += 1;
                }
            }
        }
        count
    }

    /// Apaga as dependências baixadas (yt-dlp/ffmpeg/pdfium/whisper) para reinstalar.
    pub fn reinstall_dependencies(&mut self) {
        let libs = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("LumenDownloader")
            .join("libs");
        let _ = std::fs::remove_dir_all(&libs);
        // Reinicia o motor para baixar tudo de novo.
        self.engine = None;
        *self.engine_holder.lock().unwrap() = None;
        self.engine_spawned.store(false, Ordering::Relaxed);
    }

    /// Move downloads com mais de `days` dias para uma subpasta "Arquivo".
    /// Retorna quantos arquivos foram movidos.
    pub fn archive_old(&self, days: i64) -> usize {
        let dest = self.config.default_download_dir.join("Arquivo");
        std::fs::create_dir_all(&dest).ok();
        let cutoff = chrono::Local::now().naive_local()
            - chrono::Duration::days(days);
        let mut moved = 0;
        for entry in self.db.all_active_history() {
            let created = chrono::NaiveDateTime::parse_from_str(
                &entry.created_at,
                "%Y-%m-%d %H:%M:%S",
            );
            let old_enough = created.map(|c| c < cutoff).unwrap_or(false);
            let src = std::path::PathBuf::from(&entry.file_path);
            if old_enough && src.is_file() {
                if let Some(name) = src.file_name() {
                    let target = dest.join(name);
                    if std::fs::rename(&src, &target).is_ok()
                        || (std::fs::copy(&src, &target).is_ok()
                            && std::fs::remove_file(&src).is_ok())
                    {
                        self.db
                            .update_file_path(entry.id, &target.to_string_lossy());
                        moved += 1;
                    }
                }
            }
        }
        moved
    }

    /// Adiciona um toast (notificação in-app).
    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.push_toast(text.into(), error, None);
    }

    /// Toast com botão "Desfazer" que restaura um item do histórico.
    pub fn toast_undo(&mut self, text: impl Into<String>, history_id: i64) {
        self.push_toast(text.into(), false, Some(history_id));
    }

    fn push_toast(&mut self, text: String, error: bool, undo: Option<i64>) {
        self.toasts.push(Toast {
            text,
            error,
            created: std::time::Instant::now(),
            undo,
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    /// Solicita a miniatura de um vídeo (gera com ffmpeg em background, com cache em disco).
    pub fn request_thumb(&mut self, file_path: &str) {
        if self.thumb_textures.contains_key(file_path)
            || self.thumb_inflight.contains(file_path)
        {
            return;
        }
        let src = std::path::Path::new(file_path);
        if !src.is_file() {
            return;
        }
        // Caminho de cache: data/thumbs/<hash>.jpg (inclui mtime p/ invalidar).
        let mtime = std::fs::metadata(src)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        file_path.hash(&mut hasher);
        mtime.hash(&mut hasher);
        let jpg = Self::thumb_dir().join(format!("{:x}.jpg", hasher.finish()));

        self.thumb_inflight.insert(file_path.to_string());

        // Já existe em cache: marca como pronto direto.
        if jpg.exists() {
            self.thumb_ready
                .lock()
                .unwrap()
                .push((file_path.to_string(), jpg));
            return;
        }

        let Some(engine) = self.engine.clone() else {
            return;
        };
        let ready = self.thumb_ready.clone();
        let fp = file_path.to_string();
        tokio::spawn(async move {
            if engine.generate_thumbnail(&fp, &jpg).await.is_ok() {
                ready.lock().unwrap().push((fp, jpg));
            }
        });
    }

    fn queue_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LumenDownloader")
            .join("queue.json")
    }

    fn thumb_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LumenDownloader")
            .join("thumbs")
    }

    /// Carrega as miniaturas geradas em texturas (algumas por frame).
    fn load_ready_thumbs(&mut self, ctx: &egui::Context) {
        let ready: Vec<(String, PathBuf)> = {
            let mut r = self.thumb_ready.lock().unwrap();
            if r.is_empty() {
                return;
            }
            let n = r.len().min(6);
            r.drain(..n).collect()
        };
        let more = !self.thumb_ready.lock().unwrap().is_empty();
        for (fp, jpg) in ready {
            if let Some(tex) = load_texture_from_file(ctx, &jpg) {
                self.thumb_textures.insert(fp.clone(), tex);
            }
            self.thumb_inflight.remove(&fp);
        }
        if more {
            ctx.request_repaint();
        }
    }

    /// (Re)gera a pré-visualização da marca d'água se os parâmetros mudaram.
    /// `sig` identifica vídeo+marca+posição+tamanho+opacidade.
    pub fn request_wm_preview(&mut self, sig: String) {
        if self.wm_preview_busy || sig == self.wm_preview_sig {
            return;
        }
        let (Some(video), wm) = (
            self.wm_preview_video.clone(),
            self.config.watermark_path.clone(),
        ) else {
            return;
        };
        if wm.trim().is_empty() {
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        self.wm_preview_sig = sig;
        self.wm_preview_busy = true;
        let out = Self::thumb_dir().join("wm_preview.jpg");
        let pos = self.config.watermark_pos.clone();
        let scale = self.config.watermark_scale;
        let opacity = self.config.watermark_opacity;
        let slot = self.wm_preview_ready.clone();
        tokio::spawn(async move {
            let r = engine
                .watermark_preview(&video.to_string_lossy(), &wm, &out, &pos, scale, opacity)
                .await;
            *slot.lock().unwrap() = r.ok();
        });
    }

    /// Carrega a pré-visualização da marca d'água gerada em background.
    fn load_wm_preview(&mut self, ctx: &egui::Context) {
        let ready = self.wm_preview_ready.lock().unwrap().take();
        if let Some(jpg) = ready {
            self.wm_preview_tex = load_texture_from_file(ctx, &jpg);
            self.wm_preview_busy = false;
            ctx.request_repaint();
        }
    }

    /// Abre o editor de tags ID3 para um arquivo de áudio.
    pub fn open_tag_editor(&mut self, path: String) {
        let t = crate::download::engine::read_audio_tags(&path);
        self.tag_editor = Some(TagEditState {
            path,
            t,
            detecting: false,
        });
    }

    /// Grava as tags editadas no arquivo.
    pub fn save_tags(&mut self) {
        if let Some(ed) = &self.tag_editor {
            match crate::download::engine::write_audio_tags(&ed.path, &ed.t) {
                Ok(_) => self.toast("🏷 Tags salvas", false),
                Err(e) => self.toast(&format!("Falha ao salvar tags: {}", e), true),
            }
        }
        self.tag_editor = None;
    }

    /// Detecta o BPM do arquivo em edição (em background).
    pub fn detect_bpm_editor(&mut self) {
        let Some(ed) = self.tag_editor.as_mut() else {
            return;
        };
        let path = ed.path.clone();
        let Some(eng) = self.engine.clone() else {
            return;
        };
        ed.detecting = true;
        let slot = self.bpm_result.clone();
        tokio::spawn(async move {
            let r = eng.detect_bpm(&path).await.unwrap_or(0);
            *slot.lock().unwrap() = Some(r);
        });
    }

    fn load_bpm_result(&mut self) {
        let r = self.bpm_result.lock().unwrap().take();
        if let Some(bpm) = r {
            if let Some(ed) = self.tag_editor.as_mut() {
                ed.detecting = false;
                if bpm > 0 {
                    ed.t.bpm = bpm.to_string();
                }
            }
            if bpm == 0 {
                self.toast("Não foi possível detectar o BPM.", true);
            }
        }
    }

    /// Exporta uma playlist .m3u8 com os itens informados (título, caminho).
    pub fn export_playlist(&mut self, entries: Vec<(String, String)>) {
        if entries.is_empty() {
            self.toast("Nada para exportar.", true);
            return;
        }
        let Some(mut out) = rfd::FileDialog::new()
            .add_filter("Playlist", &["m3u8", "m3u"])
            .set_file_name("playlist.m3u8")
            .save_file()
        else {
            return;
        };
        if out.extension().is_none() {
            out.set_extension("m3u8");
        }
        let mut s = String::from("#EXTM3U\n");
        for (title, path) in &entries {
            s.push_str(&format!("#EXTINF:-1,{}\n{}\n", title, path));
        }
        match std::fs::write(&out, s) {
            Ok(_) => self.toast("Playlist exportada.", false),
            Err(e) => self.toast(&format!("Falha ao exportar: {}", e), true),
        }
    }

    /// Atualiza (em background) as versões/estado das dependências.
    pub fn refresh_deps(&self) {
        let Some(eng) = self.engine.clone() else {
            return;
        };
        let slot = self.deps_status.clone();
        tokio::spawn(async move {
            let rows = eng.dependency_status().await;
            *slot.lock().unwrap() = rows;
        });
    }

    /// Lê e exibe os metadados de um arquivo de mídia (janela de informação).
    pub fn show_metadata(&mut self, file: String) {
        let title = std::path::Path::new(&file)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Arquivo".to_string());
        *self.info_window.lock().unwrap() = Some((title.clone(), "...".to_string()));
        let engine = self.engine.clone();
        let win = self.info_window.clone();
        tokio::spawn(async move {
            let Some(eng) = engine else { return };
            let body = match eng.probe_metadata(&file).await {
                Ok(t) => t,
                Err(e) => format!("Erro: {}", e),
            };
            *win.lock().unwrap() = Some((title, body));
        });
    }

    /// Exporta uma lista de arquivos para um .zip escolhido pelo usuário.
    pub fn export_zip(&mut self, files: Vec<std::path::PathBuf>) {
        let pt = self.config.lang == crate::ui::i18n::Lang::Pt;
        let files: Vec<std::path::PathBuf> = files.into_iter().filter(|p| p.is_file()).collect();
        if files.is_empty() {
            self.toast(if pt { "Nada para exportar." } else { "Nothing to export." }, true);
            return;
        }
        let Some(dest) = rfd::FileDialog::new()
            .add_filter("Zip", &["zip"])
            .set_file_name("lumen_export.zip")
            .save_file()
        else {
            return;
        };
        let q = self.toast_queue.clone();
        self.toast(if pt { "Exportando .zip..." } else { "Exporting .zip..." }, false);
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            let res = (|| -> std::io::Result<usize> {
                let f = std::fs::File::create(&dest)?;
                let mut zip = zip::ZipWriter::new(f);
                let opts = zip::write::SimpleFileOptions::default();
                let mut n = 0usize;
                for p in &files {
                    let bytes = match std::fs::read(p) {
                        Ok(b) => b,
                        Err(_) => continue,
                    };
                    let name = p
                        .file_name()
                        .map(|x| x.to_string_lossy().to_string())
                        .unwrap_or_else(|| format!("arquivo_{}", n));
                    zip.start_file(name, opts)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                    zip.write_all(&bytes)?;
                    n += 1;
                }
                zip.finish()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                Ok(n)
            })();
            let (msg, err) = match res {
                Ok(n) => (
                    if pt {
                        format!("✔ {} arquivo(s) exportado(s)", n)
                    } else {
                        format!("✔ {} file(s) exported", n)
                    },
                    false,
                ),
                Err(e) => (format!("✖ {}", e), true),
            };
            q.lock().unwrap().push((msg, err));
        });
    }

    /// Verifica a integridade de um arquivo em segundo plano (toast com o resultado).
    pub fn verify_file(&mut self, path: String) {
        let pt = self.config.lang == crate::ui::i18n::Lang::Pt;
        let engine = self.engine.clone();
        let q = self.toast_queue.clone();
        self.toast(if pt { "Verificando integridade..." } else { "Verifying integrity..." }, false);
        tokio::spawn(async move {
            let Some(eng) = engine else { return };
            let (msg, err) = match eng.verify_integrity(&path).await {
                Ok(()) => (
                    (if pt { "✔ Arquivo íntegro" } else { "✔ File OK" }).to_string(),
                    false,
                ),
                Err(e) => (
                    format!("{} {}", if pt { "✖ Problema:" } else { "✖ Issue:" }, e),
                    true,
                ),
            };
            q.lock().unwrap().push((msg, err));
        });
    }

    /// Expira toasts antigos e dispara toasts ao concluir/falhar a operação atual.
    fn update_toasts(&mut self) {
        // Drena toasts vindos de tarefas em segundo plano.
        let queued: Vec<(String, bool)> = {
            let mut q = self.toast_queue.lock().unwrap();
            std::mem::take(&mut *q)
        };
        for (text, err) in queued {
            self.toast(text, err);
        }

        self.toasts
            .retain(|t| t.created.elapsed().as_secs_f32() < 4.0);

        let phase = { self.operation.lock().unwrap().phase.clone() };
        match phase {
            DownloadPhase::Completed(ref p) => {
                let key = format!("ok:{}", p);
                if self.last_signaled.as_deref() != Some(key.as_str()) {
                    self.last_signaled = Some(key);
                    let name = std::path::Path::new(p)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.toast(format!("✔ Concluído: {}", name), false);
                }
            }
            DownloadPhase::Failed(ref e) => {
                let key = format!("err:{}", e);
                if self.last_signaled.as_deref() != Some(key.as_str()) {
                    self.last_signaled = Some(key);
                    self.toast(format!("✖ Falhou: {}", e), true);
                }
            }
            _ => self.last_signaled = None,
        }
    }

    /// Cancela o download/processamento em andamento e volta ao estado ocioso.
    pub fn cancel_operation(&mut self) {
        if let Some(handle) = self.download_task.take() {
            handle.abort();
        }
        let mut op = self.operation.lock().unwrap();
        op.phase = DownloadPhase::Idle;
    }

    /// Inicia o fluxo de download de uma URL (busca info → tela de configuração).
    /// Usado pelas abas Música/Vídeo e pela ação "baixar de novo" do histórico.
    pub fn start_url_download(&mut self, url: String, media_type: MediaType) {
        let url = url.trim().to_string();
        if url.is_empty() {
            return;
        }
        if !crate::download::engine::looks_like_url(&url) {
            let mut op = self.operation.lock().unwrap();
            op.phase = DownloadPhase::Failed(
                "URL inválida. Cole um link válido (ex.: https://...)".to_string(),
            );
            return;
        }

        let format = match media_type {
            MediaType::Music => self.config.music_format.clone(),
            MediaType::Video => self.config.video_format.clone(),
            MediaType::Convert => return, // conversão não usa este fluxo
        };

        // Guarda para "repetir último download".
        self.last_download = Some((url.clone(), media_type));

        {
            let mut op = self.operation.lock().unwrap();
            op.phase = DownloadPhase::Fetching;
            op.url = url.clone();
            op.media_type = media_type;
            op.output_format = format;
            op.quality = self.config.quality.clone();
            op.folder_path = self.config.default_download_dir.clone();
            op.create_subfolder = false;
            op.progress = None;
            op.preview = None;
        }

        let op_ref = self.operation.clone();
        let engine = self.engine.clone();
        let template = self.config.filename_template.clone();
        let smart = self.config.smart_rename;
        self.download_task = Some(tokio::spawn(async move {
            match engine {
                Some(ref eng) => {
                    // Resolve fontes especiais (ex.: Spotify → busca no YouTube).
                    let url = eng.resolve_source(&url).await;
                    op_ref.lock().unwrap().url = url.clone();
                    match eng.fetch_preview(&url).await {
                    Ok(preview) => {
                        let mut op = op_ref.lock().unwrap();
                        op.title = preview.title.clone();
                        let clean_title = if smart {
                            crate::download::engine::smart_clean_name(&preview.title)
                        } else {
                            preview.title.clone()
                        };
                        let base = crate::download::engine::apply_template(
                            &template,
                            &clean_title,
                            &preview.channel,
                        );
                        let safe = crate::download::engine::sanitize_filename(&base);
                        op.file_name = format!("{}.{}", safe, op.output_format);
                        op.preview = Some(preview);
                        op.phase = DownloadPhase::Configuring;
                    }
                    Err(e) => {
                        let mut op = op_ref.lock().unwrap();
                        op.phase = DownloadPhase::Failed(
                            crate::download::engine::friendly_error(&e.to_string()),
                        );
                    }
                    }
                }
                None => {
                    let mut op = op_ref.lock().unwrap();
                    op.phase = DownloadPhase::Failed("Engine não inicializado".to_string());
                }
            }
        }));
    }

    /// Procura arquivos de mídia na pasta de download que não estão no histórico.
    pub fn find_orphans(&mut self) {
        let known: std::collections::HashSet<String> = self
            .db
            .all_active_history()
            .iter()
            .chain(self.db.get_deleted_history("music", 9999).iter())
            .map(|e| e.file_path.to_lowercase())
            .collect();
        let exts = [
            "mp4", "mkv", "webm", "avi", "mov", "mp3", "m4a", "flac", "opus", "ogg", "wav",
            "aac", "jpg", "jpeg", "png", "webp", "gif", "pdf", "txt",
        ];
        let mut found = Vec::new();
        let dir = self.config.default_download_dir.clone();
        let mut stack = vec![dir];
        let mut depth = 0;
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    if depth < 2 {
                        stack.push(p);
                    }
                } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    if exts.contains(&ext.to_lowercase().as_str())
                        && !known.contains(&p.to_string_lossy().to_lowercase())
                    {
                        found.push(p);
                    }
                }
            }
            depth += 1;
        }
        found.sort();
        found.truncate(500);
        self.orphans = Some(found);
    }

    /// Repete o último download (mesmo link e tipo).
    pub fn repeat_last_download(&mut self) {
        if let Some((url, mt)) = self.last_download.clone() {
            self.start_url_download(url, mt);
        } else {
            self.toast("Nenhum download recente para repetir.", true);
        }
    }

    /// Abre o inspetor de formatos para um link (lista resoluções/codecs/tamanhos).
    pub fn start_inspect(&mut self, url: String) {
        let url = url.trim().to_string();
        {
            let mut i = self.inspector.lock().unwrap();
            i.open = true;
            i.url = url.clone();
            i.rows.clear();
            i.error = None;
            if !crate::download::engine::looks_like_url(&url) {
                i.loading = false;
                i.error = Some("URL inválida.".to_string());
                return;
            }
            i.loading = true;
        }
        let engine = self.engine.clone();
        let insp = self.inspector.clone();
        tokio::spawn(async move {
            let Some(eng) = engine else {
                let mut i = insp.lock().unwrap();
                i.loading = false;
                i.error = Some("Engine não inicializado".to_string());
                return;
            };
            match eng.list_formats(&url).await {
                Ok(rows) => {
                    let mut i = insp.lock().unwrap();
                    i.rows = rows;
                    i.loading = false;
                }
                Err(e) => {
                    let mut i = insp.lock().unwrap();
                    i.error = Some(crate::download::engine::friendly_error(&e.to_string()));
                    i.loading = false;
                }
            }
        });
    }

    /// Baixa apenas a miniatura/capa de um link.
    pub fn start_url_thumbnail(&mut self, url: String) {
        let url = url.trim().to_string();
        if url.is_empty() {
            return;
        }
        if !crate::download::engine::looks_like_url(&url) {
            self.operation.lock().unwrap().phase =
                DownloadPhase::Failed("URL inválida.".to_string());
            return;
        }
        {
            let mut op = self.operation.lock().unwrap();
            op.phase = DownloadPhase::Downloading("Baixando miniatura...".to_string());
            op.progress = None;
            op.preview = None;
        }
        let op_ref = self.operation.clone();
        let engine = self.engine.clone();
        let folder = self.config.default_download_dir.clone();
        self.download_task = Some(tokio::spawn(async move {
            let Some(eng) = engine else {
                op_ref.lock().unwrap().phase =
                    DownloadPhase::Failed("Engine não inicializado".to_string());
                return;
            };
            match eng.download_thumbnail_file(&url, &folder).await {
                Ok(p) => {
                    op_ref.lock().unwrap().phase =
                        DownloadPhase::Completed(p.to_string_lossy().to_string());
                }
                Err(e) => {
                    op_ref.lock().unwrap().phase = DownloadPhase::Failed(
                        crate::download::engine::friendly_error(&e.to_string()),
                    );
                }
            }
        }));
    }

    /// Baixa o áudio de um link e gera a transcrição (.txt), sem passar pelo modal.
    pub fn start_url_transcribe(&mut self, url: String, media_type: MediaType) {
        let url = url.trim().to_string();
        if url.is_empty() {
            return;
        }
        if !crate::download::engine::looks_like_url(&url) {
            let mut op = self.operation.lock().unwrap();
            op.phase = DownloadPhase::Failed(
                "URL inválida. Cole um link válido (ex.: https://...)".to_string(),
            );
            return;
        }

        {
            let mut op = self.operation.lock().unwrap();
            op.phase = DownloadPhase::Downloading("Transcrevendo: baixando áudio...".to_string());
            op.url = url.clone();
            op.media_type = media_type;
            op.progress = None;
            op.preview = None;
        }

        let op_ref = self.operation.clone();
        let engine = self.engine.clone();
        let folder = self.config.default_download_dir.clone();
        let db_path = self.config.db_path();
        let lang = self
            .config
            .sub_langs
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let notify = self.config.notify_on_complete;
        let translate = self.config.transcribe_translate;
        let mt_str = if media_type == MediaType::Music {
            "music"
        } else {
            "video"
        };

        self.download_task = Some(tokio::spawn(async move {
            let eng = match engine {
                Some(e) => e,
                None => {
                    op_ref.lock().unwrap().phase =
                        DownloadPhase::Failed("Engine não inicializado".to_string());
                    return;
                }
            };
            std::fs::create_dir_all(&folder).ok();

            let title = eng
                .fetch_info(&url)
                .await
                .unwrap_or_else(|_| "transcricao".to_string());
            op_ref.lock().unwrap().title = title.clone();

            // Baixa apenas o áudio (temporário) para transcrever.
            let safe = crate::download::engine::sanitize_filename(&title);
            let tmp = folder.join(format!("{}.transcribe.m4a", safe));
            let opts = crate::download::engine::DownloadOptions {
                is_audio: true,
                format: "m4a".to_string(),
                ..Default::default()
            };
            let audio = match eng
                .fetch_and_download(&url, &tmp.to_string_lossy(), opts, |_| {})
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    op_ref.lock().unwrap().phase = DownloadPhase::Failed(
                        crate::download::engine::friendly_error(&e.to_string()),
                    );
                    return;
                }
            };

            op_ref.lock().unwrap().phase =
                DownloadPhase::Downloading("Transcrevendo com o Whisper...".to_string());

            let result = eng
                .transcribe(&audio.to_string_lossy(), &lang, translate)
                .await;
            let _ = std::fs::remove_file(&audio);

            match result {
                Ok(txt) => {
                    let db = crate::db::database::Database::open(&db_path);
                    let size = std::fs::metadata(&txt).ok().map(|m| m.len() as i64);
                    db.add_history(
                        &url,
                        &title,
                        mt_str,
                        "txt",
                        "",
                        &folder.to_string_lossy(),
                        &txt.to_string_lossy(),
                        size,
                    );
                    op_ref.lock().unwrap().phase =
                        DownloadPhase::Completed(txt.to_string_lossy().to_string());
                    if notify {
                        crate::notify::send("Transcrição concluída", &title);
                    }
                }
                Err(e) => {
                    op_ref.lock().unwrap().phase = DownloadPhase::Failed(
                        crate::download::engine::friendly_error(&e.to_string()),
                    );
                }
            }
        }));
    }

    pub fn ensure_engine(&mut self) {
        if self.engine.is_some() {
            return;
        }

        if !self.engine_spawned.load(Ordering::Relaxed) {
            self.engine_spawned.store(true, Ordering::Relaxed);
            let output_dir = self.config.default_download_dir.clone();
            let holder = self.engine_holder.clone();

            tokio::spawn(async move {
                match DownloadEngine::new(output_dir).await {
                    Ok(eng) => {
                        let mut e = holder.lock().unwrap();
                        *e = Some(eng);
                    }
                    Err(e) => {
                        eprintln!("Engine init error: {}", e);
                    }
                }
            });
        }

        if let Some(engine) = self.engine_holder.lock().unwrap().take() {
            self.engine = Some(Arc::new(engine));
        }
    }

    pub fn setup_style(ctx: &egui::Context) {
        crate::ui::theme::apply(ctx);
    }

    fn render_loading(&self, ctx: &egui::Context) {
        use crate::ui::theme;
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::bg_app()))
            .show(ctx, |ui: &mut egui::Ui| {
                ui.add_space(ui.available_height() * 0.38);
                ui.vertical_centered(|ui: &mut egui::Ui| {
                    ui.label(
                        egui::RichText::new("◆ Lumen")
                            .size(34.0)
                            .strong()
                            .color(theme::accent()),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Inicializando o motor de download")
                            .size(15.0)
                            .color(theme::text()),
                    );
                    ui.label(
                        egui::RichText::new("Preparando yt-dlp + ffmpeg (apenas na primeira vez)")
                            .size(13.0)
                            .color(theme::text_muted()),
                    );
                    ui.add_space(16.0);
                    ui.add(egui::Spinner::new().color(theme::accent()).size(28.0));
                });
            });
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        if !self.style_set {
            crate::ui::theme::set_light(self.config.theme == crate::config::settings::Theme::Light);
            crate::ui::theme::set_high_contrast(self.config.high_contrast);
            crate::ui::theme::set_compact(self.config.compact_ui);
            Self::setup_style(ctx);
            ctx.set_pixels_per_point(self.config.ui_scale.clamp(0.7, 2.0));
            self.style_set = true;
        }

        // Reaplica o estilo quando o tema é alterado por um comando.
        if self.restyle {
            self.restyle = false;
            crate::ui::theme::set_light(self.config.theme == crate::config::settings::Theme::Light);
            crate::ui::theme::set_high_contrast(self.config.high_contrast);
            crate::ui::theme::set_compact(self.config.compact_ui);
            Self::setup_style(ctx);
        }

        if self.engine.is_none() {
            self.ensure_engine();
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
            self.render_loading(ctx);
            return;
        }

        self.handle_shortcuts(ctx);
        self.handle_dropped_files(ctx);
        self.persist_window_size(ctx);
        self.check_clipboard();

        if !self.update_checked {
            self.update_checked = true;
            self.auto_update_ytdlp();
        }

        // Processa a fila de downloads.
        if let Some(engine) = self.engine.clone() {
            let db_path = self.config.db_path();
            let subs = if self.config.subtitles {
                Some(self.config.sub_langs.clone())
            } else {
                None
            };
            let rate = if self.config.rate_limit.trim().is_empty() {
                None
            } else {
                Some(self.config.rate_limit.clone())
            };
            let cloud = if self.config.copy_to_cloud
                && !self.config.cloud_folder.trim().is_empty()
            {
                Some(self.config.cloud_folder.clone())
            } else {
                None
            };
            self.queue.pump(
                engine,
                db_path,
                subs,
                self.config.notify_on_complete,
                rate,
                self.config.concurrent_fragments,
                self.config.organize_by.clone(),
                cloud,
                self.config.auto_retry,
            );
        }
        // Persiste a fila quando muda (para retomar na próxima sessão).
        let sig = self.queue.signature();
        if sig != self.queue_sig {
            self.queue_sig = sig;
            self.queue.save(&Self::queue_path());
        }

        // Lembra a última aba aberta.
        if self.active_tab.id() != self.config.last_tab {
            self.config.last_tab = self.active_tab.id().to_string();
            self.config.save();
        }

        self.load_ready_thumbs(ctx);
        self.load_wm_preview(ctx);
        self.load_bpm_result();
        self.update_toasts();
        if self.queue.has_active()
            || *self.update_status.lock().unwrap() == UpdateStatus::Running
            || !self.toasts.is_empty()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }

        dashboard::render(self, ctx);
    }

    /// Atualiza o yt-dlp automaticamente se faz mais de 7 dias da última vez.
    fn auto_update_ytdlp(&mut self) {
        const WEEK: i64 = 7 * 24 * 60 * 60;
        let now = chrono::Utc::now().timestamp();
        if now - self.config.last_ytdlp_update < WEEK {
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        // Marca otimisticamente para não tentar de novo a cada inicialização.
        self.config.last_ytdlp_update = now;
        self.config.save();

        let status = self.update_status.clone();
        *status.lock().unwrap() = UpdateStatus::Running;
        tokio::spawn(async move {
            let result = engine.update_ytdlp().await;
            let mut s = status.lock().unwrap();
            *s = match result {
                Ok(_) => UpdateStatus::Done("ok".to_string()),
                Err(e) => UpdateStatus::Error(e.to_string()),
            };
        });
    }

    /// Arrastar e soltar: um arquivo solto vai para a aba Lumen Converter.
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(path) = dropped.into_iter().next() {
            self.active_tab = Tab::Converter;
            crate::ui::converter_tab::configure_for_file(self, path);
        }
    }

    /// Atalhos de teclado globais.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let mut new_tab: Option<Tab> = None;
        let mut escape = false;

        // Ctrl+K abre/fecha a paleta de comandos.
        let toggle_palette =
            ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::K));
        if toggle_palette {
            self.cmd_palette_open = !self.cmd_palette_open;
            self.cmd_query.clear();
        }
        // Com a paleta aberta, Esc fecha e os demais atalhos ficam suspensos.
        if self.cmd_palette_open {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.cmd_palette_open = false;
            }
            return;
        }

        // F11: alterna o modo quiosque (tela cheia).
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            self.fullscreen = !self.fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        }

        // Ctrl+Tab / Ctrl+Shift+Tab: cicla entre as abas (navegação por teclado).
        let (cycle_next, cycle_prev) = ctx.input(|i| {
            let t = i.modifiers.command && i.key_pressed(egui::Key::Tab);
            (t && !i.modifiers.shift, t && i.modifiers.shift)
        });
        if cycle_next || cycle_prev {
            const ORDER: [Tab; 12] = [
                Tab::Home,
                Tab::Music,
                Tab::Video,
                Tab::Converter,
                Tab::Queue,
                Tab::Folders,
                Tab::Gallery,
                Tab::Cloud,
                Tab::Stats,
                Tab::Achievements,
                Tab::Settings,
                Tab::Help,
            ];
            let cur = ORDER.iter().position(|t| *t == self.active_tab).unwrap_or(0);
            let n = ORDER.len();
            let next = if cycle_next {
                (cur + 1) % n
            } else {
                (cur + n - 1) % n
            };
            self.active_tab = ORDER[next];
        }

        ctx.input(|i| {
            escape = i.key_pressed(egui::Key::Escape);
            if i.modifiers.command {
                if i.key_pressed(egui::Key::Num1) {
                    new_tab = Some(Tab::Home);
                } else if i.key_pressed(egui::Key::Num2) {
                    new_tab = Some(Tab::Music);
                } else if i.key_pressed(egui::Key::Num3) {
                    new_tab = Some(Tab::Video);
                } else if i.key_pressed(egui::Key::Num4) {
                    new_tab = Some(Tab::Converter);
                } else if i.key_pressed(egui::Key::Num5) {
                    new_tab = Some(Tab::Queue);
                } else if i.key_pressed(egui::Key::Num6) {
                    new_tab = Some(Tab::Folders);
                } else if i.key_pressed(egui::Key::Num7) {
                    new_tab = Some(Tab::Settings);
                }
            }
        });

        if let Some(tab) = new_tab {
            self.active_tab = tab;
        }

        if escape {
            let phase = self.operation.lock().unwrap().phase.clone();
            match phase {
                DownloadPhase::Fetching | DownloadPhase::Downloading(_) => self.cancel_operation(),
                DownloadPhase::Idle => {}
                _ => {
                    self.operation.lock().unwrap().phase = DownloadPhase::Idle;
                }
            }
        }
    }

    /// Observa a área de transferência e sugere baixar um link recém-copiado.
    fn check_clipboard(&mut self) {
        if self.clip_last_check.elapsed().as_millis() < 1200 {
            return;
        }
        self.clip_last_check = std::time::Instant::now();
        if let Some(text) = crate::ui::theme::paste_clipboard() {
            let t = text.trim().to_string();
            if t != self.clip_seen {
                self.clip_seen = t.clone();
                if crate::download::engine::is_valid_url(&t)
                    && t != self.music_url
                    && t != self.video_url
                {
                    self.clip_suggest = Some(t);
                }
            }
        }
    }

    /// Persiste o tamanho da janela (salva no disco quando o redimensionamento termina).
    fn persist_window_size(&mut self, ctx: &egui::Context) {
        let size = ctx.input(|i| i.viewport().inner_rect.map(|r| r.size()));
        if let Some(sz) = size {
            if sz.x > 100.0 && sz.y > 100.0 {
                if (sz.x - self.config.win_w).abs() > 1.0
                    || (sz.y - self.config.win_h).abs() > 1.0
                {
                    self.config.win_w = sz.x;
                    self.config.win_h = sz.y;
                    self.win_dirty = true;
                }
            }
        }
        // Salva uma vez quando o usuário solta o mouse (fim do arraste).
        if self.win_dirty && !ctx.input(|i| i.pointer.any_down()) {
            self.config.save();
            self.win_dirty = false;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update(ctx);
    }
}

/// Gera uma textura egui com o QR code de um texto (ex.: URL de origem).
pub fn make_qr_texture(ctx: &egui::Context, data: &str) -> Option<egui::TextureHandle> {
    let code = qrcode::QrCode::new(data.as_bytes()).ok()?;
    let modules = code.width();
    let colors = code.to_colors();
    let quiet = 4usize;
    let scale = 6usize;
    let dim = (modules + quiet * 2) * scale;
    let mut rgba = vec![255u8; dim * dim * 4]; // fundo branco
    for y in 0..modules {
        for x in 0..modules {
            if colors[y * modules + x] == qrcode::Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = (x + quiet) * scale + dx;
                        let py = (y + quiet) * scale + dy;
                        let idx = (py * dim + px) * 4;
                        rgba[idx] = 0;
                        rgba[idx + 1] = 0;
                        rgba[idx + 2] = 0;
                        rgba[idx + 3] = 255;
                    }
                }
            }
        }
    }
    let img = egui::ColorImage::from_rgba_unmultiplied([dim, dim], &rgba);
    Some(ctx.load_texture("qr_code", img, egui::TextureOptions::NEAREST))
}

/// Carrega o logo (logo + nome) embutido como textura egui.
pub fn load_brand_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let bytes = include_bytes!("../assets/FULL LOGO LUMEN DOWLOADER PNG.png");
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img);
    Some(ctx.load_texture("brand_logo", color, egui::TextureOptions::LINEAR))
}

/// Carrega uma imagem do disco como textura egui (reduzida).
pub fn load_texture_from_file(
    ctx: &egui::Context,
    path: &std::path::Path,
) -> Option<egui::TextureHandle> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let img = img.thumbnail(200, 200);
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
    Some(ctx.load_texture(path.to_string_lossy(), color, egui::TextureOptions::LINEAR))
}
