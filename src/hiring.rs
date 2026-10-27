//! Native recruitment tools. User data is separate from the imported source copies.
pub mod accounts;
pub mod boss;
pub mod browser;
#[cfg(test)]
mod browser_tests;
pub mod dedupe;
#[cfg(test)]
mod export_tests;
pub mod import;
pub mod lease;
pub mod mcp;
pub mod official;
pub mod otp;
pub mod platforms;
pub mod profile;
pub mod store;
pub mod tracking;
pub mod unified;
use serde::{Deserialize, Serialize};
use std::path::Path;
pub const BOSS_MAX_DAILY: u32 = 150;
pub type Result<T> = std::result::Result<T, String>;
pub fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    atomic_write(path, &serde_json::to_vec_pretty(value).map_err(err)?)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("缺少保存目录")?;
    std::fs::create_dir_all(parent).map_err(err)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(err)?;
    use std::io::Write;
    tmp.write_all(bytes).map_err(err)?;
    tmp.as_file().sync_all().map_err(err)?;
    tmp.persist(path).map_err(err)?;
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub keyword: String,
    pub city: String,
    pub experience: String,
    pub degree: String,
    pub job_type: String,
    pub greeting: String,
    pub images: Vec<String>,
    pub daily_limit: u32,
    pub interval_min: u64,
    pub interval_max: u64,
    pub send_windows: Vec<String>,
    pub account_id: String,
    pub browser_path: String,
    pub otp_host: String,
    pub otp_port: u16,
    pub unified_max_pages: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            keyword: "Java后端开发".into(),
            city: "上海".into(),
            experience: "在校生".into(),
            degree: "本科".into(),
            job_type: "实习".into(),
            greeting: "您好，我对这个岗位很感兴趣，方便聊聊吗？".into(),
            images: vec![],
            daily_limit: 150,
            interval_min: 30,
            interval_max: 60,
            send_windows: vec!["09:00-18:00".into()],
            account_id: String::new(),
            browser_path: String::new(),
            otp_host: "auto".into(),
            otp_port: 8765,
            unified_max_pages: 50,
        }
    }
}
impl Settings {
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("settings.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        serde_json::from_slice(&std::fs::read(path).map_err(err)?).map_err(err)
    }
    pub fn save(&self, dir: &Path) -> Result<()> {
        if self.interval_min < 30
            || self.interval_max < self.interval_min
            || self.interval_max > 3600
            || self.daily_limit == 0
            || self.daily_limit > BOSS_MAX_DAILY
            || self.unified_max_pages == 0
            || self.unified_max_pages > 50
        {
            return Err("投递间隔须为 30–3600 秒，每日上限须为 1–150".into());
        }
        store::in_window(&self.send_windows, chrono::Local::now().time())?;
        atomic_json(&dir.join("settings.json"), self)
    }
}
