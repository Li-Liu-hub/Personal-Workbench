use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Deserialize)]
struct Entry {
    original: PathBuf,
    quarantine: PathBuf,
}
pub struct Journal {
    entries: Vec<Entry>,
    pub error: String,
}
impl Journal {
    pub fn open(data: &Path) -> Self {
        let path = data.join("undo-journal.json");
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(entries) => Self {
                    entries,
                    error: String::new(),
                },
                Err(_) => Self {
                    entries: vec![],
                    error: "历史撤回记录无法读取，原文件保留".into(),
                },
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self {
                entries: vec![],
                error: String::new(),
            },
            Err(e) => Self {
                entries: vec![],
                error: e.to_string(),
            },
        }
    }
    pub fn sources(&self) -> Vec<(PathBuf, PathBuf)> {
        self.entries
            .iter()
            .map(|e| (e.original.clone(), e.quarantine.clone()))
            .collect()
    }
}
