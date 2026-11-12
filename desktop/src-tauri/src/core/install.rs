use std::{
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
