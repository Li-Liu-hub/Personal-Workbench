use super::*;
use crate::library::{Book, Page};
impl Database {
    pub fn book_documents(&self, book: &str) -> Result<Vec<crate::model::DocumentRecord>> {
        let mut statement=self.desk.prepare("SELECT d.path,d.title FROM book_documents b JOIN documents d ON d.path=b.document_path WHERE b.book_id=?1 AND d.hidden=0 ORDER BY b.position,b.rowid")?;
        Ok(statement
            .query_map([book], |r| {
                let path = std::path::PathBuf::from(r.get::<_, String>(0)?);
                let title = visible_document_title(&path, r.get(1)?);
                Ok(crate::model::DocumentRecord { path, title })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn document_book(&self, path: &std::path::Path) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let display = crate::files::shell_path(&normalized);
        Ok(self.desk.query_row("SELECT bd.book_id FROM book_documents bd JOIN live_books b ON b.id=bd.book_id WHERE bd.document_path IN (?1,?2,?3) AND b.deleted=0",params![path.to_string_lossy().as_ref(),normalized.to_string_lossy().as_ref(),display.to_string_lossy().as_ref()],|r|r.get(0)).optional()?)
    }
    /// Group by reference, never move or rewrite the document files.
    #[cfg(test)]
    pub fn group_documents(
        &mut self,
        paths: &[std::path::PathBuf],
        target: Option<&str>,
    ) -> Result<Book> {
        let mut keys = vec![];
        for path in paths {
            if !path.is_file() {
                return Err(DeskError::InvalidPath("文档已不存在".into()));
            }
            use rusqlite::OptionalExtension;
            let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let display = crate::files::shell_path(&normalized);
            let key: String = self
                .desk
                .query_row(
                    "SELECT path FROM documents WHERE path IN (?1,?2,?3) LIMIT 1",
                    params![
                        path.to_string_lossy().as_ref(),
                        normalized.to_string_lossy().as_ref(),
                        display.to_string_lossy().as_ref()
                    ],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| DeskError::InvalidPath("请先将文档加入文档列表".into()))?;
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        let book = if let Some(id) = target {
            self.books()?
                .into_iter()
                .find(|b| b.id == id)
                .ok_or_else(|| DeskError::InvalidPath("图书已不存在".into()))?
        } else {
            if keys.len() < 2 {
                return Err(DeskError::InvalidPath(
                    "拖动一份文档到另一份文档上即可组成图书".into(),
                ));
            }
            let mut book = Book::new();
            book.title = "新建图书".into();
            book
        };
        let tx = self.desk.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO books(id,title,tags_json) VALUES(?1,?2,?3)",
            params![book.id, book.title, serde_json::to_string(&book.tags)?],
        )?;
        for key in keys {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE path=?1)",
                [&key],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(DeskError::InvalidPath("文档记录已不存在".into()));
            }
            tx.execute("INSERT INTO book_documents(book_id,document_path,position) VALUES(?1,?2,(SELECT COALESCE(MAX(position),0)+1 FROM book_documents WHERE book_id=?1)) ON CONFLICT(document_path) DO UPDATE SET book_id=excluded.book_id,position=excluded.position",params![book.id,key])?;
        }
        tx.commit()?;
        Ok(book)
    }

    #[cfg(test)]
    pub fn deleted_library(&self) -> Result<Vec<(String, String, bool)>> {
        let mut result = vec![];
        for (table, page) in [("books", false), ("book_pages", true)] {
            let mut stmt = self.desk.prepare(&format!(
                "SELECT id,title FROM {table} WHERE deleted=1 ORDER BY rowid DESC"
            ))?;
            for row in stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, page)))? {
                result.push(row?);
            }
        }
        Ok(result)
    }

    pub fn save_book_draft(&mut self, writer: &str, page: &Page) -> Result<()> {
        let key = format!("{writer}:{}", page.id);
        self.desk.execute("INSERT INTO book_page_drafts(id,book_id,writer,page_json) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET book_id=excluded.book_id,page_json=excluded.page_json",params![key,page.book_id,writer,serde_json::to_string(page)?])?;
        Ok(())
    }

    #[cfg(test)]
    pub fn book_drafts(&self, book_id: &str, writer: &str) -> Result<Vec<(String, Page)>> {
        let mut stmt = self
            .desk
            .prepare("SELECT id,page_json FROM book_page_drafts WHERE book_id=?1 AND writer<>?2")?;
        let rows = stmt
            .query_map(params![book_id, writer], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, json)| Ok((id, serde_json::from_str(&json)?)))
            .collect()
    }

    pub fn move_book(&mut self, id: &str, parent: Option<&str>) -> Result<()> {
        let tx = self.desk.transaction()?;
        let live: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM live_books WHERE id=?1 AND id<>?2)",
            params![id, WRITING_ROOT],
            |r| r.get(0),
        )?;
        if !live {
            return Err(DeskError::InvalidInput("图书已不存在".into()));
        }
        if let Some(parent) = parent {
            let valid: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM live_books WHERE id=?1 AND id<>?2)",
                params![parent, WRITING_ROOT],
                |r| r.get(0),
            )?;
            let cycle: bool = tx.query_row("WITH RECURSIVE subtree(id) AS (SELECT ?1 UNION SELECT b.id FROM books b JOIN subtree s ON b.parent_id=s.id) SELECT EXISTS(SELECT 1 FROM subtree WHERE id=?2)", params![id,parent], |r| r.get(0))?;
            if !valid || cycle {
                return Err(DeskError::InvalidInput(
                    "不能将图书放入自身或其子图书".into(),
                ));
            }
        }
        tx.execute(
            "UPDATE books SET parent_id=?1,updated_at=CURRENT_TIMESTAMP WHERE id=?2",
            params![parent, id],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn books(&self) -> Result<Vec<Book>> {
        let mut statement=self.desk.prepare("SELECT id,title,tags_json,parent_id FROM live_books WHERE deleted=0 AND id<>'__personal_writing__' ORDER BY updated_at DESC,rowid DESC")?;
        Ok(statement
            .query_map([], |r| {
                Ok(Book {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    tags: read_tags(r, 2)?,
                    parent_id: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn save_book(&mut self, book: &Book) -> Result<()> {
        if book.title.trim().is_empty() {
            return Err(DeskError::InvalidPath("请填写图书名称".into()));
        }
        if book.id == WRITING_ROOT || book.parent_id.as_deref() == Some(&book.id) {
            return Err(DeskError::InvalidInput("不能将图书放入自身".into()));
        }
        if let Some(parent) = &book.parent_id
            && !self.books()?.iter().any(|b| &b.id == parent)
        {
            return Err(DeskError::InvalidInput("上级图书已不存在".into()));
        }
        let tags = tags::normalize(&book.tags)?;
        let tx = self.desk.transaction()?;
        ensure_tags(&tx, TagScope::Books, &tags)?;
        tx.execute("INSERT INTO books(id,title,tags_json,parent_id) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET title=excluded.title,tags_json=excluded.tags_json,updated_at=CURRENT_TIMESTAMP",params![book.id,book.title,serde_json::to_string(&tags)?,book.parent_id])?;
        tx.commit()?;
        Ok(())
    }
    pub fn book_pages(&self, book_id: &str) -> Result<Vec<Page>> {
        let mut statement=self.desk.prepare("SELECT id,book_id,title,body,tags_json,revision,updated_at FROM book_pages WHERE book_id=?1 AND deleted=0 AND EXISTS(SELECT 1 FROM live_books WHERE id=book_id AND deleted=0) ORDER BY rowid")?;
        Ok(statement
            .query_map([book_id], |r| {
                Ok(Page {
                    id: r.get(0)?,
                    book_id: r.get(1)?,
                    title: r.get(2)?,
                    body: r.get(3)?,
                    tags: read_tags(r, 4)?,
                    revision: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn save_book_page(&mut self, page: &mut Page) -> Result<()> {
        let tx = self.desk.transaction()?;
        write_page(&tx, page)?;
        tx.commit()?;
        page_saved(page);
        Ok(())
    }
    pub fn set_library_deleted(&mut self, id: &str, page: bool, deleted: bool) -> Result<()> {
        if !page && id == WRITING_ROOT {
            return Err(DeskError::InvalidPath("不能删除文档根目录".into()));
        }
        let table = if page { "book_pages" } else { "books" };
        let tx = self.desk.transaction()?;
        if !page {
            // Both deletion and restoration invalidate editors opened before this transition.
            tx.execute("WITH RECURSIVE subtree(id) AS (SELECT ?1 UNION SELECT b.id FROM books b JOIN subtree s ON b.parent_id=s.id) UPDATE book_pages SET revision=revision+1 WHERE book_id IN (SELECT id FROM subtree) AND EXISTS(SELECT 1 FROM books WHERE id=?1 AND deleted<>?2)", params![id, deleted])?;
        }
        let revision = if page { ",revision=revision+1" } else { "" };
        tx.execute(
            &format!("UPDATE {table} SET deleted=?1{revision} WHERE id=?2 AND deleted<>?1"),
            params![deleted as i64, id],
        )?;
        tx.commit()?;
        Ok(())
    }
}

/// Shared transaction boundary for ordinary saves and saves that also clear a draft.
pub(super) fn write_page(tx: &rusqlite::Transaction<'_>, page: &Page) -> Result<()> {
    if page.id.trim().is_empty()
        || page.title.trim().is_empty()
        || page.revision < 0
        || page.revision == i64::MAX
    {
        return Err(DeskError::InvalidPath(
            "请填写页面编号、名称及有效修订号".into(),
        ));
    }
    let live_book: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM live_books WHERE id=?1 AND deleted=0)",
        [&page.book_id],
        |r| r.get(0),
    )?;
    if !live_book {
        return Err(DeskError::ConcurrentModification);
    }
    let tags = tags::normalize(&page.tags)?;
    ensure_tags(tx, TagScope::BookPages, &tags)?;
    if page.revision == 0 {
        tx.execute("INSERT INTO book_pages(id,book_id,title,body,tags_json,revision) VALUES(?1,?2,?3,?4,?5,1)",params![page.id,page.book_id,page.title,page.body,serde_json::to_string(&tags)?])?;
    } else {
        tx.execute("INSERT OR IGNORE INTO book_page_versions(page_id,revision,title,body,tags_json) SELECT id,revision,title,body,tags_json FROM book_pages WHERE id=?1 AND revision=?2 AND deleted=0",params![page.id,page.revision])?;
        let updated=tx.execute("UPDATE book_pages SET title=?1,body=?2,tags_json=?3,revision=revision+1,updated_at=CURRENT_TIMESTAMP WHERE id=?4 AND revision=?5 AND book_id=?6 AND deleted=0",params![page.title,page.body,serde_json::to_string(&tags)?,page.id,page.revision,page.book_id])?;
        if updated != 1 {
            return Err(DeskError::ConcurrentModification);
        }
    }
    Ok(())
}

pub(super) fn page_saved(page: &mut Page) {
    page.revision += 1;
    page.updated_at = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
}
