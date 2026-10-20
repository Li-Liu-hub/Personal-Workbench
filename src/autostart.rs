//! Per-user Windows sign-in startup. Tests use an isolated registry key.
use std::{path::Path, ptr};
use windows_sys::Win32::{Foundation::*, System::Registry::*};

const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE: &str = "PersonalDesk";

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub fn command(executable: &Path) -> String {
    format!("\"{}\" --autostart", executable.display())
}

pub fn registered_command() -> Result<Option<String>, String> {
    RegistryEntry(KEY).read()
}

pub fn set(enabled: bool, executable: &Path) -> Result<(), String> {
    RegistryEntry(KEY).set(enabled, executable)
}

/// Refresh an existing opt-in after moving the application; never opt in silently.
pub fn refresh_if_enabled(executable: &Path) -> Result<(), String> {
    RegistryEntry(KEY).refresh_if_enabled(executable)
}

struct RegistryEntry<'a>(&'a str);
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

impl RegistryEntry<'_> {
    fn read(&self) -> Result<Option<String>, String> {
        unsafe {
            let mut size = 0;
            let result = RegGetValueW(
                HKEY_CURRENT_USER,
                wide(self.0).as_ptr(),
                wide(VALUE).as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut size,
            );
            if matches!(result, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
                return Ok(None);
            }
            if result != ERROR_SUCCESS {
                return Err(format!("无法读取自启动状态：{result}"));
            }
            if size == 0 || size > 65536 || !size.is_multiple_of(2) {
                return Err("自启动路径格式无效".into());
            }
            let mut buffer = vec![0u16; size as usize / 2];
            let result = RegGetValueW(
                HKEY_CURRENT_USER,
                wide(self.0).as_ptr(),
                wide(VALUE).as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            );
            if result != ERROR_SUCCESS {
                return Err(format!("无法读取自启动路径：{result}"));
            }
            buffer.truncate(size as usize / 2);
            while buffer.last() == Some(&0) {
                buffer.pop();
            }
            let value = String::from_utf16(&buffer).map_err(|_| "自启动路径格式无效")?;
            Ok(Some(value))
        }
    }

    fn set(&self, enabled: bool, executable: &Path) -> Result<(), String> {
        if enabled && (!executable.is_absolute() || !executable.is_file()) {
            return Err("启动程序不存在，请先将程序保存到固定位置".into());
        }
        unsafe {
            let mut handle = ptr::null_mut();
            let result = if enabled {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    wide(self.0).as_ptr(),
                    0,
                    ptr::null(),
                    0,
                    KEY_SET_VALUE,
                    ptr::null(),
                    &mut handle,
                    ptr::null_mut(),
                )
            } else {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    wide(self.0).as_ptr(),
                    0,
                    KEY_SET_VALUE,
                    &mut handle,
                )
            };
            if !enabled && matches!(result, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
                return Ok(());
            }
            if result != ERROR_SUCCESS {
                return Err(format!("无法修改自启动：{result}"));
            }
            let key = Key(handle);
            let result = if enabled {
                let value = wide(&command(executable));
                RegSetValueExW(
                    key.0,
                    wide(VALUE).as_ptr(),
                    0,
                    REG_SZ,
                    value.as_ptr().cast(),
                    (value.len() * 2) as u32,
                )
            } else {
                RegDeleteValueW(key.0, wide(VALUE).as_ptr())
            };
            if result == ERROR_SUCCESS || (!enabled && result == ERROR_FILE_NOT_FOUND) {
                Ok(())
            } else {
                Err(format!("无法修改自启动：{result}"))
            }
        }
    }

    fn refresh_if_enabled(&self, executable: &Path) -> Result<(), String> {
        if let Some(registered) = self.read()?
            && registered != command(executable)
        {
            self.set(true, executable)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct IsolatedKey(String);
    impl Drop for IsolatedKey {
        fn drop(&mut self) {
            unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide(&self.0).as_ptr()) };
        }
    }

    #[test]
    fn startup_registration_roundtrip_disable_and_relocation_preserve_opt_in() {
        // A unique non-Run key prevents tests from changing real sign-in startup.
        let isolated = IsolatedKey(format!(
            "Software\\PersonalDesk-Test-{}",
            uuid::Uuid::new_v4()
        ));
        let entry = RegistryEntry(&isolated.0);
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("原目录 程序.exe");
        let moved = directory.path().join("新目录 程序.exe");
        std::fs::write(&first, b"synthetic").unwrap();
        std::fs::write(&moved, b"synthetic").unwrap();
        assert_eq!(entry.read().unwrap(), None);
        entry.set(false, &first).unwrap();
        entry.refresh_if_enabled(&first).unwrap();
        assert_eq!(entry.read().unwrap(), None);
        entry.set(true, &first).unwrap();
        assert_eq!(entry.read().unwrap(), Some(command(&first)));
        entry.refresh_if_enabled(&moved).unwrap();
        assert_eq!(entry.read().unwrap(), Some(command(&moved)));
        assert!(
            entry
                .set(true, &directory.path().join("missing.exe"))
                .is_err()
        );
        assert_eq!(entry.read().unwrap(), Some(command(&moved)));
        entry.set(false, &moved).unwrap();
        entry.refresh_if_enabled(&moved).unwrap();
        assert_eq!(entry.read().unwrap(), None);
    }

    #[test]
    fn startup_command_quotes_paths_with_spaces_and_unicode() {
        assert_eq!(
            command(Path::new("C:/My Apps/个人办公台.exe")),
            "\"C:/My Apps/个人办公台.exe\" --autostart"
        );
    }
}
