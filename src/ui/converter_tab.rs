use crate::app::{App, DownloadPhase, MediaType};
use crate::config::settings::ConvertEngine;
use crate::ui::theme;

pub fn render(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    let s = crate::ui::i18n::s(app.config.lang);

    ui.label(
        egui::RichText::new(s.conv_title)
            .color(theme::text())
            .size(30.0)
            .strong(),
    );
    ui.label(
        egui::RichText::new(s.conv_subtitle)
            .color(theme::text_muted())
            .size(14.0),
    );
    ui.add_space(20.0);

    let mut pick = false;
    let mut multi_pdf = false;
    let mut batch_pick = false;
    let mut frames = false;
    let mut transcribe = false;
    theme::card_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(s.conv_source)
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(s.conv_pick_hint)
                    .color(theme::text_faint())
                    .size(13.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let btn = theme::accent_button(s.conv_pick).min_size(egui::vec2(180.0, 40.0));
                if ui.add(btn).clicked() {
                    pick = true;
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.add(theme::ghost_button("📚  Imagens → PDF")).clicked() {
                multi_pdf = true;
            }
            if ui.add(theme::ghost_button(s.batch_convert)).clicked() {
                batch_pick = true;
            }
            if ui.add(theme::ghost_button(s.extract_frames)).clicked() {
                frames = true;
            }
            if ui.add(theme::ghost_button(s.transcribe)).clicked() {
                transcribe = true;
            }
        });
    });
    if pick {
        pick_and_configure(app);
    }
    if multi_pdf {
        images_to_pdf_flow(app);
    }
    if batch_pick {
        pick_batch(app);
    }
    if frames {
        extract_frames_flow(app);
    }
    if transcribe {
        transcribe_flow(app);
    }

    ui.add_space(16.0);
    watermark_card(app, ui);

    ui.add_space(16.0);
    pdf_card(app, ui);

    ui.add_space(16.0);
    image_batch_card(app, ui);

    if !app.batch_convert.is_empty() {
        ui.add_space(12.0);
        batch_panel(app, ui, s);
    }

    ui.add_space(20.0);

    let history = app.db.get_history("convert", app.config.max_history);
    crate::ui::history::render(
        app,
        ui,
        "convert",
        s.col_file,
        s.hist_conversions,
        &history,
        None,
    );
}

fn pick_and_configure(app: &mut App) {
    if let Some(picked) = rfd::FileDialog::new().pick_file() {
        configure_for_file(app, picked);
    }
}

fn pick_batch(app: &mut App) {
    use crate::download::engine::{categorize, output_formats};
    let Some(files) = rfd::FileDialog::new().pick_files() else {
        return;
    };
    if files.is_empty() {
        return;
    }
    let formats = output_formats(categorize(&files[0]));
    if formats.is_empty() {
        let mut op = app.operation.lock().unwrap();
        op.phase = DownloadPhase::Failed("Tipo de arquivo não suportado para lote.".to_string());
        return;
    }
    let src_ext = files[0]
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    app.batch_convert_format = formats
        .iter()
        .find(|f| **f != src_ext)
        .copied()
        .unwrap_or(formats[0])
        .to_string();
    app.batch_convert = files;
}

fn batch_panel(app: &mut App, ui: &mut egui::Ui, s: &crate::ui::i18n::Strings) {
    use crate::download::engine::{categorize, output_formats, FileCategory};
    let category = categorize(&app.batch_convert[0]);
    let formats = output_formats(category);
    let mut start = false;
    let mut cancel = false;

    theme::card_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(format!("{} ({})", s.batch_convert, app.batch_convert.len()))
                .color(theme::text())
                .strong(),
        );
        ui.add_space(6.0);
        ui.label(s.f_format);
        ui.horizontal_wrapped(|ui| {
            for f in &formats {
                let sel = app.batch_convert_format == *f;
                if ui
                    .add(egui::Button::new(*f).fill(if sel {
                        theme::accent()
                    } else {
                        theme::bg_card()
                    }))
                    .clicked()
                {
                    app.batch_convert_format = f.to_string();
                }
            }
        });
        if category == FileCategory::Office {
            ui.add_space(6.0);
            let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
            let eng = match app.config.convert_engine {
                ConvertEngine::Auto => if pt { "Automático" } else { "Automatic" },
                ConvertEngine::Rust => if pt { "Rust puro" } else { "Pure Rust" },
                ConvertEngine::LibreOffice => "LibreOffice",
                ConvertEngine::MsOffice => "MS Office",
            };
            ui.label(
                egui::RichText::new(if pt {
                    format!("⚙  Motor: {}  ·  troque em Configurações para mais fidelidade", eng)
                } else {
                    format!("⚙  Engine: {}  ·  change it in Settings for higher fidelity", eng)
                })
                .color(theme::text_faint())
                .size(11.0),
            );
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.add(theme::accent_button(s.batch_start)).clicked() {
                start = true;
            }
            if ui.add(theme::ghost_button(s.btn_cancel)).clicked() {
                cancel = true;
            }
        });
    });

    if cancel {
        app.batch_convert.clear();
    }
    if start {
        start_batch_convert(app);
    }
}

