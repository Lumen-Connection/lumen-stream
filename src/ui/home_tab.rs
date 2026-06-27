use crate::app::{App, MediaType, Tab};
use crate::db::database::HistoryEntry;
use crate::ui::theme;

pub fn render(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    let s = crate::ui::i18n::s(app.config.lang);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("◆").size(28.0).color(theme::accent()));
        ui.label(
            egui::RichText::new(s.home_title)
                .color(theme::text())
                .size(30.0)
                .strong(),
        );
    });
    ui.label(
        egui::RichText::new(s.home_subtitle)
            .color(theme::text_muted())
            .size(14.0),
    );
    ui.add_space(20.0);

    // Download rápido (vídeo).
    let mut submit = false;
    theme::card_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(s.home_quick)
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let resp = ui.add_sized(
                egui::vec2(ui.available_width() - 140.0, 40.0),
                egui::TextEdit::singleline(&mut app.video_url)
                    .hint_text("https://...")
                    .text_color(theme::text())
                    .margin(egui::vec2(12.0, 10.0)),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                submit = true;
            }
            if ui
                .add(theme::accent_button(&format!("⤓  {}", s.download)).min_size(egui::vec2(120.0, 40.0)))
                .clicked()
            {
                submit = true;
            }
        });
    });
    if submit {
        let url = app.video_url.clone();
        app.start_url_download(url, MediaType::Video);
    }

    ui.add_space(18.0);

    // --- Atalhos (cards reordenáveis / fixáveis) ---
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(if pt { "Atalhos" } else { "Shortcuts" })
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let lbl = if app.home_edit {
                if pt { "✓ Concluir" } else { "✓ Done" }
            } else {
                if pt { "✎ Editar" } else { "✎ Edit" }
            };
            if ui.add(theme::ghost_button(lbl)).clicked() {
                app.home_edit = !app.home_edit;
                if !app.home_edit {
                    app.config.save();
                }
            }
        });
    });
    ui.add_space(6.0);

    if app.home_edit {
        edit_shortcuts(app, ui, &s, pt);
    } else {
        let mut nav: Option<Tab> = None;
        // Fixados primeiro, mantendo a ordem configurada.
        let order: Vec<String> = {
            let cards = &app.config.home_cards;
            let pinned = &app.config.home_pinned;
            cards
                .iter()
                .filter(|c| pinned.contains(*c))
                .cloned()
                .chain(cards.iter().filter(|c| !pinned.contains(*c)).cloned())
                .collect()
        };
        ui.horizontal_wrapped(|ui| {
            for id in &order {
                if let Some((label, tab)) = card_info(id, &s) {
                    let pinned = app.config.home_pinned.contains(id);
                    let lbl = if pinned { format!("★ {}", label) } else { label };
                    if shortcut(ui, &lbl) {
                        nav = Some(tab);
                    }
                }
            }
        });
        if let Some(t) = nav {
            app.active_tab = t;
        }
    }

    ui.add_space(20.0);

    // Recentes (todos os tipos).
    ui.label(
        egui::RichText::new(s.home_recents)
            .color(theme::text())
            .size(18.0)
            .strong(),
    );
    ui.add_space(10.0);

    let mut recents: Vec<HistoryEntry> = Vec::new();
    for mt in ["music", "video", "convert"] {
        recents.extend(app.db.get_history(mt, 10));
    }
    recents.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    recents.truncate(8);

    if recents.is_empty() {
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new(s.home_empty).color(theme::text_faint()));
        });
        return;
    }

    render_recents(ui, &recents);
}

/// Catálogo de cards de atalho conhecidos.
const CATALOG: [&str; 8] = [
    "music",
    "video",
    "transcribe",
    "converter",
    "queue",
    "gallery",
    "folders",
    "stats",
];

