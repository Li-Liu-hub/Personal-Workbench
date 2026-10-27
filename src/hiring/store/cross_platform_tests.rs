use super::*;
use crate::hiring::{
    dedupe::{self, Eligibility},
    platforms::Platform,
};
use std::sync::{Arc, Barrier};
fn job(site: &str) -> Job {
    Job {
        url: match site {
            "boss" => "https://www.zhipin.com/job_detail/global.html",
            "51job" => "https://jobs.51job.com/shanghai/12345678.html",
            "zhaopin" => "https://jobs.zhaopin.com/CC123456789J00000000001.htm",
            "shixiseng" => "https://www.shixiseng.com/intern/inn_global",
            _ => "https://careers.example.test/jobs/123",
        }
        .into(),
        title: "Java 后端".into(),
        company: "示例科技".into(),
        jd: "负责后端开发，熟悉Java与Spring".into(),
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
fn every_platform_uses_one_queue_and_shared_fifteen_day_boundary() {
    let d = tempfile::tempdir().unwrap();
    let mut store = Store::open(d.path()).unwrap();
    let now = Local::now().timestamp();
    assert!(store.add(&job("boss")).unwrap());
    for p in Platform::ALL {
        assert!(!store.platform_add(p, &job(p.id())).unwrap());
    }
    assert_eq!(
        store
            .conn
            .query_row("SELECT COUNT(*) FROM queue", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='platform_queue'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let id = store
        .reserve_at("boss", &job("boss"), &settings(), now)
        .unwrap();
    for site in ["51job", "zhaopin", "shixiseng", "official"] {
        assert!(matches!(
            store.eligibility(site, &job(site)).unwrap(),
            Eligibility::NeedsReview { .. }
        ));
    }
    for at in [
        now - dedupe::WINDOW_SECONDS,
        now - dedupe::WINDOW_SECONDS - 1,
    ] {
        store
            .conn
            .execute(
                "UPDATE attempts SET status='sent',at=?1 WHERE id=?2",
                params![at, id],
            )
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE attempt_identity SET cooldown_at=?1 WHERE attempt_id=?2",
                params![at, id],
            )
            .unwrap();
        for site in ["boss", "51job", "zhaopin", "shixiseng", "official"] {
            let eligible = Store::eligibility_at(&store.conn, site, &job(site), now).unwrap();
            assert_eq!(
                eligible == Eligibility::Eligible,
                at < now - dedupe::WINDOW_SECONDS,
                "{site}"
            );
        }
    }
    assert_eq!(
        store
            .conn
            .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn concurrent_different_platforms_can_reserve_same_job_only_once() {
    let d = tempfile::tempdir().unwrap();
    Store::open(d.path()).unwrap();
    let gate = Arc::new(Barrier::new(4));
    let mut workers = vec![];
    for site in ["boss", "51job", "zhaopin", "shixiseng"] {
        let path = d.path().to_owned();
        let gate = gate.clone();
        workers.push(std::thread::spawn(move || {
            let mut store = Store::open(&path).unwrap();
            gate.wait();
            store.reserve(site, &job(site), &settings())
        }));
    }
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    for error in results.into_iter().filter_map(Result::err) {
        assert!(
            error.contains("同一岗位"),
            "a duplicate must be blocked by identity, not just the interval: {error}"
        );
    }
    assert_eq!(
        Store::open(d.path())
            .unwrap()
            .conn
            .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn changed_cross_platform_description_and_unknown_legacy_identity_fail_closed() {
    let d = tempfile::tempdir().unwrap();
    let mut store = Store::open(d.path()).unwrap();
    let id = store.reserve("boss", &job("boss"), &settings()).unwrap();
    store.finish(id, "sent", "test").unwrap();
    let mut alias = job("51job");
    alias.jd = "平台重新编辑的简介，不应被自动当作另一个职位".into();
    assert!(
        store
            .eligibility("51job", &alias)
            .unwrap()
            .error()
            .is_some()
    );
    store
        .conn
        .execute(
            "UPDATE attempt_identity SET fingerprint='' WHERE attempt_id=?1",
            [id],
        )
        .unwrap();
    assert!(
        store
            .eligibility("shixiseng", &job("shixiseng"))
            .unwrap()
            .error()
            .is_some()
    );
    alias.title = "测试开发".into();
    assert_eq!(
        store.eligibility("51job", &alias).unwrap(),
        Eligibility::Eligible
    );
}
#[test]
fn shared_quota_interval_and_queue_recovery_survive_restart() {
    let d = tempfile::tempdir().unwrap();
    let mut store = Store::open(d.path()).unwrap();
    let now = Local::now().timestamp();
    let id = store
        .reserve_at("51job", &job("51job"), &settings(), now)
        .unwrap();
    store.finish(id, "failed", "verified not sent").unwrap();
    let mut other = job("zhaopin");
    other.title = "测试工程师".into();
    assert!(
        store
            .reserve_at("zhaopin", &other, &settings(), now + 1)
            .unwrap_err()
            .contains("最短间隔")
    );
    let mut one = settings();
    one.daily_limit = 1;
    assert!(
        store
            .reserve_at("zhaopin", &other, &one, now + 31)
            .unwrap_err()
            .contains("今日投递上限")
    );
    drop(store);
    assert_eq!(
        Store::open(d.path())
            .unwrap()
            .account("default", 150)
            .unwrap()
            .today_attempts,
        1
    );
}
