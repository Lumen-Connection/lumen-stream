use egui::Color32;

use crate::app::{App, DownloadPhase, MediaType, Tab};
use crate::ui::theme;

pub fn render(app: &mut App, ctx: &egui::Context) {
    render_sidebar(app, ctx);

    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(theme::bg_app())
                .inner_margin(egui::Margin {
                    left: 36.0,
                    right: 36.0,
                    top: 28.0,
                    bottom: 24.0,
                }),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                const MAX_W: f32 = 1080.0;
                let pad = ((ui.available_width() - MAX_W) / 2.0).max(0.0);
                egui::Frame::none()
                    .inner_margin(egui::Margin {
                        left: pad,
                        right: pad,
                        top: 0.0,
                        bottom: 0.0,
                    })
                    .show(ui, |ui| {
                        let tab = app.active_tab;
                        render_tab_content(app, ctx, ui, tab);
                    });
            });
        });

    render_overlays(app, ctx);
}

fn render_overlays(app: &mut App, ctx: &egui::Context) {
    render_modal(app, ctx);
    render_toasts(app, ctx);
    render_inspector(app, ctx);
    render_info_window(app, ctx);
    render_qr_window(app, ctx);
    render_command_palette(app, ctx);
    render_tag_editor(app, ctx);
    render_tags_dialog(app, ctx);
    render_confirm_clear(app, ctx);
    render_confirm_delete(app, ctx);
    render_confirm_delete_folder(app, ctx);
    render_orphans(app, ctx);
    render_onboarding(app, ctx);
    render_detached(app, ctx);
}

fn render_onboarding(app: &mut App, ctx: &egui::Context) {
    if app.config.onboarded {
        return;
    }
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;

    // Cortina: o idioma é definido antes de usar o app, então a UI atrás não
    // responde a clique. Fica na mesma Order da janela, mas é registrada antes —
    // logo, abaixo dela.
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("onboarding_scrim"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
            ui.painter()
                .rect_filled(rect, egui::Rounding::ZERO, Color32::from_black_alpha(190));
        });

    egui::Window::new(if pt { "Bem-vindo ao Lumen Stream" } else { "Welcome to Lumen Stream" })
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_width(440.0);
            if app.brand_texture.is_none() {
                app.brand_texture = crate::app::load_brand_texture(ui.ctx());
            }
            if let Some(tex) = &app.brand_texture {
                let [tw, th] = tex.size();
                // Emblema quadrado: limita pela altura para não virar um bloco gigante.
                let h = 160.0_f32;
                let w = h * tw as f32 / th.max(1) as f32;
                ui.vertical_centered(|ui| {
                    ui.add(egui::Image::from_texture((tex.id(), egui::vec2(w, h))));
                });
                ui.add_space(10.0);
            }

            // Primeira decisão do onboarding: o idioma. O resto do modal já
            // responde no idioma escolhido no frame seguinte ao clique.
            ui.label(
                egui::RichText::new("Idioma / Language")
                    .color(theme::text_muted())
                    .size(11.0)
                    .strong(),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for (lang, label) in [
                    (crate::ui::i18n::Lang::Pt, "Português"),
                    (crate::ui::i18n::Lang::En, "English"),
                ] {
                    let selected = app.config.lang == lang;
                    let fill = if selected { theme::accent() } else { theme::bg_card() };
                    let txt = egui::RichText::new(label).size(14.0).color(if selected {
                        Color32::WHITE
                    } else {
                        theme::text()
                    });
                    if ui
                        .add(
                            egui::Button::new(txt)
                                .fill(fill)
                                .min_size(egui::vec2(150.0, 36.0)),
                        )
                        .clicked()
                        && !selected
                    {
                        app.config.lang = lang;
                        app.config.save();
                    }
                }
            });
            ui.add_space(14.0);

            ui.label(
                egui::RichText::new(if pt {
                    "Baixe vídeos e músicas de centenas de sites, converta arquivos e muito mais."
                } else {
                    "Download videos and music from hundreds of sites, convert files and more."
                })
                .color(theme::text())
                .size(14.0),
            );
            ui.add_space(10.0);
            let tips: &[&str] = if pt {
                &[
                    "Cole um link em Baixar Vídeo/Música e clique em Download.",
                    "Use a Fila para vários links ou playlists.",
                    "Converter: formatos, PDF, marca d'água e transcrição.",
                    "Ctrl+K abre a paleta de comandos; F11 = tela cheia.",
                    "Tudo offline: yt-dlp/ffmpeg são baixados automaticamente.",
                ]
            } else {
                &[
                    "Paste a link in Download Video/Music and click Download.",
                    "Use the Queue for multiple links or playlists.",
                    "Converter: formats, PDF, watermark and transcription.",
                    "Ctrl+K opens the command palette; F11 = fullscreen.",
                    "Self-contained: yt-dlp/ffmpeg download automatically.",
                ]
            };
            for t in tips {
                ui.label(egui::RichText::new(format!("•  {}", t)).color(theme::text_muted()).size(13.0));
            }
            ui.add_space(14.0);
            if ui
                .add(theme::accent_button(if pt { "Começar" } else { "Get started" }).min_size(egui::vec2(140.0, 38.0)))
                .clicked()
            {
                app.config.onboarded = true;
                app.config.save();
            }
        });
}

fn render_orphans(app: &mut App, ctx: &egui::Context) {
    let Some(list) = app.orphans.clone() else {
        return;
    };
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let mut close = false;
    let mut delete: Option<std::path::PathBuf> = None;
    egui::Window::new(if pt { "Arquivos órfãos" } else { "Orphan files" })
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_width(580.0);
            ui.label(
                egui::RichText::new(format!(
                    "{} {}",
                    list.len(),
                    if pt { "arquivo(s) fora do histórico" } else { "file(s) not in history" }
                ))
                .color(theme::text_muted())
                .size(12.0),
            );
            ui.add_space(6.0);
            if list.is_empty() {
                ui.label(
                    egui::RichText::new(if pt { "Nada encontrado. 🎉" } else { "Nothing found. 🎉" })
                        .color(theme::text_faint()),
                );
            }
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for p in &list {
                    ui.horizontal(|ui| {
                        if ui.add(theme::ghost_button("📂")).clicked() {
                            if let Some(parent) = p.parent() {
                                open::that(parent).ok();
                            }
                        }
                        if ui.add(theme::ghost_button("🗑")).clicked() {
                            delete = Some(p.clone());
                        }
                        ui.label(
                            egui::RichText::new(
                                p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                            )
                            .color(theme::text())
                            .size(12.0),
                        );
                    });
                }
            });
            ui.add_space(8.0);
            if ui.add(theme::ghost_button(if pt { "Fechar" } else { "Close" })).clicked() {
                close = true;
            }
        });
    if let Some(p) = delete {
        std::fs::remove_file(&p).ok();
        if let Some(v) = app.orphans.as_mut() {
            v.retain(|x| x != &p);
        }
    }
    if close {
        app.orphans = None;
    }
}

fn render_confirm_clear(app: &mut App, ctx: &egui::Context) {
    let Some(mt) = app.pending_clear.clone() else {
        return;
    };
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let mut confirm = false;
    let mut cancel = false;
    egui::Window::new(if pt { "Limpar histórico?" } else { "Clear history?" })
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(if pt {
                    "Isso move todos os itens para a lixeira."
                } else {
                    "This moves all items to the trash."
                })
                .color(theme::text_muted()),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(theme::accent_button(if pt { "Limpar" } else { "Clear" })).clicked() {
                    confirm = true;
                }
                if ui.add(theme::ghost_button(if pt { "Cancelar" } else { "Cancel" })).clicked() {
                    cancel = true;
                }
            });
        });
    if confirm {
        app.db.clear_history(&mt);
        app.pending_clear = None;
    } else if cancel {
        app.pending_clear = None;
    }
}

