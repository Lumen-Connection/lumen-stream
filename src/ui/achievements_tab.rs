use egui::Stroke;

use crate::app::App;
use crate::ui::i18n::Lang;
use crate::ui::theme;

struct Badge {
    icon: &'static str,
    name: String,
    desc: String,
    unlocked: bool,
}

fn build_badges(pt: bool, total: i64, mc: i64, vc: i64, cc: i64) -> Vec<Badge> {
    let mut v: Vec<Badge> = Vec::new();
    let tr = |p: &str, e: &str| if pt { p.to_string() } else { e.to_string() };

    let total_tiers: &[(i64, &str, &str)] = &[
        (1, "Iniciante", "Beginner"),
        (5, "Aprendiz", "Apprentice"),
        (10, "Colecionador", "Collector"),
        (25, "Entusiasta", "Enthusiast"),
        (50, "Veterano", "Veteran"),
        (75, "Experiente", "Experienced"),
        (100, "Centurião", "Centurion"),
        (150, "Dedicado", "Dedicated"),
        (200, "Mestre", "Master"),
        (300, "Grão-mestre", "Grandmaster"),
        (400, "Lenda", "Legend"),
        (500, "Mítico", "Mythic"),
        (750, "Épico", "Epic"),
        (1000, "Milenar", "Millennial"),
        (1500, "Imortal", "Immortal"),
        (2000, "Divino", "Divine"),
        (3000, "Titânico", "Titanic"),
        (5000, "Cósmico", "Cosmic"),
        (7500, "Galáctico", "Galactic"),
        (10000, "Universal", "Universal"),
    ];
    for (n, p, e) in total_tiers {
        v.push(Badge {
            icon: "🏆",
            name: tr(p, e),
            desc: format!("{} downloads", n),
            unlocked: total >= *n,
        });
    }

    let music_tiers: &[(i64, &str, &str)] = &[
        (1, "Ouvinte", "Listener"),
        (5, "Fã", "Fan"),
        (10, "Melômano", "Audiophile"),
        (25, "DJ", "DJ"),
        (50, "Maestro", "Maestro"),
        (100, "Discoteca", "Disco"),
        (200, "Fonoteca", "Sound library"),
        (500, "Lenda musical", "Music legend"),
        (1000, "Imortal sonoro", "Sound immortal"),
    ];
    for (n, p, e) in music_tiers {
        v.push(Badge {
            icon: "🎧",
            name: tr(p, e),
            desc: if pt { format!("{} músicas", n) } else { format!("{} songs", n) },
            unlocked: mc >= *n,
        });
    }

    let video_tiers: &[(i64, &str, &str)] = &[
        (1, "Espectador", "Viewer"),
        (5, "Cinéfilo", "Cinephile"),
        (10, "Diretor", "Director"),
        (25, "Produtor", "Producer"),
        (50, "Maratonista", "Binge-watcher"),
        (100, "Crítico", "Critic"),
        (200, "Cineasta", "Filmmaker"),
        (500, "Lenda do cinema", "Cinema legend"),
        (1000, "Imortal do cinema", "Cinema immortal"),
    ];
    for (n, p, e) in video_tiers {
        v.push(Badge {
            icon: "🎬",
            name: tr(p, e),
            desc: if pt { format!("{} vídeos", n) } else { format!("{} videos", n) },
            unlocked: vc >= *n,
        });
    }

    let convert_tiers: &[(i64, &str, &str)] = &[
        (1, "Conversor", "Converter"),
        (3, "Alquimista", "Alchemist"),
        (5, "Transformador", "Transformer"),
        (10, "Engenheiro", "Engineer"),
        (25, "Reformulador", "Reshaper"),
        (50, "Mágico", "Magician"),
        (100, "Camaleão", "Chameleon"),
        (200, "Arquimago", "Archmage"),
        (500, "Onipotente", "Omnipotent"),
    ];
    for (n, p, e) in convert_tiers {
        v.push(Badge {
            icon: "🔄",
            name: tr(p, e),
            desc: if pt { format!("{} conversões", n) } else { format!("{} conversions", n) },
            unlocked: cc >= *n,
        });
    }

    let combos: &[(&str, &str, &str, &str, &str, bool)] = &[
        ("🎉", "Estreia", "Debut", "Seu 1º download", "Your 1st download", total >= 1),
        ("🧩", "Tripé", "Tripod", "1 de cada tipo", "1 of each type", mc >= 1 && vc >= 1 && cc >= 1),
        ("🎛", "Versátil", "Versatile", "5 de cada tipo", "5 of each type", mc >= 5 && vc >= 5 && cc >= 5),
        ("⚖", "Equilíbrio", "Balance", "10 de cada tipo", "10 of each type", mc >= 10 && vc >= 10 && cc >= 10),
        ("🍽", "Onívoro", "Omnivore", "25 de cada tipo", "25 of each type", mc >= 25 && vc >= 25 && cc >= 25),
        ("🧠", "Mestre multimídia", "Multimedia master", "20 de cada tipo", "20 of each type", mc >= 20 && vc >= 20 && cc >= 20),
        ("📺", "Multimídia", "Multimedia", "50 músicas e 50 vídeos", "50 songs & 50 videos", mc >= 50 && vc >= 50),
        ("🎯", "Trifeta", "Trifecta", "100 músicas, 100 vídeos, 50 conversões", "100 songs, 100 videos, 50 conversions", mc >= 100 && vc >= 100 && cc >= 50),
        ("🔥", "Em chamas", "On fire", "50 downloads", "50 downloads", total >= 50),
        ("🏃", "Maratona", "Marathon", "250 downloads", "250 downloads", total >= 250),
        ("🌟", "Lenda viva", "Living legend", "500 downloads", "500 downloads", total >= 500),
        ("🚀", "Fora de série", "Off the charts", "2000 downloads", "2000 downloads", total >= 2000),
        ("♾", "Insaciável", "Insatiable", "5000 downloads", "5000 downloads", total >= 5000),
    ];
    for (icon, np, ne, dp, de, unlocked) in combos {
        v.push(Badge {
            icon,
            name: tr(np, ne),
            desc: tr(dp, de),
            unlocked: *unlocked,
        });
    }

    v
}

