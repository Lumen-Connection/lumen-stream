use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use yt_dlp::client::deps::Libraries;

mod audio_tags;
mod convert;
mod download;
mod fs_utils;
mod media;
mod models;
mod net;
mod office;
mod pdf;
mod text_utils;
mod whisper;
mod ytdlp_util;

pub use audio_tags::{read_audio_tags, write_audio_tags, AudioTags};
pub use fs_utils::{cleanup_partials, cleanup_temp_dir, part_bytes};
pub use models::{
    categorize, format_size, organize_subfolder, output_formats, DownloadOptions, FileCategory,
    FormatRow, NetStats, Progress, VideoPreview,
};
pub use office::engine_status;
pub use text_utils::{apply_template, sanitize_filename, smart_clean_name};
pub use ytdlp_util::{friendly_error, is_valid_url, looks_like_url};

use self::fs_utils::binary_path;
use self::whisper::find_whisper_exe;

pub struct DownloadEngine {
    ffmpeg_path: PathBuf,
    libs_dir: PathBuf,
    preview_cache: Mutex<HashMap<String, VideoPreview>>,
    net: Mutex<NetStats>,
    /// PIDs dos yt-dlp em andamento — para matar a árvore (yt-dlp + ffmpeg filho)
    /// ao cancelar/parar/fechar, evitando processos órfãos que travam o app.
    dl_pids: Mutex<Vec<u32>>,
}

impl DownloadEngine {
    pub async fn new(output_dir: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = crate::paths::data_dir();
        let libs_dir = data_dir.join("libs");
        std::fs::create_dir_all(&libs_dir)?;
        std::fs::create_dir_all(&output_dir)?;
        cleanup_temp_dir(&output_dir);

        let libraries = Libraries::new(
            binary_path(&libs_dir, "yt-dlp"),
            binary_path(&libs_dir, "ffmpeg"),
        );

        let libraries = libraries.install_dependencies().await?;
        let ffmpeg_path = libraries.ffmpeg.clone();

        Ok(DownloadEngine {
            ffmpeg_path,
            libs_dir,
            preview_cache: Mutex::new(HashMap::new()),
            net: Mutex::new(NetStats::default()),
            dl_pids: Mutex::new(Vec::new()),
        })
    }

    pub fn net_stats(&self) -> (f32, Vec<f32>) {
        let n = self.net.lock().unwrap();
        (n.current, n.history.clone())
    }

    /// Mata todos os downloads em andamento e suas árvores (yt-dlp + ffmpeg).
    /// Usado ao cancelar e ao fechar o app, evitando processos órfãos.
    pub fn kill_downloads(&self) {
        let pids: Vec<u32> = std::mem::take(&mut *self.dl_pids.lock().unwrap());
        for pid in pids {
            kill_tree(pid);
        }
    }

    pub async fn dependency_status(&self) -> Vec<(String, String)> {
        async fn version(cmd_path: &Path, args: &[&str]) -> Option<String> {
            let mut cmd = tokio::process::Command::new(cmd_path);
            cmd.args(args);
            #[cfg(windows)]
            cmd.creation_flags(0x08000000);
            let out = cmd.output().await.ok()?;
            let s = String::from_utf8_lossy(&out.stdout);
            s.lines().next().map(|l| l.trim().to_string()).filter(|l| !l.is_empty())
        }

        let missing_or_corrupt = |path: &Path| -> String {
            if path.exists() {
                "⚠ corrompido".to_string()
            } else {
                "não instalado".to_string()
            }
        };

        let mut rows = Vec::new();
        let yt = version(&self.ytdlp_path(), &["--version"])
            .await
            .unwrap_or_else(|| missing_or_corrupt(&self.ytdlp_path()));
        rows.push(("yt-dlp".to_string(), yt));

        let ff = version(&self.ffmpeg_path, &["-version"])
            .await
            .map(|l| l.replace("ffmpeg version ", "").split(' ').next().unwrap_or("").to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| missing_or_corrupt(&self.ffmpeg_path));
        rows.push(("ffmpeg".to_string(), ff));

        let pdfium = {
            use pdfium_render::prelude::Pdfium;
            let p = self.libs_dir.join(Pdfium::pdfium_platform_library_name());
            if p.exists() { "instalado".to_string() } else { "não baixado".to_string() }
        };
        rows.push(("pdfium".to_string(), pdfium));

        let whisper = {
            let dir = self.libs_dir.join("whisper");
            let has_exe = find_whisper_exe(&dir).is_some();
            let has_model = dir.join("ggml-base.bin").exists();
            match (has_exe, has_model) {
                (true, true) => "instalado (base)".to_string(),
                (true, false) => "binário ok, sem modelo".to_string(),
                _ => "não baixado".to_string(),
            }
        };
        rows.push(("whisper.cpp".to_string(), whisper));

        rows
    }
}

