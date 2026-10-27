use super::*;
use crate::hiring::{
    accounts::DEFAULT_ACCOUNT,
    dedupe::{self, Eligibility},
};
use std::collections::HashSet;
use std::sync::{Arc, Barrier};

fn candidate(key: &str) -> Job {
    Job {
        url: format!("https://www.zhipin.com/job_detail/{key}.html"),
        company: "示例软件公司".into(),
        title: "Java后端校招".into(),
        jd: "面向2027届，使用Java、Spring开发业务系统。".into(),
        ..Default::default()
    }
}
fn settings(id: &str) -> Settings {
    Settings {
        account_id: id.into(),
        daily_limit: 150,
        send_windows: vec!["00:00-23:59".into()],
        ..Settings::default()
    }
}
fn age_sent(store: &mut Store, id: i64, at: i64) {
    store
        .conn
        .execute(
            "UPDATE attempts SET at=?2,status='sent' WHERE id=?1",
            params![id, at],
        )
        .unwrap();
    store
        .conn
        .execute(
            "UPDATE attempt_identity SET cooldown_at=?2 WHERE attempt_id=?1",
            params![id, at],
        )
        .unwrap();
}

#[test]
fn fifteen_day_window_is_shared_and_retains_history_at_the_exact_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let mut first = Store::open(dir.path()).unwrap();
    let second = first.create_account("账号2").unwrap();
    let now = Local::now().timestamp();
    let original = candidate("first");
    let id = first
        .reserve_at("boss", &original, &settings(DEFAULT_ACCOUNT), now)
        .unwrap();
    let mut next = Store::for_account(dir.path(), &second.id).unwrap();
    let duplicate = candidate("new-link");
    age_sent(&mut first, id, now - dedupe::WINDOW_SECONDS);
    assert!(matches!(
        Store::eligibility_at(&next.conn, "boss", &duplicate, now).unwrap(),
        Eligibility::CoolingDown { .. }
    ));
    assert!(
        next.reserve_at("boss", &duplicate, &settings(&second.id), now)
            .is_err()
    );
    age_sent(&mut first, id, now - dedupe::WINDOW_SECONDS - 1);
    assert_eq!(
        Store::eligibility_at(&next.conn, "boss", &duplicate, now).unwrap(),
        Eligibility::Eligible
    );
    let second_id = next
        .reserve_at("boss", &duplicate, &settings(&second.id), now)
        .unwrap();
    assert_ne!(second_id, id);
    assert_eq!(
        next.conn
            .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        next.conn
            .query_row(
                "SELECT account_id FROM attempt_identity WHERE attempt_id=?1",
                [second_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        second.id
    );
}
#[test]
fn uncertain_and_partial_results_never_expire_automatically() {
    for state in ["reserved", "ambiguous", "partial"] {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let now = Local::now().timestamp();
        let id = store
            .reserve_at("boss", &candidate("a"), &settings(DEFAULT_ACCOUNT), now)
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE attempts SET status=?2,at=?3 WHERE id=?1",
                params![id, state, now - 100 * 86400],
            )
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE attempt_identity SET cooldown_at=?2 WHERE attempt_id=?1",
                params![id, now - 100 * 86400],
            )
            .unwrap();
        assert!(matches!(
            store.eligibility("boss", &candidate("alias")).unwrap(),
            Eligibility::NeedsReview { .. }
        ));
        assert!(store.resolve_attempt(id, false, false).is_err());
        if state == "partial" {
            assert!(store.resolve_attempt(id, false, true).is_err());
        }
    }
}
#[test]
fn different_roles_and_descriptions_in_one_company_remain_separate() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let first = candidate("a");
    assert!(store.add(&first).unwrap());
    let same = candidate("alias");
    assert!(!store.add(&same).unwrap());
    let mut second = candidate("b");
    second.title = "AI应用工程师".into();
    assert!(store.add(&second).unwrap());
    let mut third = candidate("c");
    third.jd = "面向2027届，负责数据检索和大模型Agent应用开发。".into();
    assert!(store.add(&third).unwrap());
    assert_eq!(store.jobs().unwrap().len(), 3);
}
#[test]
fn same_fingerprint_cannot_be_reserved_by_two_accounts_concurrently() {
    let dir = tempfile::tempdir().unwrap();
    let mut root = Store::open(dir.path()).unwrap();
    let second = root.create_account("账号2").unwrap();
    let gate = Arc::new(Barrier::new(2));
    let mut workers = Vec::new();
    for (id, url) in [(DEFAULT_ACCOUNT.to_owned(), "one"), (second.id, "two")] {
        let data = dir.path().to_path_buf();
        let gate = gate.clone();
        workers.push(std::thread::spawn(move || {
            let mut store = Store::for_account(&data, &id).unwrap();
            gate.wait();
            store
                .reserve("boss", &candidate(url), &settings(&id))
                .is_ok()
        }));
    }
    assert_eq!(
        workers
            .into_iter()
            .map(|h| usize::from(h.join().unwrap()))
            .sum::<usize>(),
        1
    );
    assert_eq!(
        root.conn
            .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn each_account_has_150_independent_slots_and_no_alias_can_reset_counts() {
    let dir = tempfile::tempdir().unwrap();
    let mut root = Store::open(dir.path()).unwrap();
    let a = root.create_account("账号2").unwrap();
    let b = root.create_account("账号3").unwrap();
    assert!(root.create_account("账号4").is_err());
    let day = Local::now().format("%Y-%m-%d").to_string();
    let now = Local::now().timestamp();
    for (slot, id) in [DEFAULT_ACCOUNT.to_owned(), a.id, b.id].iter().enumerate() {
        for i in 0..149 {
            root.conn.execute("INSERT INTO attempts(site,url,title,company,at,day,status,note) VALUES('boss',?1,'旧岗位','旧公司',0,?2,'sent','fixture')",params![format!("https://www.zhipin.com/job_detail/history-{slot}-{i}.html"),day]).unwrap();
            let row = root.conn.last_insert_rowid();
            root.conn
                .execute(
                    "INSERT INTO attempt_identity VALUES(?1,?2,'',?3)",
                    params![row, id, now - 100],
                )
                .unwrap();
        }
        let mut actor = Store::for_account(dir.path(), id).unwrap();
        let mut job = candidate(&format!("fresh-{slot}"));
        job.jd = format!("{slot}账号的不同岗位简介");
        actor.reserve_at("boss", &job, &settings(id), now).unwrap();
        let mut extra = job.clone();
        extra.url = format!("https://www.zhipin.com/job_detail/extra-{slot}.html");
        extra.jd.push_str("另一个岗位");
        assert!(
            actor
                .reserve_at("boss", &extra, &settings(id), now + 31)
                .unwrap_err()
                .contains("今日投递上限")
        );
        root.update_account(id, None, Some(false), None).unwrap();
        root.update_account(id, None, Some(true), None).unwrap();
        assert_eq!(root.account(id, 150).unwrap().remaining, 0);
    }
    assert_eq!(
        root.conn
            .query_row("SELECT COUNT(*) FROM attempts WHERE day=?1", [day], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        450
    );
}
#[test]
fn planning_balances_distinct_jobs_without_sending_and_claims_are_disjoint() {
    let dir = tempfile::tempdir().unwrap();
    let mut root = Store::open(dir.path()).unwrap();
    let a = root.create_account("账号2").unwrap();
    let b = root.create_account("账号3").unwrap();
    let ids = vec![DEFAULT_ACCOUNT.to_owned(), a.id, b.id];
    for i in 0..12 {
        let mut job = candidate(&format!("job-{i}"));
        job.jd = format!("独立岗位{i}的完整简介");
        root.add(&job).unwrap();
    }
    let preview = root.plan_accounts(&ids, 12, 150, false).unwrap();
    assert_eq!(preview.assigned, 12);
    assert!(preview.assignments.iter().all(|a| a.jobs.len() == 4));
    assert!(root.jobs().unwrap().iter().all(|j| j.account_id.is_empty()));
    root.plan_accounts(&ids, 12, 150, true).unwrap();
    let mut urls = HashSet::new();
    for id in &ids {
        let mut actor = Store::for_account(dir.path(), id).unwrap();
        let mut visited = HashSet::new();
        while let Some(job) = actor.next_pending(&visited).unwrap() {
            visited.insert(job.url.clone());
            assert!(urls.insert(job.url));
        }
        assert_eq!(visited.len(), 4);
    }
    assert_eq!(
        root.conn
            .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(
        root.plan_accounts(&[String::new(), DEFAULT_ACCOUNT.into()], 12, 150, false)
            .is_err()
    );
}
#[test]
fn reservation_and_identity_roll_back_together_and_snapshots_are_immutable() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let job = candidate("a");
    store.add(&job).unwrap();
    store.conn.execute_batch("CREATE TRIGGER reject_identity BEFORE INSERT ON attempt_identity BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(
        store
            .reserve("boss", &job, &settings(DEFAULT_ACCOUNT))
            .is_err()
    );
    assert_eq!(
        store
            .conn
            .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(store.jobs().unwrap()[0].status, "pending");
    store
        .conn
        .execute_batch("DROP TRIGGER reject_identity;")
        .unwrap();
    let id = store
        .reserve("boss", &job, &settings(DEFAULT_ACCOUNT))
        .unwrap();
    let snapshot: String = store
        .conn
        .query_row(
            "SELECT payload FROM attempt_job_snapshots WHERE attempt_id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    let mut changed = job.clone();
    changed.jd = "新的岗位简介".into();
    assert!(!store.add(&changed).unwrap());
    assert_eq!(
        store
            .conn
            .query_row(
                "SELECT payload FROM attempt_job_snapshots WHERE attempt_id=?1",
                [id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        snapshot
    );
    let before = store.stop_generation().unwrap();
    assert_ne!(store.request_stop().unwrap(), before);
    assert_eq!(
        Store::open(dir.path()).unwrap().stop_generation().unwrap(),
        before + 1
    );
}

#[test]
fn old_schema_migration_retains_rows_and_unknown_dates_cannot_expire_early() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = Connection::open(dir.path().join("hiring.sqlite")).unwrap();
    legacy.execute_batch("CREATE TABLE queue(url TEXT PRIMARY KEY,payload TEXT NOT NULL,status TEXT NOT NULL,priority INTEGER NOT NULL);
        CREATE TABLE attempts(id INTEGER PRIMARY KEY,site TEXT NOT NULL,url TEXT NOT NULL,title TEXT NOT NULL,company TEXT NOT NULL,at INTEGER NOT NULL,day TEXT NOT NULL,status TEXT NOT NULL,note TEXT NOT NULL);
        CREATE TABLE guard(key TEXT PRIMARY KEY,value INTEGER NOT NULL);
        CREATE TABLE imports(source TEXT PRIMARY KEY);
        INSERT INTO guard VALUES('boss:search:2026-10-27',55);
        INSERT INTO imports VALUES('legacy-source');").unwrap();
    let job = candidate("legacy");
    let payload = serde_json::to_string(&job).unwrap();
    legacy
        .execute(
            "INSERT INTO queue VALUES(?1,?2,'sent',7)",
            params![job.url, payload],
        )
        .unwrap();
    legacy
        .execute(
            "INSERT INTO attempts VALUES(1,'boss',?1,?2,?3,0,'unknown','sent','old note')",
            params![job.url, job.title, job.company],
        )
        .unwrap();
    drop(legacy);
    let mut store = Store::open(dir.path()).unwrap();
    assert!(matches!(
        store.eligibility("boss", &candidate("alias")).unwrap(),
        Eligibility::CoolingDown { .. }
    ));
    assert!(!store.add(&job).unwrap());
    let retained: (String, String, i64) = store
        .conn
        .query_row("SELECT payload,status,priority FROM queue", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(retained, (payload, "sent".into(), 7));
    let old: (i64, String, String) = store
        .conn
        .query_row("SELECT at,day,note FROM attempts", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(old, (0, "unknown".into(), "old note".into()));
    age_sent(
        &mut store,
        1,
        Local::now().timestamp() - dedupe::WINDOW_SECONDS - 10,
    );
    assert!(store.add(&job).unwrap());
    assert_eq!(store.jobs().unwrap()[0].status, "pending");
    assert_eq!(
        store
            .conn
            .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    let second = store.create_account("账号2").unwrap();
    let third = store.create_account("账号3").unwrap();
    let all = store.accounts(150).unwrap();
    assert_eq!(
        all.iter().map(|a| a.port()).collect::<Vec<_>>(),
        vec![19333, 19334, 19335]
    );
    assert_eq!(all[0].browser_directory(dir.path()), dir.path());
    assert_ne!(
        second.browser_directory(dir.path()),
        third.browser_directory(dir.path())
    );
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(reopened.accounts(150).unwrap().len(), 3);
    assert_eq!(
        reopened
            .conn
            .query_row(
                "SELECT value FROM guard WHERE key='boss:search:2026-10-27'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        55
    );
}