fn start_batch_convert(app: &mut App) {
    let files = std::mem::take(&mut app.batch_convert);
    let format = app.batch_convert_format.clone();
    let engine = app.engine.clone();
    let convert_engine = app.config.convert_engine;
    let op_state = app.operation.clone();
    let db = crate::db::database::Database::open(&app.config.db_path());

    {
        let mut op = op_state.lock().unwrap();
        op.phase = DownloadPhase::Downloading("Convertendo em lote...".to_string());
        op.progress = None;
    }

    app.download_task = Some(tokio::spawn(async move {
        let Some(eng) = engine else {
            op_state.lock().unwrap().phase =
                DownloadPhase::Failed("Engine não inicializado".to_string());
            return;
        };
        let total = files.len();
        let mut last_path = String::new();
        let mut errors = 0;
        for (i, file) in files.iter().enumerate() {
            {
                let mut op = op_state.lock().unwrap();
                op.phase =
                    DownloadPhase::Downloading(format!("Convertendo {}/{}...", i + 1, total));
            }
            let folder = file.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let stem = file
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "saida".to_string());
            let out = folder.join(format!("{}.{}", stem, format));
            match eng
                .convert_file(
                    &file.to_string_lossy(),
                    &out.to_string_lossy(),
                    &format,
                    "",
                    convert_engine,
                )
                .await
            {
                Ok(p) => {
                    let size = std::fs::metadata(&p).ok().map(|m| m.len() as i64);
                    db.add_history(
                        "",
                        &stem,
                        "convert",
                        &format,
                        "",
                        &folder.to_string_lossy(),
                        &p.to_string_lossy(),
                        size,
                    );
                    last_path = p.to_string_lossy().to_string();
                }
                Err(_) => errors += 1,
            }
        }
        let mut op = op_state.lock().unwrap();
        if errors == total {
            op.phase = DownloadPhase::Failed("Falha ao converter os arquivos.".to_string());
        } else {
            op.phase = DownloadPhase::Completed(last_path);
        }
    }));
}

fn images_to_pdf_flow(app: &mut App) {
    let Some(files) = rfd::FileDialog::new()
        .add_filter("Imagens", &["jpg", "jpeg", "png", "webp", "bmp", "tiff", "gif"])
        .pick_files()
    else {
        return;
    };
    if files.is_empty() {
        return;
    }
    let Some(mut out) = rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .set_file_name("imagens.pdf")
        .save_file()
    else {
        return;
    };
    out.set_extension("pdf");

    let engine = app.engine.clone();
    let op_state = app.operation.clone();
    let db = crate::db::database::Database::open(&app.config.db_path());
    let folder = out
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    let title = out
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "imagens.pdf".to_string());

    {
        let mut op = op_state.lock().unwrap();
        op.phase = DownloadPhase::Downloading("Gerando PDF...".to_string());
        op.progress = None;
    }

    app.download_task = Some(tokio::spawn(async move {
        match engine {
            Some(eng) => match eng.images_to_pdf_multi(files, &out).await {
                Ok(p) => {
                    let size = std::fs::metadata(&p).ok().map(|m| m.len() as i64);
                    db.add_history(
                        "",
                        &title,
                        "convert",
                        "pdf",
                        "",
                        &folder.to_string_lossy(),
                        &p.to_string_lossy(),
                        size,
                    );
                    op_state.lock().unwrap().phase =
                        DownloadPhase::Completed(p.to_string_lossy().to_string());
                }
                Err(e) => {
                    op_state.lock().unwrap().phase = DownloadPhase::Failed(e.to_string());
                }
            },
            None => {
                op_state.lock().unwrap().phase =
                    DownloadPhase::Failed("Engine não inicializado".to_string());
            }
        }
    }));
}

