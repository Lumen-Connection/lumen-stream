use std::collections::HashMap;

use crate::app::App;
use crate::ui::i18n::Lang;
use crate::ui::theme;

pub fn render(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    let s = crate::ui::i18n::s(app.config.lang);
    let pt = app.config.lang == Lang::Pt;
    let fmt = crate::download::engine::format_size;

    ui.label(
        egui::RichText::new(s.nav_stats)
            .color(theme::text())
            .size(30.0)
            .strong(),
    );
    ui.label(
        egui::RichText::new(if pt {
            "Um resumo discreto e detalhado da sua atividade."
        } else {
            "A discreet, detailed summary of your activity."
        })
        .color(theme::text_muted())
        .size(14.0),
    );
    ui.add_space(20.0);

    let all = app.db.all_active_history();
    let now = chrono::Local::now().naive_local();
    let parse = |s: &str| {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()
    };

    let (mut mc, mut vc, mut cc, mut tx) = (0i64, 0i64, 0i64, 0i64);
    let (mut ms, mut vs, mut cs) = (0i64, 0i64, 0i64);
    let (mut today, mut week, mut month) = (0i64, 0i64, 0i64);
    let mut largest: Option<(&str, i64)> = None;
    let mut formats: HashMap<String, i64> = HashMap::new();
    let mut sites: HashMap<String, i64> = HashMap::new();
    let mut first: Option<chrono::NaiveDateTime> = None;
    let mut last: Option<chrono::NaiveDateTime> = None;

    for e in &all {
        match e.media_type.as_str() {
            "music" => {
                mc += 1;
                ms += e.file_size.unwrap_or(0);
            }
            "video" => {
                vc += 1;
                vs += e.file_size.unwrap_or(0);
            }
            "convert" => {
                cc += 1;
                cs += e.file_size.unwrap_or(0);
            }
            _ => {}
        }
        if e.format == "txt" {
            tx += 1;
        }
        if let Some(sz) = e.file_size {
            if largest.map(|(_, s)| sz > s).unwrap_or(true) {
                largest = Some((&e.title, sz));
            }
        }
        if !e.format.is_empty() {
            *formats.entry(e.format.clone()).or_insert(0) += 1;
        }
        if let Some(d) = domain(&e.url) {
            *sites.entry(d).or_insert(0) += 1;
        }
        if let Some(dt) = parse(&e.created_at) {
            let age = now.signed_duration_since(dt);
            if age.num_hours() < 24 {
                today += 1;
            }
            if age.num_days() < 7 {
                week += 1;
            }
            if age.num_days() < 30 {
                month += 1;
            }
            first = Some(first.map_or(dt, |f| f.min(dt)));
            last = Some(last.map_or(dt, |l| l.max(dt)));
        }
    }

    let total = mc + vc + cc;
    let total_size = ms + vs + cs;
    let avg = if total > 0 { total_size / total } else { 0 };

    let dfmt = |d: Option<chrono::NaiveDateTime>| {
        d.map(|x| x.format("%d/%m/%Y").to_string())
            .unwrap_or_else(|| "—".to_string())
    };

    let ncols = if ui.available_width() > 880.0 { 2 } else { 1 };
    let right_idx = if ncols == 2 { 1 } else { 0 };

    ui.columns(ncols, |cols| {
        let left = &mut cols[0];
        card(left, |ui| {
            section(ui, if pt { "Resumo" } else { "Summary" });
            kv(ui, if pt { "Total de downloads" } else { "Total downloads" }, total.to_string());
            kv(ui, if pt { "Músicas" } else { "Music" }, format!("{} · {}", mc, fmt(ms)));
            kv(ui, if pt { "Vídeos" } else { "Videos" }, format!("{} · {}", vc, fmt(vs)));
            kv(ui, if pt { "Conversões" } else { "Conversions" }, format!("{} · {}", cc, fmt(cs)));
            kv(ui, if pt { "Transcrições" } else { "Transcripts" }, tx.to_string());
            kv(ui, if pt { "Espaço total" } else { "Total size" }, fmt(total_size));
            kv(ui, if pt { "Tamanho médio" } else { "Average size" }, fmt(avg));
        });
        left.add_space(12.0);

        if let Some((title, sz)) = largest {
            card(left, |ui| {
                section(ui, if pt { "Maior arquivo" } else { "Largest file" });
                ui.label(
                    egui::RichText::new(crate::ui::music_tab::short_link(title))
                        .color(theme::text())
                        .size(13.0),
                );
                ui.label(egui::RichText::new(fmt(sz)).color(theme::text_faint()).size(12.0));
            });
            left.add_space(12.0);
        }

        if !formats.is_empty() {
            card(left, |ui| {
                section(ui, if pt { "Formatos mais usados" } else { "Top formats" });
                let max = formats.values().copied().max().unwrap_or(1);
                for (name, count) in top(&formats, 6) {
                    bar_row(ui, &name, count, max);
                }
            });
        }

        let right = &mut cols[right_idx];
        card(right, |ui| {
            section(ui, if pt { "Atividade" } else { "Activity" });
            kv(ui, if pt { "Hoje" } else { "Today" }, today.to_string());
            kv(ui, if pt { "Últimos 7 dias" } else { "Last 7 days" }, week.to_string());
            kv(ui, if pt { "Últimos 30 dias" } else { "Last 30 days" }, month.to_string());
            kv(ui, if pt { "Primeiro download" } else { "First download" }, dfmt(first));
            kv(ui, if pt { "Último download" } else { "Last download" }, dfmt(last));
        });
        right.add_space(12.0);

        if !sites.is_empty() {
            card(right, |ui| {
                section(ui, if pt { "Sites mais usados" } else { "Top sites" });
                let max = sites.values().copied().max().unwrap_or(1);
                for (name, count) in top(&sites, 6) {
                    bar_row(ui, &name, count, max);
                }
            });
        }
    });
}

