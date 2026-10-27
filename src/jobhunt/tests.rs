use super::*;
use crate::{db::Database, hiring::tracking};

#[test]
fn jobhunt_manual_records_survive_restart_and_block_stale_edit_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut site = Site {
        company: "样例公司".into(),
        position: "Java研发".into(),
        url: "https://jobs.example.com/#/job/1".into(),
        ..Site::new()
    };
    db.save_jobhunt_site(&mut site).unwrap();
    let stale = site.clone();
    site.notes = "我的备注".into();
    db.save_jobhunt_site(&mut site).unwrap();
    let mut stale_edit = stale.clone();
    stale_edit.notes = "旧窗口".into();
    assert!(db.save_jobhunt_site(&mut stale_edit).is_err());
    assert!(db.delete_jobhunt_site(&stale).is_err());
    drop(db);
    let mut db = Database::open(dir.path()).unwrap();
    let saved = db.jobhunt_sites().unwrap().remove(0);
    assert_eq!(saved.notes, "我的备注");
    assert!(saved.url.ends_with("#/job/1"));
    db.delete_jobhunt_site(&saved).unwrap();
    assert!(db.jobhunt_sites().unwrap().is_empty());
    assert!(db.save_jobhunt_site(&mut site).is_err());
}
#[test]
fn tracking_read_keeps_running_queue_and_complete_history_without_initializing_missing_store() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    assert!(tracking::read(&missing).unwrap().attempts.is_empty());
    assert!(!missing.exists());
    let mut store = store::Store::open(dir.path()).unwrap();
    store
        .add(&store::Job {
            url: "https://www.zhipin.com/job_detail/test.html".into(),
            ..Default::default()
        })
        .unwrap();
    drop(store);
    let conn = rusqlite::Connection::open(dir.path().join("hiring.sqlite")).unwrap();
    conn.execute("UPDATE queue SET status='running'", [])
        .unwrap();
    for i in 0..205 {
        conn.execute("INSERT INTO attempts(site,url,title,company,at,day,status,note) VALUES('official',?1,'研发工程师','样例公司',?2,'2026-10-27','ambiguous','待核验')",rusqlite::params![format!("https://example.com/{i}"),i]).unwrap();
    }
    let snapshot = tracking::read(dir.path()).unwrap();
    assert_eq!(snapshot.attempts.len(), 205);
    assert_eq!(snapshot.queue[0].status, "running");
    assert_eq!(snapshot.attempts[0].at, 204);
    assert_eq!(
        conn.query_row("SELECT status FROM queue", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "running"
    );
}
#[test]
fn jobhunt_statuses_match_job_urls_and_never_treat_company_match_as_delivery() {
    let mut first = campus::Job {
        title: "Java实习生".into(),
        company: "样例公司".into(),
        source_url: "https://example.com/notice/1".into(),
        apply_url: "https://jobs.example.com/#/job/1".into(),
        ..Default::default()
    };
    first.identify();
    let mut second = first.clone();
    second.source_url = "https://example.com/notice/2".into();
    second.apply_url = "https://jobs.example.com/#/job/2".into();
    second.applied = true;
    second.identify();
    let snapshot = tracking::Snapshot {
        attempts: vec![tracking::Attempt {
            id: 1,
            site: "official".into(),
            url: first.apply_url.clone(),
            status: "ambiguous".into(),
            company: first.company.clone(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let rows = entries(&[first, second], &[], &snapshot);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter().find(|r| r.url.ends_with("/1")).unwrap().status,
        "待核验"
    );
    assert_eq!(
        rows.iter().find(|r| r.url.ends_with("/2")).unwrap().status,
        "手动已标记"
    );
    assert_eq!(tracking::status("boss", "sent"), "招呼已发送");
    assert_eq!(tracking::status("official", "sent"), "已提交");
    assert_eq!(tracking::status("boss", "partial"), "部分发送");
}
#[test]
fn jobhunt_export_contains_manual_sites_and_actual_attempts_with_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let mut db = Database::open(&data).unwrap();
    let mut site = Site {
        company: "样例公司".into(),
        url: "https://jobs.example.com".into(),
        ..Site::new()
    };
    db.save_jobhunt_site(&mut site).unwrap();
    store::Store::open(&data.join("hiring")).unwrap();
    let conn = rusqlite::Connection::open(data.join("hiring/hiring.sqlite")).unwrap();
    conn.execute("INSERT INTO attempts(site,url,title,company,at,day,status,note) VALUES('boss','https://www.zhipin.com/job_detail/test.html','Java','样例公司',1,'2026-10-27','partial','图片未确认')",[]).unwrap();
    drop(conn);
    drop(db);
    let path = dir.path().join("export.zip");
    crate::export::write(&data, &path, crate::export::Scope::JobHunt).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut text = String::new();
    use std::io::Read;
    zip.by_name("投递记录.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    let rows: Vec<tracking::Attempt> = serde_json::from_str(&text).unwrap();
    assert_eq!(rows[0].status, "partial");
    assert!(zip.by_name("求职网站.json").is_ok());
    assert!(zip.by_name("data/hiring/hiring.sqlite").is_ok());
}