fn extract_frames_flow(app: &mut App) {
    let Some(file) = rfd::FileDialog::new()
        .add_filter("Vídeo", &["mp4", "mkv", "webm", "avi", "mov", "flv", "m4v"])
        .pick_file()
    else {
        return;
    };
    let engine = app.engine.clone();
    let op_state = app.operation.clone();
    let db = crate::db::database::Database::open(&app.config.db_path());
    let folder = file.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "frames".to_string());
    {
        let mut op = op_state.lock().unwrap();
        op.phase = DownloadPhase::Downloading("Extraindo frames...".to_string());
        op.progress = None;
    }
    app.download_task = Some(tokio::spawn(async move {
        let Some(eng) = engine else {
            op_state.lock().unwrap().phase =
                DownloadPhase::Failed("Engine não inicializado".to_string());
            return;
        };
        match eng.extract_frames(&file.to_string_lossy(), 1).await {
            Ok(dir) => {
                db.add_history(
                    "",
                    &stem,
                    "convert",
                    "png",
                    "",
                    &folder.to_string_lossy(),
                    &dir.to_string_lossy(),
                    None,
                );
                op_state.lock().unwrap().phase =
                    DownloadPhase::Completed(dir.to_string_lossy().to_string());
            }
            Err(e) => op_state.lock().unwrap().phase = DownloadPhase::Failed(e.to_string()),
        }
    }));
}

fn transcribe_flow(app: &mut App) {
    let Some(file) = rfd::FileDialog::new()
        .add_filter(
            "Áudio/Vídeo",
            &["mp3", "m4a", "wav", "flac", "ogg", "opus", "mp4", "mkv", "webm", "mov", "avi"],
        )
        .pick_file()
    else {
        return;
    };
    let engine = app.engine.clone();
    let op_state = app.operation.clone();
    let db = crate::db::database::Database::open(&app.config.db_path());
    let translate = app.config.transcribe_translate;
    let folder = file.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "transcricao".to_string());
    {
        let mut op = op_state.lock().unwrap();
        op.phase = DownloadPhase::Downloading("Transcrevendo (Whisper)...".to_string());
        op.progress = None;
    }
    app.download_task = Some(tokio::spawn(async move {
        let Some(eng) = engine else {
            op_state.lock().unwrap().phase =
                DownloadPhase::Failed("Engine não inicializado".to_string());
            return;
        };
        match eng.transcribe(&file.to_string_lossy(), "", translate).await {
            Ok(p) => {
                db.add_history(
                    "",
                    &stem,
                    "convert",
                    "txt",
                    "",
                    &folder.to_string_lossy(),
                    &p.to_string_lossy(),
                    None,
                );
                op_state.lock().unwrap().phase =
                    DownloadPhase::Completed(p.to_string_lossy().to_string());
            }
            Err(e) => op_state.lock().unwrap().phase = DownloadPhase::Failed(e.to_string()),
        }
    }));
}

pub fn configure_for_file(app: &mut App, picked: std::path::PathBuf) {
    use crate::download::engine::{categorize, output_formats};

    let stem = picked
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "arquivo".to_string());
    let folder = picked
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| app.config.default_download_dir.clone());
    let src_ext = picked
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let category = categorize(&picked);
    let formats = output_formats(category);

    if formats.is_empty() {
        let mut op = app.operation.lock().unwrap();
        op.phase = DownloadPhase::Failed(format!(
            "Tipo de arquivo \".{}\" não suportado para conversão.",
            src_ext
        ));
        return;
    }

    let default_format = formats
        .iter()
        .find(|f| **f != src_ext)
        .copied()
        .unwrap_or(formats[0]);

    let mut op = app.operation.lock().unwrap();
    op.media_type = MediaType::Convert;
    op.source_file = picked.clone();
    op.title = picked
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or(stem.clone());
    op.file_name = format!("{}.{}", stem, default_format);
    op.folder_path = folder;
    op.create_subfolder = false;
    op.subfolder_name = String::new();
    op.output_format = default_format.to_string();
    op.quality = String::from("best");
    op.url = String::new();
    op.phase = DownloadPhase::Configuring;
}

