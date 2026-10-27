//! Bounded public-site crawling with durable response checkpoints. Replaying cached
//! pages rebuilds the frontier and re-emits results after interrupted database writes.
use super::{crawl_parse, *};
use rusqlite::{Connection, params};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

struct Crawler<'a, F: FnMut(Progress)> {
    agent: ureq::Agent,
    db: Connection,
    options: &'a Options,
    cancel: &'a AtomicBool,
    progress: F,
    last: HashMap<String, Instant>,
    robots: HashMap<String, String>,
    seen: HashSet<String>,
    outcome: Outcome,
    fetched: usize,
    cached: usize,
}

fn public_url(value: &str) -> bool {
    let Ok(u) = url::Url::parse(value) else {
        return false;
    };
    let Some(host) = u.host_str() else {
        return false;
    };
    matches!(u.scheme(), "http" | "https")
        && u.username().is_empty()
        && u.password().is_none()
        && !host.eq_ignore_ascii_case("localhost")
        && !host.ends_with(".local")
        && host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .map_or(true, |ip| match ip {
                std::net::IpAddr::V4(v) => {
                    !(v.is_private() || v.is_loopback() || v.is_link_local() || v.is_unspecified())
                }
                std::net::IpAddr::V6(v) => {
                    !(v.is_loopback()
                        || v.is_unspecified()
                        || v.is_unique_local()
                        || v.is_unicast_link_local())
                }
            })
}

// Robots path wildcards, anchored at the beginning, with optional terminal '$'.
fn path_matches(pattern: &str, path: &str) -> bool {
    let exact = pattern.ends_with('$');
    let pattern = pattern.trim_end_matches('$');
    let mut remaining = path;
    for (index, part) in pattern.split('*').enumerate() {
        if part.is_empty() {
            continue;
        }
        if index == 0 {
            let Some(tail) = remaining.strip_prefix(part) else {
                return false;
            };
            remaining = tail;
        } else {
            let Some(pos) = remaining.find(part) else {
                return false;
            };
            remaining = &remaining[pos + part.len()..];
        }
    }
    !exact || remaining.is_empty() || pattern.ends_with('*')
}
fn allowed(body: &str, path: &str) -> bool {
    type RobotGroup = (Vec<String>, Vec<(bool, String)>);
    let mut groups: Vec<RobotGroup> = Vec::new();
    let mut agents = Vec::new();
    let mut rules = Vec::new();
    for line in body.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if key.trim().eq_ignore_ascii_case("user-agent") {
            if !rules.is_empty() {
                groups.push((std::mem::take(&mut agents), std::mem::take(&mut rules)));
            }
            agents.push(value.to_lowercase());
        } else if ["allow", "disallow"]
            .iter()
            .any(|v| key.trim().eq_ignore_ascii_case(v))
            && !value.is_empty()
        {
            rules.push((key.trim().eq_ignore_ascii_case("allow"), value.to_owned()));
        }
    }
    groups.push((agents, rules));
    let specificity = groups
        .iter()
        .flat_map(|(a, _)| a)
        .filter(|a| a.as_str() != "*" && "personaldeskcampus".contains(a.as_str()))
        .map(String::len)
        .max()
        .unwrap_or(0);
    groups
        .iter()
        .filter(|(agents, _)| {
            agents.iter().any(|a| {
                if specificity == 0 {
                    a == "*"
                } else {
                    a.len() == specificity && "personaldeskcampus".contains(a.as_str())
                }
            })
        })
        .flat_map(|(_, r)| r)
        .filter(|(_, p)| path_matches(p, path))
        .max_by_key(|(yes, p)| (p.replace('*', "").len(), *yes))
        .is_none_or(|(yes, _)| *yes)
}

