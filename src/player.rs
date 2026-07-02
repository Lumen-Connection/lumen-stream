use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Mini-player de áudio para pré-ouvir arquivos baixados.
/// Mantém o stream do dispositivo vivo enquanto tocar. Não é `Send`
/// (roda apenas na thread da UI, como o resto do egui).
pub struct MiniPlayer {
    stream: Option<(rodio::OutputStream, rodio::OutputStreamHandle)>,
    sink: Option<rodio::Sink>,
    pub path: Option<PathBuf>,
    pub title: String,
    pub duration: Option<Duration>,
    pub volume: f32,
    started: Option<Instant>,
    elapsed_before: Duration,
    pub playing: bool,
    pub error: Option<String>,
}

impl Default for MiniPlayer {
    fn default() -> Self {
        Self {
            stream: None,
            sink: None,
            path: None,
            title: String::new(),
            duration: None,
            volume: 0.8,
            started: None,
            elapsed_before: Duration::ZERO,
            playing: false,
            error: None,
        }
    }
}

fn read_duration(path: &Path) -> Option<Duration> {
    use lofty::file::AudioFile;
    let d = lofty::read_from_path(path).ok()?.properties().duration();
    if d.is_zero() {
        None
    } else {
        Some(d)
    }
}

impl MiniPlayer {
    pub fn is_active(&self) -> bool {
        self.path.is_some()
    }

    fn ensure_stream(&mut self) -> bool {
        if self.stream.is_none() {
            match rodio::OutputStream::try_default() {
                Ok(s) => self.stream = Some(s),
                Err(e) => {
                    self.error = Some(e.to_string());
                    return false;
                }
            }
        }
        true
    }

    /// Toca um arquivo de áudio do começo.
    pub fn play(&mut self, path: PathBuf) {
        self.error = None;
        if !self.ensure_stream() {
            return;
        }
        let handle = match &self.stream {
            Some((_, h)) => h,
            None => return,
        };
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let source = match rodio::Decoder::new(std::io::BufReader::new(file)) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(format!("formato não suportado: {}", e));
                return;
            }
        };
        let sink = match rodio::Sink::try_new(handle) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        sink.set_volume(self.volume);
        sink.append(source);
        sink.play();

        self.title = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        self.duration = read_duration(&path);
        self.path = Some(path);
        self.sink = Some(sink);
        self.started = Some(Instant::now());
        self.elapsed_before = Duration::ZERO;
        self.playing = true;
    }

    pub fn toggle(&mut self) {
        let Some(sink) = &self.sink else { return };
        if self.playing {
            sink.pause();
            self.elapsed_before = self.elapsed();
            self.started = None;
            self.playing = false;
        } else {
            sink.play();
            self.started = Some(Instant::now());
            self.playing = true;
        }
    }

    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.path = None;
        self.title.clear();
        self.duration = None;
        self.started = None;
        self.elapsed_before = Duration::ZERO;
        self.playing = false;
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
        }
    }

    pub fn elapsed(&self) -> Duration {
        match self.started {
            Some(t) if self.playing => self.elapsed_before + t.elapsed(),
            _ => self.elapsed_before,
        }
    }

    /// Deve ser chamado por frame: detecta o fim da faixa e limpa o estado.
    pub fn poll_finished(&mut self) {
        if self.playing {
            if let Some(sink) = &self.sink {
                if sink.empty() {
                    self.stop();
                }
            }
        }
    }
}

/// Extensões que o mini-player consegue reproduzir (áudio).
pub fn is_playable_audio(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "mp3" | "m4a" | "aac" | "flac" | "wav" | "ogg" | "opus" | "oga" | "aiff" | "alac"
    )
}
