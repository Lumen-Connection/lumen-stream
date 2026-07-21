use crate::app::{App, MediaType};
use crate::queue::{self, JobStatus};
use crate::ui::theme;

pub fn render(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    let s = crate::ui::i18n::s(app.config.lang);

    theme::page_header(ui, s.queue_title, s.queue_subtitle);
    ui.add_space(20.0);

    theme::card_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(s.queue_input_label)
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(8.0);
        ui.add(
            egui::TextEdit::multiline(&mut app.batch_input)
                .hint_text(s.queue_hint)
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .text_color(theme::text()),
        );
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            ui.label(s.queue_type);
            if type_button(ui, s.type_music, app.batch_media_type == MediaType::Music) {
                app.batch_media_type = MediaType::Music;
                app.batch_format = app.config.music_format.clone();
            }
            if type_button(ui, s.type_video, app.batch_media_type == MediaType::Video) {
                app.batch_media_type = MediaType::Video;
                app.batch_format = app.config.video_format.clone();
            }
        });
        ui.add_space(6.0);

        let formats: Vec<(&str, &str)> = if app.batch_media_type == MediaType::Music {
            vec![("mp3", "mp3"), ("m4a", "m4a"), ("opus", "opus"), ("flac", "flac")]
        } else {
            crate::download::engine::video_profiles()
                .iter()
                .map(|profile| (profile.extension, profile.label))
                .collect()
        };
        ui.horizontal(|ui| {
            ui.label("Formato:");
            for (format, label) in formats {
                let selected = app.batch_format == format;
                let fill = if selected { theme::accent() } else { theme::bg_card() };
                if ui.add(egui::Button::new(label).fill(fill)).clicked() {
                    app.batch_format = format.to_string();
                }
            }
        });
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label(s.settings_quality);
            for q in ["best", "high", "medium"] {
                let selected = app.batch_quality == q;
                let fill = if selected { theme::accent() } else { theme::bg_card() };
                if ui.add(egui::Button::new(q).fill(fill)).clicked() {
                    app.batch_quality = q.to_string();
                }
            }
        });
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            if ui
                .add(theme::accent_button(s.queue_add).min_size(egui::vec2(160.0, 38.0)))
                .clicked()
            {
                enqueue_input(app);
            }
            if ui.add(theme::ghost_button(s.queue_clear_done)).clicked() {
                app.queue.clear_finished();
            }
        });
    });

    ui.add_space(20.0);

    render_jobs(app, ui, s);
}

fn enqueue_input(app: &mut App) {
    let lines: Vec<String> = app
        .batch_input
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    let media_type = app.batch_media_type;
    let format = app.batch_format.clone();
    let quality = app.batch_quality.clone();
    let folder = app.config.default_download_dir.clone();

    for url in lines {
        if queue::is_playlist(&url) {
            let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
            match (app.engine.clone(), queue::playlist_id_from_url(&url)) {
                (Some(engine), Some(pid)) => {
                    let jobs = app.queue.jobs.clone();
                    let next_id = app.queue.next_id.clone();
                    let toasts = app.toast_queue.clone();
                    let (mt, fmt, q, dir) =
                        (media_type, format.clone(), quality.clone(), folder.clone());
                    // Feedback imediato: a busca da playlist roda em background e
                    // antes falhava/pendurava em silêncio, sem nada aparecer na fila.
                    app.toast(
                        if pt { "Buscando itens da playlist..." } else { "Fetching playlist items..." },
                        false,
                    );
                    tokio::spawn(async move {
                        let result = match engine.fetch_playlist(&pid).await {
                            Ok(items) if !items.is_empty() => {
                                let n = items.len();
                                for (u, t) in items {
                                    queue::push_job(
                                        &jobs, &next_id, u, t, mt, fmt.clone(), q.clone(), dir.clone(),
                                    );
                                }
                                (
                                    if pt {
                                        format!("{} itens da playlist adicionados à fila", n)
                                    } else {
                                        format!("{} playlist items added to the queue", n)
                                    },
                                    false,
                                )
                            }
                            Ok(_) => (
                                if pt { "Playlist vazia ou indisponível.".to_string() }
                                else { "Playlist empty or unavailable.".to_string() },
                                true,
                            ),
                            Err(e) => {
                                let m = crate::download::engine::friendly_error(&e.to_string());
                                (
                                    if pt { format!("Falha ao buscar a playlist: {}", m) }
                                    else { format!("Failed to fetch playlist: {}", m) },
                                    true,
                                )
                            }
                        };
                        toasts.lock().unwrap().push(result);
                    });
                }
                _ => app.toast(
                    if pt {
                        "Aguarde o app terminar de iniciar e tente novamente."
                    } else {
                        "Wait for the app to finish starting and try again."
                    },
                    true,
                ),
            }
        } else {
            app.queue.add(
                url,
                String::new(),
                media_type,
                format.clone(),
                quality.clone(),
                folder.clone(),
            );
        }
    }

    app.batch_input.clear();
}

