use std::path::PathBuf;

use crate::app::App;
use crate::ui::i18n::Lang;
use crate::ui::theme;

const IMG_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];
const MAX_IMAGES: usize = 200;
const LOAD_PER_FRAME: usize = 6;

/// Seção de imagens exibida logo abaixo do histórico de downloads de vídeo.
/// Repete o padrão visual da grade do histórico (cartão com miniatura, título,
/// formato e ações), no lugar da antiga aba "Galeria".
pub fn render_images(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    let pt = app.config.lang == Lang::Pt;
    let images = collect_images(&app.config.default_download_dir);

    ui.add_space(20.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(if pt { "Imagens" } else { "Images" })
                .color(theme::text())
                .size(18.0)
                .strong(),
        );
        ui.label(
            egui::RichText::new(if pt {
                "Miniaturas e imagens da sua pasta de download."
            } else {
                "Thumbnails and images from your download folder."
            })
            .color(theme::text_faint())
            .size(12.0),
        );
    });
    ui.add_space(8.0);

    if images.is_empty() {
        theme::card_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new(if pt {
                    "Nenhuma imagem encontrada na pasta de download."
                } else {
                    "No images found in the download folder."
                })
                .color(theme::text_faint()),
            );
        });
        return;
    }

    // Carrega miniaturas de forma incremental para não travar um frame.
    let mut loaded_now = 0;
    for path in &images {
        if loaded_now >= LOAD_PER_FRAME {
            ctx.request_repaint();
            break;
        }
        if !app.gallery_textures.contains_key(path) {
            if let Some(tex) = load_thumb(ctx, path) {
                app.gallery_textures.insert(path.clone(), tex);
            } else {
                let blank = ctx.load_texture(
                    "blank",
                    egui::ColorImage::new([1, 1], theme::bg_card()),
                    egui::TextureOptions::LINEAR,
                );
                app.gallery_textures.insert(path.clone(), blank);
            }
            loaded_now += 1;
        }
    }

    // Grade no mesmo padrão do histórico de downloads (sem ScrollArea aninhado:
    // set_width prende a largura ao container e evita cortar ao rolar a página).
    theme::card_frame().show(ui, |ui| {
        let avail = ui.available_width();
        ui.set_width(avail);
        let cols = (((avail + 12.0) / 220.0).floor() as usize).max(1);
        for chunk in images.chunks(cols) {
            ui.columns(cols, |c| {
                for (k, path) in chunk.iter().enumerate() {
                    let ui = &mut c[k];
                    egui::Frame::none()
                        .fill(theme::bg_card())
                        .stroke(egui::Stroke::new(1.0_f32, theme::border()))
                        .rounding(egui::Rounding::same(8.0))
                        .inner_margin(egui::Margin::same(10.0))
                        .show(ui, |ui| {
                            let cw = ui.available_width();
                            let thumb_h = (cw * 9.0 / 16.0).min(150.0);
                            ui.set_height(thumb_h + 82.0);
                            ui.vertical(|ui| {
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(cw, thumb_h),
                                    egui::Sense::click(),
                                );
                                ui.painter().rect_filled(
                                    rect,
                                    egui::Rounding::same(4.0),
                                    theme::bg_card_hover(),
                                );
                                if let Some(tex) = app.gallery_textures.get(path) {
                                    let [w, h] = tex.size();
                                    let scale = (cw / w.max(1) as f32)
                                        .min(thumb_h / h.max(1) as f32);
                                    let disp = egui::vec2(w as f32 * scale, h as f32 * scale);
                                    let img_rect =
                                        egui::Rect::from_center_size(rect.center(), disp);
                                    ui.painter().image(
                                        tex.id(),
                                        img_rect,
                                        egui::Rect::from_min_max(
                                            egui::pos2(0.0, 0.0),
                                            egui::pos2(1.0, 1.0),
                                        ),
                                        egui::Color32::WHITE,
                                    );
                                }
                                if resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if resp.clicked() {
                                    open::that(path).ok();
                                }
                                ui.add_space(4.0);

                                let name = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&name)
                                            .color(theme::text())
                                            .size(12.0),
                                    )
                                    .truncate(true),
                                )
                                .on_hover_text(&name);

                                let ext = path
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .map(|e| e.to_uppercase())
                                    .unwrap_or_default();
                                ui.label(
                                    egui::RichText::new(ext)
                                        .color(theme::text_muted())
                                        .size(11.0),
                                );
                                ui.add_space(2.0);

                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    ui.spacing_mut().button_padding = egui::vec2(2.0, 4.0);
                                    if icon_button(ui, "▶", if pt { "Abrir" } else { "Open" }) {
                                        open::that(path).ok();
                                    }
                                    if icon_button(ui, "📁", if pt { "Pasta" } else { "Folder" }) {
                                        if let Some(p) = path.parent() {
                                            open::that(p).ok();
                                        }
                                    }
                                });
                            });
                        });
                }
            });
            ui.add_space(12.0);
        }
    });
}

fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> bool {
    ui.add(
        egui::Button::new(egui::RichText::new(icon).color(theme::text()))
            .fill(theme::bg_card())
            .min_size(egui::vec2(30.0, 26.0)),
    )
    .on_hover_text(tooltip)
    .clicked()
}

fn collect_images(root: &std::path::Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let is_img = |p: &std::path::Path| {
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| IMG_EXTS.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false)
    };
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if dir == root {
                    dirs.push(p);
                }
            } else if is_img(&p) {
                out.push(p);
                if out.len() >= MAX_IMAGES {
                    return out;
                }
            }
        }
    }
    out
}

fn load_thumb(ctx: &egui::Context, path: &std::path::Path) -> Option<egui::TextureHandle> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let img = img.thumbnail(300, 300);
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
    Some(ctx.load_texture(
        path.to_string_lossy(),
        color,
        egui::TextureOptions::LINEAR,
    ))
}
