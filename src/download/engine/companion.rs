//! Version-pinned local Cobalt service. Never uses an external API instance.
use super::routing::{DownloadFailure, FailureKind};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncWriteExt, process::Command};

type Result<T> = std::result::Result<T, DownloadFailure>;
fn dependency(message: impl Into<String>) -> DownloadFailure {
    DownloadFailure::new(FailureKind::Dependency, message)
}
#[derive(Deserialize)]
struct Manifest {
    version: String,
    commit: String,
    sha256: String,
    url: String,
}
const MANIFEST: &str = include_str!("../../../assets/cobalt/manifest.json");

#[derive(Clone)]
pub(super) struct Endpoint {
    pub url: String,
    pub key: String,
}
struct Running {
    child: tokio::process::Child,
    endpoint: Endpoint,
    _secret: tempfile::TempDir,
    _tree: ProcessTree,
}
impl Drop for Running {
    fn drop(&mut self) {
        #[cfg(not(windows))]
        if let Some(pid) = self.child.id() {
            super::kill_tree(pid);
        }
        let _ = self.child.start_kill();
    }
}
#[derive(Default)]
pub(super) struct Companion {
    #[cfg(test)]
    pub test_endpoint: Option<Endpoint>,
    running: Option<Running>,
    restarts: u8,
    pub status: String,
}
impl Companion {
    #[cfg(test)]
    pub fn fixture(endpoint: Endpoint) -> Self {
        Self {
            test_endpoint: Some(endpoint),
            ..Default::default()
        }
    }