fn watermark_card(app: &mut App, ui: &mut egui::Ui) {
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let mut pick_img = false;
    let mut apply = false;
    let mut apply_batch = false;
    let mut changed = false;

    theme::card_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(if pt { "💧 Marca d'água" } else { "💧 Watermark" })
                .color(theme::text())
                .size(16.0)
                .strong(),
        );
        ui.label(
            egui::RichText::new(if pt {
                "Insira sua marca (imagem PNG/JPG) sobre um vídeo baixado."
            } else {
                "Overlay your mark (PNG/JPG image) onto a downloaded video."
            })
            .color(theme::text_muted())
            .size(12.0),
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.add(theme::ghost_button(if pt { "🖼 Escolher imagem" } else { "🖼 Choose image" })).clicked() {
                pick_img = true;
            }
            let name = std::path::Path::new(&app.config.watermark_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| if pt { "(nenhuma)".into() } else { "(none)".into() });
            ui.label(egui::RichText::new(name).color(theme::text_faint()).size(12.0));
        });

        if !app.config.watermark_path.trim().is_empty() {
            let key = std::path::PathBuf::from(&app.config.watermark_path);
            if !app.gallery_textures.contains_key(&key) {
                if let Some(tex) = crate::app::load_texture_from_file(ui.ctx(), &key) {
                    app.gallery_textures.insert(key.clone(), tex);
                }
            }
            if let Some(tex) = app.gallery_textures.get(&key) {
                let [w, h] = tex.size();
                let tw = 90.0;
                let th = (tw * h as f32 / w.max(1) as f32).min(90.0);
                ui.add_space(4.0);
                ui.add(egui::Image::from_texture((tex.id(), egui::vec2(tw, th))));
            }
        }
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label(if pt { "Posição" } else { "Position" });
            let opts = [
                ("tl", "↖"), ("tr", "↗"), ("center", "●"), ("bl", "↙"), ("br", "↘"),
            ];
            for (val, label) in opts {
                let sel = app.config.watermark_pos == val;
                let fill = if sel { theme::accent() } else { theme::bg_card() };
                if ui.add(egui::Button::new(label).fill(fill)).clicked() && !sel {
                    app.config.watermark_pos = val.to_string();
                    changed = true;
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(if pt { "Tamanho %" } else { "Size %" });
            let mut sc = app.config.watermark_scale;
            if ui.add(egui::Slider::new(&mut sc, 5..=200)).changed() {
                app.config.watermark_scale = sc;
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label(if pt { "Opacidade" } else { "Opacity" });
            let mut op = app.config.watermark_opacity;
            if ui.add(egui::Slider::new(&mut op, 0.1..=1.0)).changed() {
                app.config.watermark_opacity = op;
                changed = true;
            }
        });
        ui.add_space(8.0);

        ui.separator();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add(theme::accent_button(if pt { "🎬 Selecionar vídeo..." } else { "🎬 Select video..." }))
                .clicked()
            {
                if let Some(v) = rfd::FileDialog::new()
                    .add_filter("Vídeo", &["mp4", "mkv", "webm", "avi", "mov"])
                    .pick_file()
                {
                    app.wm_preview_video = Some(v);
                    app.wm_preview_sig.clear();
                    app.wm_preview_tex = None;
                }
            }
            if let Some(v) = &app.wm_preview_video {
                let name = v
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                ui.label(egui::RichText::new(name).color(theme::text_faint()).size(11.0));
            }
        });

        if app.wm_preview_video.is_some() && !app.config.watermark_path.trim().is_empty() {
            let sig = format!(
                "{}|{}|{}|{}|{}",
                app.wm_preview_video.as_ref().unwrap().to_string_lossy(),
                app.config.watermark_path,
                app.config.watermark_pos,
                app.config.watermark_scale,
                app.config.watermark_opacity,
            );
            if !ui.input(|i| i.pointer.any_down()) {
                app.request_wm_preview(sig);
            }
            ui.add_space(6.0);
            if let Some(tex) = &app.wm_preview_tex {
                let [w, h] = tex.size();
                let tw = (ui.available_width() - 2.0).min(w as f32);
                let th = tw * h as f32 / w.max(1) as f32;
                ui.add(egui::Image::from_texture((tex.id(), egui::vec2(tw, th))));
            }
            if app.wm_preview_busy {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(14.0));
                    ui.label(
                        egui::RichText::new(if pt { "Atualizando prévia..." } else { "Updating preview..." })
                            .color(theme::text_muted())
                            .size(11.0),
                    );
                });
            }
        } else {
            ui.label(
                egui::RichText::new(if pt {
                    "Selecione um vídeo para ver a marca aplicada e confirmar."
                } else {
                    "Select a video to preview the mark applied and confirm."
                })
                .color(theme::text_faint())
                .size(11.0),
            );
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        let has_wm = !app.config.watermark_path.trim().is_empty();
        let has_video = app.wm_preview_video.is_some();
        ui.horizontal(|ui| {
            let btn = theme::accent_button(if pt {
                "✅ Aplicar e salvar"
            } else {
                "✅ Apply & save"
            })
            .min_size(egui::vec2(160.0, 36.0));
            if ui.add_enabled(has_wm && has_video, btn).clicked() {
                apply = true;
            }
            let bbtn = theme::ghost_button(if pt {
                "Aplicar em vários..."
            } else {
                "Apply to several..."
            });
            if ui.add_enabled(has_wm, bbtn).clicked() {
                apply_batch = true;
            }
        });
        if has_wm && !has_video {
            ui.label(
                egui::RichText::new(if pt {
                    "Selecione um vídeo acima para habilitar."
                } else {
                    "Select a video above to enable."
                })
                .color(theme::text_faint())
                .size(11.0),
            );
        }
        if !has_wm {
            ui.label(
                egui::RichText::new(if pt {
                    "Escolha a imagem da marca primeiro."
                } else {
                    "Choose the mark image first."
                })
                .color(theme::text_faint())
                .size(11.0),
            );
        }
    });

    if pick_img {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Imagem", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
        {
            app.config.watermark_path = p.to_string_lossy().to_string();
            changed = true;
        }
    }
    if changed {
        app.config.save();
    }
    if apply {
        watermark_flow(app);
    }
    if apply_batch {
        watermark_batch_flow(app);
    }
}

