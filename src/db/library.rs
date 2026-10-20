use super::*;
use crate::library::{Book, Page};
impl Database {
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
        self.desk.execute("INSERT INTO book_page_drafts(id,book_id,writer,page_json) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET page_json=excluded.page_json",params![key,page.book_id,writer,serde_json::to_string(page)?])?;
        Ok(())
    }
    pub fn clear_book_draft(&mut self, writer: &str, page_id: &str) -> Result<()> {
        self.clear_book_draft_key(&format!("{writer}:{page_id}"))
    }
    pub fn clear_book_draft_key(&mut self, key: &str) -> Result<()> {
        self.desk
            .execute("DELETE FROM book_page_drafts WHERE id=?1", [key])?;
        Ok(())
    }
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

    pub fn books(&self) -> Result<Vec<Book>> {
        let mut statement=self.desk.prepare("SELECT id,title,tags_json FROM books WHERE deleted=0 ORDER BY updated_at DESC,rowid DESC")?;
        Ok(statement
            .query_map([], |r| {
                Ok(Book {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    tags: read_tags(r, 2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn save_book(&mut self, book: &Book) -> Result<()> {
        if book.title.trim().is_empty() {
            return Err(DeskError::InvalidPath("请填写图书名称".into()));
        }
        let tags = tags::normalize(&book.tags)?;
        let tx = self.desk.transaction()?;
        ensure_tags(&tx, TagScope::Books, &tags)?;
        tx.execute("INSERT INTO books(id,title,tags_json) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET title=excluded.title,tags_json=excluded.tags_json,updated_at=CURRENT_TIMESTAMP",params![book.id,book.title,serde_json::to_string(&tags)?])?;
        tx.commit()?;
        Ok(())
    }
    pub fn book_pages(&self, book_id: &str) -> Result<Vec<Page>> {
        let mut statement=self.desk.prepare("SELECT id,book_id,title,body,tags_json,revision,updated_at FROM book_pages WHERE book_id=?1 AND deleted=0 ORDER BY rowid")?;
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
        if page.title.trim().is_empty() {
            return Err(DeskError::InvalidPath("请填写页面名称".into()));
        }
        let tags = tags::normalize(&page.tags)?;
        let tx = self.desk.transaction()?;
        ensure_tags(&tx, TagScope::BookPages, &tags)?;
        if page.revision == 0 {
            tx.execute("INSERT INTO book_pages(id,book_id,title,body,tags_json,revision) VALUES(?1,?2,?3,?4,?5,1)",params![page.id,page.book_id,page.title,page.body,serde_json::to_string(&tags)?])?;
        } else {
            tx.execute("INSERT OR IGNORE INTO book_page_versions(page_id,revision,title,body,tags_json) SELECT id,revision,title,body,tags_json FROM book_pages WHERE id=?1 AND revision=?2 AND deleted=0",params![page.id,page.revision])?;
            let updated=tx.execute("UPDATE book_pages SET title=?1,body=?2,tags_json=?3,revision=revision+1,updated_at=CURRENT_TIMESTAMP WHERE id=?4 AND revision=?5 AND deleted=0",params![page.title,page.body,serde_json::to_string(&tags)?,page.id,page.revision])?;
            if updated != 1 {
                return Err(DeskError::ConcurrentModification);
            }
        }
        tx.commit()?;
        page.revision += 1;
        page.updated_at = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        Ok(())
    }
    pub fn set_library_deleted(&mut self, id: &str, page: bool, deleted: bool) -> Result<()> {
        let table = if page { "book_pages" } else { "books" };
        self.desk.execute(
            &format!("UPDATE {table} SET deleted=?1 WHERE id=?2"),
            params![deleted as i64, id],
        )?;
        Ok(())
    }
}