    pub async fn endpoint(&mut self, libs: &Path) -> Result<Endpoint> {
        #[cfg(test)]
        if let Some(endpoint) = &self.test_endpoint {
            return Ok(endpoint.clone());
        }
        if self.running.is_none() && self.restarts >= 2 {
            return Err(dependency(
                "Cobalt recovery failed. Repair Cobalt in Settings.",
            ));
        }
        if let Some(run) = &mut self.running {
            if matches!(run.child.try_wait(), Ok(None)) {
                return Ok(run.endpoint.clone());
            }
            self.running = None;
            if self.restarts >= 1 {
                self.restarts = 2;
                return Err(dependency(
                    "Cobalt stopped repeatedly. Repair Cobalt in Settings.",
                ));
            }
            self.restarts += 1;
        }
        self.status = "installing / instalando".into();
        let result = Self::start(libs).await;
        match result {
            Ok(run) => {
                let endpoint = run.endpoint.clone();
                self.running = Some(run);
                self.status = "running / em execução".into();
                Ok(endpoint)
            }
            Err(e) => {
                if self.restarts > 0 {
                    self.restarts = 2;
                }
                self.status = e.to_string();
                Err(e)
            }
        }
    }
    pub fn display_status(&self, libs: &Path) -> String {
        if !self.status.is_empty() {
            return self.status.clone();
        }
        let Ok(manifest) = serde_json::from_str::<Manifest>(MANIFEST) else {
            return "invalid manifest / manifesto inválido".into();
        };
        if manifest.sha256.is_empty() {
            return "development build: not packaged / versão de desenvolvimento: não empacotado"
                .into();
        }
        if libs
            .join("cobalt")
            .join(manifest.version)
            .join("node.exe")
            .is_file()
        {
            "installed, stopped / instalado, parado".into()
        } else {
            "not installed; on demand / não instalado; sob demanda".into()
        }
    }
    pub fn repair(&mut self) {
        self.running = None;
        self.restarts = 0;
        self.status = "stopped / parado".into();
    }
    pub fn invalidate_install(&mut self, libs: &Path) -> Result<()> {
        self.repair();
        let manifest: Manifest =
            serde_json::from_str(MANIFEST).map_err(|_| dependency("Invalid Cobalt manifest"))?;
        if manifest
            .version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        {
            let base = libs.join("cobalt");
            let current = base.join(&manifest.version);
            if current.is_dir() {
                std::fs::rename(
                    &current,
                    base.join(format!(
                        "{}-repair-{}",
                        manifest.version,
                        uuid::Uuid::new_v4()
                    )),
                )
                .map_err(DownloadFailure::io)?;
            }
        }
        Ok(())
    }
    async fn start(libs: &Path) -> Result<Running> {
        if !cfg!(all(windows, target_arch = "x86_64")) {
            return Err(dependency(
                "The bundled Cobalt companion currently requires Windows x64.",
            ));
        }
        let root = install(libs).await?;
        match launch(&root).await {
            Ok(run) => {
                // Reuse the verified companion FFmpeg for Lumen's local finalizer.
                let ffmpeg = super::fs_utils::binary_path(&libs.to_path_buf(), "ffmpeg");
                if !ffmpeg.exists() {
                    let staged =
                        tempfile::NamedTempFile::new_in(libs).map_err(DownloadFailure::io)?;
                    std::fs::copy(
                        root.join("api/node_modules/ffmpeg-static/ffmpeg.exe"),
                        staged.path(),
                    )
                    .map_err(DownloadFailure::io)?;
                    match staged.persist_noclobber(&ffmpeg) {
                        Ok(_) => {}
                        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(e) => return Err(DownloadFailure::io(e.error)),
                    }
                }
                let active = libs.join("cobalt/active.txt");
                if let Ok(old) = std::fs::read_to_string(&active) {
                    if old != root.to_string_lossy() {
                        let _ = std::fs::write(libs.join("cobalt/previous.txt"), old);
                    }
                }
                let _ = std::fs::write(active, root.to_string_lossy().as_bytes());
                Ok(run)
            }
            Err(error) => {
                // The prior installed bundle is retained. Only use a path inside our install root.
                if let Ok(old) = std::fs::read_to_string(libs.join("cobalt/active.txt")) {
                    let old = PathBuf::from(old);
                    if old != root && old.parent() == Some(libs.join("cobalt").as_path()) {
                        return launch(&old).await;
                    }
                }
                Err(error)
            }
        }
    }
}
async fn install(libs: &Path) -> Result<PathBuf> {
    let manifest: Manifest =
        serde_json::from_str(MANIFEST).map_err(|_| dependency("Invalid Cobalt manifest"))?;
    if manifest.sha256.len() != 64
        || !manifest.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || !manifest
            .url
            .starts_with("https://github.com/Lumen-Connection/lumen-stream/releases/download/")
    {
        return Err(dependency(
            "Cobalt companion is not packaged in this development build. Build with scripts/cobalt/package.ps1 or install a packaged Lumen release.",
        ));
    }
    if !manifest
        .version
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        || manifest.commit.len() != 40
    {
        return Err(dependency("Invalid Cobalt version"));
    }
    let base = libs.join("cobalt");
    std::fs::create_dir_all(&base).map_err(DownloadFailure::io)?;
    let target = base.join(&manifest.version);
    if target.join("node.exe").is_file() && target.join("api/src/cobalt.js").is_file() {
        return Ok(target);
    }
    let staging = tempfile::tempdir_in(&base).map_err(DownloadFailure::io)?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|_| dependency("Cannot create companion download client"))?;
    let mut response = client
        .get(&manifest.url)
        .send()
        .await
        .map_err(|_| dependency("Cannot download Cobalt companion"))?
        .error_for_status()
        .map_err(|_| dependency("Cobalt companion archive is unavailable"))?;
    let archive_path = staging.path().join("bundle.zip");
    let mut file = tokio::fs::File::create(&archive_path)
        .await
        .map_err(DownloadFailure::io)?;
    let mut hash = Sha256::new();
    let mut size = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| dependency("Companion download interrupted"))?
    {
        size += chunk.len() as u64;
        if size > 1024 * 1024 * 1024 {
            return Err(dependency("Companion archive exceeds size limit"));
        }
        hash.update(&chunk);
        file.write_all(&chunk).await.map_err(DownloadFailure::io)?;
    }
    file.sync_all().await.map_err(DownloadFailure::io)?;
    drop(file);
    if hash
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        != manifest.sha256.to_lowercase()
    {
        return Err(dependency("Cobalt archive checksum mismatch"));
    }
    let unpacked = staging.path().join("unpacked");
    extract(&archive_path, &unpacked)?;
    if !unpacked.join("node.exe").is_file() || !unpacked.join("api/src/cobalt.js").is_file() {
        return Err(dependency("Incomplete Cobalt archive"));
    }
    // A concurrent installer may have won. Never replace a working install in place.
    if !target.exists() {
        std::fs::rename(&unpacked, &target).map_err(DownloadFailure::io)?;
    }
    Ok(target)
}
fn extract(archive: &Path, destination: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).map_err(DownloadFailure::io)?)
        .map_err(|_| dependency("Invalid companion archive"))?;
    let mut expanded = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|_| dependency("Invalid archive entry"))?;
        let path = entry
            .enclosed_name()
            .ok_or_else(|| dependency("Unsafe archive path"))?;
        if entry.name().contains(':')
            || entry.name().contains('\\')
            || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err(dependency("Unsafe archive entry"));
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > 3 * 1024 * 1024 * 1024 {
            return Err(dependency("Expanded companion exceeds size limit"));
        }
        let out = destination.join(path);
        if entry.is_dir() {
            std::fs::create_dir_all(out).map_err(DownloadFailure::io)?;
        } else {
            std::fs::create_dir_all(out.parent().unwrap()).map_err(DownloadFailure::io)?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(out)
                .map_err(DownloadFailure::io)?;
            std::io::copy(&mut entry, &mut file).map_err(DownloadFailure::io)?;
        }
    }
    Ok(())
}
async fn launch(root: &Path) -> Result<Running> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    for _ in 0..3 {
        let listener =
            std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(DownloadFailure::io)?;
        let port = listener.local_addr().map_err(DownloadFailure::io)?.port();
        let url = format!("http://127.0.0.1:{port}/");
        let key = uuid::Uuid::new_v4().to_string();
        let secret = tempfile::tempdir().map_err(DownloadFailure::io)?;
        let key_path = secret.path().join("keys.json");
        std::fs::write(
            &key_path,
            serde_json::json!({key.clone(): {"limit":"unlimited"}}).to_string(),
        )
        .map_err(DownloadFailure::io)?;
        // Wait for parent to attach the Job Object before importing any Cobalt code.
        let bootstrap = "process.stdin.once('data',async()=>{process.stdin.pause();await import('./src/cobalt.js')})";
        let mut command = Command::new(root.join("node.exe"));
        command
            .args(["--no-node-snapshot", "--input-type=module", "-e", bootstrap])
            .current_dir(root.join("api"))
            .env_clear();
        for name in ["SystemRoot", "WINDIR", "TEMP", "TMP", "PATH"] {
            if let Some(v) = std::env::var_os(name) {
                command.env(name, v);
            }
        }
        command
            .env("API_URL", &url)
            .env("API_PORT", port.to_string())
            .env("API_LISTEN_ADDRESS", "127.0.0.1")
            .env("API_AUTH_REQUIRED", "1")
            .env(
                "API_KEY_URL",
                reqwest::Url::from_file_path(&key_path)
                    .map_err(|_| dependency("Invalid key path"))?
                    .as_str(),
            )
            .env("CORS_WILDCARD", "0")
            .env("CORS_URL", &url)
            .env("FORCE_LOCAL_PROCESSING", "never")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        #[cfg(unix)]
        command.process_group(0);
        drop(listener);
        let child = command
            .spawn()
            .map_err(|_| dependency("Cannot start Cobalt runtime"))?;
        let tree = ProcessTree::attach(&child)?;
        let mut run = Running {
            child,
            endpoint: Endpoint {
                url: url.clone(),
                key,
            },
            _secret: secret,
            _tree: tree,
        };
        run.child
            .stdin
            .take()
            .unwrap()
            .write_all(b"start\n")
            .await
            .map_err(DownloadFailure::io)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(1))
            .build()
            .map_err(|_| dependency("Cannot create local API client"))?;
        while tokio::time::Instant::now() < deadline {
            if !matches!(run.child.try_wait(), Ok(None)) {
                break;
            }
            if let Ok(resp) = client.get(&url).send().await {
                if resp.status().is_success()
                    && resp
                        .json::<serde_json::Value>()
                        .await
                        .ok()
                        .is_some_and(|v| v["cobalt"]["version"].is_string())
                {
                    return Ok(run);
                }
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }
    Err(dependency(
        "Cobalt did not become ready within the startup deadline",
    ))
}

