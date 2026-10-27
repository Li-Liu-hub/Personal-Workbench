use super::*;
use std::sync::atomic::AtomicBool;

fn sample(url: &str) -> Job {
    let mut job = Job {
        title: "2027届 Java开发工程师".into(),
        company: "样例公司".into(),
        source_url: url.into(),
        summary: "本科，工作地点上海".into(),
        ..Default::default()
    };
    job.identify();
    job
}

#[test]
fn urls_keep_job_identity_and_hash_routes_but_remove_tracking() {
    assert_eq!(
        canonical_url("https://jobs.example.com/jobs?id=7&utm_source=x#/campus").unwrap(),
        "https://jobs.example.com/jobs?id=7#/campus"
    );
    assert_ne!(
        sample("https://jobs.example.com/?id=1").id,
        sample("https://jobs.example.com/?id=2").id
    );
    for value in [
        "file:///C:/secret",
        "javascript:alert(1)",
        "https://user:pass@example.com/",
    ] {
        assert!(canonical_url(value).is_none());
    }
}
#[test]
fn graduation_is_never_derived_from_posting_date_or_arbitrary_year() {
    assert_eq!(parse::cohort("发布日期2026-10-26，2027年入职"), "");
    assert_eq!(
        parse::cohort("2027届校园招聘，面向2026届、2027届毕业生"),
        "2026、2027"
    );
    let jobs=parse::ncss(r#"{"data":{"list":[{"jobId":"7","jobName":"Java开发","recName":"企业","publishDate":1792983600000}]}}"#).unwrap();
    assert!(jobs[0].cohort.is_empty());
    assert!(jobs[0].deadline.is_empty());
    assert!(jobs[0].apply_url.is_empty());
}
#[test]
fn ncss_rejects_challenges_and_schema_changes() {
    assert!(parse::ncss("<html>需要验证</html>").is_err());
    assert!(parse::ncss(r#"{"data":{}}"#).is_err());
}
#[test]
fn structured_job_and_explicit_application_link_are_read_without_recommendations() {
    let html = r#"<script type="application/ld+json">{"@graph":[{"@type":"JobPosting","title":"2027届Java工程师","hiringOrganization":{"name":"样例公司"},"description":"本科，Java后端","datePosted":"2026-10-25","validThrough":"2026-12-01","jobLocation":{"address":{"addressLocality":"上海"}}}]}</script><main><a href="https://jobs.example.com/campus/#/job/7">官网投递</a></main><footer><a href="https://wrong.example.com/">立即申请</a></footer>"#;
    let jobs = parse::details(html, "https://university.example.com/notice/1", "高校公告");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].cohort, "2027");
    assert_eq!(jobs[0].location, "上海");
    assert_eq!(jobs[0].deadline, "2026-12-01");
    assert_eq!(jobs[0].apply_url, "https://jobs.example.com/campus/#/job/7");
}
#[test]
fn rss_handles_entities_and_rejects_verification_pages() {
    let rows=parse::rss("<rss><channel><item><title>校招 &amp; 实习</title><link>https://example.com/?id=1&amp;type=campus</link><description>2027届招聘</description></item></channel></rss>").unwrap();
    assert_eq!(rows[0].0, "校招 & 实习");
    assert!(rows[0].1.contains("&type=campus"));
    assert!(parse::rss("<html>captcha</html>").is_err());
}
#[test]
fn cancelled_search_never_starts_network_requests() {
    let flag = AtomicBool::new(true);
    let output = search(&Options::default(), &flag, |_| {
        panic!("cancelled before start")
    })
    .unwrap();
    assert!(output.cancelled);
    assert!(output.jobs.is_empty());
}
#[test]
fn options_validate_sources_year_and_supplementary_urls() {
    let mut options = Options {
        year: "27".into(),
        ..Default::default()
    };
    assert!(options.validate().is_err());
    options.year = "2027".into();
    options.extra_urls = "file:///C:/test".into();
    assert!(options.validate().is_err());
    options.extra_urls.clear();
    options.ncss = false;
    options.wonder = false;
    options.web = false;
    assert!(options.validate().is_err());
    options.extra_urls = "https://example.com/jobs".into();
    assert!(options.validate().is_ok());
}
#[test]
fn refreshing_and_reopening_preserves_user_flags_and_first_seen() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = crate::db::Database::open(dir.path()).unwrap();
    let mut job = sample("https://jobs.example.com/job/1");
    job.first_seen = "2026-10-01 00:00:00".into();
    db.save_campus_jobs(&[job.clone()]).unwrap();
    job.favorite = true;
    job.applied = true;
    job.ignored = true;
    db.set_campus_flags(&job).unwrap();
    let mut refresh = sample(&job.source_url);
    refresh.summary = "新说明".into();
    db.save_campus_jobs(&[refresh]).unwrap();
    drop(db);
    let db = crate::db::Database::open(dir.path()).unwrap();
    let jobs = db.campus_jobs().unwrap();
    assert_eq!(jobs.len(), 1);
    assert!(jobs[0].favorite && jobs[0].applied && jobs[0].ignored);
    assert_eq!(jobs[0].summary, "新说明");
    assert_eq!(jobs[0].first_seen, "2026-10-01 00:00:00");
}
#[test]
fn export_keeps_formula_like_titles_as_text_and_preserves_existing_file_on_failure() {
    use calamine::Reader;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("校招.xlsx");
    let mut job = sample("https://jobs.example.com/1");
    job.title = "=HYPERLINK(\"wrong\")".into();
    export_xlsx(&path, &[job.clone()]).unwrap();
    let mut workbook = calamine::open_workbook_auto(&path).unwrap();
    let range = workbook.worksheet_range_at(0).unwrap().unwrap();
    assert_eq!(range.get((1, 1)).unwrap().to_string(), job.title);
    assert_eq!(range.height(), 2);
    let bad = dir.path().join("missing").join("out.xlsx");
    assert!(export_xlsx(&bad, &[job]).is_err());
    assert!(path.exists());
}
#[test]
#[ignore = "requires live recruiting websites; writes evidence only when explicitly configured"]
fn live_campus_sources() {
    let path = std::env::var("PERSONAL_DESK_CAMPUS_LIVE_OUTPUT").expect("set evidence output path");
    let extra_url = std::env::var("PERSONAL_DESK_CAMPUS_LIVE_EXTRA_URL").unwrap_or_default();
    let options = Options {
        pages: 1,
        extra_urls: extra_url.clone(),
        ..Default::default()
    };
    let output = search(&options, &AtomicBool::new(false), |_| {}).unwrap();
    let evidence = serde_json::json!({"jobs":output.jobs,"reports":output.reports,"cancelled":output.cancelled});
    std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    if !extra_url.is_empty() {
        assert!(
            output
                .jobs
                .iter()
                .any(|j| j.source_url == extra_url && !j.apply_url.is_empty()),
            "official application link missing"
        );
    }
    assert!(
        output.jobs.len() >= 5,
        "too few live results: {}",
        evidence["reports"]
    );
    assert!(
        output.reports.iter().filter(|v| v.count > 0).count() >= 2,
        "fewer than two working sources: {}",
        evidence["reports"]
    );
}

