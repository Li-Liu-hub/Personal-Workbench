use crate::{
    error::{DeskError, Result},
    model::DocumentRecord,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize)]
pub struct Document {
    pub record: DocumentRecord,
    pub tags: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    original: PathBuf,
    quarantine: PathBuf,
    documents: Vec<Document>,
    attributes: Option<u32>,
}
pub struct Journal {
    path: PathBuf,
    entries: Vec<Entry>,
    pub error: String,
}
impl Journal {
    pub fn open(data: &Path) -> Self {
        let path = data.join("undo-journal.json");
        let mut error = String::new();
        let entries = if path.exists() {
            match fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            {
                Some(entries) => entries,
                None => {
                    error = "撤回记录无法读取，已停止删除操作并保留原文件".into();
                    vec![]
                }
            }
        } else {
            vec![]
        };
        Self {
            path,
            entries,
            error,
        }
    }
    fn persist(&self) -> Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| DeskError::InvalidPath("撤回目录不可用".into()))?;
        fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&serde_json::to_vec(&self.entries)?)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&self.path)
            .map_err(|e| DeskError::Io(e.error))?;
        Ok(())
    }
    pub fn available(&self) -> bool {
        !self.entries.is_empty()
    }
    pub fn delete(&mut self, path: &Path, documents: Vec<Document>) -> Result<()> {
        if !self.error.is_empty() {
            return Err(DeskError::InvalidPath(self.error.clone()));
        }
        let original = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let parent = original
            .parent()
            .filter(|_| original.file_name().is_some())
            .ok_or_else(|| DeskError::InvalidPath("无法删除磁盘根目录".into()))?;
        if original.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .starts_with(".personal-desk-deleted-")
        }) {
            return Err(DeskError::InvalidPath(
                "这是待恢复的文件，请使用撤回".into(),
            ));
        }
        let quarantine = parent.join(format!(".personal-desk-deleted-{}", uuid::Uuid::new_v4()));
        let attributes = attributes(&original);
        self.entries.push(Entry {
            original: original.clone(),
            quarantine: quarantine.clone(),
            documents,
            attributes,
        });
        if let Err(error) = self.persist() {
            self.entries.pop();
            return Err(error);
        }
        if let Err(error) = fs::rename(&original, &quarantine) {
            self.entries.pop();
            let _ = self.persist();
            return Err(error.into());
        }
        set_hidden(&quarantine, attributes, true);
        Ok(())
    }
    pub fn undo(&mut self, restore: impl FnOnce(&[Document]) -> Result<()>) -> Result<PathBuf> {
        let entry = self
            .entries
            .last()
            .cloned()
            .ok_or_else(|| DeskError::InvalidPath("没有可撤回的删除".into()))?;
        if entry.quarantine.exists() {
            if entry.original.exists() {
                return Err(DeskError::AlreadyExists);
            }
            fs::rename(&entry.quarantine, &entry.original)?;
            set_hidden(&entry.original, entry.attributes, false);
        } else if !entry.original.exists() {
            return Err(DeskError::InvalidPath(
                "恢复文件暂不可用，撤回记录已保留".into(),
            ));
        }
        restore(&entry.documents)?;
        self.entries.pop();
        if let Err(error) = self.persist() {
            self.entries.push(entry.clone());
            return Err(error);
        }
        Ok(entry.original)
    }
    pub fn sources(&self) -> Vec<(PathBuf, PathBuf)> {
        self.entries
            .iter()
            .map(|entry| (entry.original.clone(), entry.quarantine.clone()))
            .collect()
    }
}
fn attributes(path: &Path) -> Option<u32> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let value =
            unsafe { windows_sys::Win32::Storage::FileSystem::GetFileAttributesW(wide.as_ptr()) };
        if value != u32::MAX {
            return Some(value);
        }
    }
    None
}
fn set_hidden(path: &Path, attributes: Option<u32>, hide: bool) {
    #[cfg(windows)]
    if let Some(attributes) = attributes {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            windows_sys::Win32::Storage::FileSystem::SetFileAttributesW(
                wide.as_ptr(),
                if hide { attributes | 2 } else { attributes },
            );
        }
    }
    #[cfg(not(windows))]
    let _ = (path, attributes, hide);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deletion_is_recoverable_after_restart_and_conflicts_or_failures_never_overwrite() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let folder = root.path().join("工作文件");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("原件.txt"), "重要内容").unwrap();
        let mut journal = Journal::open(&data);
        journal.delete(&folder, vec![]).unwrap();
        assert!(!folder.exists());
        drop(journal);
        let mut journal = Journal::open(&data);
        assert!(journal.available());
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("new.txt"), "new").unwrap();
        assert!(journal.undo(|_| Ok(())).is_err());
        assert_eq!(fs::read_to_string(folder.join("new.txt")).unwrap(), "new");
        fs::remove_file(folder.join("new.txt")).unwrap();
        fs::remove_dir(&folder).unwrap();
        assert!(
            journal
                .undo(|_| Err(DeskError::ConcurrentModification))
                .is_err()
        );
        assert!(journal.available());
        assert_eq!(
            fs::read_to_string(folder.join("原件.txt")).unwrap(),
            "重要内容"
        );
        journal.undo(|_| Ok(())).unwrap();
        assert!(!journal.available());
        let file = root.path().join("remain.txt");
        fs::write(&file, "stay").unwrap();
        let bad = root.path().join("not-a-directory");
        fs::write(&bad, "block").unwrap();
        assert!(Journal::open(&bad).delete(&file, vec![]).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "stay");
        fs::write(data.join("undo-journal.json"), b"malformed").unwrap();
        assert!(Journal::open(&data).delete(&file, vec![]).is_err());
        assert!(file.exists());
    }
}