impl<F: FnMut(Progress)> Crawler<'_, F> {
    fn raw(&mut self, address: &str) -> Result<String, String> {
        let cached: Option<String> = self
            .db
            .query_row(
                "SELECT body FROM pages WHERE url=?1 AND fetched_at>strftime('%s','now')-86400",
                [address],
                |r| r.get(0),
            )
            .ok();
        if let Some(body) = cached {
            self.cached += 1;
            return Ok(body);
        }
        let mut current = address.to_owned();
        for _ in 0..=5 {
            if !public_url(&current) {
                return Err("仅支持公开网站地址".into());
            }
            let u = url::Url::parse(&current).map_err(|e| e.to_string())?;
            let host = u.host_str().unwrap_or_default().to_owned();
            while self
                .last
                .get(&host)
                .is_some_and(|t| t.elapsed() < Duration::from_millis(800))
            {
                if cancelled(self.cancel) {
                    return Err("采集已停止".into());
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            if cancelled(self.cancel) {
                return Err("采集已停止".into());
            }
            self.last.insert(host, Instant::now());
            let mut response = self
                .agent
                .get(&current)
                .header(
                    "User-Agent",
                    "PersonalDeskCampus/2.0 (public recruiting index)",
                )
                .header("Accept-Language", "zh-CN,zh;q=0.9")
                .call()
                .map_err(|e| e.to_string())?;
            if response.status().is_redirection() {
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|h| h.to_str().ok())
                    .ok_or("重定向缺少地址")?;
                current = u.join(location).map_err(|e| e.to_string())?.into();
                continue;
            }
            let body = response
                .body_mut()
                .with_config()
                .limit(4_000_000)
                .read_to_string()
                .map_err(|e| e.to_string())?;
            self.db.execute("INSERT INTO pages(url,body,fetched_at) VALUES(?1,?2,strftime('%s','now')) ON CONFLICT(url) DO UPDATE SET body=excluded.body,fetched_at=excluded.fetched_at",params![address,body]).map_err(|e|e.to_string())?;
            self.fetched += 1;
            return Ok(body);
        }
        Err("重定向次数过多".into())
    }
    fn get(&mut self, address: &str) -> Result<String, String> {
        if !public_url(address) {
            return Err("仅支持公开网站地址".into());
        }
        let u = url::Url::parse(address).map_err(|e| e.to_string())?;
        let origin = u.origin().ascii_serialization();
        if !self.robots.contains_key(&origin) {
            let body = match self.raw(&format!("{origin}/robots.txt")) {
                Ok(v) => v,
                Err(e) if e.contains("404") => String::new(),
                Err(e) => return Err(format!("robots.txt 暂不可核验：{e}")),
            };
            self.robots.insert(origin.clone(), body);
        }
        let path = match u.query() {
            Some(q) => format!("{}?{q}", u.path()),
            None => u.path().into(),
        };
        if !allowed(&self.robots[&origin], &path) {
            return Err("网站 robots.txt 不允许采集此地址".into());
        }
        self.raw(address)
    }
    fn accept(&mut self, values: Vec<Job>) {
        let jobs: Vec<_> = values
            .into_iter()
            .filter(|j| {
                !j.title.is_empty()
                    && !j.source_url.is_empty()
                    && (self.options.include_unmarked || j.is_campus())
                    && (self.options.year.is_empty()
                        || j.cohort.is_empty()
                        || j.cohort.contains(&self.options.year))
                    && (self.options.city.is_empty()
                        || format!("{} {}", j.location, j.summary)
                            .contains(self.options.city.trim()))
                    && (self.options.crawl_all || self.options.terms().iter().any(|t| j.matches(t)))
                    && self.seen.insert(j.id.clone())
            })
            .collect();
        if !jobs.is_empty() {
            (self.progress)(Progress::Jobs(jobs.clone()));
            self.outcome.jobs.extend(jobs);
        }
    }
}

