
use std::sync::atomic::{AtomicBool, Ordering};

use egui::{Color32, FontId, RichText, Rounding, Stroke};

static LIGHT: AtomicBool = AtomicBool::new(false);
static HIGH_CONTRAST: AtomicBool = AtomicBool::new(false);
static COMPACT: AtomicBool = AtomicBool::new(false);

pub fn set_compact(c: bool) {
    COMPACT.store(c, Ordering::Relaxed);
}

pub fn is_compact() -> bool {
    COMPACT.load(Ordering::Relaxed)
}

pub fn set_light(light: bool) {
    LIGHT.store(light, Ordering::Relaxed);
}

pub fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

pub fn set_high_contrast(hc: bool) {
    HIGH_CONTRAST.store(hc, Ordering::Relaxed);
}

pub fn is_hc() -> bool {
    HIGH_CONTRAST.load(Ordering::Relaxed)
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub fn bg_app() -> Color32 {
    if is_light() { rgb(0xf3, 0xf6, 0xf8) } else { rgb(0x0a, 0x0e, 0x12) }
}
pub fn bg_sidebar() -> Color32 {
    if is_light() { rgb(0xe7, 0xed, 0xf1) } else { rgb(0x07, 0x0a, 0x0d) }
}
pub fn bg_card() -> Color32 {
    if is_light() { rgb(0xff, 0xff, 0xff) } else { rgb(0x12, 0x18, 0x21) }
}
pub fn bg_card_hover() -> Color32 {
    if is_light() { rgb(0xe4, 0xeb, 0xef) } else { rgb(0x1c, 0x25, 0x30) }
}
pub fn bg_input() -> Color32 {
    if is_light() { rgb(0xff, 0xff, 0xff) } else { rgb(0x16, 0x1d, 0x27) }
}
pub fn border() -> Color32 {
    if is_hc() {
        return if is_light() { rgb(0x00, 0x00, 0x00) } else { rgb(0xc8, 0xd0, 0xd6) };
    }
    if is_light() { rgb(0xd2, 0xdb, 0xe1) } else { rgb(0x26, 0x32, 0x40) }
}

pub fn accent() -> Color32 {
    rgb(0xff, 0x57, 0x22)
}
pub fn accent_soft() -> Color32 {
    let a = accent();
    if is_light() {
        blend(a, Color32::WHITE, 0.85)
    } else {
        a.linear_multiply(0.22)
    }
}

fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

pub fn text() -> Color32 {
    if is_hc() {
        return if is_light() { rgb(0x00, 0x00, 0x00) } else { rgb(0xff, 0xff, 0xff) };
    }
    if is_light() { rgb(0x16, 0x20, 0x2a) } else { rgb(0xee, 0xf3, 0xf6) }
}
pub fn text_muted() -> Color32 {
    if is_hc() {
        return if is_light() { rgb(0x20, 0x20, 0x24) } else { rgb(0xe0, 0xe6, 0xea) };
    }
    if is_light() { rgb(0x54, 0x61, 0x6c) } else { rgb(0x93, 0xa1, 0xad) }
}
pub fn text_faint() -> Color32 {
    if is_hc() {
        return if is_light() { rgb(0x40, 0x40, 0x46) } else { rgb(0xc0, 0xc6, 0xcc) };
    }
    if is_light() { rgb(0x94, 0xa2, 0xac) } else { rgb(0x5a, 0x66, 0x70) }
}

pub fn danger() -> Color32 {
    if is_light() { rgb(0xd3, 0x2f, 0x2f) } else { rgb(0xff, 0x4d, 0x4d) }
}

pub const CARD_ROUNDING: f32 = 10.0;

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;

    v.dark_mode = !is_light();
    v.override_text_color = Some(text());
    v.hyperlink_color = accent();
    v.warn_fg_color = accent();
    v.error_fg_color = danger();

    v.selection.bg_fill = accent().linear_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0_f32, accent());

    v.window_fill = bg_card();
    v.window_stroke = Stroke::new(1.0_f32, border());
    v.window_rounding = Rounding::same(12.0);
    v.window_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 8.0),
        blur: 24.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if is_light() { 60 } else { 160 }),
    };
    v.popup_shadow = v.window_shadow;

    v.panel_fill = bg_app();
    v.faint_bg_color = bg_card();
    v.extreme_bg_color = bg_input();
    v.code_bg_color = bg_input();

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = bg_card();
    w.noninteractive.weak_bg_fill = bg_card();
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, border());
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, text_muted());
    w.noninteractive.rounding = Rounding::same(8.0);

    w.inactive.bg_fill = bg_input();
    w.inactive.weak_bg_fill = bg_input();
    w.inactive.bg_stroke = Stroke::new(1.0_f32, border());
    w.inactive.fg_stroke = Stroke::new(1.0_f32, text());
    w.inactive.rounding = Rounding::same(8.0);

    w.hovered.bg_fill = bg_card_hover();
    w.hovered.weak_bg_fill = bg_card_hover();
    w.hovered.bg_stroke = Stroke::new(1.0_f32, accent());
    w.hovered.fg_stroke = Stroke::new(1.0_f32, text());
    w.hovered.rounding = Rounding::same(8.0);

    w.active.bg_fill = accent();
    w.active.weak_bg_fill = accent();
    w.active.bg_stroke = Stroke::new(1.0_f32, accent());
    w.active.fg_stroke = Stroke::new(1.0_f32, text());
    w.active.rounding = Rounding::same(8.0);

    if is_compact() {
        style.spacing.item_spacing = egui::vec2(7.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.window_margin = egui::Margin::same(12.0);
    } else {
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        style.spacing.window_margin = egui::Margin::same(18.0);
    }

    ctx.set_style(style);
}

