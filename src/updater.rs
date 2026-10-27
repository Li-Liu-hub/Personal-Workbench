//! Local and GitHub release feeds, plus a separate, waiting installer. User data is never replaced.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Public feed used by every distributed copy: the newest published GitHub release.
pub const REMOTE_FEED: &str =
    "https://github.com/Li-Liu-hub/Personal-Workbench/releases/latest/download/latest.json";
/// Downloads are only accepted from this repository's own release assets.
const RELEASE_PATH: &str = "/Li-Liu-hub/Personal-Workbench/releases/download/";
const MAX_BINARY: u64 = 512 * 1024 * 1024;
const MAX_MANIFEST: u64 = 128 * 1024;
type UpdateResult<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub app: String,
    pub version: String,
    /// Relative path inside a local feed, or an absolute https release asset.
    pub executable: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub notes: String,
}
#[derive(Clone, Debug)]
pub struct Release {
    pub manifest: Manifest,
    pub executable: PathBuf,
}
#[derive(Serialize, Deserialize)]
pub struct InstallRequest {
    target: PathBuf,
    staged: PathBuf,
    sha256: String,
    previous_sha256: String,
    parent_pid: u32,
    data_dir: PathBuf,
    restart: bool,
}

pub fn default_source() -> String {
    let local = std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("updates/latest.json");
    if local.is_file() {
        local.to_string_lossy().into_owned()
    } else {
        REMOTE_FEED.to_owned()
    }
}
fn is_remote(source: &str) -> bool {
    source.starts_with("https://") || source.starts_with("http://")
}
fn feed_agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .max_redirects(5)
        .timeout_global(Some(timeout))
        .build()
        .into()
}
pub fn digest(path: &Path) -> UpdateResult<String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn check_binary(path: &Path, hash: &str, size: u64) -> UpdateResult<()> {
    let length = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size == 0 || size > MAX_BINARY || length != size {
        return Err("更新文件大小不正确".into());
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut signature = [0; 2];
    file.read_exact(&mut signature).map_err(|e| e.to_string())?;
    if signature != *b"MZ" || digest(path)? != hash.to_ascii_lowercase() {
        return Err("更新文件不完整，请重新获取".into());
    }
    Ok(())
}
fn newer(manifest: &Manifest, current: &str) -> UpdateResult<bool> {
    if manifest.app != "personal-desk" {
        return Err("这不是个人办公台更新包".into());
    }
    let version =
        semver::Version::parse(&manifest.version).map_err(|_| "更新版本号无效".to_owned())?;
    let current = semver::Version::parse(current).map_err(|e| e.to_string())?;
    Ok(version > current)
}
fn local_executable(root: &Path, value: &str) -> UpdateResult<PathBuf> {
    let relative = Path::new(value);
    if relative.is_absolute()
        || relative
            .components()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
        || relative.extension().is_none_or(|v| v != "exe")
    {
        return Err("更新清单中的程序路径无效".into());
    }
    let executable = root
        .join(relative)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !executable.starts_with(root) {
        return Err("更新程序不在发布目录中".into());
    }
    Ok(executable)
}
fn remote_executable(value: &str) -> UpdateResult<()> {
    let url = url::Url::parse(value).map_err(|_| "更新清单中的程序地址无效".to_owned())?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.path().starts_with(RELEASE_PATH)
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("更新清单中的程序地址不在官方发布页".into());
    }
    Ok(())
}
fn check_local(source: &Path, current: &str) -> UpdateResult<Option<Release>> {
    let size = fs::metadata(source)
        .map_err(|_| "更新源尚未发布新版本，可选择更新清单".to_owned())?
        .len();
    if size > MAX_MANIFEST {
        return Err("更新清单过大".into());
    }
    let manifest: Manifest = serde_json::from_slice(&fs::read(source).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if !newer(&manifest, current)? {
        return Ok(None);
    }
    let root = source
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let executable = local_executable(&root, &manifest.executable)?;
    check_binary(&executable, &manifest.sha256, manifest.size)?;
    Ok(Some(Release {
        manifest,
        executable,
    }))
}
/// Downloads the newest release binary next to the manifest so that pressing
/// "install" only has to swap local files.
fn cached_binary(manifest: &Manifest) -> UpdateResult<PathBuf> {
    let version =
        semver::Version::parse(&manifest.version).map_err(|_| "更新版本号无效".to_owned())?;
    let token: String = manifest.sha256.chars().take(12).collect();
    let directory = std::env::temp_dir().join("personal-desk-update-cache");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    Ok(directory.join(format!("{version}-{token}.exe")))
}
fn download(url: &str, destination: &Path) -> UpdateResult<()> {
    let agent = feed_agent(Duration::from_secs(180));
    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| format!("下载更新失败：{e}"))?;
    let staging = destination.with_extension("part");
    let result = (|| {
        let mut reader = response.body_mut().with_config().limit(MAX_BINARY).reader();
        let mut file = fs::File::create(&staging).map_err(|e| e.to_string())?;
        let written =
            std::io::copy(&mut reader, &mut file).map_err(|e| format!("下载更新中断：{e}"))?;
        if written == 0 {
            return Err("更新文件为空".to_owned());
        }
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&staging, destination).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}
fn check_remote(feed: &str, current: &str) -> UpdateResult<Option<Release>> {
    let feed_url = url::Url::parse(feed).map_err(|_| "更新源地址无效".to_owned())?;
    if feed_url.scheme() != "https" {
        return Err("更新源必须使用 https".into());
    }
    let text = feed_agent(Duration::from_secs(30))
        .get(feed)
        .call()
        .map_err(|e| format!("无法连接更新源：{e}"))?
        .body_mut()
        .with_config()
        .limit(MAX_MANIFEST)
        .read_to_string()
        .map_err(|e| format!("读取更新清单失败：{e}"))?;
    let manifest: Manifest = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if !newer(&manifest, current)? {
        return Ok(None);
    }
    remote_executable(&manifest.executable)?;
    let executable = cached_binary(&manifest)?;
    if check_binary(&executable, &manifest.sha256, manifest.size).is_err() {
        download(&manifest.executable, &executable)?;
        check_binary(&executable, &manifest.sha256, manifest.size)?;
    }
    Ok(Some(Release {
        manifest,
        executable,
    }))
}
pub fn check(source: &str, current: &str) -> UpdateResult<Option<Release>> {
    if is_remote(source) {
        check_remote(source, current)
    } else {
        check_local(Path::new(source), current)
    }
}
fn snapshot(data: &Path) -> UpdateResult<()> {
    if !data.join("desk.db").exists() {
        return Ok(());
    }
    let connection = rusqlite::Connection::open(data.join("desk.db")).map_err(|e| e.to_string())?;
    connection
        .busy_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if integrity != "ok" {
        return Err("本地数据库检查未通过，未开始更新".into());
    }
    let backup = data
        .join("backups")
        .join(format!("before-update-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&backup).map_err(|e| e.to_string())?;
    connection
        .backup("main", backup.join("desk.db"), None)
        .map_err(|e| e.to_string())
}
pub fn launch(release: &Release, data: &Path) -> UpdateResult<()> {
    check_binary(
        &release.executable,
        &release.manifest.sha256,
        release.manifest.size,
    )?;
    snapshot(data)?;
    let target = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let directory =
        std::env::temp_dir().join(format!("personal-desk-update-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let staged = directory.join("payload.exe");
    fs::copy(&release.executable, &staged).map_err(|e| e.to_string())?;
    check_binary(&staged, &release.manifest.sha256, release.manifest.size)?;
    let helper = directory.join("installer.exe");
    fs::copy(&target, &helper).map_err(|e| e.to_string())?;
    let request = InstallRequest {
        previous_sha256: digest(&target)?,
        target,
        staged,
        sha256: release.manifest.sha256.to_ascii_lowercase(),
        parent_pid: std::process::id(),
        data_dir: data.to_path_buf(),
        restart: true,
    };
    let path = directory.join("request.json");
    fs::write(
        &path,
        serde_json::to_vec(&request).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut command = Command::new(helper);
    command.arg("--apply-update").arg(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn().map_err(|e| e.to_string())?;
    Ok(())
}
fn wait_for_exit(pid: u32) -> UpdateResult<()> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, WAIT_OBJECT_0},
            System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
        };
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(87) {
                return Ok(());
            }
            return Err(format!("无法等待旧窗口退出：{error}"));
        }
        let status = WaitForSingleObject(handle, 60_000);
        CloseHandle(handle);
        if status != WAIT_OBJECT_0 {
            return Err("旧窗口尚未退出，已取消更新".into());
        }
    }
    #[cfg(not(windows))]
    let _ = pid;
    Ok(())
}
fn replace(request: &InstallRequest) -> UpdateResult<PathBuf> {
    if digest(&request.target)? != request.previous_sha256 {
        return Err("安装目录已发生变化，未覆盖程序".into());
    }
    let size = fs::metadata(&request.staged)
        .map_err(|e| e.to_string())?
        .len();
    check_binary(&request.staged, &request.sha256, size)?;
    let parent = request.target.parent().ok_or("安装目录无效")?;
    let token = uuid::Uuid::new_v4();
    let incoming = parent.join(format!("personal-desk-{token}.new.exe"));
    let backup = parent.join(format!("personal-desk-{token}.previous.exe"));
    fs::copy(&request.staged, &incoming).map_err(|e| e.to_string())?;
    if digest(&incoming)? != request.sha256 {
        return Err("写入更新文件失败".into());
    }
    let pin = parent.join("personal-desk-data-path.txt");
    if !pin.exists() {
        fs::write(&pin, request.data_dir.to_string_lossy().as_bytes())
            .map_err(|e| e.to_string())?;
    }
    fs::rename(&request.target, &backup).map_err(|e| e.to_string())?;
    if let Err(error) = fs::rename(&incoming, &request.target) {
        let _ = fs::rename(&backup, &request.target);
        return Err(format!("安装失败，已保留原版：{error}"));
    }
    Ok(backup)
}
pub fn apply_request(path: &Path) -> UpdateResult<()> {
    let request: InstallRequest =
        serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut host_exited = false;
    let result = (|| {
        wait_for_exit(request.parent_pid)?;
        host_exited = true;
        let backup = replace(&request)?;
        if request.restart {
            let mut command = Command::new(&request.target);
            #[cfg(debug_assertions)]
            if std::env::var_os("PERSONAL_DESK_UPDATE_RESTART_PREVIEW").is_some() {
                command.arg("--layout-preview");
            }
            if let Err(error) = command.spawn() {
                let failed = request.target.with_extension("failed.exe");
                let _ = fs::rename(&request.target, failed);
                let _ = fs::rename(&backup, &request.target);
                return Err(format!("新版启动失败，已恢复原版：{error}"));
            }
        }
        Ok(())
    })();
    if let Some(parent) = request.target.parent() {
        let log = parent.join("personal-desk-update.log");
        let _ = fs::File::create(log).and_then(|mut file| {
            write!(
                file,
                "{}",
                match &result {
                    Ok(()) => "更新完成".to_owned(),
                    Err(e) => format!("更新失败：{e}"),
                }
            )
        });
    }
    if result.is_err() && host_exited && request.restart && request.target.exists() {
        let _ = Command::new(&request.target).spawn();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_feed_versions_integrity_and_atomic_replacement_preserve_data_and_pin() {
        let root = tempfile::tempdir().unwrap();
        let feed = root.path().join("feed");
        fs::create_dir(&feed).unwrap();
        let binary = feed.join("release.exe");
        fs::write(&binary, b"MZ synthetic update binary").unwrap();
        let manifest = Manifest {
            app: "personal-desk".into(),
            version: "99.0.0".into(),
            executable: "release.exe".into(),
            sha256: digest(&binary).unwrap(),
            size: fs::metadata(&binary).unwrap().len(),
            notes: "test".into(),
        };
        let source = feed.join("latest.json");
        fs::write(&source, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(
            check(&source.to_string_lossy(), "99.0.0")
                .unwrap()
                .is_none()
        );
        let release = check(&source.to_string_lossy(), VERSION).unwrap().unwrap();
        let target = root.path().join("desk.exe");
        fs::write(&target, b"MZ original version").unwrap();
        let data = root.path().join("data");
        fs::create_dir(&data).unwrap();
        fs::write(data.join("document.md"), "keep").unwrap();
        let pin = root.path().join("personal-desk-data-path.txt");
        fs::write(&pin, "custom-data-directory").unwrap();
        let request = InstallRequest {
            previous_sha256: digest(&target).unwrap(),
            target: target.clone(),
            staged: release.executable,
            sha256: manifest.sha256.clone(),
            parent_pid: 0,
            data_dir: data.clone(),
            restart: false,
        };
        let backup = replace(&request).unwrap();
        assert_eq!(fs::read(&target).unwrap(), fs::read(&binary).unwrap());
        assert_eq!(fs::read(&backup).unwrap(), b"MZ original version");
        assert_eq!(fs::read_to_string(pin).unwrap(), "custom-data-directory");
        assert_eq!(
            fs::read_to_string(data.join("document.md")).unwrap(),
            "keep"
        );
        fs::write(&binary, b"MZ corrupted update").unwrap();
        assert!(check(&source.to_string_lossy(), VERSION).is_err());
        assert!(replace(&request).is_err());
        let mut escaped = manifest;
        escaped.executable = "../desk.exe".into();
        fs::write(&source, serde_json::to_vec(&escaped).unwrap()).unwrap();
        assert!(check(&source.to_string_lossy(), VERSION).is_err());
    }
    #[test]
    fn release_assets_are_limited_to_the_official_repository() {
        let official = "https://github.com/Li-Liu-hub/Personal-Workbench/releases/download/v0.15.31/PersonalDesk.exe";
        assert!(remote_executable(official).is_ok());
        for rejected in [
            "http://github.com/Li-Liu-hub/Personal-Workbench/releases/download/v0.15.31/PersonalDesk.exe",
            "https://example.com/Li-Liu-hub/Personal-Workbench/releases/download/v0.15.31/PersonalDesk.exe",
            "https://github.com/other/Personal-Workbench/releases/download/v0.15.31/PersonalDesk.exe",
            "https://github.com/Li-Liu-hub/Personal-Workbench/raw/main/PersonalDesk.exe",
            "https://github.com/Li-Liu-hub/Personal-Workbench/releases/download/v0.15.31/PersonalDesk.exe?raw=1",
        ] {
            assert!(remote_executable(rejected).is_err(), "{rejected}");
        }
        assert!(is_remote(REMOTE_FEED));
        assert!(!is_remote("updates/latest.json"));
        let root = std::env::temp_dir();
        assert!(local_executable(&root, official).is_err());
    }
}

#[cfg(test)]
mod process_tests {
    use super::*;
    #[test]
    #[ignore = "Internal isolated updater wait fixture"]
    fn wait_fixture() {
        let root = PathBuf::from(std::env::var_os("PERSONAL_DESK_UPDATE_FIXTURE").unwrap());
        fs::write(root.join("ready"), b"ready").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !root.join("exit").exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    #[test]
    fn installer_waits_for_host_exit_and_preserves_database_backup() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let mut db = crate::db::Database::open(&data).unwrap();
        db.set_setting("fixture", "keep-value").unwrap();
        drop(db);
        snapshot(&data).unwrap();
        let backups = fs::read_dir(data.join("backups"))
            .unwrap()
            .collect::<std::io::Result<Vec<_>>>()
            .unwrap();
        let backup = rusqlite::Connection::open(backups[0].path().join("desk.db")).unwrap();
        assert_eq!(
            backup
                .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        drop(backup);
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "updater::process_tests::wait_fixture",
                "--ignored",
            ])
            .env("PERSONAL_DESK_UPDATE_FIXTURE", root.path());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !root.path().join("ready").exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(root.path().join("ready").exists());
        let target = root.path().join("desk.exe");
        fs::write(&target, b"MZ old fixture").unwrap();
        let staged = root.path().join("new.exe");
        fs::write(&staged, b"MZ new fixture").unwrap();
        let request = InstallRequest {
            previous_sha256: digest(&target).unwrap(),
            target: target.clone(),
            sha256: digest(&staged).unwrap(),
            staged,
            parent_pid: child.id(),
            data_dir: data.clone(),
            restart: false,
        };
        let path = root.path().join("request.json");
        fs::write(&path, serde_json::to_vec(&request).unwrap()).unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            sender.send(apply_request(&path)).unwrap();
        });
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_millis(100))
                .is_err()
        );
        assert_eq!(fs::read(&target).unwrap(), b"MZ old fixture");
        fs::write(root.path().join("exit"), b"exit").unwrap();
        child.wait().unwrap();
        receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"MZ new fixture");
        let db = crate::db::Database::open(&data).unwrap();
        assert_eq!(db.settings().unwrap()["fixture"], "keep-value");
    }
}
