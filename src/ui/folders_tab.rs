use crate::app::App;
use crate::ui::theme;

pub fn render(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    let s = crate::ui::i18n::s(app.config.lang);
    // Em janela estreita o botão desce para uma linha própria: alinhado à
    // direita do título, ele saía pela borda.
    let narrow = theme::is_narrow(ui);
    let mut new_folder = false;
    if narrow {
        theme::page_header(ui, s.folders_title, s.folders_subtitle);
        ui.add_space(8.0);
        new_folder = ui
            .add(theme::accent_button(s.folders_new).min_size(egui::vec2(140.0, 40.0)))
            .clicked();
    } else {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(s.folders_title)
                    .color(theme::text())
                    .size(30.0)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                new_folder = ui
                    .add(theme::accent_button(s.folders_new).min_size(egui::vec2(140.0, 40.0)))
                    .clicked();
            });
        });
        ui.add(
            egui::Label::new(
                egui::RichText::new(s.folders_subtitle)
                    .color(theme::text_muted())
                    .size(14.0),
            )
            .wrap(true),
        );
    }
    if new_folder {
        if let Some(picked) = rfd::FileDialog::new().pick_folder() {
            let name = picked
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Pasta".to_string());
            std::fs::create_dir_all(&picked).ok();
            app.db.add_folder(&name, &picked.to_string_lossy());
        }
    }
    ui.add_space(20.0);

    let folders = app.folders();
    if folders.is_empty() {
        theme::card_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new(s.folders_empty).color(theme::text_faint()),
            );
        });
        return;
    }

    theme::card_frame().show(ui, |ui| {
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .show(ui, |ui| {
                for folder in &folders {
                    let is_default = app.config.default_download_dir.to_string_lossy()
                        == folder.path.as_str();

                    ui.horizontal(|ui| {
                        let editing = matches!(&app.folder_edit, Some((id, _)) if *id == folder.id);
                        if editing {
                            if let Some((_, name)) = app.folder_edit.as_mut() {
                                ui.add(
                                    egui::TextEdit::singleline(name).desired_width(160.0),
                                );
                            }
                            if ui.add(theme::accent_button(s.btn_save_short)).clicked() {
                                if let Some((id, name)) = app.folder_edit.take() {
                                    if !name.trim().is_empty() {
                                        app.db.rename_folder(id, name.trim());
                                    }
                                }
                            }
                        } else {
                            let mut label = folder.name.clone();
                            if is_default {
                                label = format!("★ {}", label);
                            }
                            ui.add_sized(
                                egui::vec2(180.0, 20.0),
                                egui::Label::new(
                                    egui::RichText::new(label).color(theme::text()),
                                ),
                            );
                        }

                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if icon_button(ui, "🗑", s.folder_remove) {
                                    app.pending_delete_folder =
                                        Some((folder.id, folder.name.clone(), folder.path.clone()));
                                }
                                if icon_button(ui, "✎", s.folder_rename) {
                                    app.folder_edit =
                                        Some((folder.id, folder.name.clone()));
                                }
                                if icon_button(ui, "★", s.folder_set_default) {
                                    app.config.default_download_dir =
                                        std::path::PathBuf::from(&folder.path);
                                    app.config.save();
                                }
                                if icon_button(ui, "📂", s.folder_open) {
                                    open::that(&folder.path).ok();
                                }
                            },
                        );
                    });
                    ui.label(
                        egui::RichText::new(&folder.path)
                            .color(theme::text_faint())
                            .size(11.0),
                    );
                    ui.separator();
                }
            });
    });
}

fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> bool {
    ui.add(
        egui::Button::new(egui::RichText::new(icon).color(theme::text()))
            .fill(theme::bg_card())
            .min_size(egui::vec2(32.0, 26.0)),
    )
    .on_hover_text(tooltip)
    .clicked()
}
