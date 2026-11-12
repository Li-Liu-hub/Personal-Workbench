//! 设置页相关命令：版本信息、更新、开机启动与私网加密库直连。

use std::path::PathBuf;

use crate::{
    autostart, updater,
    vault_config::{VaultMysqlConfig, VaultMysqlSummary},
};

use crate::state::{data_directory, development_db};

/// 设置页的版本信息；包含当前版本、数据目录和发布历史。
#[derive(serde::Serialize)]
pub struct SettingsInfo {
    version: &'static str,
    data_directory: PathBuf,
    releases: serde_json::Value,
}

/// 更新检查结果；包含可用版本号和更新说明，无更新时为空。
#[derive(serde::Serialize)]
pub struct UpdateInfo {
    version: Option<String>,
    notes: Option<String>,
}

/// 读取版本、数据目录和发布历史。
#[tauri::command]
pub fn settings_info() -> Result<SettingsInfo, String> {
    let data_directory = data_directory()?;
    Ok(SettingsInfo {
        version: updater::VERSION,
        data_directory,
        releases: serde_json::from_str(include_str!("../../../../assets/release_history.json"))
            .map_err(|error| error.to_string())?,
    })
}

/// 检查 GitHub Releases 是否有新版本；无更新时 version/notes 为空。
#[tauri::command]
pub fn settings_check_update() -> Result<UpdateInfo, String> {
    let result = updater::check(&updater::default_source(), updater::VERSION)?;
    Ok(match result {
        Some(release) => UpdateInfo {
            version: Some(release.manifest.version),
            notes: Some(release.manifest.notes),
        },
        None => UpdateInfo {
            version: None,
            notes: None,
        },
    })
}

/// 下载并启动检测到的新版本安装程序。
#[tauri::command]
pub fn settings_install_update() -> Result<(), String> {
    let release = updater::check(&updater::default_source(), updater::VERSION)?
        .ok_or_else(|| "当前已是最新版本".to_string())?;
    updater::launch(&release, &data_directory()?)
}

/// 读取是否已设置开机自启动。
#[tauri::command]
pub fn settings_autostart_get() -> Result<bool, String> {
    Ok(autostart::registered_command()?.is_some())
}

/// 开启或关闭开机自启动。
#[tauri::command]
pub fn settings_autostart_set(enabled: bool) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    autostart::set(enabled, &executable)
}

/// 读取 MySQL 私网加密库连接摘要（不含密码）。
#[tauri::command]
pub fn vault_mysql_info() -> Result<VaultMysqlSummary, String> {
    VaultMysqlConfig::summary().map_err(|error| error.to_string())
}

/// 保存 MySQL 私网加密库连接配置。
#[tauri::command]
pub fn vault_mysql_save(
    host: String,
    port: u16,
    user: String,
    password: String,
    database: String,
) -> Result<(), String> {
    VaultMysqlConfig::new(host, port, user, password, database)
        .and_then(|config| config.save())
        .map_err(|error| error.to_string())
}

/// 测试一组 MySQL 连接参数。
#[tauri::command]
pub fn vault_mysql_test(
    host: String,
    port: u16,
    user: String,
    password: String,
    database: String,
) -> Result<(), String> {
    VaultMysqlConfig::new(host, port, user, password, database)
        .and_then(|config| config.test_connection())
        .map_err(|error| error.to_string())
}

/// 测试已保存的 MySQL 连接。
#[tauri::command]
pub fn vault_mysql_test_saved() -> Result<(), String> {
    let config = VaultMysqlConfig::load()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "尚未保存直连配置".to_string())?;
    config.test_connection().map_err(|error| error.to_string())
}

/// 清除已保存的 MySQL 连接配置。
#[tauri::command]
pub fn vault_mysql_clear() -> Result<(), String> {
    VaultMysqlConfig::clear().map_err(|error| error.to_string())
}

/// 立即触发一次私网加密库同步，返回是否实际执行。
#[tauri::command]
pub fn vault_sync_now() -> Result<bool, String> {
    development_db()?
        .sync_remote_now()
        .map_err(|error| error.to_string())
}
