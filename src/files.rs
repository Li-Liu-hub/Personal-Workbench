use crate::error::{DeskError, Result};
use std::path::{Path, PathBuf};
pub fn choose_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("选择快捷文件夹")
        .pick_folder()
}
pub fn choose_file() -> Option<PathBuf> {
    rfd::FileDialog::new().set_title("选择快捷文件").pick_file()
}
pub fn open_in_system(path: &Path) -> Result<()> {
    if !path.is_dir() {
        return Err(DeskError::InvalidPath("只能打开文件夹位置".into()));
    }
    open::that_detached(shell_path(path))
        .map_err(|e| DeskError::InvalidPath(format!("无法打开此位置：{e}")))
}
pub fn shell_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::{
            ffi::OsString,
            os::windows::ffi::{OsStrExt, OsStringExt},
        };
        let value = path.as_os_str().encode_wide().collect::<Vec<_>>();
        let prefix = |s: &str| s.encode_utf16().collect::<Vec<_>>();
        if value.starts_with(&prefix(r"\\?\UNC\")) {
            return OsString::from_wide(
                &prefix(r"\\")
                    .into_iter()
                    .chain(value[8..].iter().copied())
                    .collect::<Vec<_>>(),
            )
            .into();
        }
        if value.starts_with(&prefix(r"\\?\")) {
            return OsString::from_wide(&value[4..]).into();
        }
    }
    path.to_path_buf()
}
pub fn location_target(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        return Ok(shell_path(path));
    }
    let parent = path
        .parent()
        .filter(|p| p.is_dir())
        .ok_or_else(|| DeskError::InvalidPath("文件所在文件夹已不存在".into()))?;
    Ok(shell_path(parent))
}
pub fn open_location(path: &Path) -> Result<()> {
    let folder = location_target(path)?;
    if path.is_file() {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::{
                System::Com::{
                    COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                },
                UI::Shell::{SHOpenFolderAndSelectItems, SHParseDisplayName},
            };
            let wide = shell_path(path)
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>();
            unsafe {
                let initialized = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
                let mut item = std::ptr::null_mut();
                let parsed = SHParseDisplayName(
                    wide.as_ptr(),
                    std::ptr::null_mut(),
                    &mut item,
                    0,
                    std::ptr::null_mut(),
                );
                let opened = if parsed >= 0 {
                    let result = SHOpenFolderAndSelectItems(item, 0, std::ptr::null(), 0);
                    CoTaskMemFree(item.cast());
                    result >= 0
                } else {
                    false
                };
                if initialized >= 0 {
                    CoUninitialize();
                }
                if opened {
                    return Ok(());
                }
            }
        }
    }
    open_in_system(&folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locations_resolve_files_and_folders_and_report_missing_parents() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("document.txt");
        std::fs::write(&file, "keep").unwrap();
        assert_eq!(location_target(&file).unwrap(), shell_path(dir.path()));
        assert_eq!(location_target(dir.path()).unwrap(), shell_path(dir.path()));
        assert!(location_target(&dir.path().join("missing/child.txt")).is_err());
        assert!(open_in_system(&file).is_err());
        assert_eq!(std::fs::read_to_string(file).unwrap(), "keep");
    }

    #[cfg(windows)]
    #[test]
    fn shell_paths_preserve_unicode_and_convert_verbatim_drive_and_unc_prefixes() {
        for (input, expected) in [
            (r"\\?\C:\资料\文档.txt", r"C:\资料\文档.txt"),
            (r"\\?\UNC\server\share\文档.txt", r"\\server\share\文档.txt"),
            (r"C:\ordinary.txt", r"C:\ordinary.txt"),
        ] {
            assert_eq!(shell_path(Path::new(input)), PathBuf::from(expected));
        }
    }
}