fn watermark_batch_flow(app: &mut App) {
    let Some(videos) = rfd::FileDialog::new()
        .add_filter("Vídeo", &["mp4", "mkv", "webm", "avi", "mov"])
        .pick_files()
    else {
        return;
    };
    if videos.is_empty() {
        return;
    }
    let total = videos.len();
    let engine = app.engine.clone();
    let op_state = app.operation.clone();
    let db_path = app.config.db_path();
    let wm = app.config.watermark_path.clone();
    let pos = app.config.watermark_pos.clone();
    let scale = app.config.watermark_scale;
    let opacity = app.config.watermark_opacity;

    {
        let mut op = op_state.lock().unwrap();
        op.phase = DownloadPhase::Downloading(format!("Marca d'água (0/{})...", total));
        op.progress = None;
    }

    app.download_task = Some(tokio::spawn(async move {
        let Some(eng) = engine else {
            op_state.lock().unwrap().phase =
                DownloadPhase::Failed("Engine não inicializado".to_string());
            return;
        };
        let db = crate::db::database::Database::open(&db_path);
        let mut done = 0usize;
        let mut last_ok: Option<String> = None;
        let mut last_err: Option<String> = None;
        for (i, video) in videos.iter().enumerate() {
            op_state.lock().unwrap().phase =
                DownloadPhase::Downloading(format!("Marca d'água ({}/{})...", i + 1, total));
            let stem = video
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "video".to_string());
            let ext = video
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|| "mp4".to_string());
            let folder = video.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let out = folder.join(format!("{}_marca.{}", stem, ext));
            match eng
                .watermark_video(
                    &video.to_string_lossy(),
                    &wm,
                    &out.to_string_lossy(),
                    &pos,
                    scale,
                    opacity,
                )
                .await
            {
                Ok(p) => {
                    let size = std::fs::metadata(&p).ok().map(|m| m.len() as i64);
                    let title = out
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    db.add_history(
                        "",
                        &title,
                        "convert",
                        &ext,
                        "",
                        &folder.to_string_lossy(),
                        &p.to_string_lossy(),
                        size,
                    );
                    done += 1;
                    last_ok = Some(p.to_string_lossy().to_string());
                }
                Err(e) => last_err = Some(e.to_string()),
            }
        }
        let mut op = op_state.lock().unwrap();
        if done > 0 {
            op.phase = DownloadPhase::Completed(last_ok.unwrap_or_else(|| format!("{} vídeos", done)));
        } else {
            op.phase = DownloadPhase::Failed(
                last_err.unwrap_or_else(|| "nenhum vídeo processado".to_string()),
            );
        }
    }));
}

