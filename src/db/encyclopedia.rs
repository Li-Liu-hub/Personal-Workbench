//! Additive catalog expansion. Stable IDs and tombstones preserve personal work.
use super::*;
use crate::{reference::Entry, study::Card};

impl Database {
    pub(super) fn prepare_encyclopedia(&mut self) -> Result<()> {
        if self.setting("encyclopedia.ecosystem.version")?.as_deref() == Some("1") {
            return Ok(());
        }
        let entries: Vec<Entry> =
            serde_json::from_str(include_str!("../../assets/java_ecosystem.json"))?;
        let cards: Vec<Card> =
            serde_json::from_str(include_str!("../../assets/java_ecosystem_practice.json"))?;
        for entry in &entries {
            entry.validate()?;
        }
        for card in &cards {
            card.validate()?;
        }
        let tx = self.desk.transaction()?;
        for item in entries {
            tx.execute("INSERT OR IGNORE INTO command_entries(id,language,category,title,summary,syntax,example,notes,source,search_text,entry_type,tags_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![item.id,item.language,item.category,item.title,item.summary,item.syntax,item.example,item.notes,item.source,item.indexed_text(),item.entry_type,serde_json::to_string(&item.tags)?])?;
        }
        tx.commit()?;
        // Import and marker are replayable: INSERT OR IGNORE never rewrites edits or history.
        self.import_study_cards(&cards)?;
        self.set_setting("encyclopedia.ecosystem.version", "1")?;
        Ok(())
    }
    pub fn reference_count(&self, language: &str) -> Result<usize> {
        Ok(self.desk.query_row(
            "SELECT COUNT(*) FROM command_entries WHERE deleted=0 AND (?1='' OR language=?1)",
            [language],
            |r| r.get::<_, i64>(0),
        )? as usize)
    }
}
