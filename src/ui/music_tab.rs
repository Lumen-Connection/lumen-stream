use crate::app::{App, MediaType};
use crate::ui::theme;

pub fn short_link(s: &str) -> String {
    if s.chars().count() > 64 {
        let t: String = s.chars().take(64).collect();
        format!("{}…", t)
    } else {
        s.to_string()
    }
}

pub fn render(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    let s = crate::ui::i18n::s(app.config.lang);

    theme::page_header(ui, s.music_title, s.music_subtitle);
    ui.add_space(20.0);

    if let Some(link) = app.clip_suggest.clone() {
        theme::card_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new(s.clip_detected)
                    .color(theme::accent())
                    .size(12.0)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(short_link(&link))
                    .color(theme::text_muted())
                    .size(12.0),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.add(theme::accent_button(s.download)).clicked() {
                    app.music_url = link.clone();
                    app.clip_suggest = None;
                    app.start_url_download(link.clone(), MediaType::Music);
                }
                if ui.add(theme::ghost_button("✕")).clicked() {
                    app.clip_suggest = None;
                }
            });
        });
        ui.add_space(12.0);
    }

    let mut submit = false;
    let mut transcribe = false;
    theme::card_frame().show(ui, |ui| {
        // Trava no painel: sem teto, o card cresce com o conteúdo e vaza pela
        // borda em janela estreita.
        let cw = ui.available_width();
        ui.set_min_width(cw);
        ui.set_max_width(cw);
        ui.label(
            egui::RichText::new(s.music_link)
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            // Reserva "Colar" + "Download" antes de dimensionar o campo.
            let field_w = (ui.available_width() - 185.0).max(80.0);
            let resp = ui.add_sized(
                egui::vec2(field_w, 40.0),
                egui::TextEdit::singleline(&mut app.music_url)
                    .hint_text("https://...")
                    .text_color(theme::text())
                    .margin(egui::vec2(12.0, 10.0)),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                submit = true;
            }
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("📋").color(theme::text()))
                        .fill(theme::bg_card())
                        .min_size(egui::vec2(40.0, 40.0)),
                )
                .on_hover_text("Colar")
                .clicked()
            {
                if let Some(text) = theme::paste_clipboard() {
                    app.music_url = text.trim().to_string();
                }
            }
            let btn = theme::accent_button(&format!("⤓  {}", s.download))
                .min_size(egui::vec2(120.0, 40.0));
            if ui.add(btn).clicked() {
                submit = true;
            }
        });
        ui.add_space(8.0);
        // `horizontal_wrapped`: os botões passam para a linha de baixo em janela
        // estreita, em vez de sair pela borda.
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(s.transcribe).color(theme::text()).size(14.0),
                    )
                    .fill(theme::accent_soft())
                    .min_size(egui::vec2(150.0, 36.0)),
                )
                .clicked()
            {
                transcribe = true;
            }
            if ui.add(theme::ghost_button(s.paste_download)).clicked() {
                if let Some(text) = theme::paste_clipboard() {
                    let url = text.trim().to_string();
                    if !url.is_empty() {
                        app.music_url = url.clone();
                        app.start_url_download(url, MediaType::Music);
                    }
                }
            }
            if app.last_download.is_some()
                && ui.add(theme::ghost_button(s.repeat_last)).clicked()
            {
                app.repeat_last_download();
            }
            if ui.add(theme::ghost_button(s.btn_clear_temp)).clicked() {
                app.clear_temp_files_toast();
            }
        });
    });
    if submit {
        let url = app.music_url.clone();
        app.start_url_download(url, MediaType::Music);
    }
    if transcribe {
        let url = app.music_url.clone();
        app.start_url_transcribe(url, MediaType::Music);
    }

    ui.add_space(20.0);

    let history = app.history_for("music", app.config.max_history);
    crate::ui::history::render(
        app,
        ui,
        "music",
        s.col_title,
        s.hist_downloads,
        &history,
        Some(MediaType::Music),
    );
}
