use super::*;
use std::sync::{Barrier, Mutex};
struct Fake<'a> {
    dir: &'a Path,
    settings: &'a Settings,
    barrier: Barrier,
    calls: Mutex<Vec<(u32, String)>>,
    sent: Mutex<Vec<Job>>,
    omit: Vec<&'static str>,
    fail: bool,
    pages: u32,
}
fn candidate(site: &str, page: u32) -> Job {
    Job {
        platform: site.into(),
        account_id: "default".into(),
        url: match site {
            "boss" => format!("https://www.zhipin.com/job_detail/unified{page}.html"),
            "zhaopin" => format!("https://jobs.zhaopin.com/CC123456789J000000000{page:02}.htm"),
            "51job" => format!("https://jobs.51job.com/shanghai/123456{page:02}.html"),
            _ => format!("https://www.shixiseng.com/intern/inn_unified{page}"),
        },
        title: format!("Java岗位{page}"),
        company: "测试科技".into(),
        jd: format!("开发第{page}套后端服务"),
        ..Default::default()
    }
}
impl Backend for Fake<'_> {
    fn search(&self, site: &str, page: u32) -> Result<Batch> {
        self.calls.lock().unwrap().push((page, site.into()));
        self.barrier.wait();
        if self.fail && site == "boss" {
            return Err("BOSS需要人工登录".into());
        }
        Ok(Batch {
            site: site.into(),
            account_id: "default".into(),
            jobs: if page > self.pages || self.omit.contains(&site) {
                vec![]
            } else {
                vec![candidate(site, page)]
            },
        })
    }
    fn preflight(&self, job: &Job) -> Result<()> {
        assert!(
            Store::open(self.dir)?
                .eligibility(&job.platform, job)?
                .error()
                .is_none()
        );
        Ok(())
    }
    fn send(&self, job: &Job) -> Result<()> {
        // Only the synthetic ledger is aged, avoiding real sleep in this test.
        rusqlite::Connection::open(self.dir.join("hiring.sqlite"))
            .unwrap()
            .execute("UPDATE attempts SET at=at-31", [])
            .unwrap();
        let mut store = Store::open(self.dir)?;
        let id = store.reserve(&job.platform, job, self.settings)?;
        store.finish(id, "sent", "fixture")?;
        self.sent.lock().unwrap().push(job.clone());
        Ok(())
    }
}
fn settings() -> Settings {
    Settings {
        daily_limit: 3,
        unified_max_pages: 6,
        send_windows: vec!["00:00-23:59".into()],
        ..Default::default()
    }
}
fn fake<'a>(dir: &'a Path, settings: &'a Settings) -> Fake<'a> {
    Fake {
        dir,
        settings,
        barrier: Barrier::new(4),
        calls: Mutex::new(vec![]),
        sent: Mutex::new(vec![]),
        omit: vec![],
        fail: false,
        pages: 6,
    }
}
#[test]
fn four_parallel_collectors_keep_source_accounts_prefer_boss_and_refill_to_daily_cap() {
    let d = tempfile::tempdir().unwrap();
    let settings = settings();
    let backend = fake(d.path(), &settings);
    let report = run_with(
        d.path(),
        &settings,
        false,
        &AtomicBool::new(false),
        &backend,
        |_| {},
        false,
    )
    .unwrap();
    assert!(report.contains("达到统一上限"), "{report}");
    let sent = backend.sent.lock().unwrap();
    assert_eq!(sent.len(), 3);
    assert!(sent.iter().all(|j| j.platform == "boss"));
    assert_eq!(backend.calls.lock().unwrap().len(), 12);
    let store = Store::open(d.path()).unwrap();
    assert_eq!(store.today_total().unwrap(), 3);
    let queue = super::super::tracking::read(d.path()).unwrap().queue;
    assert_eq!(queue.len(), 12);
    assert!(
        queue
            .iter()
            .all(|j| !j.platform.is_empty() && j.account_id == "default")
    );
    assert_eq!(queue.iter().filter(|j| j.status == "duplicate").count(), 9);
}
#[test]
fn priority_falls_back_in_requested_order_and_never_depends_on_arrival_order() {
    for (omitted, expected) in [
        (vec!["boss"], "zhaopin"),
        (vec!["boss", "zhaopin"], "51job"),
        (vec!["boss", "zhaopin", "51job"], "shixiseng"),
    ] {
        let d = tempfile::tempdir().unwrap();
        let settings = Settings {
            daily_limit: 1,
            ..settings()
        };
        let mut backend = fake(d.path(), &settings);
        backend.omit = omitted;
        run_with(
            d.path(),
            &settings,
            false,
            &AtomicBool::new(false),
            &backend,
            |_| {},
            false,
        )
        .unwrap();
        assert_eq!(backend.sent.lock().unwrap()[0].platform, expected);
    }
    let d = tempfile::tempdir().unwrap();
    let mut store = Store::open(d.path()).unwrap();
    for site in ORDER.into_iter().rev() {
        store
            .discover(site, "default", &candidate(site, 1))
            .unwrap();
    }
    assert_eq!(store.unified_candidates().unwrap()[0].platform, "boss");
    assert!(
        matches!(store.eligibility("shixiseng",&candidate("shixiseng",1)).unwrap(),crate::hiring::dedupe::Eligibility::PreferredSource{platform,..} if platform=="boss")
    );
}
#[test]
fn search_preview_does_not_send_and_error_exhaustion_and_stop_are_bounded() {
    let d = tempfile::tempdir().unwrap();
    let settings = settings();
    let backend = fake(d.path(), &settings);
    run_with(
        d.path(),
        &settings,
        true,
        &AtomicBool::new(false),
        &backend,
        |_| {},
        false,
    )
    .unwrap();
    assert!(backend.sent.lock().unwrap().is_empty());
    assert_eq!(Store::open(d.path()).unwrap().today_total().unwrap(), 0);
    let d = tempfile::tempdir().unwrap();
    let mut backend = fake(d.path(), &settings);
    backend.fail = true;
    assert!(
        run_with(
            d.path(),
            &settings,
            false,
            &AtomicBool::new(false),
            &backend,
            |_| {},
            false
        )
        .is_err()
    );
    assert!(backend.sent.lock().unwrap().is_empty());
    let d = tempfile::tempdir().unwrap();
    let mut backend = fake(d.path(), &settings);
    backend.pages = 0;
    let report = run_with(
        d.path(),
        &settings,
        false,
        &AtomicBool::new(false),
        &backend,
        |_| {},
        false,
    )
    .unwrap();
    assert!(report.contains("连续两轮"));
    assert_eq!(backend.calls.lock().unwrap().len(), 8);
    let d = tempfile::tempdir().unwrap();
    let backend = fake(d.path(), &settings);
    run_with(
        d.path(),
        &settings,
        false,
        &AtomicBool::new(true),
        &backend,
        |_| {},
        false,
    )
    .unwrap();
    assert!(backend.calls.lock().unwrap().is_empty());
}
#[test]
fn reaching_existing_daily_cap_starts_no_search_and_no_send() {
    let d = tempfile::tempdir().unwrap();
    let settings = Settings {
        daily_limit: 1,
        ..settings()
    };
    let mut store = Store::open(d.path()).unwrap();
    let id = store
        .reserve("boss", &candidate("boss", 1), &settings)
        .unwrap();
    store.finish(id, "ambiguous", "fixture").unwrap();
    let backend = fake(d.path(), &settings);
    let report = run_with(
        d.path(),
        &settings,
        false,
        &AtomicBool::new(false),
        &backend,
        |_| {},
        false,
    )
    .unwrap();
    assert!(report.contains("达到统一上限"));
    assert!(backend.calls.lock().unwrap().is_empty());
}
