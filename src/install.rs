use std::{
    fs,
    path::{Path, PathBuf},
    ptr,
};
use windows_sys::Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*};
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub struct Instance(HANDLE);
impl Instance {
    pub fn acquire(data: &Path) -> Option<Self> {
        use sha2::{Digest, Sha256};
        let path = data
            .canonicalize()
            .unwrap_or_else(|_| data.to_path_buf())
            .to_string_lossy()
            .to_lowercase();
        let key = format!("Local\\PersonalDesk-{:x}", Sha256::digest(path.as_bytes()));
        unsafe {
            let handle = CreateMutexW(ptr::null(), 1, wide(&key).as_ptr());
            if handle.is_null() {
                return None;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                let window = FindWindowW(ptr::null(), wide("个人办公台").as_ptr());
                if !window.is_null() {
                    ShowWindow(window, SW_RESTORE);
                    SetForegroundWindow(window);
                }
                return None;
            }
            Some(Self(handle))
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}
pub fn normalize_launcher() -> bool {
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    let Some(parent) = executable.parent() else {
        return false;
    };
    let name = executable.file_name().unwrap_or_default().to_string_lossy();
    if !name.starts_with("个人办公台-") || !parent.join("personal-desk-data-path.txt").exists()
    {
        return false;
    }
    let stable = parent.join("个人办公台.exe");
    if crate::updater::digest(&stable).ok() != crate::updater::digest(&executable).ok() {
        return false;
    }
    std::process::Command::new(stable).spawn().is_ok()
}
pub fn archive_launchers(directory: PathBuf) {
    let Ok(current) = std::env::current_exe() else {
        return;
    };
    if current
        .file_name()
        .is_none_or(|name| name != "个人办公台.exe")
    {
        return;
    }
    let Ok(root) = directory.canonicalize() else {
        return;
    };
    let archive = root.join("updates/launcher-history");
    if fs::create_dir_all(&archive).is_err() {
        return;
    }
    let Ok(entries) = fs::read_dir(&root) else {
        return;
    };
    for item in entries.flatten() {
        let source = item.path();
        let name = item.file_name();
        let text = name.to_string_lossy();
        if !(text.starts_with("个人办公台-")
            || (text.starts_with("personal-desk-") && text.ends_with(".previous.exe")))
            || !text.ends_with(".exe")
            || !source.is_file()
        {
            continue;
        }
        if source.parent() != Some(root.as_path()) {
            continue;
        }
        let target = archive.join(format!(
            "{}-{}",
            chrono::Local::now().format("%Y%m%d%H%M%S"),
            text
        ));
        let _ = fs::rename(source, target);
    }
}
