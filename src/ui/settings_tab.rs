use crate::app::{App, UpdateStatus};
use crate::config::settings::{ConvertEngine, Theme};
use crate::ui::i18n::Lang;
use crate::ui::theme;

pub fn render(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    let s = crate::ui::i18n::s(app.config.lang);

    theme::page_header(ui, s.settings_title, s.settings_subtitle);
    ui.add_space(20.0);

    let mut changed = false;

    let ncols = if ui.available_width() > 980.0 { 2 } else { 1 };
    let right_idx = if ncols == 2 { 1 } else { 0 };

    // Cada coluna flui por conta própria, e não em linhas casadas: um cartão
    // começa logo abaixo do anterior *da mesma coluna*. Alinhar por linhas faria
    // a coluna toda esperar pelo cartão mais alto da linha, abrindo um vão morto
    // embaixo do mais baixo (Idioma/Tema vs. Organizar).
    ui.columns(ncols, |cols| {
        let left = &mut cols[0];
        card_language_theme(left, app, &mut changed);
        left.add_space(16.0);
        card_accessibility(left, app, &mut changed);
        left.add_space(16.0);
        card_defaults(left, app, &mut changed);

        let right = &mut cols[right_idx];
        if ncols == 1 {
            right.add_space(16.0);
        }
        card_organize(right, app, &mut changed);
        right.add_space(16.0);
        card_convert_engine(right, app, &mut changed);
        right.add_space(16.0);
        card_maintenance(right, app, &mut changed);
    });

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);
    crate::ui::stats_tab::render(app, ctx, ui);

    ui.add_space(16.0);
    ui.label(
        egui::RichText::new(format!(
            "{}: {}",
            s.settings_saved_at,
            crate::config::settings::Config::config_path().to_string_lossy()
        ))
        .color(theme::text_faint())
        .size(12.0),
    );

    if changed {
        app.config.save();
    }
}

fn card_language_theme(ui: &mut egui::Ui, app: &mut App, changed: &mut bool) {
    let s = crate::ui::i18n::s(app.config.lang);
    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(s.settings_language);
            for (lang, label) in [(Lang::Pt, "Português"), (Lang::En, "English")] {
                let selected = app.config.lang == lang;
                let fill = if selected { theme::accent() } else { theme::bg_card() };
                if ui.add(egui::Button::new(label).fill(fill)).clicked() && !selected {
                    app.config.lang = lang;
                    *changed = true;
                }
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(s.settings_theme);
            for (th, label) in [(Theme::Dark, s.theme_dark), (Theme::Light, s.theme_light)] {
                let selected = app.config.theme == th;
                let fill = if selected { theme::accent() } else { theme::bg_card() };
                if ui.add(egui::Button::new(label).fill(fill)).clicked() && !selected {
                    app.config.theme = th;
                    theme::set_light(th == Theme::Light);
                    theme::apply(ui.ctx());
                    *changed = true;
                }
            }
        });
    });
}

fn card_accessibility(ui: &mut egui::Ui, app: &mut App, changed: &mut bool) {
    let pt = app.config.lang == Lang::Pt;
    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            egui::RichText::new(if pt { "Acessibilidade" } else { "Accessibility" })
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(if pt { "Escala da interface" } else { "UI scale" });
            let mut scale = app.config.ui_scale;
            if ui
                .add(egui::Slider::new(&mut scale, 0.8..=1.6).step_by(0.1))
                .changed()
            {
                app.config.ui_scale = scale;
                ui.ctx().set_pixels_per_point(scale);
                *changed = true;
            }
        });
        ui.add_space(6.0);
        let mut hc = app.config.high_contrast;
        if ui
            .checkbox(&mut hc, if pt { "Alto contraste" } else { "High contrast" })
            .changed()
        {
            app.config.high_contrast = hc;
            theme::set_high_contrast(hc);
            theme::apply(ui.ctx());
            *changed = true;
        }
        ui.add_space(6.0);
        let mut compact = app.config.compact_ui;
        if ui
            .checkbox(&mut compact, if pt { "Interface compacta" } else { "Compact interface" })
            .changed()
        {
            app.config.compact_ui = compact;
            theme::set_compact(compact);
            theme::apply(ui.ctx());
            *changed = true;
        }
        ui.add_space(6.0);
        if ui
            .checkbox(
                &mut app.config.confirm_delete,
                if pt {
                    "Confirmar antes de limpar o histórico"
                } else {
                    "Confirm before clearing history"
                },
            )
            .changed()
        {
            *changed = true;
        }
        ui.add_space(6.0);
        let mut tr = app.config.transcribe_translate;
        if ui
            .checkbox(
                &mut tr,
                if pt {
                    "Traduzir transcrição para inglês"
                } else {
                    "Translate transcript to English"
                },
            )
            .changed()
        {
            app.config.transcribe_translate = tr;
            *changed = true;
        }
    });
}