/// Margem interna do `card_frame`. Exposta para quem precisa converter entre a
/// altura externa de um cartão e a altura do conteúdo dentro dele.
pub const CARD_MARGIN: f32 = 18.0;

/// Largura de conteúdo abaixo da qual a página entra em modo estreito. É o que
/// sobra com a janela no tamanho mínimo (700px), descontando a barra lateral e
/// as margens do painel central.
pub const NARROW_W: f32 = 470.0;

pub fn is_narrow(ui: &egui::Ui) -> bool {
    ui.available_width() < NARROW_W
}

/// Cabeçalho padrão de aba: título + subtítulo.
///
/// Os dois quebram linha e o título encolhe em janela estreita. Um `ui.label`
/// solto dentro de `horizontal` não quebra, e no tamanho mínimo de janela o
/// título simplesmente saía pela borda direita e era cortado.
pub fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    let size = if is_narrow(ui) { 21.0 } else { 30.0 };
    ui.add(
        egui::Label::new(RichText::new(title).color(text()).size(size).strong()).wrap(true),
    );
    if !subtitle.is_empty() {
        ui.add(
            egui::Label::new(RichText::new(subtitle).color(text_muted()).size(14.0)).wrap(true),
        );
    }
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(bg_card())
        .rounding(Rounding::same(CARD_ROUNDING))
        .stroke(Stroke::new(1.0_f32, border()))
        .inner_margin(egui::Margin::same(CARD_MARGIN))
}

pub fn accent_button(label: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(label.to_string()).color(Color32::WHITE).size(15.0))
        .fill(accent())
        .rounding(Rounding::same(8.0))
}

pub fn ghost_button(label: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(label.to_string()).color(text()).size(15.0))
        .fill(bg_card())
        .rounding(Rounding::same(8.0))
}

pub fn paste_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

pub fn set_clipboard(text: &str) -> bool {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .is_ok()
}

pub fn nav_item(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> bool {
    let desired = egui::vec2(ui.available_width(), 46.0);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());

    let focused = resp.has_focus();
    let bg = if selected {
        accent_soft()
    } else if resp.hovered() || focused {
        bg_card_hover()
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, Rounding::same(10.0), bg);

    if selected {
        let bar = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.min.y + 9.0),
            egui::vec2(3.0, rect.height() - 18.0),
        );
        ui.painter().rect_filled(bar, Rounding::same(2.0), accent());
    }

    // A aba ativa mantém a fonte branca (text()) — nunca troca para laranja.
    let color = text();
    ui.painter().text(
        egui::pos2(rect.min.x + 14.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        icon,
        FontId::proportional(21.0),
        color,
    );
    ui.painter().text(
        egui::pos2(rect.min.x + 48.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(17.5),
        color,
    );

    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if focused && resp.ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space)) {
        return true;
    }
    resp.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Teste único: os modos são estado global (atomics) e testes rodam em
    // paralelo — um só teste evita corrida entre set/get de temas diferentes.
    #[test]
    fn theme_modes_swap_palettes_and_keep_accent() {
        let (l0, h0, c0) = (is_light(), is_hc(), is_compact());

        set_light(false);
        set_high_contrast(false);
        let dark_bg = bg_app();
        let dark_text = text();

        set_light(true);
        assert!(is_light());
        assert_ne!(bg_app(), dark_bg, "claro e escuro devem ter fundos distintos");
        assert_ne!(text(), dark_text, "texto acompanha o tema");

        // O laranja da marca é fixo, independente do tema.
        set_light(false);
        let a = accent();
        set_light(true);
        assert_eq!(accent(), a);
        assert_eq!(a, Color32::from_rgb(0xff, 0x57, 0x22));

        let normal_border = border();
        set_high_contrast(true);
        assert!(is_hc());
        assert_ne!(border(), normal_border);

        set_compact(true);
        assert!(is_compact());

        // Restaura o estado global para não vazar para outros testes.
        set_light(l0);
        set_high_contrast(h0);
        set_compact(c0);
    }
}
