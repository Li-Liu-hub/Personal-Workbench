//! 设置页相关命令：版本信息、更新、开机启动与私网加密库直连。

use std::path::PathBuf;

use personal_desk::{
    autostart, updater,
    vault_config::{VaultMysqlConfig, VaultMysqlSummary},
};

use crate::state::{data_directory, development_db};

#[derive(serde::Serialize)]
pub struct SettingsInfo {
    version: &'static str,
    data_directory: PathBuf,
    releases: serde_json::Value,
}

#[derive(serde::Serialize)]
pub struct UpdateInfo {
    version: Option<String>,
    notes: Option<String>,
}

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

#[tauri::command]
pub fn settings_install_update() -> Result<(), String> {
    let release = updater::check(&updater::default_source(), updater::VERSION)?
        .ok_or_else(|| "当前已是最新版本".to_string())?;
    updater::launch(&release, &data_directory()?)
}

#[tauri::command]
pub fn settings_autostart_get() -> Result<bool, String> {
    Ok(autostart::registered_command()?.is_some())
}

#[tauri::command]
pub fn settings_autostart_set(enabled: bool) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    autostart::set(enabled, &executable)
}

#[tauri::command]
pub fn vault_mysql_info() -> Result<VaultMysqlSummary, String> {
    VaultMysqlConfig::summary().map_err(|error| error.to_string())
}

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

#[tauri::command]
pub fn vault_mysql_test_saved() -> Result<(), String> {
    let config = VaultMysqlConfig::load()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "尚未保存直连配置".to_string())?;
    config.test_connection().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn vault_mysql_clear() -> Result<(), String> {
    VaultMysqlConfig::clear().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn vault_sync_now() -> Result<bool, String> {
    development_db()?
        .sync_remote_now()
        .map_err(|error| error.to_string())
}
