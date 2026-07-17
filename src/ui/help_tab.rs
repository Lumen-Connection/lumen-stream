use crate::app::App;
use crate::ui::i18n::Lang;
use crate::ui::theme;

pub fn render(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    let s = crate::ui::i18n::s(app.config.lang);
    let pt = app.config.lang == Lang::Pt;

    theme::page_header(
        ui,
        s.nav_help,
        if pt {
            "Perguntas frequentes, atalhos e ferramentas de suporte."
        } else {
            "FAQ, shortcuts and support tools."
        },
    );
    ui.add_space(20.0);

    let faq: &[(&str, &str)] = if pt {
        &[
            ("Como baixar um vídeo ou música?",
             "Cole o link na aba Baixar Vídeo/Música e clique em Download. Você pode ajustar nome, pasta, formato e qualidade antes de confirmar."),
            ("O download falhou. O que fazer?",
             "Em Configurações → Manutenção, clique em \"Atualizar yt-dlp\" (o YouTube muda com frequência). Veja também o \"Modo de diagnóstico\" e o log."),
            ("Como baixar de uma playlist?",
             "Cole o link da playlist na aba Fila e escolha o tipo (Música/Vídeo). A fila expande em todos os itens."),
            ("Como transcrever um vídeo?",
             "Use o botão Transcrever nas abas Vídeo/Música, ou no Converter para um arquivo local. Na 1ª vez o Whisper é baixado (~150 MB)."),
            ("Onde ficam os arquivos?",
             "Na pasta de download padrão (defina em Configurações ou na aba Pastas). Cada item do histórico tem \"abrir arquivo/pasta\"."),
            ("Funciona offline?",
             "A conversão de arquivos locais e o PDF funcionam offline. Downloads precisam de internet."),
        ]
    } else {
        &[
            ("How do I download a video or song?",
             "Paste the link in the Download Video/Music tab and click Download. You can adjust name, folder, format and quality before confirming."),
            ("A download failed. What now?",
             "In Settings → Maintenance, click \"Update yt-dlp\" (YouTube changes often). Also check diagnostics and the log."),
            ("How do I download a playlist?",
             "Paste the playlist link in the Queue tab and pick the type (Music/Video). The queue expands into all items."),
            ("How do I transcribe a video?",
             "Use the Transcribe button in the Video/Music tabs, or in Converter for a local file. Whisper is downloaded on first use (~150 MB)."),
            ("Where are the files?",
             "In the default download folder (set in Settings or the Folders tab). Each history item has open file/folder."),
            ("Does it work offline?",
             "Local file conversion and PDF work offline. Downloads need internet."),
        ]
    };

    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            egui::RichText::new("FAQ")
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(6.0);
        for (q, a) in faq {
            egui::CollapsingHeader::new(egui::RichText::new(*q).color(theme::text()))
                .id_source(*q)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(*a).color(theme::text_muted()).size(13.0));
                });
        }
    });

    ui.add_space(16.0);

    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            egui::RichText::new(if pt { "Atalhos de teclado" } else { "Keyboard shortcuts" })
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(6.0);
        let shortcuts: &[(&str, &str)] = &[
            ("Ctrl + 1..7", if pt { "Ir para uma aba" } else { "Go to a tab" }),
            ("Ctrl + Tab", if pt { "Próxima aba / anterior (+Shift)" } else { "Next tab / previous (+Shift)" }),
            ("Ctrl + K", if pt { "Paleta de comandos" } else { "Command palette" }),
            ("Tab", if pt { "Navegar entre campos" } else { "Move between fields" }),
            ("Enter", if pt { "Baixar (campo de URL)" } else { "Download (URL field)" }),
            ("Esc", if pt { "Cancelar / fechar diálogo" } else { "Cancel / close dialog" }),
        ];
        for (k, d) in shortcuts {
            ui.horizontal(|ui| {
                ui.add_sized(
                    egui::vec2(120.0, 18.0),
                    egui::Label::new(egui::RichText::new(*k).color(theme::accent()).monospace()),
                );
                ui.label(egui::RichText::new(*d).color(theme::text_muted()).size(13.0));
            });
        }
    });

    ui.add_space(16.0);

    if !app.deps_requested {
        app.deps_requested = true;
        app.refresh_deps();
    }
    let deps = app.deps_status.lock().unwrap().clone();
    let mut do_refresh = false;
    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(if pt { "Dependências" } else { "Dependencies" })
                    .color(theme::text_muted())
                    .size(11.0)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(theme::ghost_button(if pt { "↻ Verificar" } else { "↻ Check" })).clicked() {
                    do_refresh = true;
                }
            });
        });
        ui.add_space(6.0);
        if deps.is_empty() {
            ui.label(
                egui::RichText::new(if pt { "Verificando..." } else { "Checking..." })
                    .color(theme::text_faint())
                    .size(12.0),
            );
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
        } else {
            for (name, ver) in &deps {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        egui::vec2(110.0, 18.0),
                        egui::Label::new(egui::RichText::new(name).color(theme::text()).size(12.0)),
                    );
                    ui.label(egui::RichText::new(ver).color(theme::text_muted()).size(12.0));
                });
            }
        }
    });
    if do_refresh {
        app.refresh_deps();
    }

    ui.add_space(16.0);

    let mut report = false;
    let mut suggest = false;
    let mut meta = false;
    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            egui::RichText::new(if pt { "Suporte" } else { "Support" })
                .color(theme::text_muted())
                .size(11.0)
                .strong(),
        );
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(theme::accent_button(if pt { "🐞 Reportar bug" } else { "🐞 Report a bug" }))
                .clicked()
            {
                report = true;
            }
            if ui
                .add(theme::ghost_button(if pt { "💡 Enviar sugestão" } else { "💡 Send a suggestion" }))
                .clicked()
            {
                suggest = true;
            }
            if ui
                .add(theme::ghost_button(if pt { "ℹ Ver metadados de um arquivo" } else { "ℹ View file metadata" }))
                .clicked()
            {
                meta = true;
            }
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(if pt {
                "\"Reportar bug\" e \"Enviar sugestão\" abrem um novo issue no GitHub já com um modelo."
            } else {
                "\"Report a bug\" and \"Send a suggestion\" open a new GitHub issue with a template."
            })
            .color(theme::text_faint())
            .size(11.0),
        );
    });

    if report {
        report_bug(app, pt);
    }
    if suggest {
        report_suggestion(app, pt);
    }
    if meta {
        if let Some(f) = rfd::FileDialog::new().pick_file() {
            app.show_metadata(f.to_string_lossy().to_string());
        }
    }
}

