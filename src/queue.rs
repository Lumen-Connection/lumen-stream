
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;

use crate::app::MediaType;
use crate::db::database::Database;
use crate::download::engine::DownloadEngine;

#[derive(Clone, PartialEq)]
pub enum JobStatus {
    Queued,
    Running,
    Paused,
    Completed(String),
    Failed(String),
    Cancelled,
}

pub struct QueueJob {
    pub id: u64,
    pub url: String,
    pub title: String,
    pub media_type: MediaType,
    pub format: String,
    pub quality: String,
    pub folder: PathBuf,
    pub status: JobStatus,
    pub progress: Option<f32>,
    pub retries: u32,
    pub speed: f32,
    pub eta: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedJob {
    url: String,
    title: String,
    media_type: MediaType,
    format: String,
    quality: String,
    folder: PathBuf,
}

type Jobs = Arc<Mutex<Vec<QueueJob>>>;

pub struct Queue {
    pub jobs: Jobs,
    pub next_id: Arc<AtomicU64>,
    pub max_concurrent: usize,
    handles: HashMap<u64, JoinHandle<()>>,
}

impl Queue {
    pub fn new() -> Self {
        Queue {
            jobs: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(AtomicU64::new(1)),
            max_concurrent: 3,
            handles: HashMap::new(),
        }
    }

    pub fn signature(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for j in self.jobs.lock().unwrap().iter() {
            j.id.hash(&mut h);
            std::mem::discriminant(&j.status).hash(&mut h);
        }
        h.finish()
    }

    pub fn save(&self, path: &std::path::Path) {
        let saved: Vec<SavedJob> = self
            .jobs
            .lock()
            .unwrap()
            .iter()
            .filter(|j| {
                !matches!(j.status, JobStatus::Completed(_) | JobStatus::Cancelled)
            })
            .map(|j| SavedJob {
                url: j.url.clone(),
                title: j.title.clone(),
                media_type: j.media_type,
                format: j.format.clone(),
                quality: j.quality.clone(),
                folder: j.folder.clone(),
            })
            .collect();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if saved.is_empty() {
            let _ = std::fs::remove_file(path);
        } else if let Ok(json) = serde_json::to_string_pretty(&saved) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn load(&mut self, path: &std::path::Path) {
        let Ok(content) = std::fs::read_to_string(path) else {
            return;
        };
        let Ok(saved) = serde_json::from_str::<Vec<SavedJob>>(&content) else {
            return;
        };
        for j in saved {
            push_job(
                &self.jobs,
                &self.next_id,
                j.url,
                j.title,
                j.media_type,
                j.format,
                j.quality,
                j.folder,
            );
        }
    }

    pub fn add(
        &self,
        url: String,
        title: String,
        media_type: MediaType,
        format: String,
        quality: String,
        folder: PathBuf,
    ) {
        push_job(
            &self.jobs, &self.next_id, url, title, media_type, format, quality, folder,
        );
    }

    pub fn move_to_top(&self, id: u64) {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(i) = jobs.iter().position(|j| j.id == id) {
            let job = jobs.remove(i);
            jobs.insert(0, job);
        }
    }

    pub fn move_up(&self, id: u64) {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(i) = jobs.iter().position(|j| j.id == id) {
            if i > 0 {
                jobs.swap(i, i - 1);
            }
        }
    }

    pub fn move_down(&self, id: u64) {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(i) = jobs.iter().position(|j| j.id == id) {
            if i + 1 < jobs.len() {
                jobs.swap(i, i + 1);
            }
        }
    }

    pub fn has_active(&self) -> bool {
        self.jobs
            .lock()
            .unwrap()
            .iter()
            .any(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running))
    }

    pub fn pause(&mut self, id: u64) {
        if let Some(handle) = self.handles.remove(&id) {
            handle.abort();
        }
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            if matches!(job.status, JobStatus::Running | JobStatus::Queued) {
                job.status = JobStatus::Paused;
                job.progress = None;
            }
        }
    }