fn render_confirm_delete(app: &mut App, ctx: &egui::Context) {
    let Some((id, title, path)) = app.pending_delete.clone() else {
        return;
    };
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let file_exists = std::path::Path::new(&path).exists();
    let short = crate::ui::music_tab::short_link(&title);

    let mut action: Option<bool> = None; // Some(true)=com arquivo, Some(false)=só histórico
    let mut cancel = false;
    egui::Window::new(if pt { "Excluir item?" } else { "Delete item?" })
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            ui.set_max_width(400.0);
            ui.label(
                egui::RichText::new(short)
                    .color(theme::text())
                    .strong(),
            );
            ui.add_space(6.0);
            if file_exists {
                ui.label(
                    egui::RichText::new(if pt {
                        "⚠ Excluir também apaga o arquivo do disco (vai para a Lixeira do sistema, recuperável)."
                    } else {
                        "⚠ Deleting also removes the file from disk (moved to the system Recycle Bin, recoverable)."
                    })
                    .color(theme::danger()),
                );
            } else {
                ui.label(
                    egui::RichText::new(if pt {
                        "O arquivo não está mais na pasta; só o registro será removido."
                    } else {
                        "The file is no longer in the folder; only the entry will be removed."
                    })
                    .color(theme::text_muted()),
                );
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if file_exists {
                    if ui
                        .add(theme::accent_button(if pt {
                            "🗑 Excluir item e arquivo"
                        } else {
                            "🗑 Delete item and file"
                        }))
                        .clicked()
                    {
                        action = Some(true);
                    }
                    if ui
                        .add(theme::ghost_button(if pt { "Só do histórico" } else { "History only" }))
                        .clicked()
                    {
                        action = Some(false);
                    }
                } else if ui
                    .add(theme::accent_button(if pt { "Excluir" } else { "Delete" }))
                    .clicked()
                {
                    action = Some(false);
                }
                if ui.add(theme::ghost_button(if pt { "Cancelar" } else { "Cancel" })).clicked() {
                    cancel = true;
                }
            });
        });

    if let Some(with_file) = action {
        app.delete_history_item(id, &path, with_file);
        app.pending_delete = None;
    } else if cancel {
        app.pending_delete = None;
    }
}

fn render_confirm_delete_folder(app: &mut App, ctx: &egui::Context) {
    let Some((id, name, path)) = app.pending_delete_folder.clone() else {
        return;
    };
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let is_default = app.config.default_download_dir.to_string_lossy() == path.as_str();

    let mut confirm = false;
    let mut cancel = false;
    egui::Window::new(if pt { "Excluir pasta?" } else { "Delete folder?" })
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .collapsible(false)
        .resizable(false)
        .default_width(400.0)
        .show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(egui::RichText::new(&name).color(theme::text()).strong());
            ui.label(egui::RichText::new(&path).color(theme::text_faint()).size(11.0));
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(if pt {
                    "⚠ Isso apaga a pasta e TODO o seu conteúdo do disco (vai para a Lixeira do sistema, recuperável)."
                } else {
                    "⚠ This deletes the folder and ALL its contents from disk (moved to the system Recycle Bin, recoverable)."
                })
                .color(theme::danger()),
            );
            if is_default {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(if pt {
                        "Esta é a pasta padrão de downloads."
                    } else {
                        "This is the default download folder."
                    })
                    .color(theme::text_muted())
                    .size(12.0),
                );
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add(theme::accent_button(if pt { "🗑 Excluir pasta" } else { "🗑 Delete folder" }))
                    .clicked()
                {
                    confirm = true;
                }
                if ui.add(theme::ghost_button(if pt { "Cancelar" } else { "Cancel" })).clicked() {
                    cancel = true;
                }
            });
        });

    if confirm {
        app.delete_folder(id, path);
        app.pending_delete_folder = None;
    } else if cancel {
        app.pending_delete_folder = None;
    }
}

/// Botão de ícone da barra lateral.
///
/// O ícone é posicionado pelo *ink* do glifo (`mesh_bounds`), não pela caixa da
/// linha. Cada ícone daqui cai numa fonte diferente da cadeia de fallback
/// (NotoEmoji, emoji-icon-font, Segoe UI Symbol, Segoe UI Emoji) e cada fonte tem
/// ascent/descent próprios — centralizar a caixa da linha, que é o que o
/// `egui::Button` faz, deixa um ícone alto e o outro baixo. Centralizar o
/// desenho de fato é o que mantém os dois alinhados entre si.
fn sidebar_icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(30.0, 28.0), egui::Sense::click());

    let (fill, stroke) = if resp.is_pointer_button_down_on() {
        (theme::accent(), egui::Stroke::new(1.0_f32, theme::accent()))
    } else if resp.hovered() {
        (theme::bg_card_hover(), egui::Stroke::new(1.0_f32, theme::accent()))
    } else {
        (Color32::TRANSPARENT, egui::Stroke::new(1.0_f32, theme::border()))
    };
    ui.painter()
        .rect(rect, egui::Rounding::same(8.0), fill, stroke);

    let galley = ui.painter().layout_no_wrap(
        icon.to_owned(),
        egui::FontId::proportional(15.0),
        theme::text(),
    );
    let ink = galley.mesh_bounds;
    let pos = if ink.is_finite() && ink.is_positive() {
        rect.center() - ink.center().to_vec2()
    } else {
        rect.center() - galley.rect.center().to_vec2()
    };
    ui.painter().galley(pos, galley, theme::text());

    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tooltip)
}

pub fn render_tab_content(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui, tab: Tab) {
    match tab {
        Tab::Home => crate::ui::home_tab::render(app, ctx, ui),
        Tab::Music => crate::ui::music_tab::render(app, ctx, ui),
        Tab::Video => crate::ui::video_tab::render(app, ctx, ui),
        Tab::Converter => crate::ui::converter_tab::render(app, ctx, ui),
        Tab::Queue => crate::ui::queue_tab::render(app, ctx, ui),
        Tab::Folders => crate::ui::folders_tab::render(app, ctx, ui),
        Tab::Games => crate::ui::games_tab::render(app, ctx, ui),
        Tab::Cloud => crate::ui::cloud_tab::render(app, ctx, ui),
        Tab::Achievements => crate::ui::achievements_tab::render(app, ctx, ui),
        Tab::Settings => crate::ui::settings_tab::render(app, ctx, ui),
        Tab::Help => crate::ui::help_tab::render(app, ctx, ui),
    }
}

fn tab_title(tab: Tab, s: &crate::ui::i18n::Strings) -> &'static str {
    match tab {
        Tab::Home => s.nav_home,
        Tab::Music => s.nav_music,
        Tab::Video => s.nav_video,
        Tab::Converter => s.nav_converter,
        Tab::Queue => s.nav_queue,
        Tab::Folders => s.nav_folders,
        Tab::Games => s.nav_games,
        Tab::Cloud => s.nav_cloud,
        Tab::Achievements => s.nav_achievements,
        Tab::Settings => s.nav_settings,
        Tab::Help => s.nav_help,
    }
}

fn render_detached(app: &mut App, ctx: &egui::Context) {
    let s = crate::ui::i18n::s(app.config.lang);
    let tabs = app.detached.clone();
    for tab in tabs {
        let mut close = false;
        let id = egui::ViewportId::from_hash_of(("lumen_detached", tab));
        let title = format!("Lumen — {}", tab_title(tab, &s));
        ctx.show_viewport_immediate(
            id,
            egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size([900.0, 640.0]),
            |vctx, _class| {
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::none()
                            .fill(theme::bg_app())
                            .inner_margin(egui::Margin::same(24.0)),
                    )
                    .show(vctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            render_tab_content(app, vctx, ui, tab);
                        });
                    });
                if vctx.input(|i| i.viewport().close_requested()) {
                    close = true;
                }
            },
        );
        if close {
            app.detached.retain(|t| *t != tab);
        }
    }
}

fn render_tags_dialog(app: &mut App, ctx: &egui::Context) {
    if app.history_tag_edit.is_none() {
        return;
    }
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let mut save: Option<(i64, String)> = None;
    let mut cancel = false;
    egui::Window::new(if pt { "Categorias / tags" } else { "Categories / tags" })
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.set_width(320.0);
            ui.label(
                egui::RichText::new(if pt {
                    "Separe por vírgula (ex.: estudo, favoritos, trabalho)"
                } else {
                    "Separate by comma (e.g. study, favorites, work)"
                })
                .color(theme::text_muted())
                .size(12.0),
            );
            ui.add_space(6.0);
            if let Some((_, tags)) = app.history_tag_edit.as_mut() {
                ui.add(
                    egui::TextEdit::singleline(tags)
                        .desired_width(f32::INFINITY)
                        .text_color(theme::text()),
                );
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(theme::accent_button(if pt { "Salvar" } else { "Save" })).clicked() {
                    if let Some((id, tags)) = &app.history_tag_edit {
                        save = Some((*id, tags.clone()));
                    }
                }
                if ui.add(theme::ghost_button(if pt { "Cancelar" } else { "Cancel" })).clicked() {
                    cancel = true;
                }
            });
        });
    if let Some((id, tags)) = save {
        app.db.set_tags(id, tags.trim());
        app.history_tag_edit = None;
    } else if cancel {
        app.history_tag_edit = None;
    }
}

