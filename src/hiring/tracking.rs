//! Read-only views of the existing delivery database. Viewing never resets queue state.
use super::{Result, err, store::Job};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Attempt {
    pub id: i64,
    pub site: String,
    pub url: String,
    pub title: String,
    pub company: String,
    pub at: i64,
    pub day: String,
    pub status: String,
    pub note: String,
    pub account_id: String,
    pub account_label: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub queue: Vec<Job>,
    pub attempts: Vec<Attempt>,
}
pub fn read(directory: &Path) -> Result<Snapshot> {
    let path = directory.join("hiring.sqlite");
    if !path.exists() {
        return Ok(Snapshot::default());
    }
    let mut conn =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(err)?;
    conn.busy_timeout(Duration::from_secs(3)).map_err(err)?;
    let tx = conn.transaction().map_err(err)?;
    let mut snapshot = Snapshot::default();
    let has_identity:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='attempt_identity')",[],|r|r.get(0)).map_err(err)?;
    {
        let queue_sql = if has_identity {
            "SELECT q.payload,q.status,COALESCE(m.account_id,'') FROM queue q LEFT JOIN queue_identity m ON m.url=q.url ORDER BY q.priority DESC,q.rowid"
        } else {
            "SELECT payload,status,'' FROM queue ORDER BY priority DESC,rowid"
        };
        let mut stmt = tx.prepare(queue_sql).map_err(err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?;
        for row in rows {
            let (payload, status, account_id) = row.map_err(err)?;
            let mut job: Job = serde_json::from_str(&payload).map_err(err)?;
            job.status = status;
            job.account_id = account_id;
            snapshot.queue.push(job);
        }
    }
    {
        let sql = if has_identity {
            "SELECT a.id,a.site,a.url,a.title,a.company,a.at,a.day,a.status,a.note,COALESCE(m.account_id,'default'),COALESCE(c.label,'账号1') FROM attempts a LEFT JOIN attempt_identity m ON m.attempt_id=a.id LEFT JOIN hiring_accounts c ON c.id=m.account_id ORDER BY a.at DESC,a.id DESC"
        } else {
            "SELECT id,site,url,title,company,at,day,status,note,'default','账号1' FROM attempts ORDER BY at DESC,id DESC"
        };
        let mut stmt = tx.prepare(sql).map_err(err)?;
        snapshot.attempts = stmt
            .query_map([], |r| {
                Ok(Attempt {
                    id: r.get(0)?,
                    site: r.get(1)?,
                    url: r.get(2)?,
                    title: r.get(3)?,
                    company: r.get(4)?,
                    at: r.get(5)?,
                    day: r.get(6)?,
                    status: r.get(7)?,
                    note: r.get(8)?,
                    account_id: r.get(9)?,
                    account_label: r.get(10)?,
                })
            })
            .map_err(err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(err)?;
    }
    tx.commit().map_err(err)?;
    Ok(snapshot)
}
pub fn status(site: &str, value: &str) -> &'static str {
    match value {
        "sent" if site == "boss" => "招呼已发送",
        "sent" => "已提交",
        "partial" => "部分发送",
        "reserved" | "running" | "ambiguous" => "待核验",
        "failed" => "失败",
        "pending" => "待处理",
        "skipped" => "已跳过",
        _ => "未知状态",
    }
}