    pub fn resume(&mut self, id: u64) {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            if job.status == JobStatus::Paused {
                job.status = JobStatus::Queued;
            }
        }
    }

    pub fn cancel(&mut self, id: u64) {
        if let Some(handle) = self.handles.remove(&id) {
            handle.abort();
        }
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            if matches!(job.status, JobStatus::Queued | JobStatus::Running) {
                job.status = JobStatus::Cancelled;
            }
        }
    }

    pub fn clear_finished(&mut self) {
        self.jobs
            .lock()
            .unwrap()
            .retain(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running));
        self.handles.retain(|_, h| !h.is_finished());
    }

    pub fn pump(
        &mut self,
        engine: Arc<DownloadEngine>,
        db_path: PathBuf,
        subtitle_langs: Option<String>,
        notify: bool,
        rate_limit: Option<String>,
        concurrent_fragments: u32,
        organize_by: String,
        cloud_folder: Option<String>,
        auto_retry: bool,
    ) {
        let running = self
            .jobs
            .lock()
            .unwrap()
            .iter()
            .filter(|j| j.status == JobStatus::Running)
            .count();
        let mut slots = self.max_concurrent.saturating_sub(running);
        if slots == 0 {
            return;
        }

        let mut to_start = Vec::new();
        {
            let mut jobs = self.jobs.lock().unwrap();
            for job in jobs.iter_mut() {
                if slots == 0 {
                    break;
                }
                if job.status == JobStatus::Queued {
                    job.status = JobStatus::Running;
                    to_start.push(job.id);
                    slots -= 1;
                }
            }
        }

        for id in to_start {
            let snapshot = {
                let jobs = self.jobs.lock().unwrap();
                jobs.iter().find(|j| j.id == id).map(|j| {
                    (
                        j.url.clone(),
                        j.media_type,
                        j.format.clone(),
                        j.quality.clone(),
                        j.folder.clone(),
                    )
                })
            };
            let Some((url, media_type, format, quality, folder)) = snapshot else {
                continue;
            };

            let jobs = self.jobs.clone();
            let engine = engine.clone();
            let db_path = db_path.clone();
            let subtitle_langs = subtitle_langs.clone();
            let rate_limit = rate_limit.clone();
            let organize_by = organize_by.clone();
            let cloud_folder = cloud_folder.clone();

            let handle = tokio::spawn(async move {
                let title = match engine.fetch_info(&url).await {
                    Ok(t) => t,
                    Err(e) => {
                        set_status(
                            &jobs,
                            id,
                            JobStatus::Failed(format!("Falha ao obter info: {}", e)),
                        );
                        return;
                    }
                };
                set_title(&jobs, id, title.clone());

                let is_music = media_type == MediaType::Music;
                let mut folder = folder;
                let media_str = if is_music { "music" } else { "video" };
                if let Some(sub) =
                    crate::download::engine::organize_subfolder(&organize_by, media_str, "")
                {
                    folder = folder.join(sub);
                    let _ = std::fs::create_dir_all(&folder);
                }
                let safe = crate::download::engine::sanitize_filename(&title);
                let out = folder.join(format!("{}.{}", safe, format));
                let out_str = out.to_string_lossy().to_string();

                let jobs_cb = jobs.clone();
                let on_progress = move |pr: crate::download::engine::Progress| {
                    set_progress(&jobs_cb, id, pr.fraction as f32, pr.speed_bps as f32, pr.eta_secs);
                };

                let subs = if is_music { None } else { subtitle_langs };
                let opts = crate::download::engine::DownloadOptions {
                    is_audio: is_music,
                    format: format.clone(),
                    quality: quality.clone(),
                    max_height: None,
                    subtitle_langs: subs,
                    clip: None,
                    rate_limit,
                    concurrent_fragments,
                    live_from_start: false,
                    is_live: false,
                    stop: None,
                };
                match engine
                    .fetch_and_download(&url, &out_str, opts, on_progress)
                    .await
                {
                    Ok(p) => {
                        if let Some(cloud) = &cloud_folder {
                            if let Some(name) = p.file_name() {
                                let dest = std::path::Path::new(cloud).join(name);
                                let _ = std::fs::create_dir_all(cloud);
                                let _ = std::fs::copy(&p, &dest);
                            }
                        }
                        let db = Database::open(&db_path);
                        let file_size = std::fs::metadata(&p).ok().map(|m| m.len() as i64);
                        db.add_history(
                            &url,
                            &title,
                            if is_music { "music" } else { "video" },
                            &format,
                            &quality,
                            &folder.to_string_lossy(),
                            &p.to_string_lossy(),
                            file_size,
                        );
                        set_status(&jobs, id, JobStatus::Completed(p.to_string_lossy().to_string()));
                        if notify {
                            crate::notify::send("Download concluído", &title);
                        }
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        let network = is_network_error(&msg);
                        let mut jl = jobs.lock().unwrap();
                        if let Some(j) = jl.iter_mut().find(|j| j.id == id) {
                            if auto_retry && network && j.retries < 2 {
                                j.retries += 1;
                                j.progress = None;
                                j.status = JobStatus::Queued;
                            } else {
                                j.status = JobStatus::Failed(msg);
                            }
                        }
                    }
                }
            });
            self.handles.insert(id, handle);
        }
    }
}

