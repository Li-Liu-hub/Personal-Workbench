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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_journal_is_empty_and_invalid_history_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Journal::open(dir.path()).sources().is_empty());
        assert!(Journal::open(dir.path()).error.is_empty());
        let path = dir.path().join("undo-journal.json");
        fs::write(&path, "invalid history").unwrap();
        let journal = Journal::open(dir.path());
        assert!(!journal.error.is_empty());
        assert!(journal.sources().is_empty());
        assert_eq!(fs::read_to_string(path).unwrap(), "invalid history");
    }

    #[test]
    fn reading_history_returns_sources_without_moving_or_deleting_files() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.txt");
        let quarantine = dir.path().join("quarantine.txt");
        fs::write(&source, "keep source").unwrap();
        fs::write(&quarantine, "keep quarantine").unwrap();
        fs::write(
            dir.path().join("undo-journal.json"),
            serde_json::to_vec(&serde_json::json!([{"original":source,"quarantine":quarantine}]))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            Journal::open(dir.path()).sources(),
            vec![(source.clone(), quarantine.clone())]
        );
        assert_eq!(fs::read_to_string(source).unwrap(), "keep source");
        assert_eq!(fs::read_to_string(quarantine).unwrap(), "keep quarantine");
    }
}
