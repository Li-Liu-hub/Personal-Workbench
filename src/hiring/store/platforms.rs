use super::*;
use crate::hiring::platforms::Platform;
impl Store {
    pub(super) fn migrate_queue(&mut self) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(err)?;
        let has_column: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('queue') WHERE name='platform')",
                [],
                |r| r.get(0),
            )
            .map_err(err)?;
        if !has_column {
            tx.execute_batch("ALTER TABLE queue ADD COLUMN platform TEXT NOT NULL DEFAULT 'boss'; CREATE INDEX IF NOT EXISTS queue_platform_status ON queue(platform,status);").map_err(err)?;
        }
        tx.commit().map_err(err)
    }
    pub fn platform_jobs(&self, p: Platform) -> Result<Vec<Job>> {
        let mut q = self
            .conn
            .prepare(
                "SELECT payload,status FROM queue WHERE platform=?1 ORDER BY priority DESC,rowid",
            )
            .map_err(err)?;
        q.query_map([p.id()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(err)?
        .map(|r| {
            let (payload, status) = r.map_err(err)?;
            let mut job: Job = serde_json::from_str(&payload).map_err(err)?;
            job.status = status;
            Ok(job)
        })
        .collect()
    }
    pub fn platform_add(&mut self, p: Platform, job: &Job) -> Result<bool> {
        let mut job = job.clone();
        job.account_id = super::super::accounts::DEFAULT_ACCOUNT.into();
        self.add_to(p.id(), &job)
    }
    pub fn platform_status(&mut self, p: Platform, url: &str, status: &str) -> Result<()> {
        if !["pending", "skipped"].contains(&status) {
            return Err("状态须为 pending 或 skipped".into());
        }
        let url = p.canonical(url)?;
        self.set_status(&url, status)
    }
}