// Handle ownership is local to this guard; it can move with the Tokio task.
#[cfg(windows)]
struct ProcessTree(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for ProcessTree {}
#[cfg(windows)]
impl ProcessTree {
    fn attach(child: &tokio::process::Child) -> Result<Self> {
        use windows_sys::Win32::{Foundation::*, System::JobObjects::*};
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(dependency("Cannot create Cobalt process job"));
            }
            let guard = Self(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.raw_handle().unwrap() as HANDLE) == 0
            {
                return Err(dependency("Cannot contain Cobalt process tree"));
            }
            Ok(guard)
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(not(windows))]
struct ProcessTree;
#[cfg(not(windows))]
impl ProcessTree {
    fn attach(_: &tokio::process::Child) -> Result<Self> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_traversal_archive() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bad.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file("../outside", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();
        assert!(extract(&path, &temp.path().join("out")).is_err());
        assert!(!temp.path().join("outside").exists());
    }
}

#[cfg(all(test, windows))]
mod windows_acceptance {
    use super::*;
    #[tokio::test]
    #[ignore]
    async fn cobalt_gate_real_companion_lifecycle() {
        let root = PathBuf::from(
            std::env::var_os("LUMEN_COBALT_TEST_BUNDLE").expect("package must supply bundle"),
        );
        let mut run = launch(&root).await.unwrap();
        let client = reqwest::Client::new();
        let response = client
            .post(&run.endpoint.url)
            .header("Accept", "application/json")
            .json(&serde_json::json!({"url":"https://streamable.com/lumen1"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        // Hold a process handle and verify Job Object cleanup when the supervisor drops.
        use windows_sys::Win32::Foundation::*;
        use windows_sys::Win32::System::Threading::*;
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, run.child.id().unwrap()) };
        assert!(!handle.is_null());
        run.child.stdin.take();
        drop(run);
        let result = unsafe { WaitForSingleObject(handle, 5000) };
        unsafe {
            CloseHandle(handle);
        }
        assert_eq!(result, WAIT_OBJECT_0);
    }
    #[tokio::test]
    #[ignore]
    async fn cobalt_gate_job_kills_descendants() {
        use windows_sys::Win32::{Foundation::*, System::Threading::*};
        let root =
            PathBuf::from(std::env::var_os("LUMEN_COBALT_TEST_BUNDLE").expect("packaged bundle"));
        let temp = tempfile::tempdir().unwrap();
        let pid_file = temp.path().join("child-pid");
        let mut command = Command::new(root.join("node.exe"));
        command.args(["-e","process.stdin.once('data',()=>{const c=require('child_process').spawn(process.execPath,['-e','setInterval(()=>{},1000)']);require('fs').writeFileSync(process.argv[1],String(c.pid));setInterval(()=>{},1000)})"])
            .arg(&pid_file).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).creation_flags(0x08000000);
        let mut child = command.spawn().unwrap();
        let tree = ProcessTree::attach(&child).unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"start")
            .await
            .unwrap();
        let pid = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(s) = std::fs::read_to_string(&pid_file) {
                    if let Ok(pid) = s.parse::<u32>() {
                        break pid;
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(!handle.is_null());
        drop(tree);
        let result = unsafe { WaitForSingleObject(handle, 5000) };
        unsafe {
            CloseHandle(handle);
        }
        assert_eq!(result, WAIT_OBJECT_0);
        let _ = child.wait().await;
    }
}
