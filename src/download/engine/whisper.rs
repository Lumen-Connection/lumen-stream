use std::path::{Path, PathBuf};

use super::fs_utils::{find_named, find_output};
use super::DownloadEngine;

impl DownloadEngine {
    pub async fn transcribe(
        &self,
        input: &str,
        lang: &str,
        translate: bool,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let (exe, model) = self.ensure_whisper().await?;

        let input_path = PathBuf::from(input);
        let folder = input_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = input_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "audio".to_string());

        let wav = folder.join(format!("{}.whisper.wav", stem));
        self.ensure_ffmpeg().await?;
        let mut conv = tokio::process::Command::new(&self.ffmpeg_path);
        conv.arg("-y")
            .arg("-i")
            .arg(&input_path)
            .arg("-ar")
            .arg("16000")
            .arg("-ac")
            .arg("1")
            .arg("-c:a")
            .arg("pcm_s16le")
            .arg(&wav);
        #[cfg(windows)]
        conv.creation_flags(0x08000000);
        let conv_res = conv.output().await?;
        if !conv_res.status.success() {
            let stderr = String::from_utf8_lossy(&conv_res.stderr);
            let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            return Err(format!("ffmpeg falhou ao preparar o áudio: {}", last).into());
        }

        let out_base = folder.join(&stem);
        let mut cmd = tokio::process::Command::new(&exe);
        if let Some(exe_dir) = exe.parent() {
            cmd.current_dir(exe_dir);
        }
        cmd.arg("-m")
            .arg(&model)
            .arg("-f")
            .arg(&wav)
            .arg("-otxt")
            .arg("-of")
            .arg(&out_base)
            .arg("-l")
            .arg(if lang.trim().is_empty() { "auto" } else { lang.trim() });
        if translate {
            cmd.arg("-tr");
        }
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let result = cmd.output().await?;
        let _ = std::fs::remove_file(&wav);
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            let stdout = String::from_utf8_lossy(&result.stdout);
            let combined = format!("{}\n{}", stderr, stdout);
            let last = combined
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| {
                    format!(
                        "o binário não iniciou (código {:?}). Tente novamente; pode faltar uma DLL do whisper.cpp.",
                        result.status.code()
                    )
                });
            crate::applog::error(&format!("whisper falhou: {}", last));
            return Err(format!("Whisper falhou: {}", last).into());
        }

        let out = folder.join(format!("{}.txt", stem));
        if out.exists() {
            Ok(out)
        } else {
            find_output(&folder, &stem).ok_or_else(|| "Transcrição não encontrada.".into())
        }
    }

    async fn ensure_whisper(&self) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
        let dir = self.libs_dir.join("whisper");
        std::fs::create_dir_all(&dir)?;
        let model = dir.join("ggml-base.bin");

        if find_whisper_exe(&dir).is_none() {
            #[cfg(windows)]
            {
                crate::applog::info("baixando whisper.cpp (binário)");
                let url = "https://github.com/ggerganov/whisper.cpp/releases/latest/download/whisper-bin-x64.zip";
                let bytes = reqwest::get(url).await?.error_for_status()?.bytes().await?;
                let dir2 = dir.clone();
                tokio::task::spawn_blocking(move || -> Result<(), String> {
                    let reader = std::io::Cursor::new(bytes.as_ref());
                    let mut archive = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;
                    for i in 0..archive.len() {
                        let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
                        let outpath = match f.enclosed_name() {
                            Some(p) => dir2.join(p),
                            None => continue,
                        };
                        if f.is_dir() {
                            std::fs::create_dir_all(&outpath).ok();
                        } else {
                            if let Some(p) = outpath.parent() {
                                std::fs::create_dir_all(p).ok();
                            }
                            let mut out = std::fs::File::create(&outpath).map_err(|e| e.to_string())?;
                            std::io::copy(&mut f, &mut out).map_err(|e| e.to_string())?;
                        }
                    }
                    Ok(())
                })
                .await
                .map_err(|e| e.to_string())??;
            }
            #[cfg(not(windows))]
            {
                return Err("Transcrição automática disponível apenas no Windows por enquanto.".into());
            }
        }

        let exe = find_whisper_exe(&dir)
            .ok_or("binário do whisper.cpp não encontrado após o download")?;

        let model_ok = std::fs::metadata(&model)
            .map(|m| m.len() > 1_000_000)
            .unwrap_or(false);
        if !model_ok {
            let _ = std::fs::remove_file(&model);
            crate::applog::info("baixando modelo do whisper (ggml-base)");
            let url =
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin?download=true";
            let bytes = reqwest::get(url).await?.error_for_status()?.bytes().await?;
            if bytes.len() < 1_000_000 {
                return Err("download do modelo do Whisper falhou (arquivo muito pequeno).".into());
            }
            let model2 = model.clone();
            tokio::task::spawn_blocking(move || std::fs::write(&model2, &bytes))
                .await
                .map_err(|e| e.to_string())??;
        }

        Ok((exe, model))
    }
}

pub(super) fn find_whisper_exe(dir: &Path) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["whisper-cli.exe", "main.exe"]
    } else {
        &["whisper-cli", "main"]
    };
    for name in names {
        if let Some(found) = find_named(dir, name, 3) {
            return Some(found);
        }
    }
    None
}
