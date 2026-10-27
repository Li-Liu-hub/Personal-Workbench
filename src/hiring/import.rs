use super::{Result, Settings, err, store::Store};
use serde_json::Value;
use std::path::Path;
pub fn legacy(source: &Path, dir: &Path) -> Result<String> {
    if !source.join("BOSS直聘").is_dir()
        || !source.join("官网投递").is_dir()
        || !source.join("接码").is_dir()
    {
        return Err("请选择包含 BOSS直聘、官网投递、接码 三个目录的文件夹".into());
    }
    std::fs::create_dir_all(dir).map_err(err)?;
    let mut reports = Vec::new();
    if !dir.join("settings.json").exists() {
        let path = source.join("BOSS直聘/config.yaml");
        if path.exists() {
            let raw: Value = serde_yaml_ng::from_str(&std::fs::read_to_string(&path).map_err(err)?)
                .map_err(|_| "BOSS 配置格式有误".to_owned())?;
            let mut settings = Settings::default();
            if let Some(v) = raw["delivery"]["greeting"].as_str() {
                settings.greeting = v.into();
            }
            if let Some(v) = raw["delivery"]["images"].as_array() {
                settings.images = v
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|s| {
                        let p = Path::new(s);
                        if p.is_absolute() {
                            p.to_path_buf()
                        } else {
                            source.join("BOSS直聘").join(p)
                        }
                    })
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
            }
            if let Some(v) = raw["delivery"]["daily_limit"].as_u64() {
                settings.daily_limit = v.clamp(1, u64::from(super::BOSS_MAX_DAILY)) as u32;
            }
            if let Some(v) = raw["delivery"]["interval_min"].as_u64() {
                settings.interval_min = v.clamp(30, 3600);
            }
            if let Some(v) = raw["delivery"]["interval_max"].as_u64() {
                settings.interval_max = v.clamp(settings.interval_min, 3600);
            }
            if let Some(v) = raw["delivery"]["send_windows"].as_array() {
                settings.send_windows = v
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
            }
            if let Some(v) = raw["search"]["keywords"][0].as_str() {
                settings.keyword = v.into();
            }
            if let Some(v) = raw["search"]["cities"][0].as_str() {
                settings.city = v.into();
            }
            settings.save(dir)?;
            reports.push("已导入 BOSS 配置".into());
        }
    }
    let queue = source.join("BOSS直聘/data/apply_queue.json");
    if queue.exists() {
        let n = Store::open(dir)?.import_queue(&queue)?;
        reports.push(format!("已新增 {n} 条队列记录，旧待投项默认跳过"));
    }
    let state = source.join("BOSS直聘/data/state.json");
    if state.exists() {
        let n = Store::open(dir)?.import_state(&state)?;
        reports.push(format!("已补充 {n} 条历史发送状态及当天计数"));
    }
    let profile = source.join("官网投递/官网投递资料.md");
    if profile.exists() && !dir.join("profile.md").exists() {
        super::atomic_write(
            &dir.join("profile.md"),
            &std::fs::read(profile).map_err(err)?,
        )?;
        reports.push("已复制官网资料".into());
    }
    let token = source.join("接码/data/otp_token.txt");
    if token.exists() && !dir.join("otp_token.txt").exists() {
        super::atomic_write(
            &dir.join("otp_token.txt"),
            &std::fs::read(token).map_err(err)?,
        )?;
        super::otp::private_file(&dir.join("otp_token.txt"))?;
        reports.push("已沿用接码密钥".into());
    }
    Ok(format!(
        "{}。\n原目录和数据均保留；首次使用请在专用浏览器重新登录，确认旧队列已停止。",
        reports.join("；")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "reads the user's source snapshots into a temporary directory without modifying them"]
    fn import_real_snapshots_without_writing_sources() {
        let root = std::env::var_os("PERSONAL_DESK_LEGACY_ROOT").expect("set source snapshot root");
        let source = Path::new(&root);
        let dir = tempfile::tempdir().unwrap();
        let report = legacy(source, dir.path()).unwrap();
        assert!(report.contains("原目录和数据均保留"));
        let jobs = Store::open(dir.path()).unwrap().jobs().unwrap();
        assert!(!jobs.is_empty());
        assert!(jobs.iter().all(|j| j.status != "pending"));
        assert!(!Settings::load(dir.path()).unwrap().greeting.is_empty());
        assert!(
            !super::super::profile::Profile::read(&dir.path().join("profile.md"))
                .unwrap()
                .fields
                .is_empty()
        );
        let second = legacy(source, dir.path()).unwrap();
        assert!(second.contains("新增 0 条队列记录"));
    }
}