fn render_tag_editor(app: &mut App, ctx: &egui::Context) {
    if app.tag_editor.is_none() {
        return;
    }
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let mut save = false;
    let mut cancel = false;
    let mut detect = false;

    egui::Window::new(if pt { "Editar tags" } else { "Edit tags" })
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| {
            let ed = app.tag_editor.as_mut().unwrap();
            ui.set_width(360.0);
            let field = |ui: &mut egui::Ui, label: &str, val: &mut String| {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        egui::vec2(90.0, 22.0),
                        egui::Label::new(egui::RichText::new(label).color(theme::text_muted())),
                    );
                    ui.add(
                        egui::TextEdit::singleline(val)
                            .desired_width(f32::INFINITY)
                            .text_color(theme::text()),
                    );
                });
            };
            field(ui, if pt { "Título" } else { "Title" }, &mut ed.t.title);
            field(ui, if pt { "Artista" } else { "Artist" }, &mut ed.t.artist);
            field(ui, if pt { "Álbum" } else { "Album" }, &mut ed.t.album);
            field(ui, if pt { "Ano" } else { "Year" }, &mut ed.t.year);
            field(ui, if pt { "Gênero" } else { "Genre" }, &mut ed.t.genre);
            field(ui, if pt { "Faixa" } else { "Track" }, &mut ed.t.track);
            field(ui, if pt { "Tom" } else { "Key" }, &mut ed.t.key);
            ui.horizontal(|ui| {
                ui.add_sized(
                    egui::vec2(90.0, 22.0),
                    egui::Label::new(egui::RichText::new("BPM").color(theme::text_muted())),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut ed.t.bpm)
                        .desired_width(80.0)
                        .text_color(theme::text()),
                );
                if ed.detecting {
                    ui.add(egui::Spinner::new().size(14.0));
                } else if ui
                    .add(theme::ghost_button(if pt { "Detectar" } else { "Detect" }))
                    .clicked()
                {
                    detect = true;
                }
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .add(theme::accent_button(if pt { "Salvar" } else { "Save" }))
                    .clicked()
                {
                    save = true;
                }
                if ui
                    .add(theme::ghost_button(if pt { "Cancelar" } else { "Cancel" }))
                    .clicked()
                {
                    cancel = true;
                }
            });
        });

    if detect {
        app.detect_bpm_editor();
    }
    if save {
        app.save_tags();
    } else if cancel {
        app.tag_editor = None;
    }
}

fn render_command_palette(app: &mut App, ctx: &egui::Context) {
    if !app.cmd_palette_open {
        return;
    }
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let mut chosen: Option<crate::ui::command::Cmd> = None;

    egui::Window::new(if pt { "Paleta de comandos" } else { "Command palette" })
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 90.0))
        .fixed_size([460.0, 360.0])
        .collapsible(false)
        .show(ctx, |ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut app.cmd_query)
                    .hint_text(if pt { "Buscar ação..." } else { "Search action..." })
                    .desired_width(f32::INFINITY),
            );
            resp.request_focus();

            let q = app.cmd_query.to_lowercase();
            let cmds = crate::ui::command::all_commands(pt);
            let filtered: Vec<_> = cmds
                .into_iter()
                .filter(|(label, _)| q.is_empty() || label.to_lowercase().contains(&q))
                .collect();

            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some((_, c)) = filtered.first() {
                    chosen = Some(*c);
                }
            }

            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                for (label, cmd) in &filtered {
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(label).color(theme::text()))
                                .fill(theme::bg_card())
                                .min_size(egui::vec2(ui.available_width(), 30.0)),
                        )
                        .clicked()
                    {
                        chosen = Some(*cmd);
                    }
                }
            });
        });

    if let Some(cmd) = chosen {
        crate::ui::command::run(app, cmd);
        app.cmd_palette_open = false;
        app.cmd_query.clear();
    }
}

fn render_qr_window(app: &mut App, ctx: &egui::Context) {
    let Some((url, tex)) = app.qr_window.clone() else {
        return;
    };
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let mut open = true;
    egui::Window::new(if pt { "QR do link" } else { "Link QR code" })
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .resizable(false)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add(
                    egui::Image::from_texture((tex.id(), tex.size_vec2()))
                        .fit_to_exact_size(egui::vec2(240.0, 240.0)),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(crate::ui::music_tab::short_link(&url))
                        .color(theme::text_muted())
                        .size(11.0),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(theme::ghost_button(if pt { "📋 Copiar link" } else { "📋 Copy link" }))
                        .clicked()
                    {
                        if let Ok(mut cb) = arboard::Clipboard::new() {
                            let _ = cb.set_text(url.clone());
                        }
                    }
                    if ui.add(theme::ghost_button(if pt { "Abrir" } else { "Open" })).clicked() {
                        open::that(&url).ok();
                    }
                });
            });
        });
    if !open {
        app.qr_window = None;
    }
}

fn render_info_window(app: &mut App, ctx: &egui::Context) {
    let content = app.info_window.lock().unwrap().clone();
    let Some((title, body)) = content else {
        return;
    };
    let mut open = true;
    egui::Window::new(title)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_size([520.0, 420.0])
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&body).color(theme::text()).monospace().size(12.0),
                    )
                    .wrap(true),
                );
            });
        });
    if !open {
        *app.info_window.lock().unwrap() = None;
    }
}

fn render_inspector(app: &mut App, ctx: &egui::Context) {
    let (open, loading, rows, error) = {
        let i = app.inspector.lock().unwrap();
        if !i.open {
            return;
        }
        (i.open, i.loading, i.rows.clone(), i.error.clone())
    };
    let mut keep_open = open;
    let s = crate::ui::i18n::s(app.config.lang);
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;

    egui::Window::new(if pt { "Inspetor de formatos" } else { "Format inspector" })
        .open(&mut keep_open)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_size([640.0, 460.0])
        .show(ctx, |ui| {
            if loading {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().color(theme::accent()));
                    ui.label(egui::RichText::new(if pt { "Buscando formatos..." } else { "Fetching formats..." }).color(theme::text_muted()));
                });
                ctx.request_repaint();
                return;
            }
            if let Some(e) = &error {
                ui.label(egui::RichText::new(e).color(theme::danger()));
                return;
            }
            ui.label(
                egui::RichText::new(if pt {
                    "Resoluções, codecs, bitrate e tamanho disponíveis."
                } else {
                    "Available resolutions, codecs, bitrate and size."
                })
                .color(theme::text_muted())
                .size(12.0),
            );
            ui.add_space(6.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("fmt_grid")
                    .striped(true)
                    .num_columns(8)
                    .spacing(egui::vec2(14.0, 6.0))
                    .show(ui, |ui| {
                        for h in ["ID", "Ext", if pt { "Tipo" } else { "Kind" }, if pt { "Resolução" } else { "Res" }, "FPS", "Codec", "Bitrate", if pt { "Tamanho" } else { "Size" }] {
                            ui.label(egui::RichText::new(h).color(theme::accent()).strong().size(12.0));
                        }
                        ui.end_row();
                        for r in &rows {
                            let c = |t: &str| egui::RichText::new(t.to_string()).color(theme::text()).size(12.0);
                            ui.label(c(&r.id));
                            ui.label(c(&r.ext));
                            ui.label(c(&r.kind));
                            ui.label(c(&r.resolution));
                            ui.label(c(&r.fps));
                            ui.label(egui::RichText::new(&r.codec).color(theme::text_muted()).size(12.0));
                            ui.label(c(&r.bitrate));
                            ui.label(c(&r
                                .size
                                .map(crate::download::engine::format_size)
                                .unwrap_or_else(|| "—".to_string())));
                            ui.end_row();
                        }
                    });
            });
            let _ = s;
        });

    if !keep_open {
        app.inspector.lock().unwrap().open = false;
    }
}