#[test]
fn search_api_key_never_enters_settings_or_exports_and_api_results_are_typed() {
    let options = Options {
        tavily_key: "test-secret-must-not-persist".into(),
        ..Default::default()
    };
    let encoded = serde_json::to_string(&options).unwrap();
    assert!(!encoded.contains("test-secret"));
    assert!(!encoded.contains("tavily_key"));
    let decoded: Options = serde_json::from_str(&encoded).unwrap();
    assert!(decoded.tavily_key.is_empty());
    let rows=parse::search_api(r#"{"results":[{"title":"2027届Java开发招聘公告","url":"https://example.com/jobs/1","content":"本科"},{"title":"bad","url":"javascript:bad"}]}"#).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].2, "本科");
    assert!(parse::search_api(r#"{"detail":"unauthorized"}"#).is_err());
}

#[test]
fn tool_backup_includes_campus_records_and_full_snapshot_preserves_flags() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let mut db = crate::db::Database::open(&data).unwrap();
    let mut job = sample("https://jobs.example.com/jobs/backup");
    db.save_campus_jobs(&[job.clone()]).unwrap();
    job.favorite = true;
    db.set_campus_flags(&job).unwrap();
    drop(db);
    let path = dir.path().join("tools.zip");
    crate::export::write(&data, &path, crate::export::Scope::Tools).unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut content = String::new();
    use std::io::Read;
    archive
        .by_name("校招岗位.json")
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
    let rows: Vec<Job> = serde_json::from_str(&content).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].favorite);
}