pub fn push_job(
    jobs: &Jobs,
    next_id: &Arc<AtomicU64>,
    url: String,
    title: String,
    media_type: MediaType,
    format: String,
    quality: String,
    folder: PathBuf,
) {
    let id = next_id.fetch_add(1, Ordering::SeqCst);
    jobs.lock().unwrap().push(QueueJob {
        id,
        url,
        title,
        media_type,
        format,
        quality,
        folder,
        status: JobStatus::Queued,
        progress: None,
        retries: 0,
        speed: 0.0,
        eta: 0,
    });
}

fn set_status(jobs: &Jobs, id: u64, status: JobStatus) {
    if let Some(job) = jobs.lock().unwrap().iter_mut().find(|j| j.id == id) {
        job.status = status;
    }
}

fn is_network_error(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("network")
        || m.contains("timed out")
        || m.contains("timeout")
        || m.contains("connection")
        || m.contains("getaddrinfo")
        || m.contains("temporary failure")
        || m.contains("unable to download")
        || m.contains("http error 5")
        || m.contains("read error")
}
fn set_title(jobs: &Jobs, id: u64, title: String) {
    if let Some(job) = jobs.lock().unwrap().iter_mut().find(|j| j.id == id) {
        job.title = title;
    }
}
fn set_progress(jobs: &Jobs, id: u64, p: f32, speed: f32, eta: u64) {
    if let Some(job) = jobs.lock().unwrap().iter_mut().find(|j| j.id == id) {
        job.progress = Some(p.clamp(0.0, 1.0));
        job.speed = speed;
        job.eta = eta;
    }
}

pub fn playlist_id_from_url(url: &str) -> Option<String> {
    url.split(['?', '&'])
        .find_map(|kv| kv.strip_prefix("list="))
        .map(|s| s.to_string())
}

