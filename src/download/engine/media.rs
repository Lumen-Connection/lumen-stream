use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncReadExt};

use super::models::{AudioMeta, Progress};
use super::DownloadEngine;

/// Extrai `Duration: HH:MM:SS.xx` do stderr do ffmpeg.
pub(super) fn parse_ffmpeg_duration(text: &str) -> Option<f64> {
    for line in text.lines() {
        let t = line.trim();
        let rest = t.strip_prefix("Duration:")?.trim();
        let token = rest.split(',').next()?.trim();
        let mut parts = token.split(':');
        let h: f64 = parts.next()?.parse().ok()?;
        let m: f64 = parts.next()?.parse().ok()?;
        let s: f64 = parts.next()?.parse().ok()?;
        return Some(h * 3600.0 + m * 60.0 + s);
    }
    None
}

/// Converte `out_time_us=N` (microsegundos) em segundos.
pub(super) fn parse_out_time_us(line: &str) -> Option<f64> {
    let v = line.trim().strip_prefix("out_time_us=")?;
    let us: f64 = v.parse().ok()?;
    Some(us / 1_000_000.0)
}

/// Fração de progresso, limitada a 1.0.
pub(super) fn progress_fraction(out_time_secs: f64, total_secs: f64) -> f64 {
    if total_secs <= 0.0 {
        return 0.0;
    }
    (out_time_secs / total_secs).clamp(0.0, 1.0)
}

impl DownloadEngine {
    /// Duração do arquivo via `ffmpeg -i` (stderr).
    async fn probe_duration_secs(&self, input: &Path) -> Option<f64> {
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-hide_banner").arg("-i").arg(input);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let output = cmd.output().await.ok()?;
        parse_ffmpeg_duration(&String::from_utf8_lossy(&output.stderr))
    }

