//! Candidate-side adapters. No private website APIs or copied credentials.
use super::{
    Result, Settings,
    browser::{self, Cdp, Site},
    store::{Job, Store},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Platform {
    #[serde(rename = "51job")]
    Job51,
    #[serde(rename = "zhaopin")]
    Zhaopin,
    #[serde(rename = "shixiseng")]
    Shixiseng,
}
impl Platform {
    pub fn detect(url: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.canonical(url).is_ok())
    }
    pub const ALL: [Self; 3] = [Self::Job51, Self::Zhaopin, Self::Shixiseng];
    pub fn parse(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.id() == value)
            .ok_or("平台须为 51job、zhaopin 或 shixiseng".into())
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Job51 => "51job",
            Self::Zhaopin => "zhaopin",
            Self::Shixiseng => "shixiseng",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Job51 => "前程无忧",
            Self::Zhaopin => "智联招聘",
            Self::Shixiseng => "实习僧",
        }
    }
    pub fn site(self) -> Site {
        match self {
            Self::Job51 => Site::Job51,
            Self::Zhaopin => Site::Zhaopin,
            Self::Shixiseng => Site::Shixiseng,
        }
    }
    pub fn home(self) -> &'static str {
        match self {
            Self::Job51 => "https://www.51job.com/",
            Self::Zhaopin => "https://www.zhaopin.com/",
            Self::Shixiseng => "https://www.shixiseng.com/",
        }
    }
    pub fn canonical(self, raw: &str) -> Result<String> {
        let mut u = browser::validate_url(raw, self.site())?;
        let path = u.path();
        let valid = match self {
            Self::Job51 => {
                u.host_str() == Some("jobs.51job.com")
                    && path.ends_with(".html")
                    && path.rsplit('/').next().is_some_and(|s| {
                        s.trim_end_matches(".html")
                            .chars()
                            .all(|c| c.is_ascii_digit())
                            && s.len() > 5
                    })
            }
            Self::Zhaopin => {
                u.host_str() == Some("jobs.zhaopin.com") && path.ends_with(".htm") && path.len() > 8
            }
            Self::Shixiseng => {
                u.host_str() == Some("www.shixiseng.com")
                    && (path.starts_with("/intern/") || path.starts_with("/job/"))
                    && path.split('/').nth(2).is_some_and(|s| !s.is_empty())
            }
        };
        if !valid {
            return Err(format!("请输入{}的职位详情链接", self.label()));
        }
        if self == Self::Job51 {
            let id = u.path().rsplit('/').next().unwrap().to_owned();
            u.set_path(&format!("/all/{id}"));
        }
        u.set_query(None);
        u.set_fragment(None);
        Ok(u.to_string())
    }
    pub fn search_url(self, keyword: &str, city: &str, page: u32) -> Result<String> {
        if keyword.trim().is_empty() {
            return Err("请输入搜索关键词".into());
        }
        let mut u = url::Url::parse(match self {
            Self::Job51 => "https://we.51job.com/pc/search",
            Self::Zhaopin => "https://www.zhaopin.com/sou",
            Self::Shixiseng => "https://www.shixiseng.com/interns",
        })
        .unwrap();
        let city = city.trim().trim_end_matches('市');
        // Each portal has its own city codes. Never reuse BOSS codes or silently change cities.
        let codes = [
            ("全国", "000000", "489"),
            ("北京", "010000", "530"),
            ("上海", "020000", "538"),
            ("广州", "030200", "763"),
            ("深圳", "040000", "765"),
            ("杭州", "080200", "653"),
            ("南京", "070200", "635"),
            ("苏州", "070300", "639"),
            ("成都", "090200", "801"),
            ("武汉", "180200", "736"),
            ("西安", "200200", "854"),
            ("重庆", "060000", "551"),
            ("天津", "050000", "531"),
            ("长沙", "190200", "749"),
            ("合肥", "150200", "664"),
            ("郑州", "170200", "719"),
        ];
        let city = if city.is_empty() { "全国" } else { city };
        let mut pairs = u.query_pairs_mut();
        match self {
            Self::Job51 | Self::Zhaopin => {
                let (_, j, z) = codes
                    .iter()
                    .find(|(c, _, _)| *c == city)
                    .ok_or("此城市尚未配置平台编码，请选全国后在网页筛选城市")?;
                if self == Self::Job51 {
                    pairs
                        .append_pair("keyword", keyword.trim())
                        .append_pair("jobArea", j)
                        .append_pair("pageNum", &page.clamp(1, 50).to_string());
                } else {
                    pairs
                        .append_pair("kw", keyword.trim())
                        .append_pair("jl", z)
                        .append_pair("p", &page.clamp(1, 50).to_string());
                }
            }
            Self::Shixiseng => {
                pairs
                    .append_pair("keyword", keyword.trim())
                    .append_pair("city", city)
                    .append_pair("type", "intern")
                    .append_pair("page", &page.clamp(1, 50).to_string());
            }
        }
        drop(pairs);
        Ok(u.to_string())
    }
}
pub fn catalog() -> Value {
    json!(Platform::ALL.map(|p|json!({"id":p.id(),"name":p.label(),"daily_limit":150,"independent_login":true,"filters":"关键词、城市；其他条件请在网页筛选"})))
}
pub(super) fn probe(tab: &mut Cdp, platform: Platform) -> Result<Value> {
    let page = tab.eval(
        &include_str!("../../assets/hiring/platform_page.js")
            .replace("__PLATFORM__", &json!(platform.id()).to_string()),
    )?;
    browser::validate_url(page["url"].as_str().ok_or("网页地址无效")?, platform.site())?;
    Ok(page)
}
fn allowed(page: &Value) -> Result<()> {
    if page["captcha"] == true {
        return Err("页面出现安全验证或频率限制，请在浏览器处理后重试".into());
    }
    if page["login"] == true {
        return Err("请先在此平台专用浏览器登录".into());
    }
    if page["unavailable"] == true {
        return Err("职位已关闭或下线".into());
    }
    Ok(())
}
pub fn login_status(dir: &Path, p: Platform) -> Result<String> {
    let mut tab = browser::selected(dir, p.site())?;
    let page = probe(&mut tab, p)?;
    allowed(&page)?;
    Ok(if page["logged_in"] == true {
        format!("{}网页显示个人账号入口；投递前仍需预检", p.label())
    } else {
        "尚未识别到已登录状态，请在浏览器核对后打开职位详情".into()
    })
}
fn navigate(dir: &Path, settings: &Settings, p: Platform, url: &str) -> Result<Cdp> {
    browser::validate_url(url, p.site())?;
    let mut tab = match browser::selected(dir, p.site()) {
        Ok(mut tab) => {
            tab.call("Page.navigate", json!({"url":url}))?;
            tab
        }
        Err(_) => {
            browser::open(dir, settings, p.site(), url)?;
            browser::selected(dir, p.site())?
        }
    };
    tab.wait_location(url)?;
    Ok(tab)
}
fn list(tab: &mut Cdp, p: Platform) -> Result<Vec<Job>> {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        let page = probe(tab, p)?;
        allowed(&page)?;
        let jobs: Vec<Job> = serde_json::from_value(page["jobs"].clone()).map_err(super::err)?;
        let jobs: Vec<_> = jobs
            .into_iter()
            .filter_map(|mut j| {
                j.url = p.canonical(&j.url).ok()?;
                Some(j)
            })
            .collect();
        if !jobs.is_empty() || page["empty"] == true {
            return Ok(jobs);
        }
        if Instant::now() >= deadline {
            return Err(
                "未读取到职位列表，可能尚未加载、需要登录或页面结构已变化；请在专用浏览器检查"
                    .into(),
            );
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
pub fn search(dir: &Path, settings: &Settings, p: Platform, page: u32) -> Result<Vec<Job>> {
    let url = p.search_url(&settings.keyword, &settings.city, page)?;
    let mut store = Store::open(dir)?;
    store.reserve_page(&format!("{}:search", p.id()))?;
    list(&mut navigate(dir, settings, p, &url)?, p)
}
pub fn scroll(dir: &Path, p: Platform) -> Result<Vec<Job>> {
    Store::open(dir)?.reserve_page(&format!("{}:search", p.id()))?;
    let mut tab = browser::selected(dir, p.site())?;
    allowed(&probe(&mut tab, p)?)?;
    tab.eval("window.scrollTo(0,document.body.scrollHeight);true")?;
    std::thread::sleep(Duration::from_secs(1));
    list(&mut tab, p)
}
fn read_job(page: &Value, p: Platform) -> Result<Job> {
    allowed(page)?;
    if page["truncated"] == true {
        return Err("岗位简介未完整显示，请登录后核验".into());
    }
    let mut job: Job = serde_json::from_value(page["job"].clone()).map_err(super::err)?;
    job.platform = p.id().into();
    job.account_id = super::accounts::DEFAULT_ACCOUNT.into();
    job.url = p.canonical(&job.url)?;
    if [&job.title, &job.company, &job.jd]
        .iter()
        .any(|s| s.trim().is_empty())
    {
        return Err("岗位名称、公司或完整简介未识别，已停止操作".into());
    }
    if job
        .salary
        .chars()
        .any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c))
    {
        job.salary = "薪资请在网页核验".into();
    }
    for value in [&job.title, &job.company, &job.jd] {
        if value
            .chars()
            .any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c))
        {
            return Err("岗位内容含网页加密字体，请在网页人工核验".into());
        }
    }
    Ok(job)
}
pub fn detail(dir: &Path, settings: &Settings, p: Platform, url: &str) -> Result<Job> {
    let url = p.canonical(url)?;
    Store::open(dir)?.reserve_page(&format!("{}:detail", p.id()))?;
    let mut tab = navigate(dir, settings, p, &url)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let page = probe(&mut tab, p)?;
        allowed(&page)?;
        match read_job(&page, p) {
            Ok(j) => return Ok(j),
            Err(e) => {
                if Instant::now() >= deadline {
                    return Err(e);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(400));
    }
}
fn review_digest(page: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            json!([page["url"], page["job"], page["buttons"], page["fields"]])
                .to_string()
                .as_bytes()
        )
    )
}
pub fn apply(
    dir: &Path,
    settings: &Settings,
    p: Platform,
    expected: &Job,
    dry_run: bool,
) -> Result<String> {
    let url = p.canonical(&expected.url)?;
    // A real send uses the current reviewed tab, never navigates away and back after review.
    let mut tab = if dry_run {
        if let Ok(mut current) = browser::selected(dir, p.site()) {
            let matches = current
                .eval("location.href")
                .ok()
                .and_then(|v| v.as_str().and_then(|s| p.canonical(s).ok()))
                == Some(url.clone());
            if matches {
                current
            } else {
                navigate(dir, settings, p, &url)?
            }
        } else {
            navigate(dir, settings, p, &url)?
        }
    } else {
        browser::selected(dir, p.site())?
    };
    tab.wait_ready()?;
    let page = probe(&mut tab, p)?;
    let job = read_job(&page, p)?;
    if page["logged_in"] != true {
        return Err("尚未确认此平台已登录，请登录并核对在线简历后重新预检".into());
    }
    if job.url != url
        || (!expected.title.is_empty()
            && super::dedupe::normalize(&expected.title) != super::dedupe::normalize(&job.title))
        || (!expected.company.is_empty()
            && super::dedupe::normalize(&expected.company)
                != super::dedupe::normalize(&job.company))
    {
        return Err("当前网页与待投岗位不一致，请重新核对".into());
    }
    if page["success"] == true {
        return Err("网页已显示已投递，请核验记录".into());
    }
    if page["buttons"].as_array().is_none_or(|a| a.len() != 1) {
        return Err("投递按钮不唯一或未识别，请在网页处理简历选择、登录或表单后重新预检".into());
    }
    let mut store = Store::open(dir)?;
    if let Some(e) = store.eligibility(p.id(), &job)?.error() {
        return Err(e);
    }
    let fields = super::official::inspect(&mut tab)?;
    if !super::official::missing_required(&fields).is_empty() {
        return Err("页面仍有未完成的必填项，请在网页补全简历和协议后重新预检".into());
    }
    let digest = review_digest(&page);
    let now = chrono::Local::now().timestamp();
    let path = dir.join(format!("{}-review.json", p.id()));
    if dry_run {
        super::atomic_json(&path, &json!({"digest":digest,"at":now}))?;
        return Ok(format!(
            "{}投递预检通过：{} · {}。请核对网页中的在线简历及条款，五分钟内确认投递。",
            p.label(),
            job.title,
            job.company
        ));
    }
    let review: Value =
        serde_json::from_slice(&std::fs::read(&path).map_err(|_| "请先执行投递预检")?)
            .map_err(super::err)?;
    if review["digest"] != digest
        || review["at"]
            .as_i64()
            .is_none_or(|at| !(0..=300).contains(&(now - at)))
    {
        return Err("页面、简历选择已变化或预检过期，请重新预检".into());
    }
    if !super::store::in_window(&settings.send_windows, chrono::Local::now().time())? {
        return Err("当前不在允许投递时段内".into());
    }
    let point=tab.eval(r#"(()=>{const e=document.querySelector('[data-pd-platform-apply="0"]'); if(!e||e.disabled||!e.getClientRects().length)return {ok:false};e.scrollIntoView({block:'center'});const r=e.getBoundingClientRect();const t=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);return {ok:!!t&&(t===e||e.contains(t)),x:r.x+r.width/2,y:r.y+r.height/2}})()"#)?;
    if point["ok"] != true {
        return Err("投递按钮被遮挡或已变化，请在网页检查".into());
    }
    let id = store.reserve(p.id(), &job, settings)?;
    let result = (|| -> Result<bool> {
        tab.click(&point)?;
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(400));
            let after = probe(&mut tab, p)?;
            allowed(&after)?;
            if p.canonical(after["url"].as_str().unwrap_or(""))? != url {
                return Err("投递后跳转，结果需人工核验".into());
            }
            if after["success"] == true {
                return Ok(true);
            }
        }
        Ok(false)
    })();
    let sent = matches!(result, Ok(true));
    store.finish(
        id,
        if sent { "sent" } else { "ambiguous" },
        if sent {
            "网页显示投递成功"
        } else {
            "已尝试投递，结果待网页人工核验，禁止自动重试"
        },
    )?;
    match result {
        Err(e) => Err(format!("{e}；已保留待核验记录")),
        _ => Ok(if sent {
            "网页显示投递成功"
        } else {
            "投递结果待核验；如出现简历选择或确认窗口，请在网页处理后核验记录。不会自动重投。"
        }
        .into()),
    }
}
pub fn run_queue(
    dir: &Path,
    settings: &Settings,
    p: Platform,
    limit: usize,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(String),
) -> Result<String> {
    let _lease = super::lease::Lease::coordinator(dir)?;
    let _browser = super::lease::Lease::acquire(dir)?;
    let generation = Store::open(dir)?.stop_generation()?;
    let mut sent = 0;
    for job in Store::open(dir)?
        .platform_jobs(p)?
        .into_iter()
        .filter(|j| j.status == "pending")
        .take(limit.min(settings.daily_limit as usize))
    {
        if cancel.load(Ordering::Acquire) || Store::open(dir)?.stop_generation()? != generation {
            break;
        }
        progress(format!(
            "{}：预检 {} · {}",
            p.label(),
            job.title,
            job.company
        ));
        apply(dir, settings, p, &job, true)?;
        if cancel.load(Ordering::Acquire) || Store::open(dir)?.stop_generation()? != generation {
            break;
        }
        let result = apply(dir, settings, p, &job, false)?;
        progress(result.clone());
        if !result.contains("网页显示投递成功") {
            return Err(result);
        }
        sent += 1;
        let range = settings.interval_max.saturating_sub(settings.interval_min) + 1;
        let mut seed = [0u8; 8];
        getrandom::fill(&mut seed).map_err(super::err)?;
        let delay = settings.interval_min.max(30) + u64::from_le_bytes(seed) % range;
        for _ in 0..delay {
            if cancel.load(Ordering::Acquire) || Store::open(dir)?.stop_generation()? != generation
            {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    Ok(format!("{}队列结束，已确认成功 {sent} 项", p.label()))
}
