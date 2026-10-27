use super::{parse, *};
use std::{collections::HashSet, time::Duration};

struct Search<'a, F: FnMut(Progress)> {
    agent: ureq::Agent,
    options: &'a Options,
    cancel: &'a AtomicBool,
    progress: F,
    seen: HashSet<String>,
    outcome: Outcome,
}
impl<F: FnMut(Progress)> Search<'_, F> {
    fn get(&self, url: &str) -> Result<String, String> {
        if cancelled(self.cancel) {
            return Err("搜索已停止".into());
        }
        let mut last_error = String::new();
        for attempt in 0..2 {
            if cancelled(self.cancel) {
                return Err("搜索已停止".into());
            }
            let response = (|| -> Result<String, String> {
                self.agent
                    .get(url)
                    .header(
                        "User-Agent",
                        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) PersonalDeskCampus/1.0",
                    )
                    .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.7")
                    .header("Referer", "https://www.ncss.cn/student/jobs/index.html")
                    .call()
                    .map_err(|e| format!("读取失败：{e}"))?
                    .body_mut()
                    .with_config()
                    .limit(4_000_000)
                    .read_to_string()
                    .map_err(|e| format!("读取正文失败：{e}"))
            })();
            match response {
                Ok(body) => return Ok(body),
                Err(error) => {
                    let transient = ["timeout", "io:", "connection"]
                        .iter()
                        .any(|word| error.contains(word));
                    last_error = error;
                    if attempt > 0 || !transient {
                        break;
                    }
                }
            }
        }
        Err(last_error)
    }
    fn accept(&mut self, values: Vec<Job>) -> usize {
        let mut added = Vec::new();
        for mut job in values {
            if job.source_url.is_empty() || job.title.is_empty() {
                continue;
            }
            if !self.options.include_unmarked && !job.is_campus() {
                continue;
            }
            if !self.options.year.is_empty()
                && !job.cohort.is_empty()
                && !job.cohort.contains(&self.options.year)
            {
                continue;
            }
            if !self.options.city.is_empty()
                && !format!("{} {}", job.location, job.summary).contains(self.options.city.trim())
            {
                continue;
            }
            if self.seen.insert(job.id.clone()) {
                job.summary = job.summary.chars().take(6000).collect();
                added.push(job);
            }
        }
        let count = added.len();
        self.outcome.jobs.extend(added.clone());
        if count > 0 {
            (self.progress)(Progress::Jobs(added));
        }
        count
    }
    fn report(&mut self, source: &str, start: usize, error: String) {
        let report = SourceReport {
            source: source.into(),
            count: self.outcome.jobs.len() - start,
            error,
        };
        (self.progress)(Progress::Source(report.clone()));
        self.outcome.reports.push(report);
    }
    fn enrich(&self, job: &mut Job) {
        if cancelled(self.cancel) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
        if let Ok(body) = self.get(&job.source_url)
            && let Some(detail) = parse::details(&body, &job.source_url, &job.source)
                .into_iter()
                .next()
        {
            if !detail.summary.is_empty() {
                job.summary = detail.summary;
            }
            if !detail.cohort.is_empty() {
                job.cohort = detail.cohort;
            }
            if !detail.company.is_empty() {
                job.company = detail.company;
            }
            if !detail.location.is_empty() {
                job.location = detail.location;
            }
            if !detail.deadline.is_empty() {
                job.deadline = detail.deadline;
            }
            if !detail.published.is_empty() {
                job.published = detail.published;
            }
            if !detail.title.is_empty() {
                job.title = detail.title;
            }
            job.apply_url = detail.apply_url;
        }
    }
    fn ncss(&mut self) {
        let start = self.outcome.jobs.len();
        let mut error = String::new();
        'terms: for term in self.options.terms() {
            for page in 1..=self.options.pages {
                if cancelled(self.cancel) {
                    break 'terms;
                }
                (self.progress)(Progress::Status(format!(
                    "国家大学生就业服务平台 · {term} · 第 {page} 页"
                )));
                let mut url =
                    url::Url::parse("https://www.ncss.cn/student/jobs/jobslist/ajax/").unwrap();
                url.query_pairs_mut().extend_pairs([
                    ("jobName", term.as_str()),
                    ("offset", &page.to_string()),
                    ("limit", "20"),
                ]);
                match self.get(url.as_str()).and_then(|body| parse::ncss(&body)) {
                    Ok(mut jobs) => {
                        if jobs.is_empty() {
                            break;
                        }
                        // Read a bounded number of originals to keep search responsive.
                        for job in jobs.iter_mut().take(3) {
                            self.enrich(job);
                        }
                        self.accept(jobs);
                    }
                    Err(e) => {
                        error = e;
                        break;
                    }
                }
            }
        }
        self.report("国家大学生就业服务平台", start, error);
    }
    fn wonder(&mut self) {
        let start = self.outcome.jobs.len();
        let mut error = String::new();
        for term in self.options.terms() {
            if cancelled(self.cancel) {
                break;
            }
            (self.progress)(Progress::Status(format!("校招公告 · {term}")));
            let mut url = url::Url::parse("https://www.wondercv.com/xiaozhao/").unwrap();
            url.query_pairs_mut().append_pair("keyword", &term);
            match self.get(url.as_str()) {
                Ok(body) => {
                    let mut jobs = parse::wonder(&body, url.as_str());
                    if jobs.is_empty() {
                        error = "未找到可读取的公告；可能无匹配结果或页面结构已变化".into();
                    }
                    for job in jobs.iter_mut().take(12) {
                        self.enrich(job);
                    }
                    self.accept(jobs);
                }
                Err(e) => error = e,
            }
        }
        self.report("超级简历 · 校招公告", start, error);
    }
    fn web(&mut self) {
        let start = self.outcome.jobs.len();
        let mut error = String::new();
        let terms = self.options.terms();
        for term in terms.iter().take(2) {
            if cancelled(self.cancel) {
                break;
            }
            (self.progress)(Progress::Status(format!(
                "全网发现 · {term} · 读取招聘原文"
            )));
            let query = format!(
                "校园招聘 {term} {} {} 公告 岗位",
                self.options.year, self.options.city
            );
            let mut url = url::Url::parse("https://www.bing.com/search").unwrap();
            url.query_pairs_mut().extend_pairs([
                ("format", "rss"),
                ("q", query.as_str()),
                ("mkt", "zh-CN"),
            ]);
            let results = if self.options.tavily_key.trim().is_empty() {
                self.get(url.as_str()).and_then(|body| parse::rss(&body))
            } else {
                self.agent.post("https://api.tavily.com/search")
                    .header("Authorization",&format!("Bearer {}",self.options.tavily_key.trim()))
                    .send_json(serde_json::json!({"query":query,"search_depth":"basic","max_results":10,"include_answer":false,"include_raw_content":false}))
                    .map_err(|e|format!("增强搜索请求失败，请检查密钥或额度：{e}"))
                    .and_then(|mut r|r.body_mut().with_config().limit(4_000_000).read_to_string().map_err(|e|e.to_string()))
                    .and_then(|body|parse::search_api(&body))
            };
            match results {
                Ok(items) => {
                    for (title, url, summary) in items.into_iter().take(10) {
                        if cancelled(self.cancel) {
                            break;
                        }
                        if !["招聘", "校招", "岗位", "应届"]
                            .iter()
                            .any(|v| format!("{title} {summary}").contains(v))
                        {
                            continue;
                        }
                        if let Err(e) = self.page(&url, "全网发现", Some((&title, &summary))) {
                            error = e;
                        }
                    }
                }
                Err(e) => error = e,
            }
        }
        if self.outcome.jobs.len() == start && error.is_empty() && !cancelled(self.cancel) {
            error = "未返回可读取的招聘结果；可调整关键词、使用其他来源或配置增强搜索".into();
        }
        self.report(
            if self.options.tavily_key.trim().is_empty() {
                "全网发现 · Bing"
            } else {
                "全网发现 · Tavily"
            },
            start,
            error,
        );
    }
    fn page(&mut self, url: &str, source: &str, hint: Option<(&str, &str)>) -> Result<(), String> {
        if self
            .seen
            .contains(&format!("{:x}", sha2::Sha256::digest(url.as_bytes())))
        {
            return Ok(());
        }
        {
            let body = self.get(url)?;
            let jobs = parse::details(&body, url, source);
            if !jobs.is_empty() {
                self.accept(jobs);
                return Ok(());
            }
            // A search snippet alone is a lead, never a verified job.
            if let Some((title, summary)) = hint
                && title.chars().count() >= 12
                && ["202", "公告", "简章", "应届"]
                    .iter()
                    .any(|v| title.contains(v))
            {
                let mut job = Job {
                    title: title.into(),
                    summary: format!("搜索线索，岗位要求与有效期请核对原文。{summary}"),
                    source: "全网发现 · 待核验线索".into(),
                    source_url: url.into(),
                    cohort: parse::cohort(title),
                    ..Default::default()
                };
                job.identify();
                self.accept(vec![job]);
            }
            for (title, link) in parse::links(&body, url).into_iter().take(5) {
                if cancelled(self.cancel) {
                    break;
                }
                if let Ok(detail) = self.get(&link) {
                    let jobs = parse::details(&detail, &link, source);
                    if jobs.is_empty() {
                        continue;
                    }
                    (self.progress)(Progress::Status(format!("读取公告 · {title}")));
                    self.accept(jobs);
                }
            }
        }
        Ok(())
    }
}

pub fn search(
    options: &Options,
    cancel: &AtomicBool,
    progress: impl FnMut(Progress),
) -> Result<Outcome, String> {
    options.validate()?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut search = Search {
        agent,
        options,
        cancel,
        progress,
        seen: HashSet::new(),
        outcome: Outcome::default(),
    };
    if options.ncss && !cancelled(cancel) {
        search.ncss();
    }
    if options.wonder && !cancelled(cancel) {
        search.wonder();
    }
    if options.web && !cancelled(cancel) {
        search.web();
    }
    for url in options
        .extra_urls
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        if cancelled(cancel) {
            break;
        }
        let start = search.outcome.jobs.len();
        (search.progress)(Progress::Status(format!("补充网址 · {url}")));
        let error = search.page(url, "补充公告", None).err().unwrap_or_default();
        search.report("补充公告", start, error);
    }
    search.outcome.cancelled = cancelled(cancel);
    Ok(search.outcome)
}
