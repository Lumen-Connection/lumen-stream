use std::path::PathBuf;

use crate::app::{App, Tab};
use crate::ui::theme;

/// Interface do "Modo Games": versão compacta e minimalista, focada no Início
/// com atalhos rápidos — mas com acesso a todos os recursos (as demais abas são
/// renderizadas normalmente ao serem abertas; L1/R1 também trocam de aba).
pub fn render(app: &mut App, ctx: &egui::Context) {
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    ctx.request_repaint_after(std::time::Duration::from_millis(60));

    top_bar(app, ctx, pt);

    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(theme::bg_app())
                .inner_margin(egui::Margin::symmetric(28.0, 18.0)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if app.active_tab == Tab::Home {
                    games_home(app, ui, pt);
                } else {
                    let tab = app.active_tab;
                    crate::ui::dashboard::render_tab_content(app, ctx, ui, tab);
                }
            });
        });
}

fn top_bar(app: &mut App, ctx: &egui::Context, pt: bool) {
    egui::TopBottomPanel::top("games_top")
        .frame(
            egui::Frame::none()
                .fill(theme::accent_soft())
                .inner_margin(egui::Margin::symmetric(18.0, 8.0)),
        )
        .show(ctx, |ui| {
            let mut go_home = false;
            let mut exit = false;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if pt { "🎮  Modo Games" } else { "🎮  Games Mode" })
                        .color(theme::accent())
                        .size(16.0)
                        .strong(),
                );
                ui.add_space(8.0);
                let status = if app.gamepad.connected {
                    format!("● {}", app.gamepad.name)
                } else if pt {
                    "○ sem controle".to_string()
                } else {
                    "○ no gamepad".to_string()
                };
                ui.label(
                    egui::RichText::new(status)
                        .color(if app.gamepad.connected {
                            theme::accent()
                        } else {
                            theme::text_faint()
                        })
                        .size(12.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(theme::ghost_button(if pt { "Sair (PS)" } else { "Exit (PS)" }))
                        .clicked()
                    {
                        exit = true;
                    }
                    if app.active_tab != Tab::Home
                        && ui
                            .add(theme::ghost_button(if pt { "🏠 Início" } else { "🏠 Home" }))
                            .clicked()
                    {
                        go_home = true;
                    }
                });
            });
            if go_home {
                app.active_tab = Tab::Home;
            }
            if exit {
                app.toggle_gamepad_mode();
            }
        });
}

fn games_home(app: &mut App, ui: &mut egui::Ui, pt: bool) {
    ui.label(
        egui::RichText::new(if pt { "Início" } else { "Home" })
            .color(theme::text())
            .size(26.0)
            .strong(),
    );
    ui.label(
        egui::RichText::new(if pt { "Atalhos rápidos" } else { "Quick shortcuts" })
            .color(theme::text_muted())
            .size(14.0),
    );
    ui.add_space(14.0);

    let shortcuts: [(&str, &str, Tab); 6] = [
        ("🎵", if pt { "Música" } else { "Music" }, Tab::Music),
        ("🎬", if pt { "Vídeo" } else { "Video" }, Tab::Video),
        ("🔄", if pt { "Converter" } else { "Convert" }, Tab::Converter),
        ("📋", if pt { "Fila" } else { "Queue" }, Tab::Queue),
        ("🖼", if pt { "Galeria" } else { "Gallery" }, Tab::Gallery),
        ("⚙", if pt { "Config" } else { "Settings" }, Tab::Settings),
    ];

    let cols = (((ui.available_width() + 14.0) / 200.0).floor() as usize).clamp(2, 3);
    let mut goto: Option<Tab> = None;
    for chunk in shortcuts.chunks(cols) {
        ui.columns(cols, |c| {
            for (k, (icon, label, tab)) in chunk.iter().enumerate() {
                let ui = &mut c[k];
                let resp = ui.add(
                    egui::Button::new(
                        egui::RichText::new(format!("{}\n{}", icon, label))
                            .color(theme::text())
                            .size(18.0),
                    )
                    .fill(theme::bg_card())
                    .rounding(egui::Rounding::same(12.0))
                    .min_size(egui::vec2(ui.available_width(), 86.0)),
                );
                if resp.clicked() {
                    goto = Some(*tab);
                }
            }
        });
        ui.add_space(12.0);
    }
    if let Some(t) = goto {
        app.active_tab = t;
    }

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(if pt { "Baixados recentes" } else { "Recent downloads" })
            .color(theme::text())
            .size(18.0)
            .strong(),
    );
    ui.add_space(8.0);

    let history = app.db.get_history("music", 8);
    let audio: Vec<&crate::db::database::HistoryEntry> = history
        .iter()
        .filter(|e| crate::player::is_playable_audio(&e.format))
        .collect();

    if audio.is_empty() {
        ui.label(
            egui::RichText::new(if pt {
                "Nenhuma música baixada ainda."
            } else {
                "No music downloaded yet."
            })
            .color(theme::text_faint())
            .size(14.0),
        );
        return;
    }

    for entry in audio {
        let playing = app
            .mini
            .path
            .as_ref()
            .map(|p| p.to_string_lossy() == entry.file_path)
            .unwrap_or(false);
        egui::Frame::none()
            .fill(if playing {
                theme::accent_soft()
            } else {
                theme::bg_card()
            })
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(if playing { "🎧" } else { "▶" })
                                    .size(15.0)
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(theme::accent())
                            .min_size(egui::vec2(38.0, 30.0)),
                        )
                        .clicked()
                    {
                        app.mini.play(PathBuf::from(&entry.file_path));
                    }
                    ui.add_space(6.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&entry.title).color(theme::text()).size(14.0),
                        )
                        .truncate(true),
                    );
                });
            });
        ui.add_space(6.0);
    }
}