    /// Roda o ffmpeg lendo `-progress pipe:1` e reportando fração real.
    /// Registra o PID em `dl_pids` para cancelamento matar a árvore.
    async fn run_ffmpeg_progress<F>(
        &self,
        mut cmd: tokio::process::Command,
        total_secs: Option<f64>,
        on_progress: Option<&F>,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: Fn(Progress) + Send + Sync,
    {
        // -progress pipe:1 emite key=value no stdout; -nostats evita lixo no stderr.
        cmd.arg("-progress").arg("pipe:1").arg("-nostats");
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let mut child = cmd.spawn()?;
        let pid = child.id();
        if let Some(pid) = pid {
            self.dl_pids.lock().unwrap().push(pid);
        }

        let stdout = child.stdout.take();
        let mut stderr_buf = Vec::new();
        let stderr_task = {
            let mut stderr = child.stderr.take();
            tokio::spawn(async move {
                if let Some(ref mut s) = stderr {
                    let _ = s.read_to_end(&mut stderr_buf).await;
                }
                stderr_buf
            })
        };

        if let Some(stdout) = stdout {
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim() == "progress=end" {
                    if let Some(cb) = on_progress {
                        cb(Progress {
                            fraction: 1.0,
                            ..Default::default()
                        });
                    }
                    break;
                }
                if let (Some(total), Some(secs)) = (total_secs, parse_out_time_us(&line)) {
                    if let Some(cb) = on_progress {
                        cb(Progress {
                            fraction: progress_fraction(secs, total),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        let status = child.wait().await?;
        if let Some(pid) = pid {
            self.dl_pids.lock().unwrap().retain(|running| *running != pid);
        }
        let stderr = stderr_task.await.unwrap_or_default();
        if !status.success() {
            let text = String::from_utf8_lossy(&stderr);
            let last = text
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("");
            return Err(format!("ffmpeg falhou: {}", last).into());
        }
        Ok(())
    }

    /// Re-encodes a downloaded video source into one of Lumen Stream's named
    /// video profiles. This is deliberately separate from yt-dlp's merge step:
    /// a container extension alone does not guarantee the stream codecs.
    pub(super) async fn transcode_video_profile(
        &self,
        input: &Path,
        output: &Path,
        format: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let profile = super::video_profile(format)
            .ok_or_else(|| format!("perfil de vídeo desconhecido: {}", format))?;
        let temp_output = profile_temp_output(output);
        let _ = std::fs::remove_file(&temp_output);

        let total = self.probe_duration_secs(input).await;
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(input)
            .arg("-map")
            .arg("0:v:0")
            .arg("-map")
            .arg("0:a?")
            .arg("-map_metadata")
            .arg("0")
            .args(video_profile_ffmpeg_args(profile))
            .arg(&temp_output);

        // Sem callback de UI aqui: o estágio Transcoding da task 01 já cobre
        // o feedback honesto; o progresso percentual é da conversão avulsa.
        let nop = |_: Progress| {};
        self.run_ffmpeg_progress(cmd, total, Some(&nop)).await.map_err(|e| {
            let _ = std::fs::remove_file(&temp_output);
            e
        })?;

        if std::fs::metadata(&temp_output)
            .map(|metadata| metadata.len() == 0)
            .unwrap_or(true)
        {
            let _ = std::fs::remove_file(&temp_output);
            return Err(format!("ffmpeg falhou ao gerar {}: saída vazia", profile.label).into());
        }

        // Never expose a partial final file. Replace an existing target only
        // after FFmpeg successfully produced a non-empty temporary output.
        let _ = std::fs::remove_file(output);
        std::fs::rename(&temp_output, output)?;
        Ok(())
    }

    pub async fn generate_thumbnail(
        &self,
        video: &str,
        out: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(video)
            .arg("-vf")
            .arg("thumbnail,scale=160:-1")
            .arg("-frames:v")
            .arg("1")
            .arg(out);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let output = cmd.output().await?;
        if !output.status.success() || !out.exists() {
            return Err("falha ao gerar miniatura".into());
        }
        Ok(())
    }

    pub async fn detect_bpm(&self, file: &str) -> Result<u32, Box<dyn std::error::Error>> {
        const SR: usize = 11025;
        const FRAME: usize = 512;
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-i")
            .arg(file)
            .arg("-t")
            .arg("90")
            .arg("-ac")
            .arg("1")
            .arg("-ar")
            .arg(SR.to_string())
            .arg("-f")
            .arg("s16le")
            .arg("-");
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let output = cmd.output().await?;
        if !output.status.success() && output.stdout.is_empty() {
            return Err("não foi possível ler o áudio".into());
        }

        let samples: Vec<i16> = output
            .stdout
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        if samples.len() < SR {
            return Err("áudio muito curto".into());
        }
        let mut energy: Vec<f32> = Vec::new();
        for frame in samples.chunks(FRAME) {
            let e: f64 = frame.iter().map(|&s| (s as f64) * (s as f64)).sum();
            energy.push((e / frame.len() as f64) as f32);
        }
        let onset: Vec<f32> = energy
            .windows(2)
            .map(|w| (w[1] - w[0]).max(0.0))
            .collect();
        if onset.len() < 32 {
            return Err("áudio insuficiente para estimar BPM".into());
        }

        let frame_period = FRAME as f32 / SR as f32;
        let lag_for = |bpm: f32| ((60.0 / bpm) / frame_period).round() as usize;
        let (lag_min, lag_max) = (lag_for(180.0).max(1), lag_for(60.0));
        let mut best_lag = lag_min;
        let mut best_val = f32::MIN;
        for lag in lag_min..=lag_max.min(onset.len() / 2) {
            let mut sum = 0.0f32;
            for i in lag..onset.len() {
                sum += onset[i] * onset[i - lag];
            }
            if sum > best_val {
                best_val = sum;
                best_lag = lag;
            }
        }
        let bpm = (60.0 / (best_lag as f32 * frame_period)).round() as u32;
        Ok(bpm.clamp(40, 220))
    }

    pub async fn probe_metadata(&self, file: &str) -> Result<String, Box<dyn std::error::Error>> {
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-hide_banner").arg("-i").arg(file);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let output = cmd.output().await?;
        let text = String::from_utf8_lossy(&output.stderr);
        // Reusa o parser de duração (testável) — só filtra as linhas úteis.
        let _ = parse_ffmpeg_duration(&text);
        let mut lines: Vec<String> = Vec::new();
        for l in text.lines() {
            let t = l.trim();
            if t.starts_with("Input #")
                || t.starts_with("Duration")
                || t.starts_with("Stream #")
                || t.starts_with("Metadata")
                || t.starts_with("title")
                || t.starts_with("artist")
                || t.starts_with("album")
                || t.starts_with("encoder")
                || t.starts_with("major_brand")
            {
                lines.push(t.to_string());
            }
        }
        if lines.is_empty() {
            return Err("não foi possível ler os metadados".into());
        }
        Ok(lines.join("\n"))
    }

    pub(super) async fn transcode_audio<F>(
        &self,
        input: &Path,
        output: &Path,
        format: &str,
        meta: &AudioMeta,
        on_progress: Option<&F>,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: Fn(Progress) + Send + Sync,
    {
        let total = self.probe_duration_secs(input).await;
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(input)
            .arg("-vn")
            .arg("-map_metadata")
            .arg("0");

        if let Some(t) = &meta.title {
            cmd.arg("-metadata").arg(format!("title={}", t));
        }
        if let Some(a) = &meta.artist {
            cmd.arg("-metadata").arg(format!("artist={}", a));
        }
        if let Some(al) = &meta.album {
            cmd.arg("-metadata").arg(format!("album={}", al));
        }

        match format {
            "mp3" => {
                cmd.arg("-c:a").arg("libmp3lame").arg("-q:a").arg("2");
            }
            "m4a" | "aac" => {
                cmd.arg("-c:a").arg("aac").arg("-b:a").arg("192k");
            }
            "opus" => {
                cmd.arg("-c:a").arg("libopus").arg("-b:a").arg("160k");
            }
            "ogg" => {
                cmd.arg("-c:a").arg("libvorbis").arg("-q:a").arg("5");
            }
            "wav" => {
                cmd.arg("-c:a").arg("pcm_s16le");
            }
            "flac" => {
                cmd.arg("-c:a").arg("flac");
            }
            _ => {}
        }
        cmd.arg(output);

        self.run_ffmpeg_progress(cmd, total, on_progress)
            .await
            .map_err(|e| format!("ffmpeg falhou ao converter para {}: {}", format, e).into())
    }

    pub async fn extract_frames(
        &self,
        input: &str,
        fps: u32,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let input_path = PathBuf::from(input);
        let folder = input_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = input_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "video".to_string());
        let out_dir = folder.join(format!("{}_frames", stem));
        std::fs::create_dir_all(&out_dir)?;
        let pattern = out_dir.join("frame_%04d.png");

        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(&input_path)
            .arg("-vf")
            .arg(format!("fps={}", fps.max(1)))
            .arg(&pattern);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let result = cmd.output().await?;
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            return Err(format!("ffmpeg falhou ao extrair frames: {}", last).into());
        }
        Ok(out_dir)
    }

    /// Converte imagens em lote. `on_progress(done, total)` a cada item.
    /// Retorna `(pasta, nomes que falharam)`.
    pub async fn batch_convert_images<F>(
        &self,
        inputs: Vec<PathBuf>,
        out_dir: PathBuf,
        format: String,
        max_width: u32,
        quality: u32,
        on_progress: F,
    ) -> Result<(PathBuf, Vec<String>), Box<dyn std::error::Error>>
    where
        F: Fn(usize, usize) + Send + Sync,
    {
        std::fs::create_dir_all(&out_dir)?;
        let total = inputs.len();
        let mut ok = 0usize;
        let mut failed: Vec<String> = Vec::new();
        let mut last_err = String::new();
        for (i, inp) in inputs.iter().enumerate() {
            on_progress(i, total);
            let stem = inp
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "imagem".to_string());
            let name = inp
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| stem.clone());
            let out = out_dir.join(format!("{}.{}", stem, format));

            let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
            cmd.arg("-y").arg("-i").arg(inp);
            if max_width > 0 {
                cmd.arg("-vf").arg(format!("scale='min({},iw)':-2", max_width));
            }
            for (k, v) in image_format_ffmpeg_args(&format, quality) {
                cmd.arg(k).arg(v);
            }
            cmd.arg(&out);
            #[cfg(windows)]
            cmd.creation_flags(0x08000000);

            let res = cmd.output().await?;
            if res.status.success() && out.exists() {
                ok += 1;
            } else {
                failed.push(name);
                let stderr = String::from_utf8_lossy(&res.stderr);
                last_err = stderr
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .to_string();
            }
        }
        on_progress(total, total);
        if ok == 0 {
            return Err(format!("nenhuma imagem convertida: {}", last_err).into());
        }
        Ok((out_dir, failed))
    }

    pub async fn verify_integrity(&self, file: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-v")
            .arg("error")
            .arg("-i")
            .arg(file)
            .arg("-f")
            .arg("null")
            .arg("-");
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let output = cmd.output().await?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() && stderr.trim().is_empty() {
            Ok(())
        } else {
            let last = stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("arquivo possivelmente corrompido");
            Err(last.trim().to_string().into())
        }
    }

    pub async fn watermark_video(
        &self,
        input: &str,
        watermark: &str,
        output: &str,
        position: &str,
        scale_pct: u32,
        opacity: f32,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let out = PathBuf::from(output);
        let margin = 16;
        let overlay_pos = match position {
            "tl" => format!("{m}:{m}", m = margin),
            "tr" => format!("W-w-{m}:{m}", m = margin),
            "bl" => format!("{m}:H-h-{m}", m = margin),
            "center" => "(W-w)/2:(H-h)/2".to_string(),
            _ => format!("W-w-{m}:H-h-{m}", m = margin),
        };
        let scale = (scale_pct.clamp(5, 400) as f32) / 100.0;
        let opacity = opacity.clamp(0.0, 1.0);
        let filter = format!(
            "[1:v]format=rgba,colorchannelmixer=aa={op},scale=iw*{sc}:-1[wm];\
             [0:v][wm]overlay={pos}",
            op = opacity,
            sc = scale,
            pos = overlay_pos
        );

        let total = self.probe_duration_secs(Path::new(input)).await;
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(input)
            .arg("-i")
            .arg(watermark)
            .arg("-filter_complex")
            .arg(&filter)
            .arg("-c:a")
            .arg("copy")
            .arg(&out);

        let nop = |_: Progress| {};
        self.run_ffmpeg_progress(cmd, total, Some(&nop))
            .await
            .map_err(|e| format!("ffmpeg falhou ao aplicar marca d'água: {}", e))?;
        Ok(out)
    }

    pub async fn watermark_preview(
        &self,
        video: &str,
        watermark: &str,
        out: &Path,
        position: &str,
        scale_pct: u32,
        opacity: f32,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let out = out.to_path_buf();
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let margin = 16;
        let overlay_pos = match position {
            "tl" => format!("{m}:{m}", m = margin),
            "tr" => format!("W-w-{m}:{m}", m = margin),
            "bl" => format!("{m}:H-h-{m}", m = margin),
            "center" => "(W-w)/2:(H-h)/2".to_string(),
            _ => format!("W-w-{m}:H-h-{m}", m = margin),
        };
        let scale = (scale_pct.clamp(5, 400) as f32) / 100.0;
        let opacity = opacity.clamp(0.0, 1.0);
        let filter = format!(
            "[0:v]thumbnail[b];\
             [1:v]format=rgba,colorchannelmixer=aa={op},scale=iw*{sc}:-1[wm];\
             [b][wm]overlay={pos},scale='min(640,iw)':-1[o]",
            op = opacity,
            sc = scale,
            pos = overlay_pos
        );

        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(video)
            .arg("-i")
            .arg(watermark)
            .arg("-filter_complex")
            .arg(&filter)
            .arg("-map")
            .arg("[o]")
            .arg("-frames:v")
            .arg("1")
            .arg(&out);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let output = cmd.output().await?;
        if !output.status.success() || !out.exists() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            return Err(format!("falha ao gerar pré-visualização: {}", last).into());
        }
        Ok(out)
    }

    pub(super) async fn transcode_media<F>(
        &self,
        input: &Path,
        output: &Path,
        preset: &str,
        on_progress: Option<&F>,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: Fn(Progress) + Send + Sync,
    {
        let total = self.probe_duration_secs(input).await;
        let mut cmd = tokio::process::Command::new(&self.ffmpeg_path);

        let is_gif = output
            .extension()
            .map(|e| e.eq_ignore_ascii_case("gif"))
            .unwrap_or(false);
        if is_gif {
            cmd.arg("-y")
                .arg("-i")
                .arg(input)
                .arg("-vf")
                .arg("fps=12,scale=480:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse")
                .arg(output);
            return self
                .run_ffmpeg_progress(cmd, total, on_progress)
                .await
                .map_err(|e| format!("ffmpeg falhou ao gerar GIF: {}", e).into());
        }

        cmd.arg("-y").arg("-i").arg(input).arg("-map_metadata").arg("0");

        match preset {
            "compress" => {
                cmd.arg("-c:v").arg("libx264").arg("-crf").arg("28")
                    .arg("-preset").arg("medium").arg("-c:a").arg("aac").arg("-b:a").arg("128k");
            }
            "1080" => {
                cmd.arg("-vf").arg("scale=-2:1080").arg("-c:v").arg("libx264")
                    .arg("-crf").arg("22").arg("-c:a").arg("aac").arg("-b:a").arg("160k");
            }
            "720" => {
                cmd.arg("-vf").arg("scale=-2:720").arg("-c:v").arg("libx264")
                    .arg("-crf").arg("23").arg("-c:a").arg("aac").arg("-b:a").arg("160k");
            }
            "480" => {
                cmd.arg("-vf").arg("scale=-2:480").arg("-c:v").arg("libx264")
                    .arg("-crf").arg("24").arg("-c:a").arg("aac").arg("-b:a").arg("128k");
            }
            p if p.starts_with("manual:") => {
                let parts: Vec<&str> = p.splitn(5, ':').collect();
                let g = |i: usize| parts.get(i).map(|s| s.trim()).filter(|s| !s.is_empty());
                cmd.arg("-c:v").arg("libx264");
                if let Some(h) = g(1) {
                    cmd.arg("-vf").arg(format!("scale=-2:{}", h));
                }
                if let Some(fps) = g(2) {
                    cmd.arg("-r").arg(fps);
                }
                match g(3) {
                    Some(vb) => {
                        cmd.arg("-b:v").arg(vb);
                    }
                    None => {
                        cmd.arg("-crf").arg("23");
                    }
                }
                cmd.arg("-c:a").arg("aac").arg("-b:a").arg(g(4).unwrap_or("160k"));
            }
            _ => {}
        }
        cmd.arg(output);

        self.run_ffmpeg_progress(cmd, total, on_progress)
            .await
            .map_err(|e| format!("ffmpeg falhou ao converter: {}", e).into())
    }
}

/// Args de qualidade do ffmpeg por formato de imagem.
/// PNG é sem perdas — não recebe parâmetro de qualidade.
pub fn image_format_ffmpeg_args(format: &str, quality: u32) -> Vec<(String, String)> {
    match format {
        "jpg" | "jpeg" => {
            let qv = 2 + ((100u32.saturating_sub(quality.min(100))) * 29 / 100);
            vec![("-q:v".into(), qv.to_string())]
        }
        "webp" => vec![("-quality".into(), quality.min(100).to_string())],
        "png" => vec![("-compression_level".into(), "9".into())],
        _ => Vec::new(),
    }
}

/// Resumo de falhas de lote de imagens (texto honesto para a UI).
pub fn image_batch_summary(ok: usize, failed: usize, pt: bool) -> String {
    if failed == 0 {
        if pt {
            format!("{} de {} convertidas", ok, ok)
        } else {
            format!("{} of {} converted", ok, ok)
        }
    } else if ok == 0 {
        if pt {
            format!("Nenhuma convertida — {} falharam", failed)
        } else {
            format!("None converted — {} failed", failed)
        }
    } else {
        let total = ok + failed;
        if pt {
            format!("{} de {} convertidas — {} falharam", ok, total, failed)
        } else {
            format!("{} of {} converted — {} failed", ok, total, failed)
        }
    }
}

fn profile_temp_output(output: &Path) -> PathBuf {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let stem = output
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    let ext = output.extension().and_then(|value| value.to_str()).unwrap_or("mkv");
    parent.join(format!("{}.lumen-transcoding.{}", stem, ext))
}

fn video_profile_ffmpeg_args(profile: &super::VideoProfile) -> Vec<&'static str> {
    match profile.extension {
        "mp4" => vec![
            "-c:v", profile.video_encoder, "-crf", "23", "-preset", "medium", "-pix_fmt",
            "yuv420p", "-c:a", profile.audio_encoder, "-b:a", "192k", "-movflags", "+faststart",
        ],
        "mkv" => vec![
            "-c:v", profile.video_encoder, "-crf", "30", "-b:v", "0", "-cpu-used", "6", "-c:a",
            profile.audio_encoder,
        ],
        "webm" => vec![
            "-c:v", profile.video_encoder, "-crf", "31", "-b:v", "0", "-row-mt", "1", "-c:a",
            profile.audio_encoder, "-b:a", "160k",
        ],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_profiles_have_the_promised_ffmpeg_encoders() {
        let mp4 = video_profile_ffmpeg_args(super::super::video_profile("mp4").unwrap());
        assert!(mp4.windows(2).any(|args| args == ["-c:v", "libx264"]));
        assert!(mp4.windows(2).any(|args| args == ["-c:a", "aac"]));

        let mkv = video_profile_ffmpeg_args(super::super::video_profile("mkv").unwrap());
        assert!(mkv.windows(2).any(|args| args == ["-c:v", "libaom-av1"]));
        assert!(mkv.windows(2).any(|args| args == ["-c:a", "flac"]));

        let webm = video_profile_ffmpeg_args(super::super::video_profile("webm").unwrap());
        assert!(webm.windows(2).any(|args| args == ["-c:v", "libvpx-vp9"]));
        assert!(webm.windows(2).any(|args| args == ["-c:a", "libopus"]));
    }

    #[test]
    fn parse_ffmpeg_duration_hh_mm_ss() {
        assert_eq!(
            parse_ffmpeg_duration("  Duration: 00:01:23.45, start: 0.0, bitrate: 1000 kb/s"),
            Some(83.45)
        );
        assert_eq!(
            parse_ffmpeg_duration("Duration: 01:00:00.00, start: 0.000000"),
            Some(3600.0)
        );
    }

    #[test]
    fn parse_ffmpeg_duration_missing() {
        assert_eq!(parse_ffmpeg_duration("Stream #0:0: Video"), None);
        assert_eq!(parse_ffmpeg_duration(""), None);
    }

    #[test]
    fn parse_out_time_us_to_seconds() {
        assert_eq!(parse_out_time_us("out_time_us=1500000"), Some(1.5));
        assert_eq!(parse_out_time_us("out_time_us=0"), Some(0.0));
        assert_eq!(parse_out_time_us("progress=continue"), None);
    }

    #[test]
    fn progress_fraction_clamps_above_one() {
        assert_eq!(progress_fraction(150.0, 100.0), 1.0);
        assert!((progress_fraction(50.0, 100.0) - 0.5).abs() < 1e-9);
        assert_eq!(progress_fraction(10.0, 0.0), 0.0);
    }

    #[test]
    fn image_format_args_quality_per_format() {
        let jpg = image_format_ffmpeg_args("jpg", 90);
        assert!(jpg.iter().any(|(k, _)| k == "-q:v"));
        let webp = image_format_ffmpeg_args("webp", 80);
        assert!(webp.iter().any(|(k, v)| k == "-quality" && v == "80"));
        let png = image_format_ffmpeg_args("png", 50);
        assert!(!png.iter().any(|(k, _)| k == "-q:v" || k == "-quality"));
        assert!(png.iter().any(|(k, _)| k == "-compression_level"));
    }

    #[test]
    fn image_batch_summary_three_cases() {
        assert!(image_batch_summary(12, 0, true).contains("12 de 12"));
        assert!(image_batch_summary(8, 4, true).contains("8 de 12"));
        assert!(image_batch_summary(8, 4, true).contains("4 falharam"));
        assert!(image_batch_summary(0, 5, true).contains("Nenhuma"));
        assert!(image_batch_summary(3, 0, false).contains("3 of 3"));
    }
}
