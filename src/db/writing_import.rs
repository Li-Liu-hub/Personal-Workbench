use super::*;
use crate::library::WritingImportBook;

impl Database {
    /// Import once per source. Replays preserve edits and deleted documents.
    pub fn import_writing_books(&mut self, books: &[WritingImportBook]) -> Result<(usize, usize)> {
        let tx = self.desk.transaction()?;
        let mut imported = 0;
        let mut skipped = 0;
        for group in books {
            let book = &group.book;
            if book.id.trim().is_empty() || book.id == WRITING_ROOT || book.title.trim().is_empty()
            {
                return Err(DeskError::InvalidPath("导入图书缺少名称或编号".into()));
            }
            let book_tags = tags::normalize(&book.tags)?;
            for page in &group.pages {
                if page.source.trim().is_empty() || page.title.trim().is_empty() {
                    return Err(DeskError::InvalidPath("导入文档缺少来源或标题".into()));
                }
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM writing_imports WHERE source=?1)",
                    [&page.source],
                    |r| r.get(0),
                )?;
                if exists {
                    skipped += 1;
                    continue;
                }
                let page_tags = tags::normalize(&page.tags)?;
                ensure_tags(&tx, TagScope::Books, &book_tags)?;
                ensure_tags(&tx, TagScope::BookPages, &page_tags)?;
                tx.execute(
                    "INSERT OR IGNORE INTO books(id,title,tags_json) VALUES(?1,?2,?3)",
                    params![book.id, book.title, serde_json::to_string(&book_tags)?],
                )?;
                let deleted: bool =
                    tx.query_row("SELECT deleted FROM books WHERE id=?1", [&book.id], |r| {
                        r.get(0)
                    })?;
                if deleted {
                    return Err(DeskError::InvalidPath(
                        "导入目标图书已删除，请选择新的图书编号".into(),
                    ));
                }
                let id = uuid::Uuid::new_v4().to_string();
                tx.execute("INSERT INTO book_pages(id,book_id,title,body,tags_json,revision) VALUES(?1,?2,?3,?4,?5,1)", params![id,book.id,page.title,page.body,serde_json::to_string(&page_tags)?])?;
                tx.execute(
                    "INSERT INTO writing_imports(source,page_id) VALUES(?1,?2)",
                    params![page.source, id],
                )?;
                imported += 1;
            }
        }
        tx.commit()?;
        Ok((imported, skipped))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_is_atomic_and_replays_keep_edits_and_deletions() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::open(dir.path()).unwrap();
        let mut input: Vec<WritingImportBook> = serde_json::from_value(serde_json::json!([{
            "book":{"id":"import-fixture","title":"项目笔记","tags":["Obsidian"]},
            "pages":[{"source":"fixture/a.md","title":"文档 A","body":"原文","tags":["工作"]},
                     {"source":"fixture/b.md","title":"","body":"应回滚","tags":[]}]
        }]))
        .unwrap();
        assert!(db.import_writing_books(&input).is_err());
        assert!(db.books().unwrap().is_empty());
        assert!(db.writing_pages().unwrap().is_empty());
        input[0].pages[1].title = "文档 B".into();
        assert_eq!(db.import_writing_books(&input).unwrap(), (2, 0));
        let mut pages = db.book_pages("import-fixture").unwrap();
        pages[0].body = "个人修改".into();
        db.save_book_page(&mut pages[0]).unwrap();
        db.set_library_deleted(&pages[1].id, true, true).unwrap();
        assert_eq!(db.import_writing_books(&input).unwrap(), (0, 2));
        drop(db);
        let db = Database::open(dir.path()).unwrap();
        let pages = db.book_pages("import-fixture").unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].body, "个人修改");
    }
}
