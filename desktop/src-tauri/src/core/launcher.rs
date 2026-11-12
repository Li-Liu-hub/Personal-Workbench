use crate::error::{DeskError, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// 小工具记录；包含名称、程序路径和备注。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppTool {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub notes: String,
}

impl AppTool {
    pub fn new(path: PathBuf) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            path,
            notes: String::new(),
        }
    }
}

pub fn prepare(path: &Path) -> Result<PathBuf> {
    let path = path.canonicalize()?;
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return Err(DeskError::InvalidPath("请选择一个 .exe 程序文件".into()));
    }
    // Use ordinary Windows paths for child programs, including their working directory.
    // Stored entries from older versions may still have a verbatim (\\?\) prefix.
    Ok(crate::files::shell_path(&path))
}

pub fn launch(path: &Path) -> Result<()> {
    let path = prepare(path)?;
    let mut command = Command::new(&path);
    if let Some(parent) = path.parent() {
        command.current_dir(parent);
    }
    command.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepares_legacy_paths_with_chinese_spaces_and_uppercase_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("中文 程序.EXE");
        std::fs::write(&path, b"test fixture").unwrap();
        let legacy = path.canonicalize().unwrap();
        let prepared = prepare(&legacy).unwrap();
        assert_eq!(prepared, crate::files::shell_path(&legacy));
        assert!(prepared.is_absolute());
        assert_eq!(std::fs::read(prepared).unwrap(), b"test fixture");
    }

    #[test]
    fn rejects_missing_programs_directories_and_non_executables() {
        let dir = tempfile::tempdir().unwrap();
        assert!(launch(&dir.path().join("missing.exe")).is_err());
        let folder = dir.path().join("folder.exe");
        std::fs::create_dir(&folder).unwrap();
        assert!(prepare(&folder).is_err());
        let text = dir.path().join("program.txt");
        std::fs::write(&text, b"text").unwrap();
        assert!(prepare(&text).is_err());
    }
}
