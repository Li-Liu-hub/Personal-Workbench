use super::*;
use crate::reference::{Entry, Filter, Summary};

const SELECT: &str = "SELECT id,language,category,title,summary,syntax,example,notes,source,revision,entry_type,tags_json FROM command_entries";
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
        entry_type: row.get(10)?,
        tags: serde_json::from_str(&row.get::<_, String>(11)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(e))
        })?,
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
        let columns = self
            .desk
            .prepare("PRAGMA table_info(command_entries)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if !columns.iter().any(|c| c == "entry_type") {
            self.desk.execute_batch(
                "ALTER TABLE command_entries ADD COLUMN entry_type TEXT NOT NULL DEFAULT '语法';",
            )?;
        }
        if !columns.iter().any(|c| c == "tags_json") {
            self.desk.execute_batch(
                "ALTER TABLE command_entries ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]';",
            )?;
        }
        if self.setting("reference.seed.version")?.as_deref()
            == Some(crate::reference::SEED_VERSION)
        {
            if self.setting("reference.practice.version")?.as_deref()
                != Some(crate::reference::SEED_VERSION)
            {
                let cards: Vec<crate::study::Card> =
                    serde_json::from_str(include_str!("../../assets/reference_practice.json"))?;
                self.import_study_cards(&cards)?;
                self.set_setting("reference.practice.version", crate::reference::SEED_VERSION)?;
            }
            return self.prepare_learning_quality();
        }
        let entries: Vec<Entry> =
            serde_json::from_str(include_str!("../../assets/reference.json"))?;
        for item in &entries {
            item.validate()?;
        }
        let tx = self.desk.transaction()?;
        {
            let mut statement = tx.prepare_cached("INSERT OR IGNORE INTO command_entries(id,language,category,title,summary,syntax,example,notes,source,search_text,entry_type,tags_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)")?;
            for item in entries {
                // Only enrich untouched built-ins; never overwrite personal edits or tombstones.
                tx.execute("UPDATE command_entries SET entry_type=?2,tags_json=?3,search_text=?4 WHERE id=?1 AND revision=1 AND deleted=0 AND tags_json='[]'", params![item.id,item.entry_type,serde_json::to_string(&item.tags)?,item.indexed_text()])?;
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
                    item.indexed_text(),
                    item.entry_type,
                    serde_json::to_string(&item.tags)?
                ])?;
            }
        }
        tx.execute("INSERT INTO settings(key,value) VALUES('reference.seed.version',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [crate::reference::SEED_VERSION])?;
        tx.commit()?;
        let cards: Vec<crate::study::Card> =
            serde_json::from_str(include_str!("../../assets/reference_practice.json"))?;
        self.import_study_cards(&cards)?;
        // The marker follows both imports, so a crash can replay without duplicating anything.
        self.set_setting("reference.practice.version", crate::reference::SEED_VERSION)?;
        self.prepare_learning_quality()
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
    pub fn reference_metadata(&self, language: &str) -> Result<(Vec<String>, Vec<String>)> {
        let types = self.desk.prepare_cached("SELECT DISTINCT entry_type FROM command_entries WHERE deleted=0 AND (?1='' OR language=?1) ORDER BY entry_type")?.query_map([language], |r| r.get(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let tags = self.desk.prepare_cached("SELECT DISTINCT j.value FROM command_entries c,json_each(c.tags_json) j WHERE c.deleted=0 AND (?1='' OR c.language=?1) ORDER BY j.value")?.query_map([language], |r| r.get(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        Ok((types, tags))
    }
    pub fn search_reference(&self, filter: &Filter) -> Result<Vec<Summary>> {
        let mut query = "SELECT id,title,language,category,summary FROM command_entries WHERE deleted=0 AND (?1='' OR language=?1) AND (?2='' OR category=?2)".to_owned();
        let mut values = vec![filter.language.clone(), filter.category.clone()];
        for (column, value) in [("entry_type", &filter.entry_type), ("tag", &filter.tag)] {
            if !value.is_empty() {
                values.push(value.clone());
                if column == "tag" {
                    query.push_str(&format!(" AND EXISTS(SELECT 1 FROM json_each(command_entries.tags_json) WHERE value=?{})", values.len()));
                } else {
                    query.push_str(&format!(" AND entry_type=?{}", values.len()));
                }
            }
        }
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
            tx.execute("INSERT INTO command_entries(id,language,category,title,summary,syntax,example,notes,source,search_text,entry_type,tags_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![item.id,item.language.trim(),item.category.trim(),item.title.trim(),item.summary,item.syntax,item.example,item.notes,item.source,item.indexed_text(),item.entry_type,serde_json::to_string(&item.tags)?])?;
        } else if tx.execute("UPDATE command_entries SET language=?2,category=?3,title=?4,summary=?5,syntax=?6,example=?7,notes=?8,source=?9,search_text=?10,entry_type=?12,tags_json=?13,revision=revision+1 WHERE id=?1 AND revision=?11 AND deleted=0",params![item.id,item.language.trim(),item.category.trim(),item.title.trim(),item.summary,item.syntax,item.example,item.notes,item.source,item.indexed_text(),item.revision,item.entry_type,serde_json::to_string(&item.tags)?])? == 0 {
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
