use crate::app::{App, MediaType};
use crate::db::database::HistoryEntry;
use crate::ui::theme;

pub fn render(
    app: &mut App,
    ui: &mut egui::Ui,
    media_type: &str,
    first_col: &str,
    title: &str,
    history: &[HistoryEntry],
    redownload_as: Option<MediaType>,
) {
    let s = crate::ui::i18n::s(app.config.lang);
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .color(theme::text())
                .size(18.0)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !history.is_empty() && ui.add(theme::ghost_button(s.hist_clear)).clicked() {
                if app.config.confirm_delete {
                    app.pending_clear = Some(media_type.to_string());
                } else {
                    app.db.clear_history(media_type);
                }
            }
            if !history.is_empty()
                && ui
                    .add(theme::ghost_button("⬇ .zip"))
                    .on_hover_text(s.hist_export)
                    .clicked()
            {
                let files: Vec<std::path::PathBuf> = history
                    .iter()
                    .map(|e| std::path::PathBuf::from(&e.file_path))
                    .collect();
                app.export_zip(files);
            }
            if media_type == "music"
                && !history.is_empty()
                && ui.add(theme::ghost_button("🎵 Playlist")).clicked()
            {
                let entries: Vec<(String, String)> = history
                    .iter()
                    .map(|e| (e.title.clone(), e.file_path.clone()))
                    .collect();
                app.export_playlist(entries);
            }
        });
    });
    ui.add_space(6.0);

    if !app.selected.is_empty() {
        let mut bulk_delete = false;
        let mut bulk_zip = false;
        egui::Frame::none()
            .fill(theme::accent_soft())
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {}",
                            app.selected.len(),
                            if pt { "selecionado(s)" } else { "selected" }
                        ))
                        .color(theme::accent())
                        .strong(),
                    );
                    if ui.add(theme::ghost_button(if pt { "🗑 Excluir" } else { "🗑 Delete" })).clicked() {
                        bulk_delete = true;
                    }
                    if ui.add(theme::ghost_button("⬇ .zip")).clicked() {
                        bulk_zip = true;
                    }
                    if ui.add(theme::ghost_button(if pt { "Limpar seleção" } else { "Clear" })).clicked() {
                        app.selected.clear();
                    }
                });
            });
        ui.add_space(6.0);
        if bulk_zip {
            let files: Vec<std::path::PathBuf> = history
                .iter()
                .filter(|e| app.selected.contains(&e.id))
                .map(|e| std::path::PathBuf::from(&e.file_path))
                .collect();
            app.export_zip(files);
        }
        if bulk_delete {
            let ids: Vec<i64> = app.selected.iter().copied().collect();
            for id in ids {
                app.db.delete_history(id);
            }
            app.selected.clear();
        }
    }

    let mut formats: Vec<String> = history
        .iter()
        .map(|e| e.format.clone())
        .filter(|f| !f.is_empty())
        .collect();
    formats.sort();
    formats.dedup();

    ui.horizontal(|ui| {
        ui.spacing_mut().interact_size.y = 34.0;
        ui.add(
            egui::TextEdit::singleline(&mut app.history_search)
                .hint_text(s.hist_search)
                .desired_width(380.0)
                .margin(egui::vec2(10.0, 7.0))
                .text_color(theme::text()),
        );
        let all = if app.config.lang == crate::ui::i18n::Lang::Pt { "Todos" } else { "All" };
        egui::ComboBox::from_id_source(format!("fmt_filter_{}", media_type))
            .selected_text(if app.history_format_filter.is_empty() {
                all.to_string()
            } else {
                app.history_format_filter.clone()
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.history_format_filter, String::new(), all);
                for f in &formats {
                    ui.selectable_value(&mut app.history_format_filter, f.clone(), f);
                }
            });
        let fav_fill = if app.history_fav_only {
            theme::accent()
        } else {
            theme::bg_card()
        };
        if ui
            .add(egui::Button::new(egui::RichText::new("⭐").color(theme::text())).fill(fav_fill))
            .on_hover_text(if app.config.lang == crate::ui::i18n::Lang::Pt {
                "Só favoritos"
            } else {
                "Favorites only"
            })
            .clicked()
        {
            app.history_fav_only = !app.history_fav_only;
        }
        let label = if app.config.history_grid {
            s.view_list
        } else {
            s.view_grid
        };
        if ui.add(theme::ghost_button(label)).clicked() {
            app.config.history_grid = !app.config.history_grid;
            app.config.save();
        }
    });
    ui.add_space(8.0);

    let needle = app.history_search.trim().to_lowercase();
    let fmt_filter = app.history_format_filter.clone();
    let fav_only = app.history_fav_only;
    let filtered: Vec<&HistoryEntry> = history
        .iter()
        .filter(|e| needle.is_empty() || e.title.to_lowercase().contains(&needle) || e.tags.to_lowercase().contains(&needle))
        .filter(|e| fmt_filter.is_empty() || e.format == fmt_filter)
        .filter(|e| !fav_only || e.favorite)
        .collect();

    if filtered.is_empty() {
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new(s.hist_empty).color(theme::text_faint()));
        });
    } else if app.config.history_grid {
        let tt = |p: &'static str, e: &'static str| if pt { p } else { e };
        theme::card_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            egui::ScrollArea::vertical()
                .id_source(format!("hist_grid_{}", media_type))
                .max_height(420.0)
                .show(ui, |ui| {
                    let cols = (((ui.available_width() + 12.0) / 220.0).floor() as usize).max(1);
                    for chunk in filtered.chunks(cols) {
                        ui.columns(cols, |c| {
                            for (k, entry) in chunk.iter().enumerate() {
                                let ui = &mut c[k];
                                egui::Frame::none()
                                    .fill(theme::bg_card())
                                    .stroke(egui::Stroke::new(1.0, theme::border()))
                                    .rounding(egui::Rounding::same(8.0))
                                    .inner_margin(egui::Margin::same(10.0))
                                    .show(ui, |ui| {
                                        let cw = ui.available_width();
                                        let thumb_h = (cw * 9.0 / 16.0).min(150.0);
                                        ui.set_height(thumb_h + 100.0);
                                        ui.vertical(|ui| {
                                            let is_video = matches!(
                                                entry.format.as_str(),
                                                "mp4" | "mkv" | "webm" | "avi" | "mov"
                                            );
                                            let placeholder = |ui: &mut egui::Ui, label: Option<&str>| {
                                                let (rect, _) = ui.allocate_exact_size(
                                                    egui::vec2(cw, thumb_h),
                                                    egui::Sense::hover(),
                                                );
                                                ui.painter().rect_filled(
                                                    rect,
                                                    egui::Rounding::same(4.0),
                                                    theme::bg_card_hover(),
                                                );
                                                if let Some(t) = label {
                                                    ui.painter().text(
                                                        rect.center(),
                                                        egui::Align2::CENTER_CENTER,
                                                        t,
                                                        egui::FontId::proportional(16.0),
                                                        theme::text_faint(),
                                                    );
                                                }
                                            };
                                            if is_video {
                                                app.request_thumb(&entry.file_path);
                                                if let Some(tex) =
                                                    app.thumb_textures.get(&entry.file_path)
                                                {
                                                    ui.add(
                                                        egui::Image::from_texture((
                                                            tex.id(),
                                                            egui::vec2(cw, thumb_h),
                                                        ))
                                                        .fit_to_exact_size(egui::vec2(cw, thumb_h))
                                                        .rounding(egui::Rounding::same(4.0)),
                                                    );
                                                } else {
                                                    placeholder(ui, None);
                                                }
                                            } else {
                                                let up = entry.format.to_uppercase();
                                                placeholder(ui, Some(&up));
                                            }
                                            ui.add_space(4.0);
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&entry.title)
                                                        .color(theme::text())
                                                        .size(12.0),
                                                )
                                                .truncate(true),
                                            );
                                            ui.label(
                                                egui::RichText::new(&entry.format)
                                                    .color(theme::text_muted())
                                                    .size(11.0),
                                            );
                                            // Data + seleção (ações em massa).
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(&entry.created_at)
                                                        .color(theme::text_faint())
                                                        .size(10.0),
                                                );
                                                let mut sel = app.selected.contains(&entry.id);
                                                if ui.checkbox(&mut sel, "").changed() {
                                                    if sel {
                                                        app.selected.insert(entry.id);
                                                    } else {
                                                        app.selected.remove(&entry.id);
                                                    }
                                                }
                                            });
                                            ui.add_space(2.0);
                                            ui.horizontal(|ui| {
                                                if icon_button(ui, "▶", tt("Abrir", "Open")) {
                                                    open::that(&entry.file_path).ok();
                                                }
                                                if crate::player::is_playable_audio(&entry.format)
                                                    && icon_button(ui, "🎧", tt("Pré-ouvir", "Preview"))
                                                {
                                                    app.mini.play(std::path::PathBuf::from(
                                                        &entry.file_path,
                                                    ));
                                                }
                                                if icon_button(ui, "📁", tt("Pasta", "Folder")) {
                                                    if let Some(p) =
                                                        std::path::Path::new(&entry.file_path).parent()
                                                    {
                                                        open::that(p).ok();
                                                    }
                                                }
                                                if icon_button(ui, "✕", tt("Excluir", "Delete")) {
                                                    app.pending_delete = Some((
                                                        entry.id,
                                                        entry.title.clone(),
                                                        entry.file_path.clone(),
                                                    ));
                                                }
                                                let star = if entry.favorite { "★" } else { "☆" };
                                                let star_col = if entry.favorite {
                                                    theme::accent()
                                                } else {
                                                    theme::text_faint()
                                                };
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            egui::RichText::new(star).color(star_col),
                                                        )
                                                        .fill(theme::bg_card())
                                                        .min_size(egui::vec2(30.0, 26.0)),
                                                    )
                                                    .on_hover_text(tt("Favoritar", "Favorite"))
                                                    .clicked()
                                                {
                                                    app.db.toggle_favorite(entry.id);
                                                }
                                                ui.menu_button("⋮", |ui| {
                                                    if let Some(mt) = redownload_as {
                                                        if ui.button(tt("⟳  Baixar de novo", "⟳  Download again")).clicked() {
                                                            app.start_url_download(entry.url.clone(), mt);
                                                            ui.close_menu();
                                                        }
                                                    }
                                                    if ui.button(tt("🛡  Verificar integridade", "🛡  Verify integrity")).clicked() {
                                                        app.verify_file(entry.file_path.clone());
                                                        ui.close_menu();
                                                    }
                                                    if ui.button(tt("ℹ  Ver metadados", "ℹ  View metadata")).clicked() {
                                                        app.show_metadata(entry.file_path.clone());
                                                        ui.close_menu();
                                                    }
                                                    if ui.button(tt("🔖  Categorias/tags", "🔖  Categories/tags")).clicked() {
                                                        app.history_tag_edit = Some((entry.id, entry.tags.clone()));
                                                        ui.close_menu();
                                                    }
                                                    if is_video == false
                                                        && matches!(entry.format.as_str(), "mp3" | "m4a" | "flac" | "opus" | "ogg" | "wav" | "aac")
                                                        && ui.button(tt("🏷  Editar tags", "🏷  Edit tags")).clicked()
                                                    {
                                                        app.open_tag_editor(entry.file_path.clone());
                                                        ui.close_menu();
                                                    }
                                                    if !entry.url.is_empty()
                                                        && ui.button(tt("🔳  QR do link", "🔳  Link QR")).clicked()
                                                    {
                                                        if let Some(tex) = crate::app::make_qr_texture(ui.ctx(), &entry.url) {
                                                            app.qr_window = Some((entry.url.clone(), tex));
                                                        }
                                                        ui.close_menu();
                                                    }
                                                    if ui.button(tt("📋  Copiar info", "📋  Copy info")).clicked() {
                                                        let info = format!("{}\n{}", entry.title, entry.url);
                                                        theme::set_clipboard(info.trim());
                                                        app.toast(tt("📋 Copiado", "📋 Copied"), false);
                                                        ui.close_menu();
                                                    }
                                                    if ui.button(tt("🔗  Copiar caminho", "🔗  Copy path")).clicked() {
                                                        theme::set_clipboard(&entry.file_path);
                                                        app.toast(tt("🔗 Caminho copiado", "🔗 Path copied"), false);
                                                        ui.close_menu();
                                                    }
                                                });
                                            });
                                        });
                                    });
                            }
                        });
                        ui.add_space(12.0);
                    }
                });
        });
    } else {
        theme::card_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let tt = |p: &'static str, e: &'static str| if pt { p } else { e };
            const ROW_H: f32 = 40.0;
            let fmt_w = 80.0;
            let date_w = 155.0;
            let full = ui.available_width();
            let title_w = (full - fmt_w - date_w - 190.0).clamp(160.0, 9000.0);

            // Cabeçalho (alinhado à esquerda, nas mesmas larguras das colunas de dados).
            ui.horizontal(|ui| {
                let hdr = |ui: &mut egui::Ui, w: f32, txt: &str| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(w, 18.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_min_width(w);
                            ui.label(egui::RichText::new(txt).color(theme::accent()).strong());
                        },
                    );
                };
                hdr(ui, title_w, first_col);
                hdr(ui, fmt_w, tt("Formato", "Format"));
                hdr(ui, date_w, tt("Data", "Date"));
            });
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .id_source(format!("hist_scroll_{}", media_type))
                .max_height(340.0)
                .show(ui, |ui| {
                    for (idx, entry) in filtered.iter().enumerate() {
                        let stripe = if idx % 2 == 1 {
                            theme::bg_card_hover()
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        egui::Frame::none()
                            .fill(stripe)
                            .inner_margin(egui::Margin::symmetric(4.0, 2.0))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    // Coluna: título (seleção + favorito + selo + thumb + título).
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(title_w, ROW_H),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_min_width(title_w);
                                            ui.set_max_width(title_w);
                                            let mut sel = app.selected.contains(&entry.id);
                                            if ui.checkbox(&mut sel, "").changed() {
                                                if sel {
                                                    app.selected.insert(entry.id);
                                                } else {
                                                    app.selected.remove(&entry.id);
                                                }
                                            }
                                            let star = if entry.favorite { "★" } else { "☆" };
                                            let star_col = if entry.favorite {
                                                theme::accent()
                                            } else {
                                                theme::text_faint()
                                            };
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        egui::RichText::new(star).color(star_col),
                                                    )
                                                    .fill(egui::Color32::TRANSPARENT)
                                                    .min_size(egui::vec2(22.0, 22.0)),
                                                )
                                                .clicked()
                                            {
                                                app.db.toggle_favorite(entry.id);
                                            }
                                            if is_recent(&entry.created_at) {
                                                egui::Frame::none()
                                                    .fill(theme::accent_soft())
                                                    .rounding(egui::Rounding::same(4.0))
                                                    .inner_margin(egui::Margin::symmetric(5.0, 1.0))
                                                    .show(ui, |ui| {
                                                        ui.label(
                                                            egui::RichText::new(if pt { "novo" } else { "new" })
                                                                .color(theme::accent())
                                                                .size(9.0)
                                                                .strong(),
                                                        );
                                                    });
                                            }
                                            let is_video = matches!(
                                                entry.format.as_str(),
                                                "mp4" | "mkv" | "webm" | "avi" | "mov"
                                            );
                                            if is_video {
                                                app.request_thumb(&entry.file_path);
                                                if let Some(tex) = app.thumb_textures.get(&entry.file_path) {
                                                    let [w, h] = tex.size();
                                                    let tw = 56.0;
                                                    let th = (tw * h as f32 / w.max(1) as f32).min(40.0);
                                                    ui.add(egui::Image::from_texture((tex.id(), egui::vec2(tw, th))));
                                                } else {
                                                    let (rect, _) = ui.allocate_exact_size(
                                                        egui::vec2(56.0, 32.0),
                                                        egui::Sense::hover(),
                                                    );
                                                    ui.painter().rect_filled(
                                                        rect,
                                                        egui::Rounding::same(4.0),
                                                        theme::bg_card_hover(),
                                                    );
                                                }
                                                ui.add_space(6.0);
                                            }
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&entry.title).color(theme::text()),
                                                )
                                                .truncate(true),
                                            )
                                            .on_hover_text(&entry.title);
                                        },
                                    );
                                    // Coluna: formato.
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(fmt_w, ROW_H),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_min_width(fmt_w);
                                            ui.label(egui::RichText::new(&entry.format).color(theme::text_muted()));
                                        },
                                    );
                                    // Coluna: data.
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(date_w, ROW_H),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_min_width(date_w);
                                            ui.label(egui::RichText::new(&entry.created_at).color(theme::text_muted()));
                                        },
                                    );
                                    // Ações no canto direito (right-to-left: ⋮ é o último).
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let is_audio = matches!(
                                                entry.format.as_str(),
                                                "mp3" | "m4a" | "flac" | "opus" | "ogg" | "wav" | "aac"
                                            );
                                            ui.menu_button("⋮", |ui| {
                                                if let Some(mt) = redownload_as {
                                                    if ui.button(tt("⟳  Baixar de novo", "⟳  Download again")).clicked() {
                                                        app.start_url_download(entry.url.clone(), mt);
                                                        ui.close_menu();
                                                    }
                                                }
                                                if ui.button(tt("🛡  Verificar integridade", "🛡  Verify integrity")).clicked() {
                                                    app.verify_file(entry.file_path.clone());
                                                    ui.close_menu();
                                                }
                                                if ui.button(tt("ℹ  Ver metadados", "ℹ  View metadata")).clicked() {
                                                    app.show_metadata(entry.file_path.clone());
                                                    ui.close_menu();
                                                }
                                                if ui.button(tt("🔖  Categorias/tags", "🔖  Categories/tags")).clicked() {
                                                    app.history_tag_edit = Some((entry.id, entry.tags.clone()));
                                                    ui.close_menu();
                                                }
                                                if is_audio && ui.button(tt("🏷  Editar tags", "🏷  Edit tags")).clicked() {
                                                    app.open_tag_editor(entry.file_path.clone());
                                                    ui.close_menu();
                                                }
                                                if !entry.url.is_empty()
                                                    && ui.button(tt("🔳  QR do link", "🔳  Link QR")).clicked()
                                                {
                                                    if let Some(tex) = crate::app::make_qr_texture(ui.ctx(), &entry.url) {
                                                        app.qr_window = Some((entry.url.clone(), tex));
                                                    }
                                                    ui.close_menu();
                                                }
                                                if ui.button(tt("📋  Copiar info", "📋  Copy info")).clicked() {
                                                    let info = format!("{}\n{}", entry.title, entry.url);
                                                    theme::set_clipboard(info.trim());
                                                    app.toast(tt("📋 Copiado", "📋 Copied"), false);
                                                    ui.close_menu();
                                                }
                                                if ui.button(tt("🔗  Copiar caminho", "🔗  Copy path")).clicked() {
                                                    theme::set_clipboard(&entry.file_path);
                                                    app.toast(tt("🔗 Caminho copiado", "🔗 Path copied"), false);
                                                    ui.close_menu();
                                                }
                                            });
                                            if icon_button(ui, "✕", tt("Excluir", "Delete")) {
                                                app.pending_delete = Some((
                                                    entry.id,
                                                    entry.title.clone(),
                                                    entry.file_path.clone(),
                                                ));
                                            }
                                            if icon_button(ui, "📁", tt("Abrir pasta", "Open folder")) {
                                                if let Some(parent) = std::path::Path::new(&entry.file_path).parent() {
                                                    open::that(parent).ok();
                                                }
                                            }
                                            if icon_button(ui, "▶", tt("Abrir arquivo", "Open file")) {
                                                open::that(&entry.file_path).ok();
                                            }
                                            if crate::player::is_playable_audio(&entry.format)
                                                && icon_button(ui, "🎧", tt("Pré-ouvir", "Preview"))
                                            {
                                                app.mini.play(std::path::PathBuf::from(
                                                    &entry.file_path,
                                                ));
                                            }
                                        },
                                    );
                                });
                            });
                    }
                });
        });
    }

    let trash = app.db.get_deleted_history(media_type, app.config.max_history);
    if !trash.is_empty() {
        ui.add_space(8.0);
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("{} ({})", s.trash, trash.len()))
                .color(theme::text_muted()),
        )
        .id_source(format!("trash_{}", media_type))
        .show(ui, |ui| {
            if ui.add(theme::ghost_button(s.trash_empty_btn)).clicked() {
                app.db.empty_trash(media_type);
            }
            ui.add_space(6.0);
            for (idx, entry) in trash.iter().enumerate() {
                let stripe = if idx % 2 == 1 {
                    theme::bg_card_hover()
                } else {
                    egui::Color32::TRANSPARENT
                };
                egui::Frame::none()
                    .fill(stripe)
                    .inner_margin(egui::Margin::symmetric(6.0, 3.0))
                    .rounding(egui::Rounding::same(4.0))
                    .show(ui, |ui| {
                        let w = ui.available_width();
                        ui.allocate_ui_with_layout(
                            egui::vec2(w, 26.0),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if icon_button(ui, "⟲", s.trash_restore) {
                                    app.db.restore_history(entry.id);
                                }
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&entry.title)
                                                    .color(theme::text_faint())
                                                    .size(12.0),
                                            )
                                            .truncate(true),
                                        )
                                        .on_hover_text(&entry.title);
                                    },
                                );
                            },
                        );
                    });
            }
        });
    }
}

fn is_recent(created_at: &str) -> bool {
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(created_at, "%Y-%m-%d %H:%M:%S") {
        let now = chrono::Local::now().naive_local();
        let age = now.signed_duration_since(dt);
        age.num_seconds() >= 0 && age.num_minutes() < 60
    } else {
        false
    }
}

fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> bool {
    ui.add(
        egui::Button::new(egui::RichText::new(icon).color(theme::text()))
            .fill(theme::bg_card())
            .min_size(egui::vec2(30.0, 26.0)),
    )
    .on_hover_text(tooltip)
    .clicked()
}
