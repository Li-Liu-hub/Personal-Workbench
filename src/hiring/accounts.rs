use super::{Result, Settings, store::Store};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_ACCOUNT: &str = "default";
pub const MAX_ACCOUNTS: usize = 3;
pub const MAX_POOL: usize = MAX_ACCOUNTS * super::BOSS_MAX_DAILY as usize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub label: String,
    pub slot: u16,
    pub daily_limit: u32,
    pub enabled: bool,
    pub login_checked_at: i64,
    pub today_attempts: u32,
    pub today_sent: u32,
    pub remaining: u32,
}
impl Account {
    pub fn port(&self) -> u16 {
        19333 + self.slot
    }
    pub fn browser_directory(&self, root: &Path) -> PathBuf {
        if self.id == DEFAULT_ACCOUNT {
            root.to_path_buf()
        } else {
            root.join("accounts").join(&self.id)
        }
    }
}
pub fn id(value: &str) -> Result<&str> {
    if value.is_empty() || value == DEFAULT_ACCOUNT {
        return Ok(DEFAULT_ACCOUNT);
    }
    uuid::Uuid::parse_str(value).map_err(|_| "账号标识无效".to_owned())?;
    Ok(value)
}
pub fn active(root: &Path, settings: &Settings) -> Result<Account> {
    Store::open(root)?.account(id(&settings.account_id)?, settings.daily_limit)
}
pub fn label(value: &str) -> Result<&str> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 40 || value.chars().any(char::is_control) {
        return Err("账号名称须为 1–40 个字符，且不能含换行或控制字符".into());
    }
    Ok(value)
}
