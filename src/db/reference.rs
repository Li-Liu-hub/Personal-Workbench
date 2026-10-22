use super::*;
use crate::reference::{Entry, Filter, Summary};

const SELECT: &str = "SELECT id,language,category,title,summary,syntax,example,notes,source,revision FROM command_entries";
fn entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        language: row.get(1)?,
        category: row.get(2)?,
        title: row.get(3)?,
        summary: row.get(4)?,
        syntax: row.get(5)?,
        example: row.get(6)?,
        notes: row.get(7)?,
        source: row.get(8)?,
        revision: row.get(9)?,
    })
}
impl Database {
    pub(super) fn prepare_reference(&mut self) -> Result<()> {
        self.desk.execute_batch("CREATE TABLE IF NOT EXISTS command_entries(
            id TEXT PRIMARY KEY,language TEXT NOT NULL,category TEXT NOT NULL,title TEXT NOT NULL,
            summary TEXT NOT NULL,syntax TEXT NOT NULL,example TEXT NOT NULL,notes TEXT NOT NULL,source TEXT NOT NULL,
            revision INTEGER NOT NULL DEFAULT 1,deleted INTEGER NOT NULL DEFAULT 0,search_text TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS command_entries_category ON command_entries(deleted,language,category);
            CREATE VIRTUAL TABLE IF NOT EXISTS command_entries_fts USING fts5(search_text,content='command_entries',content_rowid='rowid',tokenize='trigram');
            CREATE TRIGGER IF NOT EXISTS command_entries_ai AFTER INSERT ON command_entries BEGIN
                INSERT INTO command_entries_fts(rowid,search_text) VALUES(new.rowid,new.search_text); END;
            CREATE TRIGGER IF NOT EXISTS command_entries_ad AFTER DELETE ON command_entries BEGIN
                INSERT INTO command_entries_fts(command_entries_fts,rowid,search_text) VALUES('delete',old.rowid,old.search_text); END;
            CREATE TRIGGER IF NOT EXISTS command_entries_au AFTER UPDATE OF search_text ON command_entries BEGIN
                INSERT INTO command_entries_fts(command_entries_fts,rowid,search_text) VALUES('delete',old.rowid,old.search_text);
                INSERT INTO command_entries_fts(rowid,search_text) VALUES(new.rowid,new.search_text); END;")?;
        if self.setting("reference.seed.version")?.as_deref()
            == Some(crate::reference::SEED_VERSION)
        {
            return Ok(());
        }
        let entries: Vec<Entry> =
            serde_json::from_str(include_str!("../../assets/reference.json"))?;
        for item in &entries {
            item.validate()?;
        }
        let tx = self.desk.transaction()?;
        {
            let mut statement = tx.prepare_cached("INSERT OR IGNORE INTO command_entries(id,language,category,title,summary,syntax,example,notes,source,search_text) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)")?;
            for item in entries {
                statement.execute(params![
                    item.id,
                    item.language,
                    item.category,
                    item.title,
                    item.summary,
                    item.syntax,
                    item.example,
                    item.notes,
                    item.source,
                    item.search_text()
                ])?;
            }
        }
        tx.execute("INSERT INTO settings(key,value) VALUES('reference.seed.version',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [crate::reference::SEED_VERSION])?;
        tx.commit()?;
        Ok(())
    }
    pub fn reference_entry(&self, id: &str) -> Result<Option<Entry>> {
        Ok(self
            .desk
            .prepare_cached(&format!("{SELECT} WHERE id=?1 AND deleted=0"))?
            .query_row([id], entry)
            .optional()?)
    }
    pub fn reference_entries(&self) -> Result<Vec<Entry>> {
        Ok(self
            .desk
            .prepare(&format!(
                "{SELECT} WHERE deleted=0 ORDER BY language,category,title,id"
            ))?
            .query_map([], entry)?
            .collect::<std::result::Result<_, _>>()?)
    }
    pub fn reference_facets(&self) -> Result<Vec<(String, String)>> {
        Ok(self.desk.prepare_cached("SELECT DISTINCT language,category FROM command_entries WHERE deleted=0 ORDER BY language,category")?.query_map([], |r| Ok((r.get(0)?,r.get(1)?)))?.collect::<std::result::Result<_,_>>()?)
    }
    pub fn search_reference(&self, filter: &Filter) -> Result<Vec<Summary>> {
        let mut query = "SELECT id,title,language,category,summary FROM command_entries WHERE deleted=0 AND (?1='' OR language=?1) AND (?2='' OR category=?2)".to_owned();
        let mut values = vec![filter.language.clone(), filter.category.clone()];
        for term in filter.terms() {
            values.push(if term.chars().count() >= 3 {
                format!("\"{}\"", term.replace('"', "\"\""))
            } else {
                format!(
                    "%{}%",
                    term.replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                )
            });
            if term.chars().count() >= 3 {
                query.push_str(&format!(" AND rowid IN(SELECT rowid FROM command_entries_fts WHERE command_entries_fts MATCH ?{})",values.len()));
            } else {
                query.push_str(&format!(
                    " AND search_text LIKE ?{} ESCAPE '\\'",
                    values.len()
                ));
            }
        }
        query.push_str(" ORDER BY language,category,title,id");
        Ok(self
            .desk
            .prepare_cached(&query)?
            .query_map(rusqlite::params_from_iter(values.iter()), |r| {
                Ok(Summary {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    language: r.get(2)?,
                    category: r.get(3)?,
                    summary: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?)
    }
    pub fn save_reference(&mut self, item: &Entry) -> Result<Entry> {
        item.validate()?;
        let tx = self.desk.transaction()?;
        if item.revision == 0 {
            tx.execute("INSERT INTO command_entries(id,language,category,title,summary,syntax,example,notes,source,search_text) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![item.id,item.language.trim(),item.category.trim(),item.title.trim(),item.summary,item.syntax,item.example,item.notes,item.source,item.search_text()])?;
        } else if tx.execute("UPDATE command_entries SET language=?2,category=?3,title=?4,summary=?5,syntax=?6,example=?7,notes=?8,source=?9,search_text=?10,revision=revision+1 WHERE id=?1 AND revision=?11 AND deleted=0",params![item.id,item.language.trim(),item.category.trim(),item.title.trim(),item.summary,item.syntax,item.example,item.notes,item.source,item.search_text(),item.revision])? == 0 {
            return Err(DeskError::ConcurrentModification);
        }
        tx.commit()?;
        self.reference_entry(&item.id)?
            .ok_or(DeskError::ConcurrentModification)
    }
    pub fn delete_reference(&mut self, item: &Entry) -> Result<()> {
        if self.desk.execute("UPDATE command_entries SET deleted=1,revision=revision+1 WHERE id=?1 AND revision=?2 AND deleted=0",params![item.id,item.revision])? == 0 { return Err(DeskError::ConcurrentModification); }
        Ok(())
    }
}
