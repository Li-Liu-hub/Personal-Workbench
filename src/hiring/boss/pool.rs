use super::{prepare, send_prepared};
use crate::hiring::{Result, Settings, accounts, dedupe::Eligibility, lease::Lease, store::Store};
use std::{
    collections::HashSet,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct Report {
    sent: usize,
    skipped: usize,
    reason: String,
}

fn cancelled(store: &Store, cancel: &AtomicBool, generation: i64) -> Result<bool> {
    if cancel.load(Ordering::Acquire) || store.stop_generation()? != generation {
        cancel.store(true, Ordering::Release);
        return Ok(true);
    }
    Ok(false)
}
fn worker(
    dir: &Path,
    settings: &Settings,
    limit: usize,
    cancel: &AtomicBool,
    generation: i64,
    mut progress: impl FnMut(String),
) -> Result<Report> {
    let _lease = Lease::account(dir, &settings.account_id)?;
    let mut store = Store::for_account(dir, &settings.account_id)?;
    let mut report = Report {
        sent: 0,
        skipped: 0,
        reason: "没有更多待投岗位".into(),
    };
    let mut visited = HashSet::new();
    while report.sent < limit {
        if cancelled(&store, cancel, generation)? {
            report.reason = "已请求停止".into();
            break;
        }
        let Some(job) = store.next_pending(&visited)? else {
            break;
        };
        visited.insert(job.url.clone());
        if store.eligibility("boss", &job)? != Eligibility::Eligible {
            store.set_status(&job.url, "skipped")?;
            report.skipped += 1;
            progress(format!(
                "跳过15天内重复或待核验岗位：{} · {}",
                job.company, job.title
            ));
            continue;
        }
        progress(format!("正在预检：{} · {}", job.company, job.title));
        let before = store
            .latest_attempt("boss", &job.url)?
            .map(|a| a.0)
            .unwrap_or(0);
        let mut actual = job.clone();
        let outcome = prepare(dir, settings, &job).and_then(|prepared| {
            actual = prepared.clone();
            if cancelled(&store, cancel, generation)? {
                return Err("已请求停止，预检后未发送".into());
            }
            progress(format!(
                "预检通过，正在发送：{} · {}",
                prepared.company, prepared.title
            ));
            send_prepared(dir, settings, &prepared)
        });
        match outcome {
            Ok(message) if message == "投递已确认发送" => {
                report.sent += 1;
                progress(format!("已确认文字与图片发送，累计{}项", report.sent));
            }
            Ok(message) => {
                report.reason = message;
                cancel.store(true, Ordering::Release);
                break;
            }
            Err(error) => {
                let after = store.latest_attempt("boss", &job.url)?;
                let new_attempt = after.is_some_and(|a| a.0 > before);
                if !new_attempt && store.eligibility("boss", &actual)? != Eligibility::Eligible {
                    store.set_status(&job.url, "skipped")?;
                    report.skipped += 1;
                    progress("另一账号已领取或投递同一岗位，已跳过".into());
                    continue;
                }
                report.reason = error.clone();
                if !error.contains("已达到此账号今日投递上限") {
                    cancel.store(true, Ordering::Release);
                }
                break;
            }
        }
        if report.sent >= limit {
            report.reason = "本账号本轮目标已完成".into();
            break;
        }
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).map_err(crate::hiring::err)?;
        let min = settings.interval_min.max(30);
        let max = settings.interval_max.max(min);
        let wait = min + u64::from_le_bytes(random) % (max - min + 1);
        progress(format!("本账号等待{wait}秒，可随时停止全部队列"));
        let deadline = Instant::now() + Duration::from_secs(wait);
        while Instant::now() < deadline && !cancelled(&store, cancel, generation)? {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    progress(format!(
        "队列结束，确认{}项，跳过{}项；{}",
        report.sent, report.skipped, report.reason
    ));
    Ok(report)
}
pub(super) fn run_single(
    dir: &Path,
    settings: &Settings,
    limit: usize,
    cancel: Arc<AtomicBool>,
    progress: impl FnMut(String),
) -> Result<String> {
    let _coordinator = Lease::coordinator(dir)?;
    let account = accounts::active(dir, settings)?;
    if !account.enabled {
        return Err("此账号已停用".into());
    }
    let generation = Store::open(dir)?.stop_generation()?;
    let report = worker(
        dir,
        settings,
        limit
            .min(crate::hiring::BOSS_MAX_DAILY as usize)
            .min(account.remaining as usize),
        &cancel,
        generation,
        progress,
    )?;
    Ok(format!(
        "{}队列已停止，本轮确认发送{}项，跳过{}项；{}",
        account.label, report.sent, report.skipped, report.reason
    ))
}
pub fn run_pool(
    dir: &Path,
    settings: &Settings,
    ids: &[String],
    limit: usize,
    cancel: Arc<AtomicBool>,
    progress: impl FnMut(String) + Send,
) -> Result<String> {
    let _coordinator = Lease::coordinator(dir)?;
    let mut store = Store::open(dir)?;
    let generation = store.stop_generation()?;
    let preview = store.plan_accounts(ids, limit, settings.daily_limit, false)?;
    for assignment in &preview.assignments {
        if !assignment.login_checked {
            return Err(format!(
                "{}尚未核验登录，请先在该账号的专用窗口登录并检查",
                assignment.account_label
            ));
        }
        let mut actor = settings.clone();
        actor.account_id = assignment.account_id.clone();
        let _lease = Lease::account(dir, &actor.account_id)?;
        let status = super::login_status(dir, &actor)?;
        if !status.starts_with("检测到登录界面信息") {
            return Err(format!("{}需要重新登录", assignment.account_label));
        }
    }
    let plan = store.plan_accounts(ids, limit, settings.daily_limit, true)?;
    let progress = Mutex::new(progress);
    let outcomes = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for assignment in &plan.assignments {
            if assignment.jobs.is_empty() {
                continue;
            }
            let mut actor = settings.clone();
            actor.account_id = assignment.account_id.clone();
            let cancel = cancel.clone();
            let progress = &progress;
            let label = assignment.account_label.clone();
            let target = assignment.jobs.len();
            handles.push(scope.spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    worker(dir, &actor, target, &cancel, generation, |message| {
                        (progress.lock().unwrap())(format!("[{label}] {message}"))
                    })
                }));
                let result =
                    result.unwrap_or_else(|_| Err("账号任务异常，已请求停止全部队列".into()));
                if result.is_err() {
                    cancel.store(true, Ordering::Release);
                }
                (label, result)
            }));
        }
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| ("账号任务".into(), Err("后台任务异常".into())))
            })
            .collect::<Vec<_>>()
    });
    let mut sent = 0;
    let mut details = Vec::new();
    for (label, result) in outcomes {
        match result {
            Ok(report) => {
                sent += report.sent;
                details.push(format!(
                    "{label}：确认{}项，跳过{}项，{}",
                    report.sent, report.skipped, report.reason
                ));
            }
            Err(error) => details.push(format!("{label}：{error}")),
        }
    }
    Ok(format!(
        "多账号队列已停止，确认发送{sent}项，原计划{}项。{}",
        plan.assigned,
        details.join("；")
    ))
}
