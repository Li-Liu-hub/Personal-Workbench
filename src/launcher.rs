use crate::error::{DeskError, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

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
    Ok(path)
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