fn report_bug(app: &mut App, pt: bool) {
    let title = "Bug: ";
    let body = if pt {
        "**Descreva o que aconteceu:**\n- \n\n\
         **Passos para reproduzir:**\n1. \n\n\
         **Comportamento esperado:**\n- \n"
    } else {
        "**Describe what happened:**\n- \n\n\
         **Steps to reproduce:**\n1. \n\n\
         **Expected behavior:**\n- \n"
    };
    open_new_issue(app, pt, "bug", title, body);
}

fn report_suggestion(app: &mut App, pt: bool) {
    let title = if pt { "Sugestão: " } else { "Suggestion: " };
    let body = if pt {
        "**Qual é a sua ideia?**\n- \n\n\
         **Que problema ela resolve?**\n- \n\n\
         **Como você imagina que funcionaria?**\n- \n"
    } else {
        "**What's your idea?**\n- \n\n\
         **What problem does it solve?**\n- \n\n\
         **How would it work?**\n- \n"
    };
    open_new_issue(app, pt, "enhancement", title, body);
}

fn open_new_issue(app: &mut App, pt: bool, label: &str, title: &str, body: &str) {
    // Reserva: copia o modelo caso o link pré-preenchido exceda o limite do GitHub.
    theme::set_clipboard(&format!("{}\n\n{}", title, body));

    let url = format!(
        "https://github.com/Lumen-Connection/lumen-stream/issues/new?labels={}&title={}&body={}",
        percent_encode(label),
        percent_encode(title),
        percent_encode(body),
    );
    open::that(&url).ok();

    app.toast(
        if pt {
            "Abrindo o GitHub para você abrir um novo issue."
        } else {
            "Opening GitHub to file a new issue."
        },
        false,
    );
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
