use super::*;
use crate::library::{Book, Page};
pub(crate) const WRITING_ROOT: &str = "__personal_writing__";

impl Database {
    pub(super) fn prepare_writing(&mut self, data: &Path) -> Result<()> {
        self.desk.execute_batch("CREATE TABLE IF NOT EXISTS writing_imports(source TEXT PRIMARY KEY,page_id TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS writing_logs(day TEXT PRIMARY KEY,page_id TEXT NOT NULL);")?;
        // Only copy legacy managed text files. External indices remain untouched.
        let root = data.join("documents");
        if root.is_dir() {
            let records = self.document_records()?;
            for entry in walkdir::WalkDir::new(&root).follow_links(false) {
                let Ok(entry) = entry else { continue };
                if !entry.file_type().is_file()
                    || entry
                        .metadata()
                        .map(|m| m.len() > 8 * 1024 * 1024)
                        .unwrap_or(true)
                {
                    continue;
                }
                if !matches!(
                    entry
                        .path()
                        .extension()
                        .and_then(|v| v.to_str())
                        .map(str::to_lowercase)
                        .as_deref(),
                    Some("md" | "txt" | "markdown")
                ) {
                    continue;
                }
                let Ok(body) = std::fs::read_to_string(entry.path()) else {
                    continue;
                };
                let source = format!(
                    "managed:{}",
                    entry
                        .path()
                        .strip_prefix(&root)
                        .unwrap_or(entry.path())
                        .to_string_lossy()
                );
                if self.writing_imported(&source)? {
                    continue;
                }
                let record = records
                    .iter()
                    .find(|r| r.path.canonicalize().ok() == entry.path().canonicalize().ok());
                let title = record.map(|r| r.title.clone()).unwrap_or_else(|| {
                    entry
                        .path()
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into()
                });
                let book = self
                    .document_book(entry.path())?
                    .unwrap_or_else(|| WRITING_ROOT.into());
                let mut page = Page::new(book);
                page.title = title;
                page.body = body;
                if let Some(record) = record {
                    page.tags = self
                        .cached_record_tags(TagScope::Documents, &record.path.to_string_lossy())?;
                }
                self.import_writing_copy(&source, &page)?;
            }
        }
        let recovery = data.join("recovery");
        if let Ok(entries) = std::fs::read_dir(&recovery) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|v| v.to_str()) != Some("json") {
                    continue;
                }
                let Some(id) = path.file_stem().and_then(|v| v.to_str()) else {
                    continue;
                };
                if Uuid::parse_str(id).is_err() {
                    continue;
                }
                let draft = recovery.join(format!("{id}.recovery.txt"));
                if std::fs::symlink_metadata(&draft)
                    .map(|m| {
                        m.file_type().is_symlink() || !m.is_file() || m.len() > 8 * 1024 * 1024
                    })
                    .unwrap_or(true)
                {
                    continue;
                }
                let source = format!("legacy-recovery:{id}");
                if self.writing_imported(&source)? {
                    continue;
                }
                let Ok(body) = std::fs::read_to_string(&draft) else {
                    continue;
                };
                let title = std::fs::read(&path)
                    .ok()
                    .and_then(|v| serde_json::from_slice::<serde_json::Value>(&v).ok())
                    .and_then(|v| {
                        v.get("original_path").and_then(|v| v.as_str()).map(|v| {
                            Path::new(v)
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        })
                    })
                    .unwrap_or_else(|| "文档".into());
                let mut page = Page::new(WRITING_ROOT.into());
                page.title = format!("{title}（恢复草稿）");
                page.body = body;
                self.import_writing_copy(&source, &page)?;
            }
        }
        Ok(())
    }
    pub fn recover_writing_drafts(&mut self) -> Result<()> {
        // Old and interrupted drafts are recovered as separate documents. Never overwrite a saved page.
        let drafts = self
            .desk
            .prepare("SELECT id,page_json FROM book_page_drafts")?
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (key, json) in drafts {
            let Ok(mut page) = serde_json::from_str::<Page>(&json) else {
                continue;
            };
            if self.writing_page(&page.id)?.is_some_and(|saved| {
                saved.title == page.title && saved.body == page.body && saved.tags == page.tags
            }) {
                continue;
            }
            use sha2::Digest;
            let source = format!("draft:{key}:{:x}", sha2::Sha256::digest(json.as_bytes()));
            if self.writing_imported(&source)? {
                continue;
            }
            page.id = Uuid::new_v4().to_string();
            page.revision = 0;
            page.title = format!("{}（恢复草稿）", page.title);
            self.import_writing_copy(&source, &page)?;
        }
        Ok(())
    }
    fn writing_imported(&self, source: &str) -> Result<bool> {
        Ok(self.desk.query_row(
            "SELECT EXISTS(SELECT 1 FROM writing_imports WHERE source=?1)",
            [source],
            |r| r.get(0),
        )?)
    }
    fn import_writing_copy(&mut self, source: &str, page: &Page) -> Result<()> {
        let tx = self.desk.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO books(id,title) VALUES(?1,'文档')",
            [WRITING_ROOT],
        )?;
        let book: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM books WHERE id=?1 AND deleted=0)",
            [&page.book_id],
            |r| r.get(0),
        )?;
        let book = if book {
            page.book_id.as_str()
        } else {
            WRITING_ROOT
        };
        ensure_tags(&tx, TagScope::BookPages, &page.tags)?;
        tx.execute("INSERT INTO book_pages(id,book_id,title,body,tags_json,revision) VALUES(?1,?2,?3,?4,?5,1)",params![page.id,book,page.title,page.body,serde_json::to_string(&page.tags)?])?;
        tx.execute(
            "INSERT INTO writing_imports(source,page_id) VALUES(?1,?2)",
            params![source, page.id],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn create_writing(
        &mut self,
        title: &str,
        book: Option<&str>,
        day: Option<&str>,
    ) -> Result<Page> {
        if let Some(day) = day {
            let existing = self
                .desk
                .query_row(
                    "SELECT page_id FROM writing_logs WHERE day=?1",
                    [day],
                    |r| r.get::<_, String>(0),
                )
                .optional()?;
            if let Some(id) = existing
                && let Some(page) = self.writing_page(&id)?
            {
                return Ok(page);
            }
        }
        let mut page = Page::new(book.unwrap_or(WRITING_ROOT).into());
        page.title = title.into();
        let tx = self.desk.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO books(id,title) VALUES(?1,'文档')",
            [WRITING_ROOT],
        )?;
        tx.execute(
            "INSERT INTO book_pages(id,book_id,title,body,revision) VALUES(?1,?2,?3,'',1)",
            params![page.id, page.book_id, page.title],
        )?;
        if let Some(day) = day {
            tx.execute("INSERT INTO writing_logs(day,page_id) VALUES(?1,?2) ON CONFLICT(day) DO UPDATE SET page_id=excluded.page_id",params![day,page.id])?;
        }
        tx.commit()?;
        page.revision = 1;
        Ok(page)
    }
    pub fn writing_pages(&self) -> Result<Vec<Page>> {
        let mut statement=self.desk.prepare("SELECT p.id,p.book_id,p.title,p.body,p.tags_json,p.revision,p.updated_at FROM book_pages p JOIN books b ON b.id=p.book_id WHERE p.deleted=0 AND b.deleted=0 ORDER BY p.updated_at DESC,p.rowid DESC")?;
        Ok(statement
            .query_map([], |r| {
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
    pub fn change_count(&self) -> u64 {
        self.desk.total_changes()
    }
    pub fn writing_summaries(&self, query: &str) -> Result<Vec<crate::library::WritingSummary>> {
        let mut s=self.desk.prepare("SELECT p.id,p.book_id,p.title,substr(p.body,1,120),p.tags_json,p.updated_at FROM book_pages p JOIN books b ON b.id=p.book_id WHERE p.deleted=0 AND b.deleted=0 AND instr(lower(p.title||char(10)||p.body||p.tags_json),lower(?1))>0 ORDER BY p.updated_at DESC,p.rowid DESC")?;
        Ok(s.query_map([query.trim()], |r| {
            Ok(crate::library::WritingSummary {
                id: r.get(0)?,
                book_id: r.get(1)?,
                title: r.get(2)?,
                preview: r.get(3)?,
                tags: read_tags(r, 4)?,
                updated_at: r.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn writing_page(&self, id: &str) -> Result<Option<Page>> {
        Ok(self.desk.query_row("SELECT id,book_id,title,body,tags_json,revision,updated_at FROM book_pages WHERE id=?1 AND deleted=0",[id],|r|Ok(Page{id:r.get(0)?,book_id:r.get(1)?,title:r.get(2)?,body:r.get(3)?,tags:read_tags(r,4)?,revision:r.get(5)?,updated_at:r.get(6)?})).optional()?)
    }
    pub fn finish_writing_save(&mut self, writer: &str, page: &mut Page) -> Result<()> {
        self.save_book_page(page)?;
        self.desk.execute(
            "DELETE FROM book_page_drafts WHERE id=?1",
            [format!("{writer}:{}", page.id)],
        )?;
        Ok(())
    }
    pub fn group_writing(&mut self, ids: &[String], target: Option<&str>) -> Result<Book> {
        let mut unique = Vec::new();
        for id in ids {
            if !unique.contains(id) {
                unique.push(id.clone());
            }
        }
        if unique.is_empty() || (target.is_none() && unique.len() < 2) {
            return Err(DeskError::InvalidPath("请选择要归组的文档".into()));
        }
        let book = if let Some(id) = target {
            self.books()?
                .into_iter()
                .find(|b| b.id == id)
                .ok_or_else(|| DeskError::InvalidPath("图书已不存在".into()))?
        } else {
            let mut b = Book::new();
            b.title = "新建图书".into();
            b
        };
        let tx = self.desk.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO books(id,title,tags_json) VALUES(?1,?2,?3)",
            params![book.id, book.title, serde_json::to_string(&book.tags)?],
        )?;
        for id in unique {
            let current: Option<String> = tx
                .query_row(
                    "SELECT book_id FROM book_pages WHERE id=?1 AND deleted=0",
                    [&id],
                    |r| r.get(0),
                )
                .optional()?;
            if current.as_deref() == Some(&book.id) {
                continue;
            }
            if tx.execute(
                "UPDATE book_pages SET book_id=?1,revision=revision+1 WHERE id=?2 AND deleted=0",
                params![book.id, id],
            )? != 1
            {
                return Err(DeskError::InvalidPath("文档已不存在".into()));
            }
        }
        tx.commit()?;
        Ok(book)
    }
}
