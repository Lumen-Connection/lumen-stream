use std::path::PathBuf;

use crate::config::settings::ConvertEngine;

use super::models::{categorize, is_audio_format, AudioMeta, FileCategory, Progress};
use super::DownloadEngine;

impl DownloadEngine {
    pub async fn convert_file<F>(
        &self,
        input: &str,
        output_path: &str,
        format: &str,
        preset: &str,
        engine: ConvertEngine,
        on_progress: F,
    ) -> Result<PathBuf, Box<dyn std::error::Error>>
    where
        F: Fn(Progress) + Send + Sync,
    {
        let input_path = PathBuf::from(input);
        let mut out = PathBuf::from(output_path);
        out.set_extension(format);

        if out == input_path {
            let stem = out
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "convertido".to_string());
            out.set_file_name(format!("{}_convertido.{}", stem, format));
        }

        // PDF / Office / Markdown / imagem→PDF não têm base temporal (out_time):
        // a UI mantém barra indeterminada. Só mídia A/V emite progresso real.
        match categorize(&input_path) {
            FileCategory::Document => {
                if format == "md" {
                    return self.convert_to_markdown(&input_path, &out).await;
                }
                if format == "txt" {
                    return self.pdf_to_text(&input_path, &out).await;
                }
                return self.pdf_to_images(&input_path, &out, format).await;
            }
            FileCategory::Office => {
                return self.office_convert(&input_path, &out, format, engine).await;
            }
            FileCategory::Markdown => {
                return self.convert_from_markdown(&input_path, &out, format).await;
            }
            _ => {}
        }

        if format == "pdf" {
            return self.image_to_pdf(&input_path, &out).await;
        }

        if is_audio_format(format) {
            self.transcode_audio(
                &input_path,
                &out,
                format,
                &AudioMeta::default(),
                Some(&on_progress),
            )
            .await?;
        } else {
            self.transcode_media(&input_path, &out, preset, Some(&on_progress))
                .await?;
        }
        Ok(out)
    }
}
