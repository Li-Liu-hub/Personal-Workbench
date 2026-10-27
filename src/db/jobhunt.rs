use super::*;
use crate::jobhunt::Site;

impl Database {
    pub(super) fn init_jobhunt(&self) -> Result<()> {
        self.desk.execute_batch("CREATE TABLE IF NOT EXISTS jobhunt_sites(id TEXT PRIMARY KEY,payload TEXT NOT NULL,revision INTEGER NOT NULL,deleted INTEGER NOT NULL DEFAULT 0);")?;
        Ok(())
    }
    pub fn jobhunt_sites(&self) -> Result<Vec<Site>> {
        let mut stmt = self.desk.prepare(
            "SELECT payload,revision FROM jobhunt_sites WHERE deleted=0 ORDER BY rowid DESC",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        let mut sites = Vec::new();
        for row in rows {
            let (payload, revision) = row?;
            let mut site: Site = serde_json::from_str(&payload)?;
            site.revision = revision;
            sites.push(site);
        }
        Ok(sites)
    }
    pub fn save_jobhunt_site(&mut self, site: &mut Site) -> Result<()> {
        let mut next = site.clone();
        next.validate().map_err(DeskError::InvalidInput)?;
        if next.id.is_empty() {
            return Err(DeskError::InvalidInput("记录编号无效".into()));
        }
        let tx = self.desk.transaction()?;
        let payload = serde_json::to_string(&next)?;
        let changed = if next.revision == 0 {
            tx.execute(
                "INSERT OR IGNORE INTO jobhunt_sites(id,payload,revision) VALUES(?1,?2,1)",
                params![next.id, payload],
            )?
        } else {
            tx.execute("UPDATE jobhunt_sites SET payload=?2,revision=revision+1 WHERE id=?1 AND revision=?3 AND deleted=0",params![next.id,payload,next.revision])?
        };
        if changed != 1 {
            return Err(DeskError::ConcurrentModification);
        }
        tx.commit()?;
        next.revision += 1;
        *site = next;
        Ok(())
    }
    pub fn delete_jobhunt_site(&mut self, site: &Site) -> Result<()> {
        let changed=self.desk.execute("UPDATE jobhunt_sites SET deleted=1,revision=revision+1 WHERE id=?1 AND revision=?2 AND deleted=0",params![site.id,site.revision])?;
        if changed != 1 {
            return Err(DeskError::ConcurrentModification);
        }
        Ok(())
    }
}
