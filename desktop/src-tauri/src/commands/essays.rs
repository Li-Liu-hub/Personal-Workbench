//! 每日随笔相关命令。

use crate::{
    essays::{Assignment, Summary},
    library::Page,
};
use chrono::Local;

use crate::state::development_db;

/// 获取今天的随笔主题；当天还没有分配时自动创建。
#[tauri::command]
pub fn essay_today() -> Result<Assignment, String> {
    development_db()?
        .ensure_essay_day(Local::now().date_naive())
        .map_err(|error| error.to_string())
}

/// 重新抽取指定日期的随笔主题；expected 传当前 topic id，已开始写作时拒绝。
#[tauri::command]
pub fn essay_reroll(day: String, expected: String) -> Result<Assignment, String> {
    development_db()?
        .reroll_essay(&day, &expected)
        .map_err(|error| error.to_string())
}

/// 开始或继续指定日期的随笔，返回对应的文档页。
#[tauri::command]
pub fn essay_start(day: String) -> Result<Page, String> {
    development_db()?
        .start_essay(&day)
        .map_err(|error| error.to_string())
}

/// 按关键词搜索历史随笔，返回摘要列表。
#[tauri::command]
pub fn essay_history(query: String) -> Result<Vec<Summary>, String> {
    development_db()?
        .essay_summaries(&query)
        .map_err(|error| error.to_string())
}

/// 按文档 ID 读取一篇随笔页面；不存在时返回 null。
#[tauri::command]
pub fn essay_page(id: String) -> Result<Option<Page>, String> {
    development_db()?
        .writing_page(&id)
        .map_err(|error| error.to_string())
}
