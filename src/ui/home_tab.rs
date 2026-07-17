use crate::app::{App, MediaType, Tab};
use crate::db::database::HistoryEntry;
use crate::ui::theme;

pub fn render(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    let s = crate::ui::i18n::s(app.config.lang);

    // Dentro de um `horizontal` o label não quebra linha: na janela no tamanho
    // mínimo o título saía pela borda direita e era cortado. Encolher a fonte
    // mantém ele inteiro em vez de escondê-lo.
    let narrow = ui.available_width() < 470.0;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("◆")
                .size(if narrow { 20.0 } else { 28.0 })
                .color(theme::accent()),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(s.home_title)
                    .color(theme::text())
                    .size(if narrow { 21.0 } else { 30.0 })
                    .strong(),
            )
            .wrap(true),
        );
    });
    ui.add(
        egui::Label::new(
            egui::RichText::new(s.home_subtitle)
                .color(theme::text_muted())
                .size(14.0),
        )
        .wrap(true),
    );
    ui.add_space(20.0);

    let mut submit = false;
    theme::card_frame().show(ui, |ui| {
        // Trava no painel: `set_min_width` é só um piso, e sem um teto o card
        // cresce junto com o conteúdo e vaza pela borda na janela estreita.
        let w = ui.available_width();
        ui.set_min_width(w);
        ui.set_max_width(w);
        ui.label(
            egui::RichText::new(s.home_quick)
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            // Reserva o botão (120 + espaçamento) antes de dimensionar o campo —
            // senão, em janela estreita, o "Download" era empurrado para fora.
            let field_w = (ui.available_width() - 140.0).max(80.0);
            let resp = ui.add_sized(
                egui::vec2(field_w, 40.0),
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

    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let edit_lbl = if app.home_edit {
        if pt { "✓ Concluir" } else { "✓ Done" }
    } else {
        if pt { "✎ Editar" } else { "✎ Edit" }
    };
    let mut toggle_edit = false;
    let mut clear_temp = false;
    let shortcuts_lbl = egui::RichText::new(if pt { "Atalhos" } else { "Shortcuts" })
        .color(theme::text_muted())
        .size(11.0)
        .strong();

    // Em janela estreita os botões descem para a linha de baixo e quebram entre
    // si: alinhados à direita na mesma linha do rótulo, eles saíam pela borda.
    if narrow {
        ui.label(shortcuts_lbl);
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            toggle_edit = ui.add(theme::ghost_button(edit_lbl)).clicked();
            clear_temp = ui.add(theme::ghost_button(s.btn_clear_temp)).clicked();
        });
    } else {
        ui.horizontal(|ui| {
            ui.label(shortcuts_lbl);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                toggle_edit = ui.add(theme::ghost_button(edit_lbl)).clicked();
                clear_temp = ui.add(theme::ghost_button(s.btn_clear_temp)).clicked();
            });
        });
    }
    if toggle_edit {
        app.home_edit = !app.home_edit;
        if !app.home_edit {
            app.config.save();
        }
    }
    if clear_temp {
        app.clear_temp_files_toast();
    }
    ui.add_space(6.0);

    if app.home_edit {
        edit_shortcuts(app, ui, &s, pt);
    } else {
        let mut nav: Option<Tab> = None;
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

    ui.add_space(18.0);
    let mut go_games = false;
    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("🎮").size(22.0));
            ui.label(
                egui::RichText::new(if pt { "Sincronizar Jogos" } else { "Sync to Games" })
                    .color(theme::text())
                    .size(15.0)
                    .strong(),
            );
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(if pt {
                "Envie suas músicas baixadas para o rádio personalizado do jogo e \
                 ouça suas faixas favoritas enquanto joga (ex.: GTA V)."
            } else {
                "Send your downloaded songs to the game's custom radio and listen \
                 to your favorite tracks while you play (e.g. GTA V)."
            })
            .color(theme::text_muted())
            .size(12.0),
        );
        ui.add_space(8.0);
        if ui
            .add(theme::accent_button(if pt {
                "🎮 Abrir Sincronizar Jogos"
            } else {
                "🎮 Open Sync to Games"
            }))
            .clicked()
        {
            go_games = true;
        }
    });
    if go_games {
        app.active_tab = Tab::Games;
    }

    ui.add_space(20.0);

    ui.label(
        egui::RichText::new(s.home_recents)
            .color(theme::text())
            .size(18.0)
            .strong(),
    );
    ui.add_space(10.0);

    let mut recents: Vec<HistoryEntry> = Vec::new();
    for mt in ["music", "video", "convert"] {
        recents.extend(app.history_for(mt, 10));
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

const CATALOG: [&str; 6] = [
    "music",
    "video",
    "transcribe",
    "converter",
    "queue",
    "folders",
];

fn card_info(id: &str, s: &crate::ui::i18n::Strings) -> Option<(String, Tab)> {
    Some(match id {
        "music" => (format!("🎵  {}", s.nav_music), Tab::Music),
        "video" => (format!("🎬  {}", s.nav_video), Tab::Video),
        "transcribe" => (s.transcribe.to_string(), Tab::Video),
        "converter" => (format!("🔄  {}", s.nav_converter), Tab::Converter),
        "queue" => (format!("📋  {}", s.nav_queue), Tab::Queue),
        "folders" => (format!("📁  {}", s.nav_folders), Tab::Folders),
        _ => return None,
    })
}

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
