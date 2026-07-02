
use std::io::Write;
use std::path::PathBuf;

fn log_path() -> PathBuf {
    let dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LumenDownloader");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("lumen.log")
}

pub fn log(level: &str, msg: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(f, "[{}] {}: {}", ts, level, msg);
    }
}

pub fn info(msg: &str) {
    log("INFO", msg);
}

pub fn error(msg: &str) {
    log("ERROR", msg);
}