fn render_toasts(app: &mut App, ctx: &egui::Context) {
    if app.toasts.is_empty() {
        return;
    }
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let mut undo_id: Option<i64> = None;

    // Um aviso não pode roubar o clique de quem está usando o app: por padrão
    // toda `Area` captura o ponteiro na sua região, e aí o usuário teria que
    // esperar o toast sumir para clicar. Só quando há "Desfazer" existe algo
    // para clicar dentro dele — fora isso, o clique atravessa.
    let has_undo = app.toasts.iter().any(|t| t.undo.is_some());

    egui::Area::new("toasts".into())
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
        .interactable(has_undo)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                for t in app.toasts.iter().rev() {
                    let accent = if t.error { theme::danger() } else { theme::accent() };
                    egui::Frame::none()
                        .fill(theme::bg_card())
                        .rounding(egui::Rounding::same(8.0))
                        .stroke(egui::Stroke::new(1.0_f32, accent))
                        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                        .show(ui, |ui| {
                            ui.set_max_width(360.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&t.text)
                                        .color(theme::text())
                                        .size(13.0),
                                );
                                if let Some(id) = t.undo {
                                    if ui
                                        .add(theme::ghost_button(if pt { "Desfazer" } else { "Undo" }))
                                        .clicked()
                                    {
                                        undo_id = Some(id);
                                    }
                                }
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        });
    if let Some(id) = undo_id {
        app.db.restore_history(id);
        app.toasts.retain(|t| t.undo != Some(id));
        app.toast(if pt { "Restaurado" } else { "Restored" }, false);
    }
}

fn render_sidebar(app: &mut App, ctx: &egui::Context) {
    egui::SidePanel::left("sidebar")
        .resizable(false)
        .exact_width(228.0)
        .frame(
            egui::Frame::none()
                .fill(theme::bg_sidebar())
                .inner_margin(egui::Margin {
                    left: 14.0,
                    right: 14.0,
                    top: 18.0,
                    bottom: 24.0,
                }),
        )
        .show(ctx, |ui| {
            if app.brand_texture.is_none() {
                app.brand_texture = crate::app::load_brand_texture(ui.ctx());
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if let Some(tex) = &app.brand_texture {
                    let [tw, th] = tex.size();
                    let h = 42.0;
                    let w = h * tw as f32 / th.max(1) as f32;
                    ui.add(egui::Image::from_texture((tex.id(), egui::vec2(w, h))));
                }
                ui.label(
                    egui::RichText::new("Lumen Stream")
                        .size(19.0)
                        .strong()
                        .color(theme::text()),
                );
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if sidebar_icon_button(ui, "🧰", "Ctrl+K").clicked() {
                    app.cmd_palette_open = true;
                    app.cmd_query.clear();
                }
                if sidebar_icon_button(
                    ui,
                    "⧉",
                    if app.config.lang == crate::ui::i18n::Lang::Pt {
                        "Abrir aba em nova janela"
                    } else {
                        "Open tab in a new window"
                    },
                )
                .clicked()
                {
                    if !app.detached.contains(&app.active_tab) {
                        app.detached.push(app.active_tab);
                    }
                }
            });

            ui.add_space(22.0);

            let s = crate::ui::i18n::s(app.config.lang);
            let nav = [
                ("🏠", s.nav_home, Tab::Home),
                ("🎵", s.nav_music, Tab::Music),
                ("🎬", s.nav_video, Tab::Video),
                ("🔄", s.nav_converter, Tab::Converter),
                ("📋", s.nav_queue, Tab::Queue),
                ("📁", s.nav_folders, Tab::Folders),
                ("🎮", s.nav_games, Tab::Games),
                ("☁", s.nav_cloud, Tab::Cloud),
                ("🏆", s.nav_achievements, Tab::Achievements),
                ("⚙", s.nav_settings, Tab::Settings),
                ("❓", s.nav_help, Tab::Help),
            ];
            // O rodapé é reservado antes da navegação: assim ele sempre cabe e a
            // lista rola dentro do que sobrar. Reservar uma altura fixa para a
            // navegação fazia o contrário — numa janela baixa ela pedia mais do
            // que havia e o cartão de armazenamento cobria os últimos itens.
            egui::TopBottomPanel::bottom("sidebar_footer")
                .frame(egui::Frame::none())
                .show_inside(ui, |ui| {
                    ui.add_space(6.0);
                    storage_footer(ui, app);
                    ui.add_space(6.0);
                    if app.connection_texture.is_none() {
                        app.connection_texture = crate::app::load_connection_texture(ui.ctx());
                    }
                    credit_footer(
                        ui,
                        app.connection_texture.as_ref(),
                        app.config.lang == crate::ui::i18n::Lang::Pt,
                    );
                });

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (icon, label, tab) in nav {
                        if !tab.visible() {
                            continue;
                        }
                        if theme::nav_item(ui, icon, label, app.active_tab == tab) {
                            app.active_tab = tab;
                        }
                        ui.add_space(3.0);
                    }
                });
        });
}

fn storage_footer(ui: &mut egui::Ui, app: &App) {
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
    let (free, total) = disk_usage(&app.config.default_download_dir);

    let frame = egui::Frame::none()
        .fill(theme::bg_card())
        .rounding(egui::Rounding::same(10.0))
        .stroke(egui::Stroke::new(1.0_f32, theme::border()))
        .inner_margin(egui::Margin::symmetric(12.0, 10.0));
    frame.show(ui, |ui| {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::hover());
        let cy = rect.center().y;

        ui.painter().text(
            egui::pos2(rect.min.x, cy - 8.0),
            egui::Align2::LEFT_CENTER,
            if pt { "Armazenamento" } else { "All Storage" },
            egui::FontId::proportional(13.0),
            theme::text(),
        );
        let free_txt = format!(
            "{} {}",
            crate::download::engine::format_size(free),
            if pt { "livres" } else { "free" }
        );
        ui.painter().text(
            egui::pos2(rect.min.x, cy + 9.0),
            egui::Align2::LEFT_CENTER,
            free_txt,
            egui::FontId::proportional(11.5),
            theme::text_muted(),
        );

        let used_frac = if total > 0 {
            ((total - free) as f32 / total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let center = egui::pos2(rect.max.x - 18.0, cy);
        let radius = 15.0;
        ui.painter()
            .circle_stroke(center, radius, egui::Stroke::new(3.0_f32, theme::bg_card_hover()));
        draw_arc(ui.painter(), center, radius, used_frac, theme::accent(), 3.0);
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            format!("{:.0}%", used_frac * 100.0),
            egui::FontId::proportional(10.0),
            theme::text(),
        );
    });
}

fn draw_arc(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    frac: f32,
    color: egui::Color32,
    width: f32,
) {
    if frac <= 0.0 {
        return;
    }
    let start = -std::f32::consts::FRAC_PI_2;
    let sweep = frac * std::f32::consts::TAU;
    let n = ((frac * 64.0).ceil() as usize).max(2);
    let pts: Vec<egui::Pos2> = (0..=n)
        .map(|i| {
            let a = start + sweep * (i as f32 / n as f32);
            egui::pos2(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect();
    painter.add(egui::Shape::line(pts, egui::Stroke::new(width, color)));
}

fn disk_usage(folder: &std::path::Path) -> (i64, i64) {
    let mut p = folder;
    loop {
        if p.exists() {
            let free = fs2::available_space(p).unwrap_or(0) as i64;
            let total = fs2::total_space(p).unwrap_or(0) as i64;
            return (free, total);
        }
        match p.parent() {
            Some(parent) => p = parent,
            None => return (0, 0),
        }
    }
}

fn credit_footer(ui: &mut egui::Ui, logo: Option<&egui::TextureHandle>, pt: bool) {
    const URL: &str = "https://lumenconnection.com.br/";

    ui.label(
        egui::RichText::new(if pt { "CRÉDITO · CONHEÇA-NOS" } else { "CREDIT · GET TO KNOW US" })
            .size(9.0)
            .strong()
            .color(theme::text_faint()),
    );
    ui.add_space(3.0);

    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 50.0), egui::Sense::click());

    let hovered = resp.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(10.0), theme::bg_card_hover());
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let cx = rect.min.x + 18.0;
    let cy = rect.center().y;
    match logo {
        Some(tex) => {
            // Recorta a logo num disco (formato circular): malha em leque com o UV
            // mapeado para o círculo inscrito na imagem quadrada.
            let r = 14.0;
            let center = egui::pos2(cx, cy);
            let n = 48usize;
            let mut mesh = egui::Mesh::with_texture(tex.id());
            mesh.vertices.push(egui::epaint::Vertex {
                pos: center,
                uv: egui::pos2(0.5, 0.5),
                color: egui::Color32::WHITE,
            });
            for i in 0..=n {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let (sin, cos) = a.sin_cos();
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: egui::pos2(center.x + cos * r, center.y + sin * r),
                    uv: egui::pos2(0.5 + cos * 0.5, 0.5 + sin * 0.5),
                    color: egui::Color32::WHITE,
                });
            }
            for i in 1..=n as u32 {
                mesh.indices.extend_from_slice(&[0, i, i + 1]);
            }
            ui.painter().add(egui::Shape::mesh(mesh));
        }
        None => {
            // Fallback (imagem não carregou): losango desenhado.
            ui.painter().circle_filled(egui::pos2(cx, cy), 13.0, theme::accent());
            ui.painter().text(
                egui::pos2(cx, cy),
                egui::Align2::CENTER_CENTER,
                "◆",
                egui::FontId::proportional(13.0),
                egui::Color32::WHITE,
            );
        }
    }

    ui.painter().text(
        egui::pos2(rect.min.x + 40.0, cy - 8.0),
        egui::Align2::LEFT_CENTER,
        "Lumen Connection",
        egui::FontId::proportional(14.0),
        theme::text(),
    );
    let link_color = if hovered { theme::accent() } else { theme::text_muted() };
    ui.painter().text(
        egui::pos2(rect.min.x + 40.0, cy + 9.0),
        egui::Align2::LEFT_CENTER,
        if pt { "Conheça-nos  ↗" } else { "Get to know us  ↗" },
        egui::FontId::proportional(11.0),
        link_color,
    );

    if resp.clicked() {
        open::that(URL).ok();
    }
    resp.on_hover_text(format!("{}\n{}", URL, if pt { "Abrir site" } else { "Open site" }));
}

