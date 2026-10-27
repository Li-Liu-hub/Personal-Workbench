use super::*;
use crate::hiring::{
    accounts::DEFAULT_ACCOUNT,
    dedupe::{self, Eligibility},
};
use chrono::TimeZone;
type LegacyAttempt = (i64, String, String, String, i64, String, Option<String>);

fn timestamp(at: i64, day: &str, fallback: i64) -> i64 {
    if at > 0 {
        return at;
    }
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(23, 59, 59))
        .and_then(|d| Local.from_local_datetime(&d).latest())
        .map(|d| d.timestamp())
        .unwrap_or(fallback)
}
impl Store {
    pub(super) fn migrate_identity(&mut self) -> Result<()> {
        let now = Local::now().timestamp();
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(err)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS hiring_accounts(
            id TEXT PRIMARY KEY,label TEXT NOT NULL COLLATE NOCASE UNIQUE,slot INTEGER NOT NULL UNIQUE CHECK(slot BETWEEN 0 AND 2),
            daily_limit INTEGER NOT NULL CHECK(daily_limit BETWEEN 1 AND 150),enabled INTEGER NOT NULL DEFAULT 1,
            login_checked_at INTEGER NOT NULL DEFAULT 0,created_at INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS attempt_identity(attempt_id INTEGER PRIMARY KEY,account_id TEXT NOT NULL DEFAULT 'default',fingerprint TEXT NOT NULL DEFAULT '',cooldown_at INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS attempt_job_snapshots(attempt_id INTEGER PRIMARY KEY,payload TEXT NOT NULL,source TEXT NOT NULL);
          CREATE INDEX IF NOT EXISTS attempt_identity_fingerprint ON attempt_identity(fingerprint,cooldown_at);
          CREATE INDEX IF NOT EXISTS attempt_identity_account ON attempt_identity(account_id,attempt_id);
          CREATE TABLE IF NOT EXISTS queue_identity(url TEXT PRIMARY KEY,fingerprint TEXT NOT NULL DEFAULT '',account_id TEXT NOT NULL DEFAULT '',added_at INTEGER NOT NULL);
          CREATE INDEX IF NOT EXISTS queue_identity_fingerprint ON queue_identity(fingerprint);").map_err(err)?;
        tx.execute(
            "INSERT OR IGNORE INTO hiring_accounts VALUES('default','账号1',0,150,1,0,?1)",
            [now],
        )
        .map_err(err)?;
        tx.execute(
            "INSERT OR IGNORE INTO guard VALUES('identity:migrated_at',?1)",
            [now],
        )
        .map_err(err)?;
        let fallback: i64 = tx
            .query_row(
                "SELECT value FROM guard WHERE key='identity:migrated_at'",
                [],
                |r| r.get(0),
            )
            .map_err(err)?;
        let queues: Vec<(String, String)> = {
            let mut q=tx.prepare("SELECT q.url,q.payload FROM queue q LEFT JOIN queue_identity m ON m.url=q.url WHERE m.url IS NULL").map_err(err)?;
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(err)?
                .collect::<std::result::Result<_, _>>()
                .map_err(err)?
        };
        for (url, payload) in queues {
            let fp = serde_json::from_str::<Job>(&payload)
                .ok()
                .and_then(|j| dedupe::fingerprint(&j))
                .unwrap_or_default();
            tx.execute(
                "INSERT OR IGNORE INTO queue_identity VALUES(?1,?2,'',?3)",
                params![url, fp, now],
            )
            .map_err(err)?;
        }
        let attempts: Vec<LegacyAttempt> = {
            let mut q=tx.prepare("SELECT a.id,a.url,a.title,a.company,a.at,a.day,q.payload FROM attempts a LEFT JOIN attempt_identity m ON m.attempt_id=a.id LEFT JOIN queue q ON q.url=a.url WHERE m.attempt_id IS NULL").map_err(err)?;
            q.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })
            .map_err(err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(err)?
        };
        for (id, _url, title, company, at, day, payload) in attempts {
            let observed = payload
                .and_then(|p| serde_json::from_str::<Job>(&p).ok())
                .filter(|j| {
                    dedupe::normalize(&j.title) == dedupe::normalize(&title)
                        && dedupe::normalize(&j.company) == dedupe::normalize(&company)
                });
            let fp = observed
                .as_ref()
                .and_then(dedupe::fingerprint)
                .unwrap_or_default();
            tx.execute(
                "INSERT OR IGNORE INTO attempt_identity VALUES(?1,'default',?2,?3)",
                params![id, fp, timestamp(at, &day, fallback)],
            )
            .map_err(err)?;
            if let Some(job) = observed.filter(|j| dedupe::fingerprint(j).is_some()) {
                tx.execute("INSERT OR IGNORE INTO attempt_job_snapshots VALUES(?1,?2,'imported_queue_snapshot')",params![id,serde_json::to_string(&job).map_err(err)?]).map_err(err)?;
            }
        }
        tx.commit().map_err(err)
    }
    pub fn eligibility(&self, site: &str, job: &Job) -> Result<Eligibility> {
        Self::eligibility_at(&self.conn, site, job, Local::now().timestamp())
    }
    pub(super) fn eligibility_at(
        conn: &Connection,
        site: &str,
        job: &Job,
        now: i64,
    ) -> Result<Eligibility> {
        let url = Self::canonical_site(&job.url, site)?;
        let fp = dedupe::fingerprint(job).unwrap_or_default();
        // Portals often rewrite the same advert. A changed description on another
        // portal is insufficient evidence that this is a different position.
        if !dedupe::normalize(&job.company).is_empty() && !dedupe::normalize(&job.title).is_empty()
        {
            let mut q=conn.prepare("SELECT a.id,a.company,a.title,a.status,COALESCE(m.cooldown_at,a.at),COALESCE(m.fingerprint,'') FROM attempts a LEFT JOIN attempt_identity m ON m.attempt_id=a.id WHERE (a.site<>?1 OR COALESCE(m.fingerprint,'')='') AND a.status IN ('reserved','sent','ambiguous','partial')").map_err(err)?;
            let rows = q
                .query_map([site], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                })
                .map_err(err)?;
            for row in rows {
                let (id, company, title, status, at, prior_fp) = row.map_err(err)?;
                if dedupe::normalize(&company) == dedupe::normalize(&job.company)
                    && dedupe::normalize(&title) == dedupe::normalize(&job.title)
                    && (status != "sent" || at.saturating_add(dedupe::WINDOW_SECONDS) >= now)
                    && (fp.is_empty() || prior_fp != fp)
                {
                    return Ok(Eligibility::NeedsReview { attempt_id: id });
                }
            }
        }
        let fallback: i64 = conn
            .query_row(
                "SELECT value FROM guard WHERE key='identity:migrated_at'",
                [],
                |r| r.get(0),
            )
            .map_err(err)?;
        let legacy_51 = if super::super::platforms::Platform::detect(&url)
            == Some(super::super::platforms::Platform::Job51)
        {
            format!(
                "https://jobs.51job.com/%/{}",
                url.rsplit('/').next().unwrap()
            )
        } else {
            String::new()
        };
        let mut stmt=conn.prepare("SELECT a.id,a.status,a.at,a.day,m.cooldown_at FROM attempts a LEFT JOIN attempt_identity m ON m.attempt_id=a.id WHERE (a.url=?1 OR (?2<>'' AND m.fingerprint=?2) OR (?3<>'' AND a.url LIKE ?3)) AND a.status IN ('reserved','sent','ambiguous','partial') ORDER BY a.id DESC").map_err(err)?;
        let rows = stmt
            .query_map(params![url, fp, legacy_51], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                ))
            })
            .map_err(err)?;
        let mut until = 0;
        for row in rows {
            let (id, status, at, day, cooldown) = row.map_err(err)?;
            if status != "sent" {
                return Ok(Eligibility::NeedsReview { attempt_id: id });
            }
            let end = cooldown
                .unwrap_or_else(|| timestamp(at, &day, fallback))
                .saturating_add(dedupe::WINDOW_SECONDS);
            if end >= now {
                until = until.max(end);
            }
        }
        Ok(if until > 0 {
            Eligibility::CoolingDown { until }
        } else {
            let has_platform: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info('queue') WHERE name='platform')",
                    [],
                    |r| r.get(0),
                )
                .map_err(err)?;
            if has_platform {
                let mut candidate = job.clone();
                candidate.platform = site.into();
                let mut q=conn.prepare("SELECT payload,platform FROM queue WHERE url<>?1 AND status IN ('pending','candidate')").map_err(err)?;
                let rows = q
                    .query_map([&url], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })
                    .map_err(err)?;
                for row in rows {
                    let (payload, platform) = row.map_err(err)?;
                    if super::super::unified::rank(&platform) >= super::super::unified::rank(site) {
                        continue;
                    }
                    let mut prior: Job = serde_json::from_str(&payload).map_err(err)?;
                    prior.platform = platform.clone();
                    if super::super::unified::same_candidate(&prior, &candidate) {
                        return Ok(Eligibility::PreferredSource {
                            url: prior.url,
                            platform,
                        });
                    }
                }
            }
            Eligibility::Eligible
        })
    }
    pub fn resolve_attempt(&mut self, id: i64, sent: bool, verified_on_site: bool) -> Result<()> {
        if !verified_on_site {
            return Err("请先在招聘网页核对实际发送结果".into());
        }
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(err)?;
        let (status, url): (String, String) = tx
            .query_row("SELECT status,url FROM attempts WHERE id=?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(err)?;
        if !["reserved", "ambiguous", "partial"].contains(&status.as_str()) {
            return Err("此记录无需核验，不能改写已确认结果".into());
        }
        if status == "partial" && !sent {
            return Err("文字已确认发送的记录不能标为未发送，请先补齐并核对图片".into());
        }
        let next = if sent { "sent" } else { "failed" };
        tx.execute(
            "UPDATE attempts SET status=?2,note=note||'；已在网页人工核验' WHERE id=?1",
            params![id, next],
        )
        .map_err(err)?;
        if sent && status == "partial" {
            tx.execute(
                "UPDATE attempt_identity SET cooldown_at=?2 WHERE attempt_id=?1",
                params![id, Local::now().timestamp()],
            )
            .map_err(err)?;
        }
        tx.execute("UPDATE queue SET status=?2 WHERE url=?1 AND ?3=(SELECT MAX(id) FROM attempts WHERE url=?1)",params![url,next,id]).map_err(err)?;
        tx.commit().map_err(err)
    }
    pub(super) fn account_key(&self, site: &str, suffix: &str) -> String {
        if site != "boss" || self.account_id == DEFAULT_ACCOUNT {
            format!("{site}:{suffix}")
        } else {
            format!("{site}:{}:{suffix}", self.account_id)
        }
    }
}
