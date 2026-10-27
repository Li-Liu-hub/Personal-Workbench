//! One four-platform search -> ingest -> prioritize -> apply -> refill loop.
use super::{
    Result, Settings, accounts,
    platforms::Platform,
    store::{Job, Store},
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
pub const ORDER: [&str; 4] = ["boss", "zhaopin", "51job", "shixiseng"];
pub fn rank(site: &str) -> usize {
    ORDER.iter().position(|s| *s == site).unwrap_or(usize::MAX)
}
pub fn same_candidate(a: &Job, b: &Job) -> bool {
    if a.url == b.url {
        return true;
    }
    if super::dedupe::fingerprint(a).is_some()
        && super::dedupe::fingerprint(a) == super::dedupe::fingerprint(b)
    {
        return true;
    }
    a.platform != b.platform
        && !a.company.trim().is_empty()
        && !a.title.trim().is_empty()
        && super::dedupe::normalize(&a.company) == super::dedupe::normalize(&b.company)
        && super::dedupe::normalize(&a.title) == super::dedupe::normalize(&b.title)
}
#[derive(Clone, Debug)]
pub struct Batch {
    pub site: String,
    pub account_id: String,
    pub jobs: Vec<Job>,
}
pub trait Backend: Sync {
    fn search(&self, site: &str, page: u32) -> Result<Batch>;
    fn preflight(&self, job: &Job) -> Result<()>;
    fn send(&self, job: &Job) -> Result<()>;
}
struct Native<'a> {
    dir: &'a Path,
    settings: &'a Settings,
    cancel: &'a AtomicBool,
    stop_generation: i64,
}
impl Backend for Native<'_> {
    fn search(&self, site: &str, page: u32) -> Result<Batch> {
        let account_id = if site == "boss" {
            accounts::id(&self.settings.account_id)?
        } else {
            accounts::DEFAULT_ACCOUNT
        };
        let mut jobs = if site == "boss" {
            super::boss::search(self.dir, self.settings, page)?
        } else {
            super::platforms::search(self.dir, self.settings, Platform::parse(site)?, page)?
        };
        // Detail acquisition happens per platform in parallel, before cross-platform selection.
        for job in &mut jobs {
            if self.cancel.load(Ordering::Acquire)
                || Store::open(self.dir)?.stop_generation()? != self.stop_generation
            {
                return Err("已停止搜索，未继续读取详情".into());
            }
            let mut detail = if site == "boss" {
                super::boss::detail(self.dir, self.settings, &job.url)?
            } else {
                super::platforms::detail(self.dir, self.settings, Platform::parse(site)?, &job.url)?
            };
            detail.platform = site.into();
            detail.account_id = account_id.into();
            *job = detail;
        }
        Ok(Batch {
            site: site.into(),
            account_id: account_id.into(),
            jobs,
        })
    }
    fn preflight(&self, job: &Job) -> Result<()> {
        if job.platform == "boss" {
            let mut settings = self.settings.clone();
            settings.account_id = job.account_id.clone();
            super::boss::deliver(self.dir, &settings, job, true)?;
        } else {
            super::platforms::apply(
                self.dir,
                self.settings,
                Platform::parse(&job.platform)?,
                job,
                true,
            )?;
        }
        Ok(())
    }
    fn send(&self, job: &Job) -> Result<()> {
        if job.platform == "boss" {
            let mut settings = self.settings.clone();
            settings.account_id = job.account_id.clone();
            super::boss::deliver(self.dir, &settings, job, false)?;
        } else {
            super::platforms::apply(
                self.dir,
                self.settings,
                Platform::parse(&job.platform)?,
                job,
                false,
            )?;
        }
        let snapshot = super::tracking::read(self.dir)?;
        if snapshot
            .attempts
            .iter()
            .find(|a| a.url == job.url)
            .is_none_or(|a| a.status != "sent")
        {
            return Err("当前岗位投递结果待核验，统一任务已停止".into());
        }
        Ok(())
    }
}
fn stopped(store: &Store, cancel: &AtomicBool, generation: i64) -> Result<bool> {
    Ok(cancel.load(Ordering::Acquire) || store.stop_generation()? != generation)
}
pub fn run(
    dir: &Path,
    settings: &Settings,
    dry_run: bool,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(String),
) -> Result<String> {
    let _lease = super::lease::Lease::coordinator(dir)?;
    run_with(
        dir,
        settings,
        dry_run,
        &cancel,
        &Native {
            dir,
            settings,
            cancel: &cancel,
            stop_generation: Store::open(dir)?.stop_generation()?,
        },
        progress,
        true,
    )
}
pub fn run_with(
    dir: &Path,
    settings: &Settings,
    dry_run: bool,
    cancel: &AtomicBool,
    backend: &impl Backend,
    progress: impl Fn(String),
    wait: bool,
) -> Result<String> {
    let mut store = Store::open(dir)?;
    let generation = store.stop_generation()?;
    let day = chrono::Local::now().date_naive();
    let mut empty_rounds = 0;
    let mut discovered = 0;
    if store.today_total()? >= settings.daily_limit {
        return Ok(format!(
            "今日已占用{}项，已达到统一上限{}项",
            store.today_total()?,
            settings.daily_limit
        ));
    }
    if !dry_run && !super::store::in_window(&settings.send_windows, chrono::Local::now().time())? {
        return Err("当前不在允许投递时段，尚未搜索或投递".into());
    }
    for page in 1..=settings.unified_max_pages {
        if stopped(&store, cancel, generation)? {
            return Ok("已停止统一求职任务".into());
        }
        if chrono::Local::now().date_naive() != day {
            return Ok("日期已变化，请按新一天的额度重新启动".into());
        }
        progress(format!(
            "第{page}轮：四个平台同时搜索，今日已占用{}/{}项",
            store.today_total()?,
            settings.daily_limit
        ));
        let batches = std::thread::scope(|scope| {
            let handles: Vec<_> = ORDER
                .into_iter()
                .map(|site| scope.spawn(move || backend.search(site, page)))
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .map_err(|_| "平台搜索线程异常".into())
                        .and_then(|r| r)
                })
                .collect::<Vec<Result<Batch>>>()
        });
        let mut added = 0;
        let mut errors = vec![];
        for batch in batches {
            match batch {
                Ok(batch) => {
                    for job in batch.jobs {
                        if stopped(&store, cancel, generation)? {
                            return Ok("已停止统一求职任务".into());
                        }
                        added +=
                            usize::from(store.discover(&batch.site, &batch.account_id, &job)?);
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        discovered += added;
        let winners = store.unified_candidates()?;
        progress(format!(
            "第{page}轮入库{added}条，按 BOSS→智联→前程无忧→实习僧筛选后待投{}条",
            winners.len()
        ));
        if !errors.is_empty() {
            return Err(format!(
                "本轮搜索有平台未完成，结果已保存；为保证来源优先级暂停投递：{}",
                errors.join("；")
            ));
        }
        if dry_run
            && winners.len() >= settings.daily_limit.saturating_sub(store.today_total()?) as usize
        {
            return Ok(format!(
                "搜索筛选完成：新增{discovered}条来源记录，{}个去重岗位可投；未发送申请",
                winners.len()
            ));
        }
        if !dry_run {
            for job in winners {
                if stopped(&store, cancel, generation)? {
                    return Ok("已停止统一求职任务".into());
                }
                if store.today_total()? >= settings.daily_limit {
                    return Ok(format!(
                        "今日已占用{}项（确认成功{}项），达到统一上限{}项，任务完成",
                        store.today_total()?,
                        store.today_sent()?,
                        settings.daily_limit
                    ));
                }
                if chrono::Local::now().date_naive() != day {
                    return Ok("日期已变化，统一任务已停止".into());
                }
                if store.eligibility(&job.platform, &job)?.error().is_some() {
                    continue;
                }
                progress(format!(
                    "预检{} · {} · {}",
                    super::tracking::site_label(&job.platform),
                    job.company,
                    job.title
                ));
                backend.preflight(&job)?;
                if stopped(&store, cancel, generation)? {
                    return Ok("预检后已停止，尚未发送当前项".into());
                }
                backend.send(&job)?;
                if store.today_total()? >= settings.daily_limit {
                    return Ok(format!(
                        "今日已占用{}项，达到统一上限，任务完成",
                        store.today_total()?
                    ));
                }
                if wait {
                    let mut seed = [0u8; 8];
                    getrandom::fill(&mut seed).map_err(super::err)?;
                    let seconds = settings.interval_min.max(30)
                        + u64::from_le_bytes(seed)
                            % (settings.interval_max.saturating_sub(settings.interval_min) + 1);
                    for _ in 0..seconds {
                        if stopped(&store, cancel, generation)? {
                            return Ok("已停止统一求职任务".into());
                        }
                        std::thread::sleep(Duration::from_secs(1));
                    }
                }
            }
        }
        if added == 0 {
            empty_rounds += 1;
        } else {
            empty_rounds = 0;
        }
        if empty_rounds >= 2 {
            return Ok(format!(
                "连续两轮没有新增岗位，已暂停；今日已占用{}/{}项，可更换关键词或城市后继续",
                store.today_total()?,
                settings.daily_limit
            ));
        }
    }
    Ok(format!(
        "本次搜索已到页数上限，新增{discovered}条来源记录，今日已占用{}/{}项",
        store.today_total()?,
        settings.daily_limit
    ))
}
#[cfg(test)]
mod tests;