fn open_folder(path: &str) {
    if let Some(parent) = std::path::Path::new(path).parent() {
        open::that(parent).ok();
    }
}

fn sparkline(ui: &mut egui::Ui, data: &[f32]) {
    let w = ui.available_width().min(340.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 48.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::Rounding::same(6.0), theme::bg_card());
    if data.len() < 2 {
        return;
    }
    let max = data.iter().cloned().fold(1.0f32, f32::max);
    let n = data.len();
    let pts: Vec<egui::Pos2> = data
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.min.x + 6.0 + (rect.width() - 12.0) * (i as f32 / (n - 1) as f32);
            let y = rect.max.y - 6.0 - (rect.height() - 12.0) * (v / max);
            egui::pos2(x, y)
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(pts, egui::Stroke::new(2.0_f32, theme::accent())));
}

fn render_preview(
    ui: &mut egui::Ui,
    s: &crate::ui::i18n::Strings,
    preview: &Option<crate::download::engine::VideoPreview>,
    media_type: MediaType,
    folder: &std::path::Path,
    thumb: Option<&egui::TextureHandle>,
) {
    let Some(pv) = preview else {
        return;
    };
    ui.add_space(4.0);
    if let Some(tex) = thumb {
        let [tw, th] = tex.size();
        // Keep portrait thumbnails from making the fixed-size dialog taller
        // than the viewport. Both dimensions are caps, not a forced aspect.
        let scale = (200.0_f32 / tw.max(1) as f32)
            .min(140.0_f32 / th.max(1) as f32);
        let size = egui::vec2(tw as f32 * scale, th as f32 * scale);
        ui.add(egui::Image::from_texture((tex.id(), size)));
        ui.add_space(4.0);
    }
    ui.horizontal(|ui| {
        if !pv.channel.is_empty() {
            ui.label(
                egui::RichText::new(format!("👤 {}", pv.channel))
                    .color(theme::text_muted())
                    .size(12.0),
            );
        }
        if !pv.duration.is_empty() {
            ui.label(
                egui::RichText::new(format!("⏱ {}", pv.duration))
                    .color(theme::text_muted())
                    .size(12.0),
            );
        }
    });
    if media_type == MediaType::Video && !pv.resolutions.is_empty() {
        let tops: Vec<String> = pv.resolutions.iter().take(5).map(|h| format!("{}p", h)).collect();
        ui.label(
            egui::RichText::new(format!("{} {}", s.prev_resolutions, tops.join(" · ")))
                .color(theme::text_faint())
                .size(12.0),
        );
    }
    let est = if media_type == MediaType::Music {
        pv.est_size_audio
    } else {
        pv.est_size_video
    };
    ui.horizontal(|ui| {
        if let Some(sz) = est {
            ui.label(
                egui::RichText::new(format!(
                    "{} {}",
                    s.prev_est_size,
                    crate::download::engine::format_size(sz)
                ))
                .color(theme::text_muted())
                .size(12.0),
            );
        }
        if let Some(free) = free_space(folder) {
            ui.label(
                egui::RichText::new(format!(
                    "• {} {}",
                    s.prev_free,
                    crate::download::engine::format_size(free)
                ))
                .color(theme::text_faint())
                .size(12.0),
            );
            let low = est.map(|e| free < e).unwrap_or(false) || free < 200 * 1024 * 1024;
            if low {
                ui.label(
                    egui::RichText::new(s.low_space)
                        .color(theme::danger())
                        .size(12.0),
                );
            }
        }
    });
}

fn free_space(folder: &std::path::Path) -> Option<i64> {
    let mut p = folder;
    loop {
        if p.exists() {
            return fs2::available_space(p).ok().map(|b| b as i64);
        }
        match p.parent() {
            Some(parent) => p = parent,
            None => return None,
        }
    }
}