fn card_defaults(ui: &mut egui::Ui, app: &mut App, changed: &mut bool) {
    let s = crate::ui::i18n::s(app.config.lang);
    let pt = app.config.lang == Lang::Pt;
    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            egui::RichText::new(s.settings_defaults)
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(10.0);

        ui.label(s.settings_folder);
        ui.horizontal(|ui| {
            let mut path_str = app.config.default_download_dir.to_string_lossy().to_string();
            if ui
                .add(egui::TextEdit::singleline(&mut path_str).text_color(theme::text_muted()))
                .changed()
            {
                app.config.default_download_dir = std::path::PathBuf::from(&path_str);
                *changed = true;
            }
            if ui
                .add(egui::Button::new(egui::RichText::new("📁").color(egui::Color32::WHITE)).fill(theme::accent()))
                .clicked()
            {
                if let Some(picked) = rfd::FileDialog::new().pick_folder() {
                    app.config.default_download_dir = picked;
                    *changed = true;
                }
            }
        });
        ui.add_space(10.0);

        *changed |= format_row(ui, s.settings_music_format, &[("mp3", "mp3"), ("m4a", "m4a"), ("opus", "opus"), ("flac", "flac")], &mut app.config.music_format);
        ui.add_space(6.0);
        let video_formats: Vec<(&str, &str)> = crate::download::engine::video_profiles()
            .iter()
            .map(|profile| (profile.extension, profile.label))
            .collect();
        *changed |= format_row(ui, s.settings_video_format, &video_formats, &mut app.config.video_format);
        ui.add_space(6.0);
        *changed |= format_row(ui, s.settings_quality, &[("best", "best"), ("high", "high"), ("medium", "medium")], &mut app.config.quality);
        ui.add_space(10.0);

        ui.label(s.settings_max_history);
        let mut max = app.config.max_history as u32;
        if ui
            .add(egui::Slider::new(&mut max, 10..=500))
            .changed()
        {
            app.config.max_history = max as usize;
            *changed = true;
        }
        ui.add_space(10.0);

        ui.label(if pt { "Nome do arquivo (template)" } else { "Filename template" });
        if ui
            .add(
                egui::TextEdit::singleline(&mut app.config.filename_template)
                    .desired_width(260.0)
                    .hint_text("%(title)s")
                    .text_color(theme::text()),
            )
            .changed()
        {
            *changed = true;
        }
        ui.label(
            egui::RichText::new("%(title)s · %(uploader)s")
                .color(theme::text_faint())
                .size(11.0),
        );
        if ui
            .checkbox(
                &mut app.config.smart_rename,
                if pt {
                    "Limpar nome automaticamente ([Official Video], - Topic…)"
                } else {
                    "Clean name automatically ([Official Video], - Topic…)"
                },
            )
            .changed()
        {
            *changed = true;
        }
        ui.add_space(10.0);

        if ui.checkbox(&mut app.config.subtitles, s.settings_subtitles).changed() {
            *changed = true;
        }
        if app.config.subtitles {
            ui.horizontal(|ui| {
                ui.label(s.settings_sub_langs);
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut app.config.sub_langs)
                            .desired_width(120.0)
                            .text_color(theme::text()),
                    )
                    .changed()
                {
                    *changed = true;
                }
            });
        }
        ui.add_space(10.0);

        ui.label(s.settings_rate_limit);
        if ui
            .add(
                egui::TextEdit::singleline(&mut app.config.rate_limit)
                    .desired_width(120.0)
                    .hint_text("ex.: 2M")
                    .text_color(theme::text()),
            )
            .changed()
        {
            *changed = true;
        }
        ui.add_space(8.0);

        ui.label(s.settings_fragments);
        let mut frags = app.config.concurrent_fragments;
        if ui.add(egui::Slider::new(&mut frags, 1..=16)).changed() {
            app.config.concurrent_fragments = frags;
            *changed = true;
        }
        ui.add_space(10.0);

        if ui
            .checkbox(&mut app.config.notify_on_complete, s.settings_notify)
            .changed()
        {
            *changed = true;
        }
    });
}

