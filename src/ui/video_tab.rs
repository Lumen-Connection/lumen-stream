use crate::app::{App, MediaType};
use crate::ui::theme;

pub fn render(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    let s = crate::ui::i18n::s(app.config.lang);

    ui.label(
        egui::RichText::new(s.video_title)
            .color(theme::text())
            .size(30.0)
            .strong(),
    );
    ui.label(
        egui::RichText::new(s.video_subtitle)
            .color(theme::text_muted())
            .size(14.0),
    );
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
                egui::RichText::new(crate::ui::music_tab::short_link(&link))
                    .color(theme::text_muted())
                    .size(12.0),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.add(theme::accent_button(s.download)).clicked() {
                    app.video_url = link.clone();
                    app.clip_suggest = None;
                    app.start_url_download(link.clone(), MediaType::Video);
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
    let mut thumbnail = false;
    let mut inspect = false;
    theme::card_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(s.video_link)
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let resp = ui.add_sized(
                egui::vec2(ui.available_width() - 185.0, 40.0),
                egui::TextEdit::singleline(&mut app.video_url)
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
                    app.video_url = text.trim().to_string();
                }
            }
            let btn = theme::accent_button(&format!("⤓  {}", s.download))
                .min_size(egui::vec2(120.0, 40.0));
            if ui.add(btn).clicked() {
                submit = true;
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(s.transcribe).color(theme::accent()).size(14.0),
                    )
                    .fill(theme::accent_soft())
                    .min_size(egui::vec2(150.0, 36.0)),
                )
                .clicked()
            {
                transcribe = true;
            }
            if ui.add(theme::ghost_button(s.thumbnail_only)).clicked() {
                thumbnail = true;
            }
            if ui.add(theme::ghost_button(s.inspect_formats)).clicked() {
                inspect = true;
            }
            if ui.add(theme::ghost_button(s.paste_download)).clicked() {
                if let Some(text) = theme::paste_clipboard() {
                    let url = text.trim().to_string();
                    if !url.is_empty() {
                        app.video_url = url.clone();
                        app.start_url_download(url, MediaType::Video);
                    }
                }
            }
            if app.last_download.is_some()
                && ui.add(theme::ghost_button(s.repeat_last)).clicked()
            {
                app.repeat_last_download();
            }
        });
    });
    if submit {
        let url = app.video_url.clone();
        app.start_url_download(url, MediaType::Video);
    }
    if transcribe {
        let url = app.video_url.clone();
        app.start_url_transcribe(url, MediaType::Video);
    }
    if thumbnail {
        let url = app.video_url.clone();
        app.start_url_thumbnail(url);
    }
    if inspect {
        let url = app.video_url.clone();
        app.start_inspect(url);
    }

    ui.add_space(20.0);

    let history = app.db.get_history("video", app.config.max_history);
    crate::ui::history::render(
        app,
        ui,
        "video",
        s.col_title,
        s.hist_downloads,
        &history,
        Some(MediaType::Video),
    );
}
