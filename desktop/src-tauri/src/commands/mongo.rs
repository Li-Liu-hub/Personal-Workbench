//! MongoDB 文档库配置与发布命令。
//!
//! 数据层实现在核心库 `crate::mongo_config` / `crate::mongo_docs`，
//! 命令层只做参数转发，并在读取数据库时避免把 `MutexGuard` 带过 `await`。

use crate::{
    library::Page,
    mongo_config::{MongoConfig, MongoSettings, MongoSummary},
    mongo_docs,
};
use tauri::async_runtime::spawn_blocking;

use crate::state::{data_directory, development_db};

/// 读取 MongoDB 文档库连接摘要（不含密码）。
#[tauri::command]
pub fn mongo_info() -> Result<MongoSummary, String> {
    MongoConfig::summary().map_err(|error| error.to_string())
}

/// 保存 MongoDB 文档库连接配置。
#[tauri::command]
pub fn mongo_save(
    host: String,
    port: u16,
    user: String,
    password: String,
    auth_database: String,
    database: String,
    collection: String,
) -> Result<(), String> {
    MongoConfig::new(MongoSettings {
        host,
        port,
        user,
        password,
        auth_database,
        database,
        collection,
    })
    .and_then(|config| config.save())
    .map_err(|error| error.to_string())
}

/// 测试一组 MongoDB 连接参数。
#[tauri::command]
pub async fn mongo_test(
    host: String,
    port: u16,
    user: String,
    password: String,
    auth_database: String,
    database: String,
    collection: String,
) -> Result<(), String> {
    let config = MongoConfig::new(MongoSettings {
        host,
        port,
        user,
        password,
        auth_database,
        database,
        collection,
    })
    .map_err(|error| error.to_string())?;
    config
        .test_connection()
        .await
        .map_err(|error| error.to_string())
}

/// 测试已保存的 MongoDB 连接。
#[tauri::command]
pub async fn mongo_test_saved() -> Result<(), String> {
    let config = MongoConfig::load()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "尚未保存 MongoDB 文档库配置".to_string())?;
    config
        .test_connection()
        .await
        .map_err(|error| error.to_string())
}

/// 清除已保存的 MongoDB 连接配置。
#[tauri::command]
pub fn mongo_clear() -> Result<(), String> {
    MongoConfig::clear().map_err(|error| error.to_string())
}

/// 将一篇本机文档（含受管图片）发布到 MongoDB 文档库，返回 `库.集合._id`。
#[tauri::command]
pub async fn writing_publish_mongo(id: String) -> Result<String, String> {
    let data = data_directory()?;
    let page: Page = spawn_blocking(move || {
        let database = development_db()?;
        database
            .writing_page(&id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "文档已不存在".to_string())
    })
    .await
    .map_err(|error| error.to_string())??;
    mongo_docs::publish(&data, &page)
        .await
        .map_err(|error| error.to_string())
}