fn watermark_flow(app: &mut App) {
    let Some(video) = app.wm_preview_video.clone() else {
        return;
    };
    let stem = video
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".to_string());
    let ext = video
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "mp4".to_string());
    let folder = video.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let out = folder.join(format!("{}_marca.{}", stem, ext));

    let engine = app.engine.clone();
    let op_state = app.operation.clone();
    let db = crate::db::database::Database::open(&app.config.db_path());
    let wm = app.config.watermark_path.clone();
    let pos = app.config.watermark_pos.clone();
    let scale = app.config.watermark_scale;
    let opacity = app.config.watermark_opacity;
    let title = out
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let folder_s = folder.to_string_lossy().to_string();

    {
        let mut op = op_state.lock().unwrap();
        op.phase = DownloadPhase::Downloading("Aplicando marca d'água...".to_string());
        op.progress = None;
    }

    app.download_task = Some(tokio::spawn(async move {
        let Some(eng) = engine else {
            op_state.lock().unwrap().phase =
                DownloadPhase::Failed("Engine não inicializado".to_string());
            return;
        };
        match eng
            .watermark_video(
                &video.to_string_lossy(),
                &wm,
                &out.to_string_lossy(),
                &pos,
                scale,
                opacity,
            )
            .await
        {
            Ok(p) => {
                let size = std::fs::metadata(&p).ok().map(|m| m.len() as i64);
                db.add_history("", &title, "convert", &ext, "", &folder_s, &p.to_string_lossy(), size);
                op_state.lock().unwrap().phase =
                    DownloadPhase::Completed(p.to_string_lossy().to_string());
            }
            Err(e) => {
                op_state.lock().unwrap().phase = DownloadPhase::Failed(e.to_string());
            }
        }
    }));
}

fn image_batch_card(app: &mut App, ui: &mut egui::Ui) {
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let mut run = false;
    theme::card_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(if pt { "🖼 Imagens em lote" } else { "🖼 Batch images" })
                .color(theme::text())
                .size(16.0)
                .strong(),
        );
        ui.label(
            egui::RichText::new(if pt {
                "Converte/redimensiona/comprime várias imagens de uma vez (inclui HEIC)."
            } else {
                "Convert/resize/compress many images at once (HEIC included)."
            })
            .color(theme::text_muted())
            .size(12.0),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(if pt { "Formato" } else { "Format" });
            for f in ["jpg", "png", "webp"] {
                let sel = app.config.image_format == f;
                let fill = if sel { theme::accent() } else { theme::bg_card() };
                if ui.add(egui::Button::new(f).fill(fill)).clicked() {
                    app.config.image_format = f.to_string();
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(if pt { "Largura máx (0 = original)" } else { "Max width (0 = original)" });
            ui.add(egui::Slider::new(&mut app.config.image_max_width, 0..=3840));
        });
        ui.horizontal(|ui| {
            ui.label(if pt { "Qualidade" } else { "Quality" });
            ui.add(egui::Slider::new(&mut app.config.image_quality, 10..=100));
        });
        ui.add_space(8.0);
        if ui
            .add(theme::accent_button(if pt {
                "Selecionar imagens e converter..."
            } else {
                "Select images & convert..."
            }))
            .clicked()
        {
            run = true;
        }
    });
    if run {
        image_batch_flow(app);
    }
}

