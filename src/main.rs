#![windows_subsystem = "windows"]

mod app;
mod applog;
mod config;
mod db;
mod download;
mod games;
mod notify;
mod paths;
mod queue;
mod ui;

fn main() -> eframe::Result<()> {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let _enter = rt.enter();

    applog::info("Lumen Stream iniciado");
    let cfg = config::settings::Config::load();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([cfg.win_w, cfg.win_h])
        .with_min_inner_size([700.0, 450.0])
        .with_title("Lumen Stream");
    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Lumen Stream",
        options,
        Box::new(|_cc| {
            let app: Box<dyn eframe::App> = Box::new(app::App::new());
            app
        }),
    )
}

fn load_window_icon() -> Option<egui::IconData> {
    // Logo transparente: a arte (losango 1968×2112) vai até as bordas, então
    // quadrar com padding em vez de crop — o crop cortaria as pontas.
    let bytes = include_bytes!("../assets/LogoOficialLumenStreamTransparente2.png");
    let rgba = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = rgba.dimensions();
    let side = w.max(h);
    let mut square = image::RgbaImage::new(side, side);
    let ox = (side - w) / 2;
    let oy = (side - h) / 2;
    image::imageops::replace(&mut square, &rgba, ox as i64, oy as i64);
    let icon = image::DynamicImage::ImageRgba8(square)
        .thumbnail(256, 256)
        .to_rgba8();
    let (iw, ih) = icon.dimensions();
    Some(egui::IconData {
        rgba: icon.into_raw(),
        width: iw,
        height: ih,
    })
}
