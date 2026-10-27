use super::{Result, Settings, err};
use chrono::{Local, NaiveTime};
use rusqlite::OptionalExtension;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::Path, time::Duration};
mod account_ops;
#[cfg(test)]
mod cross_platform_tests;
mod identity;
#[cfg(test)]
mod multi_account_tests;
mod planning;
mod platforms;
mod unified;
pub use planning::PoolPlan;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Job {
    pub platform: String,
    pub url: String,
    pub title: String,
    pub company: String,
    pub salary: String,
    pub location: String,
    pub experience: String,
    pub education: String,
    pub jd: String,
    pub hr_name: String,
    pub status: String,
    pub priority: i64,
    pub note: String,
    pub account_id: String,
}
pub struct Store {
    conn: Connection,
    account_id: String,
}
pub fn canonical(raw: &str, boss: bool) -> Result<String> {
    let site = if boss {
        super::browser::Site::Boss
    } else {
        super::browser::Site::Official
    };
    let mut url = super::browser::validate_url(raw, site)?;
    url.set_fragment(None);
    if boss {
        let path = url.path();
        if !path.starts_with("/job_detail/") || !path.ends_with(".html") {
            return Err("请输入 BOSS 职位详情链接".into());
        }
        url.set_query(None);
        url.set_host(Some("www.zhipin.com")).map_err(err)?;
        url.set_scheme("https").map_err(|_| "网址无效".to_owned())?;
    } else {
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| !k.starts_with("utm_") && k != "from" && k != "source")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        url.set_query(None);
        if !pairs.is_empty() {
            url.query_pairs_mut().extend_pairs(pairs);
        }
    }
    Ok(url.to_string())
}
pub fn site_for_url(raw: &str) -> &'static str {
    if let Some(p) = super::platforms::Platform::detect(raw) {
        p.id()
    } else if canonical(raw, true).is_ok() {
        "boss"
    } else {
        "official"
    }
}
impl Store {
    pub fn canonical_site(raw: &str, site: &str) -> Result<String> {
        match site {
            "boss" => canonical(raw, true),
            "official" => {
                if let Some(p) = super::platforms::Platform::detect(raw) {
                    p.canonical(raw)
                } else {
                    canonical(raw, false)
                }
            }
            _ => super::platforms::Platform::parse(site)?.canonical(raw),
        }
    }
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(err)?;
        let conn = Connection::open(dir.join("hiring.sqlite")).map_err(err)?;
        conn.busy_timeout(Duration::from_secs(5)).map_err(err)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;
          CREATE TABLE IF NOT EXISTS queue(url TEXT PRIMARY KEY, payload TEXT NOT NULL,status TEXT NOT NULL,priority INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS attempts(id INTEGER PRIMARY KEY, site TEXT NOT NULL,url TEXT NOT NULL,title TEXT NOT NULL,company TEXT NOT NULL,at INTEGER NOT NULL,day TEXT NOT NULL,status TEXT NOT NULL,note TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS guard(key TEXT PRIMARY KEY,value INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS imports(source TEXT PRIMARY KEY);
          UPDATE queue SET status='ambiguous' WHERE status='running';").map_err(err)?;
        let mut store = Self {
            conn,
            account_id: super::accounts::DEFAULT_ACCOUNT.into(),
        };
        store.migrate_queue()?;
        store.migrate_identity()?;
        Ok(store)
    }
    pub fn for_account(dir: &Path, account_id: &str) -> Result<Self> {
        let mut store = Self::open(dir)?;
        store.account_id = super::accounts::id(account_id)?.into();
        store.account(&store.account_id, super::BOSS_MAX_DAILY)?;
        Ok(store)
    }
    pub fn jobs(&self) -> Result<Vec<Job>> {
        let mut q = self
            .conn
            .prepare("SELECT q.payload,q.status,COALESCE(m.account_id,'') FROM queue q LEFT JOIN queue_identity m ON m.url=q.url WHERE q.platform='boss' ORDER BY q.priority DESC,q.rowid")
            .map_err(err)?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?;
        rows.map(|r| {
            let (payload, status, account_id) = r.map_err(err)?;
            let mut job: Job = serde_json::from_str(&payload).map_err(err)?;
            job.status = status;
            job.account_id = account_id;
            Ok(job)
        })
        .collect()
    }
    pub fn add(&mut self, job: &Job) -> Result<bool> {
        self.add_to("boss", job)
    }
    fn add_to(&mut self, site: &str, job: &Job) -> Result<bool> {
        let url = Self::canonical_site(&job.url, site)?;
        let mut job = job.clone();
        job.platform = site.into();
        job.url = url;
        if job.account_id.is_empty() && site != "boss" {
            job.account_id = super::accounts::DEFAULT_ACCOUNT.into();
        }
        if site == "boss" && !job.account_id.is_empty() {
            self.account(&job.account_id, super::BOSS_MAX_DAILY)?;
        }
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
        if let Some((payload, _)) = &old
            && let Ok(prior) = serde_json::from_str::<Job>(payload)
        {
            job.priority = job.priority.max(prior.priority);
            for (new, old) in [
                (&mut job.title, prior.title),
                (&mut job.company, prior.company),
                (&mut job.jd, prior.jd),
                (&mut job.salary, prior.salary),
                (&mut job.location, prior.location),
                (&mut job.experience, prior.experience),
                (&mut job.education, prior.education),
                (&mut job.hr_name, prior.hr_name),
                (&mut job.note, prior.note),
            ] {
                if new.is_empty() {
                    *new = old;
                }
            }
        }
        if Self::eligibility_at(&tx, site, &job, Local::now().timestamp())?
            != super::dedupe::Eligibility::Eligible
        {
            return Ok(false);
        }
        let fp = super::dedupe::fingerprint(&job).unwrap_or_default();
        if !fp.is_empty() && tx.query_row("SELECT EXISTS(SELECT 1 FROM queue q JOIN queue_identity m ON m.url=q.url WHERE m.fingerprint=?1 AND q.status='pending' AND q.url<>?2)",params![fp,job.url],|r|r.get::<_,bool>(0)).map_err(err)? {return Ok(false);}
        let inserted = old.as_ref().is_none_or(|(_, status)| status != "pending");
        job.status = "pending".into();
        tx.execute("INSERT INTO queue(url,payload,status,priority,platform) VALUES(?1,?2,'pending',?3,?4) ON CONFLICT(url) DO UPDATE SET payload=excluded.payload,status='pending',priority=excluded.priority",params![job.url,serde_json::to_string(&job).map_err(err)?,job.priority,site]).map_err(err)?;
        tx.execute("INSERT INTO queue_identity VALUES(?1,?2,?3,?4) ON CONFLICT(url) DO UPDATE SET fingerprint=excluded.fingerprint,account_id=CASE WHEN excluded.account_id='' THEN queue_identity.account_id ELSE excluded.account_id END",params![job.url,fp,job.account_id,Local::now().timestamp()]).map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(inserted)
    }
    pub fn set_status(&mut self, url: &str, status: &str) -> Result<()> {
        if ![
            "pending",
            "skipped",
            "ambiguous",
            "sent",
            "partial",
            "failed",
        ]
        .contains(&status)
        {
            return Err("无效队列状态".into());
        }
        // An uncertain send must be checked on the website before it can be retried.
        let current: String = self
            .conn
            .query_row("SELECT status FROM queue WHERE url=?1", [url], |r| r.get(0))
            .map_err(err)?;
        if status == "pending" && ["ambiguous", "partial"].contains(&current.as_str()) {
            return Err("此项发送结果待核验，不能自动重投".into());
        }
        if status == "pending" {
            let payload: String = self
                .conn
                .query_row("SELECT payload FROM queue WHERE url=?1", [url], |r| {
                    r.get(0)
                })
                .map_err(err)?;
            let job: Job = serde_json::from_str(&payload).map_err(err)?;
            if let Some(error) = self.eligibility(site_for_url(url), &job)?.error() {
                return Err(error);
            }
        }
        self.conn
            .execute(
                "UPDATE queue SET status=?2 WHERE url=?1",
                params![url, status],
            )
            .map_err(err)?;
        Ok(())
    }
    pub fn history(&self) -> Result<Vec<String>> {
        let mut q=self.conn.prepare("SELECT a.day,a.site,a.title,a.company,a.status,a.note,COALESCE(c.label,'账号1') FROM attempts a LEFT JOIN attempt_identity m ON m.attempt_id=a.id LEFT JOIN hiring_accounts c ON c.id=m.account_id ORDER BY a.id DESC LIMIT 200").map_err(err)?;
        q.query_map([], |r| {
            Ok(format!(
                "{} · {} · {} · {} · {} · {} · {}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?
            ))
        })
        .map_err(err)?
        .map(|r| r.map_err(err))
        .collect()
    }
    pub fn reserve(&mut self, site: &str, job: &Job, settings: &Settings) -> Result<i64> {
        self.reserve_at(site, job, settings, Local::now().timestamp())
    }
    fn reserve_at(&mut self, site: &str, job: &Job, settings: &Settings, now: i64) -> Result<i64> {
        use chrono::TimeZone;
        let instant = Local.timestamp_opt(now, 0).single().ok_or("投递时间无效")?;
        if !in_window(&settings.send_windows, instant.time())? {
            return Err("当前时间不在允许投递窗口内".into());
        }
        let url = Self::canonical_site(&job.url, site)?;
        let account_id = if site == "boss" {
            super::accounts::id(&settings.account_id)?.to_owned()
        } else {
            super::accounts::DEFAULT_ACCOUNT.into()
        };
        if site == "boss" && self.account_id != account_id {
            return Err("任务账号与计数账号不一致，已停止投递".into());
        }
        let account = self.account(&account_id, settings.daily_limit)?;
        if !account.enabled {
            return Err("此账号已停用".into());
        }
        let day = instant.format("%Y-%m-%d").to_string();
        let risk_key = self.account_key(site, "lock");
        let imported_key = self.account_key("boss", &format!("imported:{day}"));
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(err)?;
        let lock: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(value),0) FROM guard WHERE key=?1",
                [risk_key],
                |r| r.get(0),
            )
            .map_err(err)?;
        if lock > now {
            return Err("触发风险提示或连续失败，暂时停止投递".into());
        }
        if let Some(error) = Self::eligibility_at(&tx, site, job, now)?.error() {
            return Err(error);
        }
        let count:i64=tx.query_row("SELECT COUNT(*) FROM attempts a LEFT JOIN attempt_identity m ON m.attempt_id=a.id WHERE a.day=?1 AND COALESCE(m.account_id,'default')=?2",params![day,account_id],|r|r.get(0)).map_err(err)?;
        let imported: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(value),0) FROM guard WHERE key=?1",
                [imported_key],
                |r| r.get(0),
            )
            .map_err(err)?;
        let limit = settings
            .daily_limit
            .min(account.daily_limit)
            .min(super::BOSS_MAX_DAILY);
        if count + imported >= i64::from(limit) {
            return Err("已达到此账号今日投递上限".into());
        }
        let last:i64=tx.query_row("SELECT COALESCE(MAX(a.at),0) FROM attempts a LEFT JOIN attempt_identity m ON m.attempt_id=a.id WHERE COALESCE(m.account_id,'default')=?1",params![account_id],|r|r.get(0)).map_err(err)?;
        if now - last < settings.interval_min.max(30) as i64 {
            return Err("尚未达到此账号两次投递的最短间隔".into());
        }
        tx.execute("INSERT INTO attempts(site,url,title,company,at,day,status,note) VALUES(?1,?2,?3,?4,?5,?6,'reserved','')",params![site,url,job.title,job.company,now,day]).map_err(err)?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO attempt_identity VALUES(?1,?2,?3,?4)",
            params![
                id,
                account_id,
                super::dedupe::fingerprint(job).unwrap_or_default(),
                now
            ],
        )
        .map_err(err)?;
        tx.execute("INSERT INTO attempt_job_snapshots VALUES(?1,?2,'delivery_preflight')",params![id,serde_json::to_string(&serde_json::json!({"url":job.url,"company":job.company,"title":job.title,"jd":job.jd,"salary":job.salary,"location":job.location,"experience":job.experience,"education":job.education})).map_err(err)?]).map_err(err)?;
        tx.execute("UPDATE queue SET status='ambiguous' WHERE url=?1", [&url])
            .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(id)
    }
    pub fn finish(&mut self, id: i64, status: &str, note: &str) -> Result<()> {
        let tx = self.conn.transaction().map_err(err)?;
        tx.execute(
            "UPDATE attempts SET status=?2,note=?3 WHERE id=?1",
            params![id, status, note],
        )
        .map_err(err)?;
        if status == "sent" {
            tx.execute(
                "UPDATE attempt_identity SET cooldown_at=?2 WHERE attempt_id=?1",
                params![id, Local::now().timestamp()],
            )
            .map_err(err)?;
        }
        tx.execute(
            "UPDATE queue SET status=?2 WHERE url=(SELECT url FROM attempts WHERE id=?1) AND ?1=(SELECT MAX(a.id) FROM attempts a WHERE a.url=queue.url)",
            params![id, status],
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(())
    }
    pub fn risk_lock(&mut self, site: &str) -> Result<()> {
        self.conn.execute("INSERT INTO guard VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![self.account_key(site,"lock"),Local::now().timestamp()+600]).map_err(err)?;
        Ok(())
    }
    pub fn reserve_page(&mut self, kind: &str) -> Result<()> {
        let day = Local::now().format("%Y-%m-%d");
        let key = if self.account_id == super::accounts::DEFAULT_ACCOUNT {
            format!("page:{day}:{kind}")
        } else {
            format!("page:{}:{day}:{kind}", self.account_id)
        };
        let limit = if kind == "search" || kind.ends_with(":search") {
            60
        } else {
            400
        };
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(err)?;
        let count: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(value),0) FROM guard WHERE key=?1",
                [&key],
                |r| r.get(0),
            )
            .map_err(err)?;
        if count >= limit {
            return Err("已达到今日页面读取预算".into());
        }
        tx.execute(
            "INSERT INTO guard VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, count + 1],
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(())
    }
    pub fn import_queue(&mut self, source: &Path) -> Result<usize> {
        let data: Value =
            serde_json::from_slice(&std::fs::read(source).map_err(err)?).map_err(err)?;
        let items = data["items"].as_array().ok_or("队列文件缺少 items")?;
        let mut added = 0;
        let tx = self.conn.transaction().map_err(err)?;
        let source_key = source
            .canonicalize()
            .map_err(err)?
            .to_string_lossy()
            .into_owned();
        let done: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM imports WHERE source=?1)",
                [&source_key],
                |r| r.get(0),
            )
            .map_err(err)?;
        if done {
            return Ok(0);
        }
        for value in items {
            let mut normalized = value.clone();
            if normalized["location"].as_str().is_none_or(|s| s.is_empty()) {
                normalized["location"] = value["city"].as_str().unwrap_or("").into();
            }
            let mut job: Job = serde_json::from_value(normalized).map_err(err)?;
            job.url = canonical(&job.url, true)?;
            let old = value["status"].as_str().unwrap_or("pending");
            job.status = match old {
                "sent" | "skipped" | "partial" | "ambiguous" => old,
                _ => "skipped",
            }
            .into();
            job.note = "从旧队列快照导入；待投项默认跳过，核验旧队列停止后可重新启用".into();
            added += tx
                .execute(
                    "INSERT OR IGNORE INTO queue(url,payload,status,priority) VALUES(?1,?2,?3,?4)",
                    params![
                        job.url,
                        serde_json::to_string(&job).map_err(err)?,
                        job.status,
                        job.priority
                    ],
                )
                .map_err(err)?;
            if ["sent", "partial", "ambiguous"].contains(&job.status.as_str()) {
                tx.execute("INSERT INTO attempts(site,url,title,company,at,day,status,note) VALUES('boss',?1,?2,?3,0,'imported',?4,'旧队列记录')",params![job.url,job.title,job.company,job.status]).map_err(err)?;
            }
        }
        tx.execute("INSERT INTO imports VALUES(?1)", [source_key])
            .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(added)
    }
    pub fn import_state(&mut self, source: &Path) -> Result<usize> {
        let raw: Value =
            serde_json::from_slice(&std::fs::read(source).map_err(err)?).map_err(err)?;
        let key = source
            .canonicalize()
            .map_err(err)?
            .to_string_lossy()
            .into_owned();
        let tx = self.conn.transaction().map_err(err)?;
        if tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM imports WHERE source=?1)",
                [&key],
                |r| r.get::<_, bool>(0),
            )
            .map_err(err)?
        {
            return Ok(0);
        }
        let mut added = 0;
        let day = raw["date"].as_str().unwrap_or("imported");
        let mut imported_today = 0;
        for delivery in raw["deliveries"]
            .as_object()
            .into_iter()
            .flat_map(|v| v.values())
        {
            let Some(url) = delivery["url"].as_str() else {
                continue;
            };
            let url = canonical(url, true)?;
            let at = delivery["at"].as_f64().unwrap_or(0.) as i64;
            let date = chrono::DateTime::from_timestamp(at, 0)
                .map(|d| d.with_timezone(&Local).format("%Y-%m-%d").to_string())
                .unwrap_or("imported".into());
            let exists: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM attempts WHERE site='boss' AND url=?1)",
                    [&url],
                    |r| r.get(0),
                )
                .map_err(err)?;
            let status = match delivery["status"].as_str().unwrap_or("") {
                "sent" => "sent",
                "partial" => "partial",
                _ => "ambiguous",
            };
            if !exists {
                tx.execute("INSERT INTO attempts(site,url,title,company,at,day,status,note) VALUES('boss',?1,?2,?3,?4,?5,?6,'旧状态导入')",params![url,delivery["title"].as_str().unwrap_or(""),delivery["company"].as_str().unwrap_or(""),at,date,status]).map_err(err)?;
                added += 1;
            } else {
                tx.execute("UPDATE attempts SET at=?2,day=?3 WHERE site='boss' AND url=?1 AND day='imported'",params![url,at,date]).map_err(err)?;
            }
            if date == day {
                imported_today += 1;
            }
        }
        let extra = (raw["sends"].as_i64().unwrap_or(0) - imported_today).max(0);
        tx.execute(
            "INSERT OR REPLACE INTO guard VALUES(?1,?2)",
            params![format!("boss:imported:{day}"), extra],
        )
        .map_err(err)?;
        for kind in ["search", "detail", "chat"] {
            let n = raw["pages"][kind].as_i64().unwrap_or(0);
            tx.execute("INSERT INTO guard VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=MAX(value,excluded.value)",params![format!("page:{day}:{kind}"),n]).map_err(err)?;
        }
        tx.execute("INSERT INTO imports VALUES(?1)", [key])
            .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(added)
    }
}
pub fn in_window(windows: &[String], now: NaiveTime) -> Result<bool> {
    if windows.is_empty() {
        return Err("请至少设置一个允许投递时间窗口".into());
    }
    let mut result = false;
    for window in windows {
        let (a, b) = window
            .split_once('-')
            .ok_or("时间窗口格式须为 09:00-18:00")?;
        let start = NaiveTime::parse_from_str(a.trim(), "%H:%M").map_err(err)?;
        let end = NaiveTime::parse_from_str(b.trim(), "%H:%M").map_err(err)?;
        if start >= end {
            return Err("时间窗口结束须晚于开始".into());
        }
        result |= now >= start && now < end;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn job() -> Job {
        Job {
            url: "https://www.zhipin.com/job_detail/test.html?ka=1".into(),
            ..Default::default()
        }
    }
    fn settings() -> Settings {
        Settings {
            send_windows: vec!["00:00-23:59".into()],
            ..Default::default()
        }
    }
    #[test]
    fn canonical_boss_query_deduplicates() {
        assert_eq!(
            canonical(&job().url, true).unwrap(),
            "https://www.zhipin.com/job_detail/test.html"
        );
        assert!(canonical("https://www.zhipin.com.evil.test/job_detail/x.html", true).is_err());
    }
    #[test]
    fn reservation_survives_restart_and_blocks_duplicate() {
        let d = tempfile::tempdir().unwrap();
        let id = Store::open(d.path())
            .unwrap()
            .reserve("official", &job(), &settings())
            .unwrap();
        let mut s = Store::open(d.path()).unwrap();
        assert!(s.reserve("official", &job(), &settings()).is_err());
        s.finish(id, "ambiguous", "待核验").unwrap();
        assert!(s.reserve("official", &job(), &settings()).is_err());
    }
    #[test]
    fn boss_daily_reservations_allow_the_150th_and_block_the_151st() {
        let d = tempfile::tempdir().unwrap();
        let mut store = Store::open(d.path()).unwrap();
        let today = Local::now().format("%Y-%m-%d").to_string();
        for i in 0..149 {
            store.conn.execute("INSERT INTO attempts(site,url,title,company,at,day,status,note) VALUES('boss',?1,'','',0,?2,'sent','fixture')",params![format!("https://www.zhipin.com/job_detail/history-{i}.html"),today]).unwrap();
        }
        let requested = Settings {
            daily_limit: 150,
            send_windows: vec!["00:00-23:59".into()],
            ..Settings::default()
        };
        requested.save(d.path()).unwrap();
        let id = store.reserve("boss", &job(), &requested).unwrap();
        store.finish(id, "sent", "fixture").unwrap();
        let mut other = job();
        other.url = "https://www.zhipin.com/job_detail/another.html".into();
        assert_eq!(
            store.reserve("boss", &other, &requested).unwrap_err(),
            "已达到此账号今日投递上限"
        );
    }
    #[test]
    fn malformed_queue_import_is_atomic() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("queue.json");
        std::fs::write(
            &f,
            r#"{"items":[{"url":"https://www.zhipin.com/job_detail/a.html"},{"url":"bad"}]}"#,
        )
        .unwrap();
        let mut s = Store::open(d.path()).unwrap();
        assert!(s.import_queue(&f).is_err());
        assert!(s.jobs().unwrap().is_empty());
    }
    #[test]
    fn imported_pending_is_skipped_and_repeat_import_noop() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("q.json");
        std::fs::write(
            &f,
            r#"{"items":[{"url":"https://www.zhipin.com/job_detail/a.html","status":"pending"}]}"#,
        )
        .unwrap();
        let mut s = Store::open(d.path()).unwrap();
        assert_eq!(s.import_queue(&f).unwrap(), 1);
        assert_eq!(s.jobs().unwrap()[0].status, "skipped");
        assert_eq!(s.import_queue(&f).unwrap(), 0);
    }
    #[test]
    fn window_and_query_id_preservation() {
        assert!(in_window(&["bad".into()], Local::now().time()).is_err());
        assert_ne!(
            canonical("https://jobs.test/apply?id=1", false).unwrap(),
            canonical("https://jobs.test/apply?id=2", false).unwrap()
        );
    }
}
