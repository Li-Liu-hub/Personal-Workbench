use super::*;
fn job(p: Platform) -> Job {
    Job {
        url: match p {
            Platform::Job51 => "https://jobs.51job.com/shanghai/12345678.html",
            Platform::Zhaopin => "https://jobs.zhaopin.com/CC123456789J00000000001.htm",
            Platform::Shixiseng => "https://www.shixiseng.com/intern/inn_fixture",
        }
        .into(),
        title: "Java开发".into(),
        company: "测试科技".into(),
        jd: "负责后端开发与自动化测试".into(),
        ..Default::default()
    }
}
#[test]
fn platform_boundaries_city_codes_and_url_dedup() {
    let mut ports = std::collections::HashSet::new();
    for p in Platform::ALL {
        assert!(ports.insert(p.site().port()));
        let j = job(p);
        assert_eq!(
            p.canonical(&format!("{}?utm_source=x#tracking", j.url))
                .unwrap(),
            j.url
        );
        for bad in [
            "https://51job.com.evil.test/job.htm",
            "http://jobs.51job.com/x.html",
            "https://jobs.51job.com:8443/x.html",
            "https://user:pass@www.shixiseng.com/intern/x",
            "https://127.0.0.1/job.htm",
        ] {
            assert!(p.canonical(bad).is_err());
        }
        assert!(p.canonical(p.home()).is_err());
        let url = p.search_url("C++ & Java", "上海市", 2).unwrap();
        let u = url::Url::parse(&url).unwrap();
        let pairs: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
        assert_eq!(
            pairs
                .get(if p == Platform::Zhaopin {
                    "kw"
                } else {
                    "keyword"
                })
                .unwrap(),
            "C++ & Java"
        );
        assert_eq!(
            pairs
                .get(match p {
                    Platform::Job51 => "jobArea",
                    Platform::Zhaopin => "jl",
                    Platform::Shixiseng => "city",
                })
                .unwrap(),
            match p {
                Platform::Job51 => "020000",
                Platform::Zhaopin => "538",
                Platform::Shixiseng => "上海",
            }
        );
    }
    assert!(
        Platform::Job51
            .search_url("Java", "未配置的城市", 1)
            .is_err()
    );
}
#[test]
fn platform_queues_survive_restart_share_dedupe_and_preserve_boss() {
    let d = tempfile::tempdir().unwrap();
    let mut store = Store::open(d.path()).unwrap();
    let boss = Job {
        url: "https://www.zhipin.com/job_detail/fixture.html".into(),
        ..Default::default()
    };
    store.add(&boss).unwrap();
    let settings = Settings {
        send_windows: vec!["00:00-23:59".into()],
        ..Default::default()
    };
    for p in Platform::ALL {
        let mut separate = job(p);
        separate.title = format!("{}岗位", p.id());
        assert!(store.platform_add(p, &separate).unwrap());
        assert!(!store.platform_add(p, &separate).unwrap());
        assert_eq!(store.platform_jobs(p).unwrap().len(), 1);
    }
    let p = Platform::Job51;
    let id = store.reserve(p.id(), &job(p), &settings).unwrap();
    drop(store);
    let mut store = Store::open(d.path()).unwrap();
    assert_eq!(store.platform_jobs(p).unwrap()[0].status, "ambiguous");
    assert!(store.platform_status(p, &job(p).url, "pending").is_err());
    assert!(!store.platform_add(p, &job(p)).unwrap());
    assert!(
        store
            .eligibility(Platform::Zhaopin.id(), &job(Platform::Zhaopin))
            .unwrap()
            .error()
            .is_some()
    );
    store.resolve_attempt(id, false, true).unwrap();
    store.platform_status(p, &job(p).url, "pending").unwrap();
    // Move the verified failed fixture into the past without waiting in a unit test.
    rusqlite::Connection::open(d.path().join("hiring.sqlite"))
        .unwrap()
        .execute("UPDATE attempts SET at=at-31", [])
        .unwrap();
    // A confirmed result blocks the same company/title/JD on every platform.
    let mut distinct = job(Platform::Shixiseng);
    distinct.jd.push_str("和数据库优化");
    let id = store
        .reserve(Platform::Shixiseng.id(), &distinct, &settings)
        .unwrap();
    store.finish(id, "sent", "fixture").unwrap();
    let mut copy = job(Platform::Zhaopin);
    copy.jd = distinct.jd.clone();
    assert!(!store.platform_add(Platform::Zhaopin, &copy).unwrap());
    assert_eq!(store.jobs().unwrap().len(), 1);
    assert_eq!(store.jobs().unwrap()[0].url, boss.url);
    assert!(
        crate::hiring::tracking::read(d.path())
            .unwrap()
            .attempts
            .len()
            >= 2
    );
}
#[test]
fn page_changes_and_missing_identity_cannot_pass_review() {
    let p = Platform::Shixiseng;
    let page = json!({"url":job(p).url,"job":job(p),"buttons":[{"id":0,"label":"投递简历"}],"fields":[["resume","select","resume-a",false]]});
    assert!(read_job(&page, p).is_ok());
    let mut encrypted = page.clone();
    encrypted["job"]["salary"] = json!("\u{e123}/天");
    assert_eq!(read_job(&encrypted, p).unwrap().salary, "薪资请在网页核验");
    encrypted["job"]["jd"] = json!("\u{e123}负责开发");
    assert!(read_job(&encrypted, p).is_err());
    let digest = review_digest(&page);
    for key in ["url", "job", "buttons", "fields"] {
        let mut changed = page.clone();
        changed[key] = json!("changed");
        assert_ne!(digest, review_digest(&changed));
    }
    for signal in ["captcha", "login", "unavailable", "truncated"] {
        let mut changed = page.clone();
        changed[signal] = json!(true);
        assert!(read_job(&changed, p).is_err());
    }
    let mut changed = page.clone();
    changed["job"]["jd"] = json!("");
    assert!(read_job(&changed, p).is_err());
}