fn render_jobs(app: &mut App, ui: &mut egui::Ui, s: &crate::ui::i18n::Strings) {
    let snapshot: Vec<(u64, String, String, JobStatus, Option<f32>, f32, u64)> = app
        .queue
        .jobs
        .lock()
        .unwrap()
        .iter()
        .map(|j| {
            let title = if j.title.is_empty() {
                j.url.clone()
            } else {
                j.title.clone()
            };
            (j.id, title, j.format.clone(), j.status.clone(), j.progress, j.speed, j.eta)
        })
        .collect();

    ui.label(
        egui::RichText::new(s.queue_items)
            .color(theme::text())
            .size(18.0)
            .strong(),
    );
    ui.add_space(10.0);

    if snapshot.is_empty() {
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new(s.queue_empty).color(theme::text_faint()));
        });
        return;
    }

    let mut cancel_id: Option<u64> = None;
    let mut move_action: Option<(u64, bool)> = None;
    let mut top_action: Option<u64> = None;
    let mut pause_id: Option<u64> = None;
    let mut resume_id: Option<u64> = None;

    theme::card_frame().show(ui, |ui| {
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (id, title, format, status, progress, speed, eta) in &snapshot {
                ui.horizontal(|ui| {
                    let (label, color) = status_label(status, s);
                    ui.add_sized(
                        egui::vec2(90.0, 18.0),
                        egui::Label::new(egui::RichText::new(label).color(color).size(12.0)),
                    );

                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(truncate(title, 70))
                                .color(theme::text())
                                .size(13.0),
                        );
                        match status {
                            JobStatus::Running => {
                                match progress {
                                    Some(p) => {
                                        ui.add(
                                            egui::ProgressBar::new(*p)
                                                .desired_width(330.0)
                                                .fill(theme::accent())
                                                .show_percentage(),
                                        );
                                    }
                                    None => {
                                        ui.add(
                                            egui::ProgressBar::new(0.0)
                                                .desired_width(330.0)
                                                .fill(theme::accent())
                                                .animate(true),
                                        );
                                    }
                                }
                                if *speed > 0.0 {
                                    let eta_txt = if *eta > 0 {
                                        format!(" · ETA {}:{:02}", eta / 60, eta % 60)
                                    } else {
                                        String::new()
                                    };
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{}/s{}",
                                            crate::download::engine::format_size(*speed as i64),
                                            eta_txt
                                        ))
                                        .color(theme::text_muted())
                                        .size(11.0),
                                    );
                                }
                            }
                            JobStatus::Failed(e) => {
                                ui.label(
                                    egui::RichText::new(truncate(e, 70))
                                        .color(theme::danger())
                                        .size(11.0),
                                );
                            }
                            _ => {
                                ui.label(
                                    egui::RichText::new(format.as_str())
                                        .color(theme::text_faint())
                                        .size(11.0),
                                );
                            }
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if matches!(
                            status,
                            JobStatus::Queued | JobStatus::Running | JobStatus::Paused
                        ) {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("✕").color(theme::text()),
                                    )
                                    .fill(theme::bg_card())
                                    .min_size(egui::vec2(30.0, 26.0)),
                                )
                                .clicked()
                            {
                                cancel_id = Some(*id);
                            }
                            if matches!(status, JobStatus::Running) {
                                if icon_btn(ui, "⏸") {
                                    pause_id = Some(*id);
                                }
                            } else if matches!(status, JobStatus::Paused) {
                                if icon_btn(ui, "▶") {
                                    resume_id = Some(*id);
                                }
                            }
                            if matches!(status, JobStatus::Queued) {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("⚡").color(theme::accent()),
                                        )
                                        .fill(theme::bg_card())
                                        .min_size(egui::vec2(26.0, 26.0)),
                                    )
                                    .on_hover_text(if app.config.lang == crate::ui::i18n::Lang::Pt {
                                        "Baixar agora"
                                    } else {
                                        "Download now"
                                    })
                                    .clicked()
                                {
                                    top_action = Some(*id);
                                }
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("▼").color(theme::text()),
                                        )
                                        .fill(theme::bg_card())
                                        .min_size(egui::vec2(26.0, 26.0)),
                                    )
                                    .clicked()
                                {
                                    move_action = Some((*id, false));
                                }
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("▲").color(theme::text()),
                                        )
                                        .fill(theme::bg_card())
                                        .min_size(egui::vec2(26.0, 26.0)),
                                    )
                                    .clicked()
                                {
                                    move_action = Some((*id, true));
                                }
                            }
                        } else if matches!(status, JobStatus::Completed(_)) {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("📁").color(theme::text()),
                                    )
                                    .fill(theme::bg_card())
                                    .min_size(egui::vec2(30.0, 26.0)),
                                )
                                .clicked()
                            {
                                if let JobStatus::Completed(path) = status {
                                    if let Some(parent) = std::path::Path::new(path).parent() {
                                        open::that(parent).ok();
                                    }
                                }
                            }
                        }
                    });
                });
                ui.separator();
            }
        });
    });

    if let Some(id) = cancel_id {
        app.queue.cancel(id);
    }
    if let Some(id) = pause_id {
        app.queue.pause(id);
    }
    if let Some(id) = resume_id {
        app.queue.resume(id);
    }
    if let Some((id, up)) = move_action {
        if up {
            app.queue.move_up(id);
        } else {
            app.queue.move_down(id);
        }
    }
    if let Some(id) = top_action {
        app.queue.move_to_top(id);
    }
}

fn icon_btn(ui: &mut egui::Ui, icon: &str) -> bool {
    ui.add(
        egui::Button::new(egui::RichText::new(icon).color(theme::text()))
            .fill(theme::bg_card())
            .min_size(egui::vec2(26.0, 26.0)),
    )
    .clicked()
}

fn status_label(status: &JobStatus, s: &crate::ui::i18n::Strings) -> (&'static str, egui::Color32) {
    match status {
        JobStatus::Queued => (s.st_queued, theme::text_muted()),
        JobStatus::Running => (s.st_running, theme::accent()),
        JobStatus::Paused => (s.st_paused, theme::text_muted()),
        JobStatus::Completed(_) => (s.st_completed, theme::accent()),
        JobStatus::Failed(_) => (s.st_failed, theme::danger()),
        JobStatus::Cancelled => (s.st_cancelled, theme::text_faint()),
    }
}

fn type_button(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let fill = if selected { theme::accent() } else { theme::bg_card() };
    ui.add(egui::Button::new(label).fill(fill)).clicked()
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let s: String = text.chars().take(max).collect();
        format!("{}…", s)
    } else {
        text.to_string()
    }
}