fn render_modal(app: &mut App, ctx: &egui::Context) {
    let phase;
    let url;
    let title;
    let file_name;
    let folder_path;
    let create_subfolder;
    let subfolder_name;
    let media_type;
    let output_format;
    let quality;
    let source_file;
    let progress;
    let preview;
    let clip_enabled;
    let clip_start;
    let clip_end;
    let max_height;
    let convert_preset;
    let live_from_start;
    let is_live;
    let live_bytes;

    {
        let op = app.operation.lock().unwrap();
        phase = op.phase.clone();
        progress = op.progress;
        is_live = op.is_live;
        live_bytes = op.live_bytes;
        preview = op.preview.clone();
        url = op.url.clone();
        title = op.title.clone();
        file_name = op.file_name.clone();
        folder_path = op.folder_path.clone();
        create_subfolder = op.create_subfolder;
        subfolder_name = op.subfolder_name.clone();
        media_type = op.media_type;
        source_file = op.source_file.clone();
        output_format = op.output_format.clone();
        quality = op.quality.clone();
        clip_enabled = op.clip_enabled;
        clip_start = op.clip_start.clone();
        clip_end = op.clip_end.clone();
        max_height = op.max_height;
        convert_preset = op.convert_preset.clone();
        live_from_start = op.live_from_start;
    }

    let s = crate::ui::i18n::s(app.config.lang);
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;

    match &phase {
        DownloadPhase::Idle => {}

        DownloadPhase::Fetching => {
            let mut cancel = false;
            egui::Window::new("Processando")
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .resizable(false)
                .fixed_size([400.0, 150.0])
                .show(ctx, |ui: &mut egui::Ui| {
                    ui.vertical_centered(|ui: &mut egui::Ui| {
                        ui.label(s.dl_searching);
                        ui.add(egui::Spinner::new());
                        ui.add_space(12.0);
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(s.btn_cancel).color(theme::text()),
                                )
                                .fill(theme::bg_card()),
                            )
                            .clicked()
                        {
                            cancel = true;
                        }
                    });
                });
            if cancel {
                app.cancel_operation();
            }
            ctx.request_repaint();
        }

        DownloadPhase::Configuring => {
            let mut new_file_name = file_name.clone();
            let mut new_folder_path = folder_path.clone();
            let mut new_create_subfolder = create_subfolder;
            let mut new_subfolder_name = subfolder_name.clone();
            let mut new_format = output_format.clone();
            let mut new_quality = quality.clone();
            let mut new_clip_enabled = clip_enabled;
            let mut new_clip_start = clip_start.clone();
            let mut new_clip_end = clip_end.clone();
            let mut new_max_height = max_height;
            let mut new_convert_preset = convert_preset.clone();
            let mut new_live_from_start = live_from_start;

            let is_convert = media_type == MediaType::Convert;
            let formats: Vec<(&str, &str)> = match media_type {
                MediaType::Music => vec![("mp3", "mp3"), ("m4a", "m4a"), ("opus", "opus"), ("flac", "flac")],
                MediaType::Video => crate::download::engine::video_profiles()
                    .iter()
                    .map(|profile| (profile.extension, profile.label))
                    .collect(),
                MediaType::Convert => {
                    crate::download::engine::output_formats(
                        crate::download::engine::categorize(&source_file),
                    )
                    .into_iter()
                    .map(|format| (format, format))
                    .collect()
                }
            };
            let qualities = vec!["best", "high", "medium"];
            let window_title = if is_convert { s.cfg_convert } else { s.cfg_download };

            if let Some(pv) = &preview {
                match &pv.thumbnail {
                    Some(thumb) if app.thumb_key.as_deref() != Some(pv.title.as_str()) => {
                        let color = egui::ColorImage::from_rgba_unmultiplied(
                            [thumb.width, thumb.height],
                            &thumb.rgba,
                        );
                        app.thumb_texture = Some(ctx.load_texture(
                            "preview_thumb",
                            color,
                            egui::TextureOptions::LINEAR,
                        ));
                        app.thumb_key = Some(pv.title.clone());
                    }
                    None => {
                        app.thumb_texture = None;
                        app.thumb_key = None;
                    }
                    _ => {}
                }
            }
            let thumb_tex = app.thumb_texture.clone();

            let mut win_height: f32 =
                if preview.as_ref().and_then(|p| p.thumbnail.as_ref()).is_some() {
                    580.0
                } else {
                    480.0
                };
            if new_clip_enabled {
                win_height += 40.0;
            }
            // Clamp to the viewport so the dialog never gets positioned
            // partially off-screen on displays smaller than 1920x1080
            // (e.g. 1366x768), where a fixed 500x580+ window would push its
            // header or the Confirmar/Cancelar row past the visible area.
            let screen_rect = ctx.screen_rect();
            let win_width = 500.0_f32.min(screen_rect.width() - 24.0).max(280.0);
            win_height = win_height.min(screen_rect.height() - 24.0).max(240.0);
            // Reserve space for the fixed header (title + separator) and the
            // fixed footer (Cancelar/Confirmar row) around the scroll area.
            let scroll_height = (win_height - 170.0).max(120.0);
            egui::Window::new(window_title)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .resizable(false)
                .fixed_size([win_width, win_height])
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(window_title).size(18.0).color(theme::accent()),
                    );
                    ui.separator();
                    ui.add_space(6.0);

                    egui::ScrollArea::vertical()
                        .max_height(scroll_height)
                        .show(ui, |ui| {

                    if is_convert {
                        ui.label(s.f_source);
                        ui.label(
                            egui::RichText::new(source_file.to_string_lossy())
                                .color(theme::text_muted()),
                        );
                    } else {
                        ui.label(s.f_title);
                        ui.label(&title);
                        render_preview(
                            ui,
                            s,
                            &preview,
                            media_type,
                            &new_folder_path,
                            thumb_tex.as_ref(),
                        );
                    }

                    if !is_convert && app.db.url_exists(&url) {
                        ui.label(
                            egui::RichText::new(s.dup_warning)
                                .color(theme::danger())
                                .size(12.0),
                        );
                    }
                    ui.add_space(6.0);

                    ui.label(s.f_filename);
                    ui.text_edit_singleline(&mut new_file_name);
                    ui.add_space(6.0);

                    ui.label(s.f_folder);
                    ui.horizontal(|ui| {
                        let path_str = new_folder_path.to_string_lossy().to_string();
                        let mut path_edit = path_str;
                        ui.add(
                            egui::TextEdit::singleline(&mut path_edit)
                                .text_color(theme::text_muted()),
                        );
                        new_folder_path = std::path::PathBuf::from(&path_edit);
                        if ui
                            .add(
                                egui::Button::new(egui::RichText::new("📁").color(Color32::WHITE))
                                    .fill(theme::accent()),
                            )
                            .clicked()
                        {
                            if let Some(picked) = rfd::FileDialog::new().pick_folder() {
                                new_folder_path = picked;
                            }
                        }
                    });
                    ui.add_space(6.0);

                    ui.checkbox(&mut new_create_subfolder, s.f_subfolder);
                    if new_create_subfolder {
                        ui.horizontal(|ui| {
                            ui.label(s.f_name);
                            ui.text_edit_singleline(&mut new_subfolder_name);
                        });
                    }
                    ui.add_space(6.0);

                    ui.label(if is_convert { s.f_format_to } else { s.f_format });
                    ui.horizontal_wrapped(|ui| {
                        for (format, label) in &formats {
                            let is_selected = new_format == *format;
                            if ui
                                .add(
                                    egui::Button::new(*label).fill(if is_selected {
                                        theme::accent()
                                    } else {
                                        theme::bg_card()
                                    }),
                                )
                                .clicked()
                            {
                                new_format = (*format).to_string();
                            }
                        }
                    });
                    {
                        let stem = std::path::Path::new(&new_file_name)
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| new_file_name.clone());
                        new_file_name = format!("{}.{}", stem, new_format);
                    }

                    if is_convert
                        && crate::download::engine::categorize(&source_file)
                            == crate::download::engine::FileCategory::Office
                    {
                        let eng = match app.config.convert_engine {
                            crate::config::settings::ConvertEngine::Auto => {
                                if pt { "Automático" } else { "Automatic" }
                            }
                            crate::config::settings::ConvertEngine::Rust => {
                                if pt { "Rust puro" } else { "Pure Rust" }
                            }
                            crate::config::settings::ConvertEngine::LibreOffice => "LibreOffice",
                            crate::config::settings::ConvertEngine::MsOffice => "MS Office",
                        };
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(if pt {
                                format!("⚙  Motor: {}  ·  troque em Configurações para mais fidelidade", eng)
                            } else {
                                format!("⚙  Engine: {}  ·  change it in Settings for higher fidelity", eng)
                            })
                            .color(theme::text_faint())
                            .size(11.0),
                        );
                    }
                    ui.add_space(6.0);

                    if is_convert
                        && crate::download::engine::categorize(&source_file)
                            == crate::download::engine::FileCategory::Video
                        && matches!(new_format.as_str(), "mp4" | "mkv" | "webm" | "avi" | "mov")
                    {
                        let is_manual = new_convert_preset.starts_with("manual:");
                        ui.label(s.conv_preset);
                        ui.horizontal_wrapped(|ui| {
                            let presets = [
                                ("", s.preset_original),
                                ("compress", s.preset_compress),
                                ("720", "720p"),
                                ("480", "480p"),
                                ("manual:::", s.preset_manual),
                            ];
                            for (val, label) in presets {
                                let sel = if val.starts_with("manual") {
                                    is_manual
                                } else {
                                    new_convert_preset == val
                                };
                                if ui
                                    .add(egui::Button::new(label).fill(if sel {
                                        theme::accent()
                                    } else {
                                        theme::bg_card()
                                    }))
                                    .clicked()
                                    && !sel
                                {
                                    new_convert_preset = val.to_string();
                                }
                            }
                        });
                        if new_convert_preset.starts_with("manual:") {
                            let parts: Vec<String> = new_convert_preset
                                .splitn(5, ':')
                                .map(|s| s.to_string())
                                .collect();
                            let mut h = parts.get(1).cloned().unwrap_or_default();
                            let mut fps = parts.get(2).cloned().unwrap_or_default();
                            let mut vb = parts.get(3).cloned().unwrap_or_default();
                            let mut ab = parts.get(4).cloned().unwrap_or_default();
                            ui.add_space(4.0);
                            egui::Grid::new("manual_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                                ui.label(s.manual_height);
                                ui.add(egui::TextEdit::singleline(&mut h).desired_width(90.0).hint_text("720"));
                                ui.end_row();
                                ui.label(s.manual_fps);
                                ui.add(egui::TextEdit::singleline(&mut fps).desired_width(90.0).hint_text("30"));
                                ui.end_row();
                                ui.label(s.manual_vbitrate);
                                ui.add(egui::TextEdit::singleline(&mut vb).desired_width(90.0).hint_text("2000k"));
                                ui.end_row();
                                ui.label(s.manual_abitrate);
                                ui.add(egui::TextEdit::singleline(&mut ab).desired_width(90.0).hint_text("160k"));
                                ui.end_row();
                            });
                            new_convert_preset = format!("manual:{}:{}:{}:{}", h.trim(), fps.trim(), vb.trim(), ab.trim());
                        }
                        ui.add_space(6.0);
                    }

                    if !is_convert {
                        ui.label(s.f_quality);
                        ui.horizontal(|ui| {
                            for q in &qualities {
                                let is_selected = new_quality == *q;
                                if ui
                                    .add(
                                        egui::Button::new(*q).fill(if is_selected {
                                            theme::accent()
                                        } else {
                                            theme::bg_card()
                                        }),
                                    )
                                    .clicked()
                                {
                                    new_quality = q.to_string();
                                }
                            }
                        });
                        ui.add_space(6.0);

                        if media_type == MediaType::Video {
                            if let Some(pv) = &preview {
                                if !pv.resolutions.is_empty() {
                                    ui.label(s.f_resolution);
                                    ui.horizontal_wrapped(|ui| {
                                        let best_selected = new_max_height.is_none();
                                        if ui
                                            .add(egui::Button::new(s.res_best).fill(
                                                if best_selected {
                                                    theme::accent()
                                                } else {
                                                    theme::bg_card()
                                                },
                                            ))
                                            .clicked()
                                        {
                                            new_max_height = None;
                                        }
                                        for h in pv.resolutions.iter().take(6) {
                                            let sel = new_max_height == Some(*h);
                                            if ui
                                                .add(egui::Button::new(format!("{}p", h)).fill(
                                                    if sel {
                                                        theme::accent()
                                                    } else {
                                                        theme::bg_card()
                                                    },
                                                ))
                                                .clicked()
                                            {
                                                new_max_height = Some(*h);
                                            }
                                        }
                                    });
                                    ui.add_space(6.0);
                                }
                            }
                        }

                        ui.checkbox(&mut new_clip_enabled, s.clip_label);
                        if new_clip_enabled {
                            ui.horizontal(|ui| {
                                ui.label(s.clip_from);
                                ui.add(
                                    egui::TextEdit::singleline(&mut new_clip_start)
                                        .desired_width(70.0)
                                        .hint_text("0:00"),
                                );
                                ui.label(s.clip_to);
                                ui.add(
                                    egui::TextEdit::singleline(&mut new_clip_end)
                                        .desired_width(70.0)
                                        .hint_text("1:30"),
                                );
                            });
                        }

                        if is_live {
                            ui.add_space(6.0);
                            egui::Frame::none()
                                .fill(theme::bg_card_hover())
                                .rounding(egui::Rounding::same(8.0))
                                .inner_margin(egui::Margin::same(10.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(if pt {
                                                "🔴 AO VIVO"
                                            } else {
                                                "🔴 LIVE"
                                            })
                                            .color(theme::danger())
                                            .strong(),
                                        );
                                        ui.label(
                                            egui::RichText::new(if pt {
                                                "— será gravado até você parar"
                                            } else {
                                                "— records until you stop"
                                            })
                                            .color(theme::text_muted())
                                            .size(12.0),
                                        );
                                    });
                                    ui.add_space(4.0);
                                    ui.radio_value(
                                        &mut new_live_from_start,
                                        false,
                                        if pt { "Gravar a partir de agora" } else { "Record from now" },
                                    );
                                    ui.radio_value(
                                        &mut new_live_from_start,
                                        true,
                                        if pt {
                                            "Desde o início (DVR) — pode ser enorme em lives longas"
                                        } else {
                                            "From the start (DVR) — can be huge on long lives"
                                        },
                                    );
                                });
                        } else if media_type == MediaType::Video {
                            ui.checkbox(&mut new_live_from_start, s.live_label);
                        }
                    }
                        });
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(s.btn_cancel).color(theme::text()))
                                .fill(theme::bg_card()),
                        )
                        .clicked()
                    {
                        let mut op = app.operation.lock().unwrap();
                        op.phase = DownloadPhase::Idle;
                    }
                    let confirm_label = if is_convert { s.btn_convert } else { s.btn_confirm };
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(confirm_label).color(Color32::WHITE))
                                .fill(theme::accent())
                                .min_size(egui::vec2(200.0, 36.0)),
                        )
                        .clicked()
                        {
                            let mut target_folder = new_folder_path.clone();
                            if new_create_subfolder && !new_subfolder_name.is_empty() {
                                target_folder = target_folder.join(&new_subfolder_name);
                                std::fs::create_dir_all(&target_folder).ok();
                                app.db.add_folder(
                                    &new_subfolder_name,
                                    &target_folder.to_string_lossy(),
                                );
                            }

                            let media_str = match media_type {
                                MediaType::Music => "music",
                                MediaType::Video => "video",
                                MediaType::Convert => "convert",
                            };
                            let channel = preview
                                .as_ref()
                                .map(|p| p.channel.clone())
                                .unwrap_or_default();
                            if let Some(sub) = crate::download::engine::organize_subfolder(
                                &app.config.organize_by,
                                media_str,
                                &channel,
                            ) {
                                target_folder = target_folder.join(sub);
                                std::fs::create_dir_all(&target_folder).ok();
                            }

                            let safe_name = crate::download::engine::sanitize_filename(&new_file_name);
                            let output_path = target_folder.join(&safe_name);
                            let captured_url = url.clone();
                            let captured_path = output_path.to_string_lossy().to_string();
                            let captured_format = new_format.clone();
                            let captured_quality = new_quality.clone();
                            let captured_folder = target_folder.to_string_lossy().to_string();
                            let captured_title = title.clone();
                            let captured_media_type = media_type;
                            let captured_source = source_file.to_string_lossy().to_string();
                            let captured_subs = if app.config.subtitles
                                && media_type == MediaType::Video
                            {
                                Some(app.config.sub_langs.clone())
                            } else {
                                None
                            };
                            let captured_clip = if new_clip_enabled
                                && (!new_clip_start.trim().is_empty()
                                    || !new_clip_end.trim().is_empty())
                            {
                                Some((new_clip_start.clone(), new_clip_end.clone()))
                            } else {
                                None
                            };
                            let captured_notify = app.config.notify_on_complete;
                            let captured_cloud = if app.config.copy_to_cloud
                                && !app.config.cloud_folder.trim().is_empty()
                            {
                                Some(app.config.cloud_folder.clone())
                            } else {
                                None
                            };
                            let captured_max_height = new_max_height;
                            let captured_preset = new_convert_preset.clone();
                            let captured_live = new_live_from_start;
                            let captured_rate = if app.config.rate_limit.trim().is_empty() {
                                None
                            } else {
                                Some(app.config.rate_limit.clone())
                            };
                            let captured_fragments = app.config.concurrent_fragments;
                            let captured_convert_engine = app.config.convert_engine;
                            let captured_is_live =
                                app.operation.lock().map(|o| o.is_live).unwrap_or(false) && !is_convert;
                            let captured_stop = if captured_is_live {
                                let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                                app.live_stop = Some(flag.clone());
                                app.live_started = Some(std::time::Instant::now());
                                Some(flag)
                            } else {
                                app.live_stop = None;
                                app.live_started = None;
                                None
                            };
                            let engine = app.engine.clone();
                            let op_state = app.operation.clone();
                            let db = crate::db::database::Database::open(&app.config.db_path());

                            {
                                let mut op = app.operation.lock().unwrap();
                                op.phase = DownloadPhase::Downloading(
                                    if is_convert {
                                        "Convertendo arquivo...".to_string()
                                    } else {
                                        "Iniciando download...".to_string()
                                    },
                                );
                                op.file_name = new_file_name.clone();
                                op.folder_path = new_folder_path.clone();
                                op.create_subfolder = new_create_subfolder;
                                op.subfolder_name = new_subfolder_name.clone();
                                op.output_format = new_format.clone();
                                op.quality = new_quality.clone();
                                op.progress = None;
                            }

                            let progress_state = app.operation.clone();
                            app.download_task = Some(tokio::spawn(async move {
                                match engine {
                                    Some(eng) => {
                                        let result = if captured_media_type == MediaType::Convert {
                                            eng.convert_file(
                                                &captured_source,
                                                &captured_path,
                                                &captured_format,
                                                &captured_preset,
                                                captured_convert_engine,
                                            )
                                            .await
                                        } else {
                                            let on_progress =
                                                move |pr: crate::download::engine::Progress| {
                                                    if let Ok(mut s) = progress_state.lock() {
                                                        s.progress =
                                                            Some((pr.fraction.clamp(0.0, 1.0)) as f32);
                                                        s.live_bytes = pr.downloaded_bytes;
                                                    }
                                                };
                                            let opts = crate::download::engine::DownloadOptions {
                                                is_audio: captured_media_type == MediaType::Music,
                                                format: captured_format.clone(),
                                                quality: captured_quality.clone(),
                                                max_height: captured_max_height,
                                                subtitle_langs: captured_subs,
                                                clip: captured_clip,
                                                rate_limit: captured_rate,
                                                concurrent_fragments: captured_fragments,
                                                live_from_start: captured_live,
                                                is_live: captured_is_live,
                                                stop: captured_stop,
                                            };
                                            eng.fetch_and_download(
                                                &captured_url,
                                                &captured_path,
                                                opts,
                                                on_progress,
                                            )
                                            .await
                                        };
                                        match result
                                        {
                                            Ok(p) => {
                                                let file_size = std::fs::metadata(&p)
                                                    .ok()
                                                    .map(|m| m.len() as i64);
                                                if let Some(cloud) = &captured_cloud {
                                                    if let Some(name) = p.file_name() {
                                                        let dest =
                                                            std::path::Path::new(cloud).join(name);
                                                        std::fs::create_dir_all(cloud).ok();
                                                        std::fs::copy(&p, &dest).ok();
                                                    }
                                                }
                                                db.add_history(
                                                    &captured_url,
                                                    &captured_title,
                                                    match captured_media_type {
                                                        MediaType::Music => "music",
                                                        MediaType::Video => "video",
                                                        MediaType::Convert => "convert",
                                                    },
                                                    &captured_format,
                                                    &captured_quality,
                                                    &captured_folder,
                                                    &p.to_string_lossy(),
                                                    file_size,
                                                );
                                                if captured_notify {
                                                    crate::notify::send(
                                                        "Download concluído",
                                                        &captured_title,
                                                    );
                                                }
                                                let mut s = op_state.lock().unwrap();
                                                s.phase = DownloadPhase::Completed(
                                                    p.to_string_lossy().to_string(),
                                                );
                                            }
                                            Err(e) => {
                                                let mut s = op_state.lock().unwrap();
                                                s.phase =
                                                    DownloadPhase::Failed(e.to_string());
                                            }
                                        }
                                    }
                                    None => {
                                        let mut s = op_state.lock().unwrap();
                                        s.phase =
                                            DownloadPhase::Failed("Engine não inicializado".to_string());
                                    }
                                }
                            }));
                        }
                    });
                });

            {
                let mut op = app.operation.lock().unwrap();
                if op.phase == DownloadPhase::Configuring {
                    op.file_name = new_file_name;
                    op.folder_path = new_folder_path;
                    op.create_subfolder = new_create_subfolder;
                    op.subfolder_name = new_subfolder_name;
                    op.output_format = new_format;
                    op.quality = new_quality;
                    op.clip_enabled = new_clip_enabled;
                    op.clip_start = new_clip_start;
                    op.clip_end = new_clip_end;
                    op.max_height = new_max_height;
                    op.convert_preset = new_convert_preset;
                    op.live_from_start = new_live_from_start;
                }
            }
        }

        DownloadPhase::Downloading(msg) => {
            let mut cancel = false;
            let (speed, hist) = app
                .engine
                .as_ref()
                .map(|e| e.net_stats())
                .unwrap_or((0.0, Vec::new()));
            egui::Window::new("Baixando")
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .resizable(false)
                .fixed_size([400.0, 230.0])
                .show(ctx, |ui: &mut egui::Ui| {
                    ui.vertical_centered(|ui: &mut egui::Ui| {
                        let mut stop_save = false;
                        if is_live {
                            ui.label(
                                egui::RichText::new(if pt {
                                    "🔴 Gravando ao vivo"
                                } else {
                                    "🔴 Recording live"
                                })
                                .color(theme::danger())
                                .size(16.0)
                                .strong(),
                            );
                            ui.add_space(2.0);
                            let sub = if msg.contains("Finaliz") || msg.contains("Finali") {
                                msg.clone()
                            } else if live_bytes == 0 {
                                if pt { "Conectando à live...".to_string() } else { "Connecting to live...".to_string() }
                            } else if pt {
                                "Gravando — clique em Parar quando quiser".to_string()
                            } else {
                                "Recording — click Stop whenever you want".to_string()
                            };
                            ui.label(sub);
                            ui.add(
                                egui::ProgressBar::new(0.0)
                                    .fill(theme::danger())
                                    .animate(true),
                            );
                            ui.add_space(6.0);
                            let elapsed = app
                                .live_started
                                .map(|t| t.elapsed().as_secs())
                                .unwrap_or(0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "⏱ {:02}:{:02}:{:02}   ·   💾 {}",
                                    elapsed / 3600,
                                    (elapsed % 3600) / 60,
                                    elapsed % 60,
                                    crate::download::engine::format_size(live_bytes as i64),
                                ))
                                .color(theme::text())
                                .strong(),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "⬇  {}/s",
                                    crate::download::engine::format_size(speed as i64)
                                ))
                                .color(theme::text_muted())
                                .size(12.0),
                            );
                            sparkline(ui, &hist);
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(if pt {
                                                "⏹ Parar e salvar"
                                            } else {
                                                "⏹ Stop & save"
                                            })
                                            .color(Color32::WHITE),
                                        )
                                        .fill(theme::accent())
                                        .min_size(egui::vec2(150.0, 34.0)),
                                    )
                                    .clicked()
                                {
                                    stop_save = true;
                                }
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(if pt {
                                                "Descartar"
                                            } else {
                                                "Discard"
                                            })
                                            .color(theme::text()),
                                        )
                                        .fill(theme::bg_card()),
                                    )
                                    .clicked()
                                {
                                    cancel = true;
                                }
                            });
                        } else {
                            // "Iniciando download..." fica obsoleto assim que os
                            // primeiros bytes chegam — troca por um rótulo honesto.
                            let display = if live_bytes > 0 && msg.starts_with("Iniciando") {
                                if pt { "Baixando..." } else { "Downloading..." }
                            } else {
                                msg.as_str()
                            };
                            ui.label(display);
                            match progress {
                                Some(p) => {
                                    ui.add(
                                        egui::ProgressBar::new(p)
                                            .fill(theme::accent())
                                            .show_percentage(),
                                    );
                                }
                                None => {
                                    ui.add(
                                        egui::ProgressBar::new(0.0)
                                            .fill(theme::accent())
                                            .animate(true)
                                            .text(s.dl_processing),
                                    );
                                }
                            }
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "⬇  {}/s",
                                    crate::download::engine::format_size(speed as i64)
                                ))
                                .color(theme::accent())
                                .strong(),
                            );
                            if live_bytes > 0 {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "💾 {} {}",
                                        crate::download::engine::format_size(live_bytes as i64),
                                        if pt { "baixados" } else { "downloaded" }
                                    ))
                                    .color(theme::text_muted())
                                    .size(12.0),
                                );
                            }
                            sparkline(ui, &hist);
                            ui.add_space(8.0);
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(s.btn_cancel_dl).color(theme::text()),
                                    )
                                    .fill(theme::bg_card()),
                                )
                                .clicked()
                            {
                                cancel = true;
                            }
                        }
                        if stop_save {
                            app.stop_live_recording();
                        }
                    });
                });
            if cancel {
                app.cancel_operation();
            }
            ctx.request_repaint();
        }

        DownloadPhase::Completed(path) => {
            egui::Window::new("Download Concluído")
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .resizable(false)
                .fixed_size([400.0, 140.0])
                .show(ctx, |ui: &mut egui::Ui| {
                    ui.vertical_centered(|ui: &mut egui::Ui| {
                        ui.label(
                            egui::RichText::new(s.dl_completed)
                                .color(theme::accent())
                                .size(16.0),
                        );
                        ui.add_space(8.0);
                        ui.label(format!("{} {}", s.dl_saved, path));
                        ui.add_space(12.0);
                        ui.horizontal(|ui: &mut egui::Ui| {
                            if ui
                                .add(
                                    egui::Button::new(egui::RichText::new(s.btn_open_folder).color(Color32::WHITE))
                                        .fill(theme::accent()),
                                )
                                .clicked()
                            {
                                open_folder(&path);
                            }
                            if ui
                                .add(
                                    egui::Button::new(egui::RichText::new(s.btn_close).color(theme::text()))
                                        .fill(theme::bg_card()),
                                )
                                .clicked()
                            {
                                let mut op = app.operation.lock().unwrap();
                                op.phase = DownloadPhase::Idle;
                            }
                        });
                    });
                });
        }

        DownloadPhase::Failed(msg) => {
            egui::Window::new("Erro")
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .resizable(false)
                .fixed_size([400.0, 130.0])
                .show(ctx, |ui: &mut egui::Ui| {
                    ui.vertical_centered(|ui: &mut egui::Ui| {
                        ui.label(
                            egui::RichText::new(s.dl_error)
                                .color(theme::danger())
                                .size(16.0),
                        );
                        ui.add_space(6.0);
                        ui.label(msg.as_str());
                        ui.add_space(12.0);
                        if ui
                            .add(
                                egui::Button::new(egui::RichText::new(s.btn_close).color(Color32::WHITE))
                                    .fill(theme::accent()),
                            )
                            .clicked()
                        {
                            let mut op = app.operation.lock().unwrap();
                            op.phase = DownloadPhase::Idle;
                        }
                    });
                });
        }
    }
}