fn card_organize(ui: &mut egui::Ui, app: &mut App, changed: &mut bool) {
    let s = crate::ui::i18n::s(app.config.lang);
    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(s.settings_organize);
        ui.horizontal(|ui| {
            for (val, label) in [
                ("none", s.org_none),
                ("type", s.org_type),
                ("date", s.org_date),
                ("channel", s.org_channel),
            ] {
                let sel = app.config.organize_by == val;
                let fill = if sel { theme::accent() } else { theme::bg_card() };
                if ui.add(egui::Button::new(label).fill(fill)).clicked() && !sel {
                    app.config.organize_by = val.to_string();
                    *changed = true;
                }
            }
        });
        ui.add_space(10.0);
        if ui
            .checkbox(&mut app.config.copy_to_cloud, s.settings_cloud_copy)
            .changed()
        {
            *changed = true;
        }
        if app.config.copy_to_cloud {
            ui.label(s.settings_cloud);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut app.config.cloud_folder)
                            .desired_width(260.0)
                            .text_color(theme::text_muted()),
                    )
                    .changed()
                {
                    *changed = true;
                }
                if ui
                    .add(egui::Button::new(egui::RichText::new("📁").color(egui::Color32::WHITE)).fill(theme::accent()))
                    .clicked()
                {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        app.config.cloud_folder = p.to_string_lossy().to_string();
                        *changed = true;
                    }
                }
            });
        }
    });
}

fn card_convert_engine(ui: &mut egui::Ui, app: &mut App, changed: &mut bool) {
    let pt = app.config.lang == Lang::Pt;
    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            egui::RichText::new(if pt {
                "Motor de conversão de documentos"
            } else {
                "Document conversion engine"
            })
            .color(theme::text_muted())
            .size(11.0)
            .strong(),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(if pt {
                "Como docx/xlsx/pptx viram PDF. Maior fidelidade usa Office/LibreOffice; Rust puro é leve e sem dependências."
            } else {
                "How docx/xlsx/pptx become PDF. Higher fidelity uses Office/LibreOffice; pure Rust is light and dependency-free."
            })
            .color(theme::text_faint())
            .size(11.0),
        );
        ui.add_space(8.0);

        let st = crate::download::engine::engine_status();
        let options: [(ConvertEngine, &str, bool, String); 4] = [
            (
                ConvertEngine::Auto,
                if pt { "Automático" } else { "Automatic" },
                true,
                if pt {
                    "Usa o melhor motor disponível".to_string()
                } else {
                    "Uses the best available engine".to_string()
                },
            ),
            (
                ConvertEngine::MsOffice,
                "MS Office",
                st.msoffice,
                if st.msoffice {
                    format!("{}: {}", if pt { "Detectado" } else { "Detected" }, st.msoffice_detail)
                } else if pt {
                    "Não instalado".to_string()
                } else {
                    "Not installed".to_string()
                },
            ),
            (
                ConvertEngine::LibreOffice,
                "LibreOffice",
                st.libreoffice,
                if st.libreoffice {
                    if pt { "Detectado".to_string() } else { "Detected".to_string() }
                } else if pt {
                    "Não instalado".to_string()
                } else {
                    "Not installed".to_string()
                },
            ),
            (
                ConvertEngine::Rust,
                if pt { "Rust puro (leve)" } else { "Pure Rust (light)" },
                true,
                if pt {
                    "Sempre disponível · fidelidade básica".to_string()
                } else {
                    "Always available · basic fidelity".to_string()
                },
            ),
        ];

        for (eng, label, available, detail) in options {
            let selected = app.config.convert_engine == eng;
            ui.horizontal(|ui| {
                let fill = if selected {
                    theme::accent_soft()
                } else {
                    theme::bg_card()
                };
                let txt_color = if available {
                    theme::text()
                } else {
                    theme::text_faint()
                };
                let mark = if selected { "●  " } else { "○  " };
                let btn = egui::Button::new(
                    egui::RichText::new(format!("{}{}", mark, label)).color(txt_color),
                )
                .fill(fill)
                .min_size(egui::vec2(160.0, 30.0));
                if ui.add_enabled(available, btn).clicked() && !selected {
                    app.config.convert_engine = eng;
                    *changed = true;
                }
                ui.label(
                    egui::RichText::new(detail)
                        .color(theme::text_faint())
                        .size(10.0),
                );
            });
            ui.add_space(2.0);
        }

        if !st.msoffice && !st.libreoffice {
            ui.add_space(4.0);
            if ui
                .link(if pt {
                    "↗ Instalar LibreOffice (grátis) para maior fidelidade"
                } else {
                    "↗ Install LibreOffice (free) for higher fidelity"
                })
                .clicked()
            {
                open::that("https://www.libreoffice.org/download/download/").ok();
            }
        }
    });
}

