//! 每日随笔相关命令。

use chrono::Local;
use personal_desk::{
    essays::{Assignment, Summary},
    library::Page,
};

use crate::state::development_db;

#[tauri::command]
pub fn essay_today() -> Result<Assignment, String> {
    development_db()?
        .ensure_essay_day(Local::now().date_naive())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn essay_reroll(day: String, expected: String) -> Result<Assignment, String> {
    development_db()?
        .reroll_essay(&day, &expected)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn essay_start(day: String) -> Result<Page, String> {
    development_db()?
        .start_essay(&day)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn essay_history(query: String) -> Result<Vec<Summary>, String> {
    development_db()?
        .essay_summaries(&query)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn essay_page(id: String) -> Result<Option<Page>, String> {
    development_db()?
        .writing_page(&id)
        .map_err(|error| error.to_string())
}
