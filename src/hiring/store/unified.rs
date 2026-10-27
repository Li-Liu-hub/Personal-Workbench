use super::*;
use crate::hiring::{dedupe, unified};
impl Store {
    /// Preserve every discovered source in the same queue, including duplicates.
    pub fn discover(&mut self, site: &str, account: &str, candidate: &Job) -> Result<bool> {
        let mut job = candidate.clone();
        job.url = Self::canonical_site(&job.url, site)?;
        job.platform = site.into();
        job.account_id = super::super::accounts::id(account)?.into();
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(err)?;
        let old: Option<(String, String)> = tx
            .query_row(
                "SELECT payload,status FROM queue WHERE url=?1",
                [&job.url],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(err)?;
        if let Some((payload, status)) = &old {
            if !["candidate", "duplicate", "pending"].contains(&status.as_str()) {
                return Ok(false);
            }
            let prior: Job = serde_json::from_str(payload).map_err(err)?;
            if job.jd.is_empty() {
                job.jd = prior.jd;
            }
        }
        job.status = "candidate".into();
        tx.execute("INSERT INTO queue(url,payload,status,priority,platform) VALUES(?1,?2,'candidate',?3,?4) ON CONFLICT(url) DO UPDATE SET payload=excluded.payload,platform=excluded.platform,status='candidate'",params![job.url,serde_json::to_string(&job).map_err(err)?,job.priority,site]).map_err(err)?;
        tx.execute("INSERT INTO queue_identity VALUES(?1,?2,?3,?4) ON CONFLICT(url) DO UPDATE SET fingerprint=excluded.fingerprint,account_id=excluded.account_id",params![job.url,dedupe::fingerprint(&job).unwrap_or_default(),job.account_id,Local::now().timestamp()]).map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(old.is_none())
    }
    pub fn today_total(&self) -> Result<u32> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM attempts WHERE day=?1",
                [Local::now().format("%Y-%m-%d").to_string()],
                |r| r.get(0),
            )
            .map_err(err)
    }
    pub fn today_sent(&self) -> Result<u32> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM attempts WHERE day=?1 AND status='sent'",
                [Local::now().format("%Y-%m-%d").to_string()],
                |r| r.get(0),
            )
            .map_err(err)
    }
    pub fn unified_candidates(&mut self) -> Result<Vec<Job>> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(err)?;
        let mut jobs: Vec<Job> = {
            let mut q=tx.prepare("SELECT q.payload,q.status,q.platform,COALESCE(m.account_id,'default') FROM queue q LEFT JOIN queue_identity m ON m.url=q.url WHERE q.status IN ('candidate','duplicate','pending')").map_err(err)?;
            q.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(err)?
            .map(|r| {
                let (payload, status, platform, account_id) = r.map_err(err)?;
                let mut j: Job = serde_json::from_str(&payload).map_err(err)?;
                j.status = status;
                j.platform = platform;
                j.account_id = account_id;
                Ok(j)
            })
            .collect::<Result<_>>()?
        };
        jobs.sort_by(|a, b| {
            unified::rank(&a.platform)
                .cmp(&unified::rank(&b.platform))
                .then(b.priority.cmp(&a.priority))
                .then(a.url.cmp(&b.url))
        });
        let mut winners: Vec<Job> = vec![];
        for mut job in jobs {
            let duplicate = winners
                .iter()
                .find(|old| unified::same_candidate(old, &job));
            let eligibility =
                Self::eligibility_at(&tx, &job.platform, &job, Local::now().timestamp())?;
            let (status, note) = if let Some(old) = duplicate {
                (
                    "duplicate",
                    format!(
                        "优先保留{}：{}",
                        super::super::tracking::site_label(&old.platform),
                        old.url
                    ),
                )
            } else if let Some(error) = eligibility.error() {
                ("duplicate", error)
            } else {
                ("pending", String::new())
            };
            job.status = status.into();
            job.note = note.clone();
            tx.execute(
                "UPDATE queue SET status=?2,payload=?3 WHERE url=?1",
                params![job.url, status, serde_json::to_string(&job).map_err(err)?],
            )
            .map_err(err)?;
            if status == "pending" {
                winners.push(job);
            }
        }
        tx.commit().map_err(err)?;
        Ok(winners)
    }
}
