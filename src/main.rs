#![windows_subsystem = "windows"]

mod app;
mod applog;
mod config;
mod db;
mod download;
mod gamepad;
mod notify;
mod player;
mod queue;
mod ui;

fn main() -> eframe::Result<()> {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let _enter = rt.enter();

    applog::info("Lumen Downloader iniciado");
    let cfg = config::settings::Config::load();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([cfg.win_w, cfg.win_h])
        .with_min_inner_size([700.0, 450.0])
        .with_title("Lumen Downloader");
    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Lumen Downloader",
        options,
        Box::new(|_cc| {
            let app: Box<dyn eframe::App> = Box::new(app::App::new());
            app
        }),
    )
}

fn load_window_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../assets/FULL LOGO LUMEN DOWLOADER PNG.png");
    let rgba = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = rgba.dimensions();
    let side = w.min(h);
    let cropped = image::imageops::crop_imm(&rgba, 0, 0, side, side).to_image();
    let icon = image::DynamicImage::ImageRgba8(cropped)
        .thumbnail(256, 256)
        .to_rgba8();
    let (iw, ih) = icon.dimensions();
    Some(egui::IconData {
        rgba: icon.into_raw(),
        width: iw,
        height: ih,
    })
}