pub fn collect(
    options: &Options,
    directory: &Path,
    cancel: &AtomicBool,
    progress: impl FnMut(Progress),
) -> Result<Outcome, String> {
    if !options.crawl {
        return super::search(options, cancel, progress);
    }
    options.validate()?;
    let dir = directory.join("campus-crawler");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut lock_options = std::fs::OpenOptions::new();
    lock_options
        .create(true)
        .truncate(false)
        .read(true)
        .write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        lock_options.share_mode(0);
    }
    let _lock = lock_options
        .open(dir.join("crawl.lock"))
        .map_err(|_| "另一个校招采集任务正在运行，请先等待或停止".to_owned())?;
    let db = Connection::open(dir.join("checkpoint.sqlite")).map_err(|e| e.to_string())?;
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS pages(url TEXT PRIMARY KEY,body TEXT NOT NULL,fetched_at INTEGER NOT NULL); DELETE FROM pages WHERE fetched_at<strftime('%s','now')-604800;").map_err(|e|e.to_string())?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(12)))
        .max_redirects(0)
        .build()
        .into();
    let mut crawl = Crawler {
        agent,
        db,
        options,
        cancel,
        progress,
        last: HashMap::new(),
        robots: HashMap::new(),
        seen: HashSet::new(),
        outcome: Outcome::default(),
        fetched: 0,
        cached: 0,
    };
    let mut queue = VecDeque::new();
    if options.web {
        queue.push_back((format!("https://qiuzhaowang.org/?year={}", options.year), 0));
        for seed in [
            "https://campus.sma-wiki.cn/campus/campus_recruit.html",
            "https://job.hust.edu.cn/zpinfo1.htm",
            "https://career.xujc.com/zpfw/list.htm",
        ] {
            queue.push_back((seed.to_owned(), 0u8));
        }
    }
    if options.wonder {
        queue.push_back(("https://www.wondercv.com/xiaozhao/".into(), 0));
    }
    for url in options
        .extra_urls
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        queue.push_back((url.to_owned(), 0));
    }
    if options.ncss {
        for page in 1..=options.pages {
            let term = if options.crawl_all {
                "校园招聘"
            } else {
                options.keywords.trim()
            };
            let mut u = url::Url::parse("https://www.ncss.cn/student/jobs/jobslist/ajax/").unwrap();
            u.query_pairs_mut().extend_pairs([
                ("jobName", term),
                ("offset", &page.to_string()),
                ("limit", "20"),
            ]);
            queue.push_back((u.into(), 0));
        }
    }
    let mut visited = HashSet::new();
    let mut detail_count = 0;
    let mut processed = 0;
    let mut errors = Vec::new();
    let mut listing_counts: HashMap<String, u32> = HashMap::new();
    while let Some((address, depth)) = queue.pop_front() {
        if cancelled(cancel) || processed >= options.crawl_pages {
            break;
        }
        let Some(address) = canonical_url(&address) else {
            continue;
        };
        if !visited.insert(address.clone()) {
            continue;
        }
        if depth == 0 && !address.contains("campus.sma-wiki.cn/campus/data/") {
            let host = url::Url::parse(&address)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned))
                .unwrap_or_default();
            let count = listing_counts.entry(host).or_default();
            if *count >= options.pages {
                continue;
            }
            *count += 1;
        }
        if depth > 0 {
            if detail_count >= options.crawl_details {
                continue;
            }
            detail_count += 1;
        }
        processed += 1;
        (crawl.progress)(Progress::Status(format!(
            "网站直采 · {processed}/{} 页 · 已读取 {} 条 · {address}",
            options.crawl_pages,
            crawl.outcome.jobs.len()
        )));
        match crawl.get(&address).and_then(|body| {
            if address.contains("ncss.cn/student/jobs/jobslist/ajax/") {
                parse::ncss(&body).map(|v| (v, vec![]))
            } else {
                crawl_parse::catalog(&body, &address)
            }
        }) {
            Ok((jobs, links)) => {
                // Index batches are processed before optional detail enrichment.
                for link in links.into_iter().take(100).rev() {
                    let pagination = link.contains("campus.sma-wiki.cn/campus/data/")
                        || link.contains("?page=")
                        || link.contains("&page=")
                        || link.contains("/xiaozhao/page/")
                        || link.contains("/zpfw/list")
                        || link.contains("/zpinfo1_");
                    if pagination {
                        queue.push_front((link, 0));
                    } else if depth < 2 && queue.len() < 10000 {
                        queue.push_back((link, depth + 1));
                    }
                }
                if depth == 0 {
                    for j in &jobs {
                        let link = if !j.apply_url.is_empty() {
                            &j.apply_url
                        } else {
                            &j.source_url
                        };
                        if crawl_parse::direct_entry(link) && queue.len() < 10000 {
                            queue.push_back((link.clone(), 1));
                        }
                    }
                }
                crawl.accept(jobs);
            }
            Err(e) => {
                if errors.len() < 25 {
                    errors.push(format!("{address}：{e}"));
                }
            }
        }
    }
    let report = SourceReport {
        source: format!(
            "网站直采：{processed} 页，联网 {}，断点复用 {}",
            crawl.fetched, crawl.cached
        ),
        count: crawl.outcome.jobs.len(),
        error: errors.join("\n"),
    };
    (crawl.progress)(Progress::Source(report.clone()));
    crawl.outcome.reports.push(report);
    crawl.outcome.cancelled = cancelled(cancel);
    Ok(crawl.outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn robots_respects_longest_rule_and_agent_groups() {
        let text = "User-agent: *\nDisallow: /api/\nAllow: /api/public\nUser-agent: OtherBot\nDisallow: /\n";
        assert!(!allowed(text, "/api/jobs"));
        assert!(allowed(text, "/api/public/a"));
        assert!(allowed(text, "/jobs"));
        assert!(!allowed(
            "User-agent: PersonalDeskCampus\nDisallow: /\nUser-agent: *\nAllow: /",
            "/jobs"
        ));
        assert!(path_matches("/jobs/*/apply$", "/jobs/3/apply"));
        assert!(!path_matches("/jobs/*/apply$", "/jobs/3/apply-more"));
    }
    #[test]
    fn public_urls_exclude_local_and_credential_addresses() {
        for u in [
            "http://127.0.0.1/",
            "http://[::1]/",
            "http://192.168.1.2/",
            "file:///a",
            "http://user:pass@example.org/",
        ] {
            assert!(!public_url(u));
        }
        assert!(public_url("https://example.org/jobs"));
    }
    #[test]
    fn cancelled_crawl_does_not_fetch_and_creates_replayable_store() {
        let temp = tempfile::tempdir().unwrap();
        let options = Options {
            crawl: true,
            ..Default::default()
        };
        let out = collect(&options, temp.path(), &AtomicBool::new(true), |_| {}).unwrap();
        assert!(out.cancelled);
        let db = Connection::open(temp.path().join("campus-crawler/checkpoint.sqlite")).unwrap();
        let count: i64 = db
            .query_row("SELECT count(*) FROM pages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn cached_pages_replay_after_stop_without_network_and_filters_still_apply() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("campus-crawler");
        std::fs::create_dir_all(&dir).unwrap();
        let db = Connection::open(dir.join("checkpoint.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE pages(url TEXT PRIMARY KEY,body TEXT NOT NULL,fetched_at INTEGER NOT NULL);").unwrap();
        for (u, b) in [
            ("https://example.org/robots.txt", "User-agent: *\nAllow: /"),
            (
                "https://example.org/notice",
                r#"<h1>2027届Java校园招聘</h1><article>公司公开招聘，上海，本科</article>"#,
            ),
        ] {
            db.execute(
                "INSERT INTO pages VALUES(?1,?2,strftime('%s','now'))",
                params![u, b],
            )
            .unwrap();
        }
        drop(db);
        let options = Options {
            crawl: true,
            web: false,
            ncss: false,
            wonder: false,
            extra_urls: "https://example.org/notice".into(),
            ..Default::default()
        };
        let stop = AtomicBool::new(false);
        let first = collect(&options, temp.path(), &stop, |p| {
            if matches!(p, Progress::Jobs(_)) {
                stop.store(true, Ordering::Relaxed);
            }
        })
        .unwrap();
        assert_eq!(first.jobs.len(), 1);
        assert!(first.cancelled);
        let second = collect(&options, temp.path(), &AtomicBool::new(false), |_| {}).unwrap();
        assert_eq!(first.jobs[0].id, second.jobs[0].id);
        assert!(second.reports[0].source.contains("联网 0"));
        let filtered = Options {
            year: "2028".into(),
            ..options
        };
        assert!(
            collect(&filtered, temp.path(), &AtomicBool::new(false), |_| {})
                .unwrap()
                .jobs
                .is_empty()
        );
    }
}