fn image_batch_flow(app: &mut App) {
    let Some(files) = rfd::FileDialog::new()
        .add_filter(
            "Imagens",
            &["jpg", "jpeg", "png", "webp", "bmp", "tiff", "gif", "heic", "heif"],
        )
        .pick_files()
    else {
        return;
    };
    if files.is_empty() {
        return;
    }
    app.config.save();
    let out_dir = files
        .first()
        .and_then(|f| f.parent())
        .map(|p| p.join("imagens_convertidas"))
        .unwrap_or_else(|| std::path::PathBuf::from("imagens_convertidas"));
    let engine = app.engine.clone();
    let op_state = app.operation.clone();
    let format = app.config.image_format.clone();
    let maxw = app.config.image_max_width;
    let quality = app.config.image_quality;
    let n = files.len();

    {
        let mut op = op_state.lock().unwrap();
        op.phase = DownloadPhase::Downloading(format!("Convertendo {} imagem(ns)...", n));
        op.progress = None;
    }
    app.download_task = Some(tokio::spawn(async move {
        let Some(eng) = engine else {
            op_state.lock().unwrap().phase =
                DownloadPhase::Failed("Engine não inicializado".to_string());
            return;
        };
        match eng
            .batch_convert_images(files, out_dir.clone(), format, maxw, quality)
            .await
        {
            Ok(p) => {
                op_state.lock().unwrap().phase =
                    DownloadPhase::Completed(p.to_string_lossy().to_string());
            }
            Err(e) => {
                op_state.lock().unwrap().phase = DownloadPhase::Failed(e.to_string());
            }
        }
    }));
}

fn pdf_card(app: &mut App, ui: &mut egui::Ui) {
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let mut merge = false;
    let mut split = false;
    let mut rotate: Option<i32> = None;
    let mut reorder = false;
    let mut compress = false;

    theme::card_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(if pt { "📄 Ferramentas de PDF" } else { "📄 PDF tools" })
                .color(theme::text())
                .size(16.0)
                .strong(),
        );
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.add(theme::ghost_button(if pt { "📎 Juntar PDFs" } else { "📎 Merge PDFs" })).clicked() {
                merge = true;
            }
            if ui.add(theme::ghost_button(if pt { "✂ Separar páginas" } else { "✂ Split pages" })).clicked() {
                split = true;
            }
            if ui.add(theme::ghost_button("↻ 90°")).clicked() {
                rotate = Some(90);
            }
            if ui.add(theme::ghost_button("↻ 180°")).clicked() {
                rotate = Some(180);
            }
            if ui.add(theme::ghost_button("↻ 270°")).clicked() {
                rotate = Some(270);
            }
            if ui
                .add(theme::ghost_button(if pt { "🔀 Reordenar" } else { "🔀 Reorder" }))
                .clicked()
            {
                reorder = true;
            }
            if ui
                .add(theme::ghost_button(if pt { "🗜 Comprimir" } else { "🗜 Compress" }))
                .clicked()
            {
                compress = true;
            }
        });
        ui.label(
            egui::RichText::new(if pt {
                "Comprimir rasteriza as páginas (bom p/ escaneados; o texto vira imagem)."
            } else {
                "Compress rasterizes pages (good for scans; text becomes image)."
            })
            .color(theme::text_faint())
            .size(11.0),
        );
    });

    if merge {
        pdf_merge_flow(app);
    }
    if split {
        pdf_split_flow(app);
    }
    if let Some(deg) = rotate {
        pdf_rotate_flow(app, deg);
    }
    if reorder {
        if let Some(input) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() {
            app.pdf_reorder = Some((input, String::new()));
        }
    }
    if compress {
        pdf_compress_flow(app);
    }

    let mut apply: Option<(std::path::PathBuf, String)> = None;
    let mut cancel = false;
    if let Some((path, order)) = app.pdf_reorder.as_mut() {
        egui::Window::new(if pt { "Reordenar páginas" } else { "Reorder pages" })
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.label(
                    egui::RichText::new(if pt {
                        "Nova ordem das páginas (ex.: 3,1,2 ou 2-5,1):"
                    } else {
                        "New page order (e.g. 3,1,2 or 2-5,1):"
                    })
                    .color(theme::text_muted())
                    .size(12.0),
                );
                ui.add_space(6.0);
                ui.add(egui::TextEdit::singleline(order).hint_text("3,1,2").desired_width(220.0));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::accent_button(if pt { "Aplicar" } else { "Apply" })).clicked()
                        && !order.trim().is_empty()
                    {
                        apply = Some((path.clone(), order.clone()));
                    }
                    if ui.add(theme::ghost_button(if pt { "Cancelar" } else { "Cancel" })).clicked() {
                        cancel = true;
                    }
                });
            });
    }
    if let Some((input, order)) = apply {
        app.pdf_reorder = None;
        let stem = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "pdf".to_string());
        let out = input
            .parent()
            .map(|p| p.join(format!("{}_reordenado.pdf", stem)))
            .unwrap_or_else(|| std::path::PathBuf::from("reordenado.pdf"));
        run_pdf_task(app, "Reordenando...", move |eng| {
            Box::pin(async move { eng.reorder_pdf(&input, &out, order).await })
        });
    }
    if cancel {
        app.pdf_reorder = None;
    }
}