fn card_maintenance(ui: &mut egui::Ui, app: &mut App, changed: &mut bool) {
    let s = crate::ui::i18n::s(app.config.lang);
    let pt = app.config.lang == Lang::Pt;
    let mut do_clear = false;
    let mut do_archive = false;
    let mut do_reinstall = false;
    let mut do_orphans = false;

    theme::card_frame().show(ui, |ui| {
        // Trava a largura na coluna: `set_min_width` sozinho é só um piso, e o
        // texto ao lado do botão de atualizar esticava o cartão para fora dela.
        let w = ui.available_width();
        ui.set_min_width(w);
        ui.set_max_width(w);
        ui.label(
            egui::RichText::new(s.settings_maintenance)
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(10.0);

        let status = app.update_status.lock().unwrap().clone();
        let running = status == UpdateStatus::Running;

        ui.horizontal(|ui| {
            let btn = theme::accent_button(s.settings_update).min_size(egui::vec2(180.0, 40.0));
            if ui.add_enabled(!running, btn).clicked() {
                start_update(app);
            }
            ui.add_space(10.0);
            // Os textos abaixo quebram linha: dentro de `horizontal` o padrão é
            // não quebrar, e aí eles empurrariam a largura do cartão de novo.
            match status {
                UpdateStatus::Idle => {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                "Mantém o download funcionando quando o YouTube muda.",
                            )
                            .color(theme::text_faint()),
                        )
                        .wrap(true),
                    );
                }
                UpdateStatus::Running => {
                    ui.add(egui::Spinner::new().color(theme::accent()));
                    ui.label(egui::RichText::new("Atualizando...").color(theme::text_muted()));
                }
                UpdateStatus::Done(_) => {
                    ui.label(egui::RichText::new("✔ yt-dlp atualizado!").color(theme::accent()));
                }
                UpdateStatus::Error(e) => {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("Falha: {}", e)).color(theme::danger()),
                        )
                        .wrap(true),
                    );
                }
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let data_dir = crate::config::settings::Config::config_path()
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default();
            if ui.add(theme::ghost_button(s.open_log)).clicked() {
                open::that(data_dir.join("lumen.log")).ok();
            }
            if ui.add(theme::ghost_button(s.open_data)).clicked() {
                open::that(&data_dir).ok();
            }
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(if pt { "Cache & dados" } else { "Cache & data" })
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(theme::ghost_button(if pt {
                    "🧹 Limpar temporários"
                } else {
                    "🧹 Clear temp files"
                }))
                .clicked()
            {
                do_clear = true;
            }
            if ui
                .add(theme::ghost_button(if pt {
                    "📦 Arquivar antigos (>30d)"
                } else {
                    "📦 Archive old (>30d)"
                }))
                .clicked()
            {
                do_archive = true;
            }
            if ui
                .add(theme::ghost_button(if pt {
                    "♻ Reinstalar dependências"
                } else {
                    "♻ Reinstall dependencies"
                }))
                .clicked()
            {
                do_reinstall = true;
            }
            if ui
                .add(theme::ghost_button(if pt {
                    "🔍 Procurar arquivos órfãos"
                } else {
                    "🔍 Find orphan files"
                }))
                .clicked()
            {
                do_orphans = true;
            }
        });
        ui.add_space(8.0);
        if ui
            .checkbox(
                &mut app.config.auto_retry,
                if pt {
                    "Re-tentar downloads automaticamente em falha de rede"
                } else {
                    "Auto-retry downloads on network failure"
                },
            )
            .changed()
        {
            *changed = true;
        }
    });

    if do_clear {
        app.clear_temp_files_toast();
    }
    if do_archive {
        let n = app.archive_old(30);
        app.toast(
            if pt {
                format!("{} arquivo(s) arquivado(s) em /Arquivo", n)
            } else {
                format!("{} file(s) archived to /Arquivo", n)
            },
            false,
        );
    }
    if do_reinstall {
        app.reinstall_dependencies();
        app.toast(
            if pt {
                "Dependências removidas; serão baixadas de novo."
            } else {
                "Dependencies removed; they will be re-downloaded."
            },
            false,
        );
    }
    if do_orphans {
        app.find_orphans();
    }
}

fn format_row(ui: &mut egui::Ui, label: &str, options: &[(&str, &str)], value: &mut String) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.horizontal(|ui| {
        for (value_id, label) in options {
            let selected = value == *value_id;
            let fill = if selected { theme::accent() } else { theme::bg_card() };
            if ui.add(egui::Button::new(*label).fill(fill)).clicked() && !selected {
                *value = (*value_id).to_string();
                changed = true;
            }
        }
    });
    changed
}

pub fn start_update(app: &mut App) {
    let Some(engine) = app.engine.clone() else {
        return;
    };
    let status = app.update_status.clone();
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
