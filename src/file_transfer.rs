//! Explorer-compatible file clipboard and outbound drag transfers.
use crate::error::{DeskError, Result};
use std::path::Path;
use windows::{
    Win32::{
        Foundation::HWND,
        System::{
            Com::IDataObject,
            Ole::{
                DROPEFFECT_COPY, OleFlushClipboard, OleInitialize, OleSetClipboard, OleUninitialize,
            },
        },
        UI::Shell::{BHID_DataObject, IShellItem, SHCreateItemFromParsingName, SHDoDragDrop},
    },
    core::PCWSTR,
};

struct Apartment;
impl Apartment {
    fn new() -> Result<Self> {
        unsafe { OleInitialize(None) }.map_err(shell_error)?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { OleUninitialize() };
    }
}
fn shell_error(error: windows::core::Error) -> DeskError {
    DeskError::InvalidPath(format!("文件操作失败：{error}"))
}
fn data_object(path: &Path) -> Result<IDataObject> {
    use std::os::windows::ffi::OsStrExt;
    let path = path.canonicalize()?;
    let wide = crate::files::shell_path(&path)
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        let item: IShellItem =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).map_err(shell_error)?;
        item.BindToHandler(None, &BHID_DataObject)
            .map_err(shell_error)
    }
}

pub fn copy(path: &Path) -> Result<()> {
    let _apartment = Apartment::new()?;
    let data = data_object(path)?;
    unsafe {
        OleSetClipboard(&data).map_err(shell_error)?;
        // Render the data before releasing the apartment so paste also works
        // after the office app closes. Only paths/metadata are on the clipboard.
        OleFlushClipboard().map_err(shell_error)
    }
}

pub fn drag(path: &Path, window: usize) -> Result<bool> {
    let _apartment = Apartment::new()?;
    let data = data_object(path)?;
    // Copy is intentional for shortcuts/managed documents: moving a record's
    // target outside the app would invalidate its saved path.
    let effect = unsafe { SHDoDragDrop(HWND(window as isize), &data, None, DROPEFFECT_COPY) }
        .map_err(shell_error)?;
    Ok(effect == DROPEFFECT_COPY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::{
        System::{
            Com::{DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL},
            Ole::ReleaseStgMedium,
        },
        UI::Shell::{DragQueryFileW, HDROP},
    };
    #[test]
    fn shell_data_contains_a_real_unicode_file_and_folder_without_changing_the_clipboard() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("拖拽样例 文档.txt");
        std::fs::write(&path, "synthetic").unwrap();
        let _apartment = Apartment::new().unwrap();
        for path in [&path, directory.path()] {
            let data = data_object(path).unwrap();
            let format = FORMATETC {
                cfFormat: 15,
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
                ..Default::default()
            };
            let mut medium = unsafe { data.GetData(&format) }.unwrap();
            let drop = HDROP(unsafe { medium.u.hGlobal }.0 as isize);
            assert_eq!(unsafe { DragQueryFileW(drop, u32::MAX, None) }, 1);
            let length = unsafe { DragQueryFileW(drop, 0, None) };
            let mut buffer = vec![0u16; length as usize + 1];
            unsafe { DragQueryFileW(drop, 0, Some(&mut buffer)) };
            let actual = String::from_utf16_lossy(&buffer[..length as usize]);
            unsafe { ReleaseStgMedium(&mut medium) };
            assert_eq!(
                Path::new(&actual).canonicalize().unwrap(),
                path.canonicalize().unwrap()
            );
        }
        assert_eq!(std::fs::read_to_string(path).unwrap(), "synthetic");
    }
}