pub fn render(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    let s = crate::ui::i18n::s(app.config.lang);
    let pt = app.config.lang == Lang::Pt;

    let mc = app.history_count("music");
    let vc = app.history_count("video");
    let cc = app.history_count("convert");
    let total = mc + vc + cc;

    theme::page_header(
        ui,
        s.nav_achievements,
        if pt {
            "Desbloqueie conquistas usando o Lumen."
        } else {
            "Unlock achievements as you use Lumen."
        },
    );
    ui.add_space(20.0);

    let badges = build_badges(pt, total, mc, vc, cc);

    let done = badges.iter().filter(|b| b.unlocked).count();
    ui.label(
        egui::RichText::new(format!(
            "{} {} / {}",
            if pt { "Desbloqueadas:" } else { "Unlocked:" },
            done,
            badges.len()
        ))
        .color(theme::text_faint())
        .size(12.0),
    );
    ui.add_space(12.0);

    let avail = ui.available_width();
    let col_w = CARD_W + 28.0 + 12.0;
    let cols = ((avail / col_w).floor() as usize).max(1);

    // Todos os cards reservam a altura da descrição mais longa (a "Trifeta"
    // quebra em duas linhas; as demais, em uma). Sem a reserva, só ela fica mais
    // alta e destoa da grade. Medido a cada frame porque depende do idioma, da
    // escala da interface e da fonte — um número fixo voltaria a estourar.
    let desc_h = badges
        .iter()
        .map(|b| {
            ui.fonts(|f| {
                f.layout(
                    b.desc.clone(),
                    egui::FontId::proportional(11.0),
                    theme::text_faint(),
                    CARD_W,
                )
                .rect
                .height()
            })
        })
        .fold(0.0_f32, f32::max);

    egui::Grid::new("badges_grid")
        .spacing(egui::vec2(12.0, 12.0))
        .show(ui, |ui| {
            for (i, b) in badges.iter().enumerate() {
                badge_card(ui, b, pt, desc_h);
                if (i + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
}

/// Largura do conteúdo do card — também é a largura em que a descrição quebra.
const CARD_W: f32 = 150.0;

/// `desc_h`: altura reservada para a descrição, igual em todos os cards.
fn badge_card(ui: &mut egui::Ui, b: &Badge, pt: bool, desc_h: f32) {
    let (fill, brd, icon_col, name_col) = if b.unlocked {
        (theme::accent_soft(), theme::accent(), theme::accent(), theme::text())
    } else {
        (theme::bg_card(), theme::border(), theme::text_faint(), theme::text_faint())
    };

    egui::Frame::none()
        .fill(fill)
        .rounding(egui::Rounding::same(theme::CARD_ROUNDING))
        .stroke(Stroke::new(1.0_f32, brd))
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_width(CARD_W);
            ui.set_min_height(120.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(b.icon).size(34.0).color(icon_col));
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(b.name.as_str())
                        .size(14.0)
                        .strong()
                        .color(name_col),
                );
                // Bloco de altura fixa: a descrição curta sobra espaço em vez de
                // encolher o card, e a longa cabe sem esticá-lo.
                ui.allocate_ui_with_layout(
                    egui::vec2(CARD_W, desc_h),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        ui.set_min_height(desc_h);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(b.desc.as_str()).size(11.0).color(
                                    if b.unlocked {
                                        theme::text_muted()
                                    } else {
                                        theme::text_faint()
                                    },
                                ),
                            )
                            .wrap(true),
                        );
                    },
                );
                ui.add_space(4.0);
                let (status, col) = if b.unlocked {
                    (if pt { "✔ Conquistado" } else { "✔ Unlocked" }, theme::accent())
                } else {
                    (if pt { "🔒 Bloqueado" } else { "🔒 Locked" }, theme::text_faint())
                };
                ui.label(egui::RichText::new(status).size(11.0).strong().color(col));
            });
        });
}
