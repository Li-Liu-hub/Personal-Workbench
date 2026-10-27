use super::*;
use crate::db::Database;
use std::path::PathBuf;

/// Headless search uses the same sources and storage as the desktop tool.
pub fn run(mut args: impl Iterator<Item = std::ffi::OsString>) -> Result<(), String> {
    let directory = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(crate::app::data_directory);
    if !directory.is_absolute() {
        return Err("数据目录必须为绝对路径".into());
    }
    let options_file = args.next().map(PathBuf::from);
    let report_file = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err("参数：--campus-search [数据目录] [搜索条件.json] [结果报告.json]".into());
    }
    let mut db = Database::open(&directory).map_err(|e| e.to_string())?;
    let options: Options = match options_file {
        Some(path) => serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
        None => db
            .setting("campus.options")
            .map_err(|e| e.to_string())?
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default(),
    };
    options.validate()?;
    let cancel = AtomicBool::new(false);
    let mut save_error = None;
    let outcome = collect(&options, &directory, &cancel, |progress| {
        if let Progress::Jobs(jobs) = progress
            && let Err(error) = db.save_campus_jobs(&jobs)
        {
            save_error = Some(error.to_string());
            cancel.store(true, Ordering::Relaxed);
        }
    })?;
    if let Some(error) = save_error {
        return Err(format!("结果保存失败：{error}"));
    }
    let report = serde_json::json!({"checked_at":Local::now().to_rfc3339(),"saved":outcome.jobs.len(),"jobs":outcome.jobs,"reports":outcome.reports,"cancelled":outcome.cancelled});
    let summary = serde_json::json!({"checked_at":report["checked_at"],"saved":report["saved"],"reports":report["reports"]});
    db.set_setting(
        "campus.options",
        &serde_json::to_string(&options).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    db.set_setting("campus.last_search", &summary.to_string())
        .map_err(|e| e.to_string())?;
    if let Some(path) = report_file {
        let parent = path.parent().ok_or("报告路径无效")?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        use std::io::Write;
        temporary
            .write_all(&serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        temporary.persist(path).map_err(|e| e.to_string())?;
    }
    println!("{}", report);
    if outcome.jobs.is_empty() {
        return Err("本轮未读到可保存的岗位，详见来源报告".into());
    }
    Ok(())
}