/// Mata um processo e toda a sua árvore de filhos (ex.: yt-dlp + ffmpeg).
fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
        let _ = cmd.output();
    }
    #[cfg(not(windows))]
    {
        // O yt-dlp é criado como líder do próprio process group (process_group(0)
        // no spawn), então o pgid == pid: matar "-pid" derruba o grupo inteiro,
        // incluindo o ffmpeg filho — que um kill só no pid deixaria órfão.
        let _ = std::process::Command::new("kill")
            .args(["-9", "--", &format!("-{}", pid)])
            .output();
        // Fallback: se o processo não era líder de grupo (não veio do spawn do
        // download), mata ao menos o pid direto.
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
}

/// Aguarda até a flag de parada ser acionada; se não houver flag, nunca resolve.
async fn wait_for_stop(stop: &Option<Arc<AtomicBool>>) {
    match stop {
        Some(s) => loop {
            if s.load(Ordering::Relaxed) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        },
        None => std::future::pending::<()>().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Teste real de rede (opt-in): garante que a busca de informações de um vídeo
    // do YouTube NÃO pendura e retorna um preview. Reproduz o bug corrigido —
    // antes o caminho do YouTube usava `Downloader::fetch_video_infos` (crate),
    // que travava (yt-dlp não retornava); agora usa `ytdlp_preview` (cmd.output,
    // com stdout/stderr drenados). Marcado `#[ignore]` por depender de rede e do
    // yt-dlp instalado — rode com `cargo test -- --ignored`.
    #[ignore]
    #[tokio::test]
    async fn youtube_preview_does_not_hang() {
        let out = std::env::temp_dir().join("lumen_stream_engine_test");
        let engine = DownloadEngine::new(out).await.expect("engine deve inicializar");
        let url = "https://www.youtube.com/watch?v=ZYmMCJMUhnw";
        let res = tokio::time::timeout(Duration::from_secs(90), engine.fetch_preview(url)).await;
        let preview = res
            .expect("fetch_preview não deve pendurar (timeout de 90s)")
            .expect("fetch_preview deve retornar um preview");
        assert!(!preview.title.trim().is_empty(), "preview deve ter título");
    }

    // Teste real de rede (opt-in): a busca de uma playlist do YouTube não pendura
    // e retorna itens com URL de vídeo. Reproduz o bug corrigido — antes usava
    // `youtube_extractor().fetch_playlist_paginated` (crate), que podia travar/
    // falhar em silêncio e deixar a fila vazia; agora usa yt-dlp `--flat-playlist`
    // drenado via `cmd.output()`. Rode com `cargo test -- --ignored`.
    #[ignore]
    #[tokio::test]
    async fn playlist_fetch_returns_items() {
        let out = std::env::temp_dir().join("lumen_stream_engine_test_pl");
        let engine = DownloadEngine::new(out).await.expect("engine deve inicializar");
        // Playlist de "uploads" de um canal real (id estável).
        let res = tokio::time::timeout(
            Duration::from_secs(90),
            engine.fetch_playlist("UU57eS8alHoHqPE7iutLb-HQ"),
        )
        .await;
        let items = res
            .expect("fetch_playlist não deve pendurar (timeout de 90s)")
            .expect("fetch_playlist deve retornar itens");
        assert!(!items.is_empty(), "playlist deve ter itens");
        assert!(
            items.iter().all(|(u, _)| u.contains("watch?v=")),
            "cada item deve ter uma URL de vídeo do YouTube"
        );
    }
}