pub fn is_playlist(url: &str) -> bool {
    playlist_id_from_url(url).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::MediaType;

    fn add(q: &Queue, url: &str) {
        q.add(
            url.to_string(),
            url.to_string(),
            MediaType::Music,
            "mp3".to_string(),
            "best".to_string(),
            PathBuf::from("C:/out"),
        );
    }

    fn ids(q: &Queue) -> Vec<u64> {
        q.jobs.lock().unwrap().iter().map(|j| j.id).collect()
    }

    fn status_of(q: &Queue, id: u64) -> JobStatus {
        q.jobs.lock().unwrap().iter().find(|j| j.id == id).unwrap().status.clone()
    }

    fn set(q: &Queue, id: u64, st: JobStatus) {
        q.jobs.lock().unwrap().iter_mut().find(|j| j.id == id).unwrap().status = st;
    }

    fn temp_file(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("lumen_queue_test_{tag}_{nanos}.json"))
    }

    #[test]
    fn add_assigns_sequential_ids_and_queued_status() {
        let q = Queue::new();
        add(&q, "u1");
        add(&q, "u2");
        assert_eq!(ids(&q), vec![1, 2]);
        assert!(status_of(&q, 1) == JobStatus::Queued);
        assert!(q.has_active());
    }

    #[test]
    fn move_operations_reorder_jobs() {
        let q = Queue::new();
        add(&q, "a");
        add(&q, "b");
        add(&q, "c");
        q.move_to_top(3);
        assert_eq!(ids(&q), vec![3, 1, 2]);
        q.move_down(3);
        assert_eq!(ids(&q), vec![1, 3, 2]);
        q.move_up(2);
        assert_eq!(ids(&q), vec![1, 2, 3]);
        // Extremos são no-op.
        q.move_up(1);
        q.move_down(3);
        assert_eq!(ids(&q), vec![1, 2, 3]);
    }

    #[test]
    fn pause_resume_cancel_transitions() {
        let mut q = Queue::new();
        add(&q, "a");
        q.pause(1);
        assert!(status_of(&q, 1) == JobStatus::Paused);
        q.resume(1);
        assert!(status_of(&q, 1) == JobStatus::Queued);
        q.cancel(1);
        assert!(status_of(&q, 1) == JobStatus::Cancelled);
        // Cancelado não volta com resume nem vira pausado.
        q.resume(1);
        q.pause(1);
        assert!(status_of(&q, 1) == JobStatus::Cancelled);
        assert!(!q.has_active());
    }

    #[test]
    fn pause_does_not_touch_completed() {
        let mut q = Queue::new();
        add(&q, "a");
        set(&q, 1, JobStatus::Completed("f".into()));
        q.pause(1);
        assert!(matches!(status_of(&q, 1), JobStatus::Completed(_)));
    }

    #[test]
    fn clear_finished_keeps_only_active() {
        let mut q = Queue::new();
        for u in ["a", "b", "c", "d"] {
            add(&q, u);
        }
        set(&q, 2, JobStatus::Completed("f".into()));
        set(&q, 3, JobStatus::Failed("e".into()));
        set(&q, 4, JobStatus::Running);
        q.clear_finished();
        assert_eq!(ids(&q), vec![1, 4]);
    }

    #[test]
    fn signature_changes_with_status() {
        let q = Queue::new();
        add(&q, "a");
        let s1 = q.signature();
        set(&q, 1, JobStatus::Running);
        assert_ne!(q.signature(), s1, "mudança de status deve mudar a assinatura");
    }

    #[test]
    fn save_load_roundtrip_skips_finished() {
        let q = Queue::new();
        for u in ["fica1", "some1", "some2", "fica2"] {
            add(&q, u);
        }
        set(&q, 2, JobStatus::Completed("f".into()));
        set(&q, 3, JobStatus::Cancelled);
        set(&q, 4, JobStatus::Paused); // pendente: deve ser salvo

        let path = temp_file("roundtrip");
        q.save(&path);

        let mut q2 = Queue::new();
        q2.load(&path);
        let urls: Vec<String> = q2.jobs.lock().unwrap().iter().map(|j| j.url.clone()).collect();
        assert_eq!(urls, vec!["fica1", "fica2"]);
        // Tudo volta como Queued, pronto para o pump.
        assert!(q2.jobs.lock().unwrap().iter().all(|j| j.status == JobStatus::Queued));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_removes_file_when_nothing_pending() {
        let q = Queue::new();
        add(&q, "a");
        set(&q, 1, JobStatus::Completed("f".into()));
        let path = temp_file("empty");
        std::fs::write(&path, "[]").unwrap();
        q.save(&path);
        assert!(!path.exists(), "fila sem pendências apaga o arquivo salvo");
    }

    #[test]
    fn load_ignores_missing_or_corrupt_file() {
        let mut q = Queue::new();
        q.load(&temp_file("inexistente"));
        assert!(ids(&q).is_empty());
        let path = temp_file("corrupt");
        std::fs::write(&path, "{nao é json válido").unwrap();
        q.load(&path);
        assert!(ids(&q).is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn progress_is_clamped() {
        let q = Queue::new();
        add(&q, "a");
        set_progress(&q.jobs, 1, 1.5, 10.0, 3);
        let jobs = q.jobs.lock().unwrap();
        assert_eq!(jobs[0].progress, Some(1.0));
        assert_eq!(jobs[0].speed, 10.0);
        assert_eq!(jobs[0].eta, 3);
    }

    #[test]
    fn network_errors_are_detected_for_auto_retry() {
        for msg in [
            "Connection reset by peer",
            "request timed out",
            "getaddrinfo failed",
            "HTTP Error 503",
            "unable to download video data",
        ] {
            assert!(is_network_error(msg), "{msg} deveria ser erro de rede");
        }
        assert!(!is_network_error("Video unavailable"));
        assert!(!is_network_error("requested format is not available"));
    }

    #[test]
    fn playlist_detection_from_url() {
        assert_eq!(
            playlist_id_from_url("https://youtube.com/watch?v=abc&list=PL123"),
            Some("PL123".to_string())
        );
        assert_eq!(
            playlist_id_from_url("https://youtube.com/playlist?list=PL9&index=2"),
            Some("PL9".to_string())
        );
        assert_eq!(playlist_id_from_url("https://youtube.com/watch?v=abc"), None);
        assert!(is_playlist("https://youtube.com/watch?v=a&list=x"));
        assert!(!is_playlist("https://youtube.com/watch?v=a"));
    }
}
