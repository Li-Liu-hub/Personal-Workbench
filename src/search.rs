use std::path::Path;

use crate::{db::Database, error::Result, model::SearchResult};

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
