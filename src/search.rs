use std::path::Path;

use crate::{db::Database, error::Result, files, model::SearchResult};

pub fn index_folder(db: &mut Database, root_id: &str, root: &Path) -> Result<usize> {
    let records = files::scan_root(root, 10_000, 2 * 1024 * 1024, 64 * 1024 * 1024)?;
    let count = records.len();
    for item in records {
        db.index_file(root_id, &item.path, &item.title, &item.body)?;
    }
    Ok(count)
}

pub fn index_folder_at(data_dir: &Path, root_id: &str, root: &Path) -> Result<usize> {
    let mut db = Database::open(data_dir)?;
    index_folder(&mut db, root_id, root)
}

pub fn search(db: &Database, query: &str) -> Result<Vec<SearchResult>> {
    let term = query.trim();
    if term.is_empty() {
        return Ok(Vec::new());
    }
    let mut results = Vec::new();
    for shortcut in db
        .shortcuts()?
        .into_iter()
        .filter(|s| {
            s.name.to_lowercase().contains(&term.to_lowercase())
                || s.path
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&term.to_lowercase())
        })
        .take(25)
    {
        results.push(SearchResult {
            category: if shortcut.is_dir {
                "文件夹"
            } else {
                "文件"
            }
            .into(),
            title: shortcut.name,
            detail: shortcut.path.display().to_string(),
            path: Some(shortcut.path.to_string_lossy().into_owned()),
        });
    }
    for (title, path) in db.search_files(term)? {
        if !Path::new(&path).is_file() {
            continue;
        }
        results.push(SearchResult {
            category: "已索引文件".into(),
            title,
            detail: path.clone(),
            path: Some(path),
        });
    }
    Ok(results)
}
