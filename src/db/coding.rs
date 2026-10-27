use super::*;
impl Database {
    pub(super) fn prepare_coding(&mut self) -> Result<()> {
        if self.setting("study.coding.version")?.as_deref() != Some("1") {
            self.import_study_cards(&crate::coding::cards())?;
            self.set_setting("study.coding.version", "1")?;
        }
        Ok(())
    }
    pub(crate) fn knowledge_cards(&self, id: &str) -> Result<Vec<crate::study::Card>> {
        let rows = self.desk.prepare_cached("SELECT c.card_json FROM study_cards c JOIN study_knowledge k ON k.card_id=c.id WHERE k.knowledge_id=?1 ORDER BY c.id")?.query_map([id], |r| r.get::<_, String>(0))?.collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|row| serde_json::from_str(&row).map_err(Into::into))
            .collect()
    }
}