/// Rótulo + aba de destino de um id de card.
fn card_info(id: &str, s: &crate::ui::i18n::Strings) -> Option<(String, Tab)> {
    Some(match id {
        "music" => (format!("🎵  {}", s.nav_music), Tab::Music),
        "video" => (format!("🎬  {}", s.nav_video), Tab::Video),
        "transcribe" => (s.transcribe.to_string(), Tab::Video),
        "converter" => (format!("🔄  {}", s.nav_converter), Tab::Converter),
        "queue" => (format!("📋  {}", s.nav_queue), Tab::Queue),
        "gallery" => (format!("🖼  {}", s.nav_gallery), Tab::Gallery),
        "folders" => (format!("📁  {}", s.nav_folders), Tab::Folders),
        "stats" => (format!("📊  {}", s.nav_stats), Tab::Stats),
        _ => return None,
    })
}

/// Modo de edição: reordenar (▲▼), fixar (★), remover (✕) e adicionar cards.
fn edit_shortcuts(app: &mut App, ui: &mut egui::Ui, s: &crate::ui::i18n::Strings, pt: bool) {
    let mut do_save = false;
    let cards = app.config.home_cards.clone();
    let n = cards.len();
    for (i, id) in cards.iter().enumerate() {
        let Some((label, _)) = card_info(id, s) else {
            continue;
        };
        ui.horizontal(|ui| {
            let pinned = app.config.home_pinned.contains(id);
            let star = if pinned { "★" } else { "☆" };
            let col = if pinned { theme::accent() } else { theme::text_faint() };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(star).color(col))
                        .fill(egui::Color32::TRANSPARENT),
                )
                .on_hover_text(if pt { "Fixar" } else { "Pin" })
                .clicked()
            {
                if pinned {
                    app.config.home_pinned.retain(|c| c != id);
                } else {
                    app.config.home_pinned.push(id.clone());
                }
                do_save = true;
            }
            ui.add_sized(
                egui::vec2(220.0, 24.0),
                egui::Label::new(egui::RichText::new(label).color(theme::text())),
            );
            if ui.add(theme::ghost_button("▲")).clicked() && i > 0 {
                app.config.home_cards.swap(i, i - 1);
                do_save = true;
            }
            if ui.add(theme::ghost_button("▼")).clicked() && i + 1 < n {
                app.config.home_cards.swap(i, i + 1);
                do_save = true;
            }
            if ui.add(theme::ghost_button("✕")).clicked() {
                app.config.home_cards.retain(|c| c != id);
                app.config.home_pinned.retain(|c| c != id);
                do_save = true;
            }
        });
    }

    // Cards do catálogo ainda não exibidos.
    let hidden: Vec<&str> = CATALOG
        .iter()
        .copied()
        .filter(|c| !app.config.home_cards.iter().any(|x| x == c))
        .collect();
    if !hidden.is_empty() {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(if pt { "Adicionar:" } else { "Add:" })
                .color(theme::text_faint())
                .size(11.0),
        );
        ui.horizontal_wrapped(|ui| {
            for id in hidden {
                if let Some((label, _)) = card_info(id, s) {
                    if ui.add(theme::ghost_button(&format!("➕ {}", label))).clicked() {
                        app.config.home_cards.push(id.to_string());
                        do_save = true;
                    }
                }
            }
        });
    }

    if do_save {
        app.config.save();
    }
}

/// Botão de atalho grande para uma aba. Retorna `true` se clicado.
fn shortcut(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label).color(theme::text()).size(15.0),
        )
        .fill(theme::bg_card())
        .rounding(egui::Rounding::same(10.0))
        .min_size(egui::vec2(175.0, 52.0)),
    )
    .clicked()
}

fn render_recents(ui: &mut egui::Ui, recents: &[HistoryEntry]) {
    theme::card_frame().show(ui, |ui| {
        for entry in recents {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("▶").color(theme::text()))
                            .fill(theme::bg_card())
                            .min_size(egui::vec2(28.0, 24.0)),
                    )
                    .clicked()
                {
                    open::that(&entry.file_path).ok();
                }
                ui.label(
                    egui::RichText::new(crate::ui::music_tab::short_link(&entry.title))
                        .color(theme::text()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(&entry.created_at)
                            .color(theme::text_faint())
                            .size(11.0),
                    );
                });
            });
            ui.separator();
        }
    });
}
