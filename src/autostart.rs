use std::{path::Path, ptr};
use windows_sys::Win32::System::Registry::*;
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
pub fn command(executable: &Path) -> String {
    format!("\"{}\" --autostart", executable.display())
}
pub fn enabled() -> bool {
    unsafe {
        let mut size = 0;
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(KEY).as_ptr(),
            wide("PersonalDesk").as_ptr(),
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut size,
        ) == 0
    }
}
pub fn set(enabled: bool, executable: &Path) -> Result<(), String> {
    unsafe {
        let mut key = ptr::null_mut();
        let result = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(KEY).as_ptr(),
            0,
            ptr::null(),
            0,
            KEY_SET_VALUE,
            ptr::null(),
            &mut key,
            ptr::null_mut(),
        );
        if result != 0 {
            return Err(format!("无法修改自启动：{result}"));
        }
        let result = if enabled {
            let value = wide(&command(executable));
            RegSetValueExW(
                key,
                wide("PersonalDesk").as_ptr(),
                0,
                REG_SZ,
                value.as_ptr() as *const u8,
                (value.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(key, wide("PersonalDesk").as_ptr())
        };
        RegCloseKey(key);
        if result == 0 || (!enabled && result == 2) {
            Ok(())
        } else {
            Err(format!("无法修改自启动：{result}"))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn startup_command_quotes_paths_with_spaces_and_unicode_without_touching_registry() {
        assert_eq!(
            super::command(std::path::Path::new("C:/My Apps/个人办公台.exe")),
            "\"C:/My Apps/个人办公台.exe\" --autostart"
        );
    }
}