fn domain(url: &str) -> Option<String> {
    let u = url.trim();
    if u.is_empty() {
        return None;
    }
    let after = u.split("://").nth(1).unwrap_or(u);
    let host = after.split('/').next().unwrap_or(after);
    let host = host.strip_prefix("www.").unwrap_or(host);
    if host.is_empty() {
        None
    } else {
        Some(host.to_lowercase())
    }
}

fn top(map: &HashMap<String, i64>, n: usize) -> Vec<(String, i64)> {
    let mut v: Vec<(String, i64)> = map.iter().map(|(k, c)| (k.clone(), *c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width() - 1.0);
        add(ui);
    });
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .color(theme::text_muted())
            .size(11.0)
            .strong(),
    );
    ui.add_space(6.0);
}

fn kv(ui: &mut egui::Ui, label: &str, value: String) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .color(theme::text_muted())
                .size(12.0),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value)
                    .color(theme::text_faint())
                    .size(12.0),
            );
        });
    });
    ui.add_space(3.0);
}

fn bar_row(ui: &mut egui::Ui, label: &str, count: i64, max: i64) {
    ui.horizontal(|ui| {
        ui.add_sized(
            egui::vec2(110.0, 16.0),
            egui::Label::new(
                egui::RichText::new(label).color(theme::text_muted()).size(12.0),
            ),
        );
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(200.0, 10.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(3.0), theme::bg_card_hover());
        let frac = if max > 0 { count as f32 / max as f32 } else { 0.0 };
        let fill = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(rect.width() * frac, rect.height()),
        );
        ui.painter()
            .rect_filled(fill, egui::Rounding::same(3.0), theme::accent());
        ui.label(
            egui::RichText::new(count.to_string())
                .color(theme::text_faint())
                .size(12.0),
        );
    });
    ui.add_space(4.0);
}
