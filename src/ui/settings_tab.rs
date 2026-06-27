use crate::app::{App, UpdateStatus};
use crate::config::settings::Theme;
use crate::ui::i18n::Lang;
use crate::ui::theme;

pub fn render(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    let s = crate::ui::i18n::s(app.config.lang);

    ui.label(
        egui::RichText::new(s.settings_title)
            .color(theme::text())
            .size(30.0)
            .strong(),
    );
    ui.label(
        egui::RichText::new(s.settings_subtitle)
            .color(theme::text_muted())
            .size(14.0),
    );
    ui.add_space(20.0);

    let mut changed = false;
    let pt = app.config.lang == Lang::Pt;
    let mut do_clear = false;
    let mut do_archive = false;
    let mut do_reinstall = false;
    let mut do_orphans = false;

    // Layout em 2 colunas (1 em janelas estreitas) para preencher a tela.
    let ncols = if ui.available_width() > 980.0 { 2 } else { 1 };
    let right_idx = if ncols == 2 { 1 } else { 0 };

    ui.columns(ncols, |cols| {
        let ui = &mut cols[0];

    // --- Aparência (idioma + tema) ---
    theme::card_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(s.settings_language);
            for (lang, label) in [(Lang::Pt, "Português"), (Lang::En, "English")] {
                let selected = app.config.lang == lang;
                let fill = if selected { theme::accent() } else { theme::bg_card() };
                if ui.add(egui::Button::new(label).fill(fill)).clicked() && !selected {
                    app.config.lang = lang;
                    changed = true;
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
                    changed = true;
                }
            }
        });
    });

    ui.add_space(16.0);

    // --- Acessibilidade ---
    theme::card_frame().show(ui, |ui| {
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
                changed = true;
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
            changed = true;
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
            changed = true;
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
            changed = true;
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
            changed = true;
        }
    });

    ui.add_space(16.0);

    // --- Preferências de download ---
    theme::card_frame().show(ui, |ui| {
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
                changed = true;
            }
            if ui
                .add(egui::Button::new(egui::RichText::new("📁").color(egui::Color32::WHITE)).fill(theme::accent()))
                .clicked()
            {
                if let Some(picked) = rfd::FileDialog::new().pick_folder() {
                    app.config.default_download_dir = picked;
                    changed = true;
                }
            }
        });
        ui.add_space(10.0);

        changed |= format_row(ui, s.settings_music_format, &["mp3", "m4a", "opus", "flac"], &mut app.config.music_format);
        ui.add_space(6.0);
        changed |= format_row(ui, s.settings_video_format, &["mp4", "mkv", "webm"], &mut app.config.video_format);
        ui.add_space(6.0);
        changed |= format_row(ui, s.settings_quality, &["best", "medium", "high"], &mut app.config.quality);
        ui.add_space(10.0);

        ui.label(s.settings_max_history);
        let mut max = app.config.max_history as u32;
        if ui
            .add(egui::Slider::new(&mut max, 10..=500))
            .changed()
        {
            app.config.max_history = max as usize;
            changed = true;
        }
        ui.add_space(10.0);

        // Template do nome do arquivo.
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
            changed = true;
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
            changed = true;
        }
        ui.add_space(10.0);

        // Legendas (vídeo)
        if ui.checkbox(&mut app.config.subtitles, s.settings_subtitles).changed() {
            changed = true;
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
                    changed = true;
                }
            });
        }
        ui.add_space(10.0);

        // Limite de velocidade
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
            changed = true;
        }
        ui.add_space(8.0);

        // Fragmentos em paralelo
        ui.label(s.settings_fragments);
        let mut frags = app.config.concurrent_fragments;
        if ui.add(egui::Slider::new(&mut frags, 1..=16)).changed() {
            app.config.concurrent_fragments = frags;
            changed = true;
        }
        ui.add_space(10.0);

        // Notificação ao concluir
        if ui
            .checkbox(&mut app.config.notify_on_complete, s.settings_notify)
            .changed()
        {
            changed = true;
        }
    });

    ui.add_space(16.0);

    // --- Perfis de download ---
    theme::card_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(if pt { "Perfis de download" } else { "Download profiles" })
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.label(
            egui::RichText::new(if pt {
                "Presets de formato/qualidade aplicáveis no modal de download."
            } else {
                "Format/quality presets you can apply in the download dialog."
            })
            .color(theme::text_faint())
            .size(11.0),
        );
        ui.add_space(8.0);
        let mut remove: Option<usize> = None;
        for (i, p) in app.config.profiles.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{}  ·  {}  ·  {}/{}",
                        p.name, p.media_type, p.format, p.quality
                    ))
                    .color(theme::text())
                    .size(12.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(theme::ghost_button("✕")).clicked() {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            app.config.profiles.remove(i);
            changed = true;
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.profile_draft.name)
                    .hint_text(if pt { "Nome" } else { "Name" })
                    .desired_width(100.0),
            );
            for (val, lbl) in [("music", "🎵"), ("video", "🎬")] {
                let sel = app.profile_draft.media_type == val;
                let fill = if sel { theme::accent() } else { theme::bg_card() };
                if ui.add(egui::Button::new(lbl).fill(fill)).clicked() {
                    app.profile_draft.media_type = val.to_string();
                }
            }
            ui.add(
                egui::TextEdit::singleline(&mut app.profile_draft.format)
                    .hint_text("mp4")
                    .desired_width(56.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut app.profile_draft.quality)
                    .hint_text("best")
                    .desired_width(64.0),
            );
            if ui.add(theme::accent_button("＋")).clicked()
                && !app.profile_draft.name.trim().is_empty()
            {
                app.config.profiles.push(app.profile_draft.clone());
                app.profile_draft.name.clear();
                changed = true;
            }
        });
    });

    ui.add_space(16.0);

        let ui = &mut cols[right_idx];

    // --- Organização & nuvem ---
    theme::card_frame().show(ui, |ui| {
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
                    changed = true;
                }
            }
        });
        ui.add_space(10.0);
        if ui
            .checkbox(&mut app.config.copy_to_cloud, s.settings_cloud_copy)
            .changed()
        {
            changed = true;
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
                    changed = true;
                }
                if ui
                    .add(egui::Button::new(egui::RichText::new("📁").color(egui::Color32::WHITE)).fill(theme::accent()))
                    .clicked()
                {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        app.config.cloud_folder = p.to_string_lossy().to_string();
                        changed = true;
                    }
                }
            });
        }
    });

    ui.add_space(16.0);

    // --- Manutenção ---
    theme::card_frame().show(ui, |ui| {
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
            match status {
                UpdateStatus::Idle => {
                    ui.label(
                        egui::RichText::new("Mantém o download funcionando quando o YouTube muda.")
                            .color(theme::text_faint()),
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
                    ui.label(egui::RichText::new(format!("Falha: {}", e)).color(theme::danger()));
                }
            }
        });
        ui.add_space(10.0);
        // Diagnóstico: abrir log e a pasta de dados.
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
            changed = true;
        }
    });
    }); // fim de ui.columns

    if do_clear {
        let n = app.clear_temp_files();
        app.toast(
            if pt {
                format!("{} arquivo(s) temporário(s) removido(s)", n)
            } else {
                format!("{} temp file(s) removed", n)
            },
            false,
        );
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

/// Linha de botões de seleção que grava no campo `value`. Retorna `true` se mudou.
fn format_row(ui: &mut egui::Ui, label: &str, options: &[&str], value: &mut String) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.horizontal(|ui| {
        for opt in options {
            let selected = value == *opt;
            let fill = if selected { theme::accent() } else { theme::bg_card() };
            if ui.add(egui::Button::new(*opt).fill(fill)).clicked() && !selected {
                *value = opt.to_string();
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