#[test]
fn recommendation_cards_never_become_application_links_and_social_jobs_are_identified() {
    let html = r#"<script type="application/ld+json">{"@type":"JobPosting","title":"2027届Java开发","description":"本科毕业生"}</script><main><a class="campus-job-card" href="/other-job">立即投递</a><a href="/another-job">推荐：其他公司2027届招聘公告，面向高校应届毕业生招聘研发工程师，立即投递简历，查看完整信息</a></main>"#;
    let jobs = parse::details(html, "https://example.com/job/1", "样例");
    assert_eq!(jobs.len(), 1);
    assert!(jobs[0].apply_url.is_empty());
    assert!(jobs[0].is_campus());
    let mut social = Job {
        title: "高级Java工程师".into(),
        summary: "5年以上经验".into(),
        ..Default::default()
    };
    assert!(!social.is_campus());
    social.title = "Java实习生".into();
    assert!(social.is_campus());
}

#[test]
fn official_campus_header_and_public_application_link_are_kept() {
    let html = r#"<script type="application/ld+json">{"@type":"JobPosting","title":"前沿部署工程师 FDE","description":"本科，计算机专业","hiringOrganization":{"name":"样例公司"}}</script><main><div class="careers-jobhero"><p>2027 届 · 校园招聘</p><h1>前沿部署工程师 FDE</h1><a href="https://apply.example.com/campus">前往网申</a></div></main>"#;
    let job = parse::details(
        html,
        "https://example.com/careers/campus-fde.html",
        "企业官网",
    )
    .remove(0);
    assert_eq!(job.cohort, "2027");
    assert_eq!(job.apply_url, "https://apply.example.com/campus");
    assert!(job.is_campus());
}

#[test]
fn university_context_links_and_multiple_json_jobs_keep_correct_addresses() {
    let jobs = parse::details(
        r#"<h4>某公司2027届校园招聘</h4><div class="v_news_content"><p>2027届本科，Java方向</p><p>网申地址：<a href="https://campus.example.org/jobs">https://campus.example.org/jobs</a></p></div>"#,
        "https://uni.example.org/notice/1",
        "高校就业网",
    );
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].apply_url, "https://campus.example.org/jobs");
    let jobs = parse::details(
        r#"<script type="application/ld+json">[{"@type":"JobPosting","title":"2027届Java","url":"https://company.example/jobs/1"},{"@type":"JobPosting","title":"2027届测试","url":"https://company.example/jobs/2"}]</script>"#,
        "https://company.example/campus",
        "官网",
    );
    assert_eq!(jobs.len(), 2);
    assert_ne!(jobs[0].id, jobs[1].id);
}

#[test]
fn university_specific_heading_beats_site_section_heading() {
    let jobs = parse::details(
        r#"<h4>招聘信息</h4><h4 class="tit">江铃汽车2027届校园招聘</h4><div id="vsb_content_4_1009_u171"><p>2027届本科，Java方向</p><p>简历投递通道：<a href="https://campus.example.org/jobs">https://campus.example.org/jobs</a></p></div>"#,
        "https://uni.example.org/notice/1",
        "高校就业网",
    );
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].title, "江铃汽车2027届校园招聘");
    assert_eq!(jobs[0].apply_url, "https://campus.example.org/jobs");
    let mut job = jobs[0].clone();
    job.deadline = "2020/6/23".into();
    assert!(job.expired());
}