fn pdf_compress_flow(app: &mut App) {
    let Some(input) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() else {
        return;
    };
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "pdf".to_string());
    let out = input
        .parent()
        .map(|p| p.join(format!("{}_comprimido.pdf", stem)))
        .unwrap_or_else(|| std::path::PathBuf::from("comprimido.pdf"));
    run_pdf_task(app, "Comprimindo PDF...", move |eng| {
        Box::pin(async move { eng.compress_pdf(&input, &out, 110.0).await })
    });
}

fn pdf_merge_flow(app: &mut App) {
    let Some(files) = rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .pick_files()
    else {
        return;
    };
    if files.len() < 2 {
        app.toast("Selecione 2 ou mais PDFs.", true);
        return;
    }
    let Some(mut out) = rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .set_file_name("juntado.pdf")
        .save_file()
    else {
        return;
    };
    out.set_extension("pdf");
    run_pdf_task(app, "Juntando PDFs...", move |eng| {
        Box::pin(async move { eng.merge_pdfs(files, &out).await })
    });
}

fn pdf_split_flow(app: &mut App) {
    let Some(input) = rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .pick_file()
    else {
        return;
    };
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "pdf".to_string());
    let folder = input
        .parent()
        .map(|p| p.join(format!("{}_paginas", stem)))
        .unwrap_or_else(|| std::path::PathBuf::from("paginas"));
    run_pdf_task(app, "Separando páginas...", move |eng| {
        Box::pin(async move { eng.split_pdf(&input, &folder).await })
    });
}

fn pdf_rotate_flow(app: &mut App, degrees: i32) {
    let Some(input) = rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .pick_file()
    else {
        return;
    };
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "pdf".to_string());
    let out = input
        .parent()
        .map(|p| p.join(format!("{}_rot{}.pdf", stem, degrees)))
        .unwrap_or_else(|| std::path::PathBuf::from("rotacionado.pdf"));
    run_pdf_task(app, "Rotacionando...", move |eng| {
        Box::pin(async move { eng.rotate_pdf(&input, &out, degrees).await })
    });
}

fn run_pdf_task<F>(app: &mut App, msg: &str, make: F)
where
    F: FnOnce(
            std::sync::Arc<crate::download::engine::DownloadEngine>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<std::path::PathBuf, Box<dyn std::error::Error>>> + Send>,
        > + Send
        + 'static,
{
    let engine = app.engine.clone();
    let op_state = app.operation.clone();
    {
        let mut op = op_state.lock().unwrap();
        op.phase = DownloadPhase::Downloading(msg.to_string());
        op.progress = None;
    }
    app.download_task = Some(tokio::spawn(async move {
        let Some(eng) = engine else {
            op_state.lock().unwrap().phase =
                DownloadPhase::Failed("Engine não inicializado".to_string());
            return;
        };
        match make(eng).await {
            Ok(p) => {
                op_state.lock().unwrap().phase =
                    DownloadPhase::Completed(p.to_string_lossy().to_string());
            }
            Err(e) => {
                op_state.lock().unwrap().phase = DownloadPhase::Failed(e.to_string());
            }
        }
    }));
}
