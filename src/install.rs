use std::{
    fs,
    path::{Path, PathBuf},
    ptr,
};
use windows_sys::Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*};
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

/// A launcher moved to another folder still finds the user's chosen data directory.
/// This stores only a path; the database and documents never enter the registry.
pub fn remembered_data_directory() -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::*;
    unsafe {
        let mut size = 0;
        let key = wide("Software\\PersonalDesk");
        let name = wide("DataDirectory");
        if RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut size,
        ) != ERROR_SUCCESS
            || size == 0
            || size > 65536
            || !size.is_multiple_of(2)
        {
            return None;
        }
        let mut buffer = vec![0u16; size as usize / 2];
        if RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        ) != ERROR_SUCCESS
        {
            return None;
        }
        buffer.truncate(size as usize / 2);
        while buffer.last() == Some(&0) {
            buffer.pop();
        }
        let path = PathBuf::from(String::from_utf16(&buffer).ok()?);
        path.is_absolute().then_some(path)
    }
}
pub fn remember_data_directory(path: &Path) {
    use windows_sys::Win32::System::Registry::*;
    if !path.is_absolute() {
        return;
    }
    unsafe {
        let mut key = ptr::null_mut();
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide("Software\\PersonalDesk").as_ptr(),
            0,
            ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            ptr::null(),
            &mut key,
            ptr::null_mut(),
        ) != ERROR_SUCCESS
        {
            return;
        }
        let value = wide(&path.to_string_lossy());
        RegSetValueExW(
            key,
            wide("DataDirectory").as_ptr(),
            0,
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * 2) as u32,
        );
        RegCloseKey(key);
    }
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
