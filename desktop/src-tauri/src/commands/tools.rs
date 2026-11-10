//! 小工具（程序启动器）相关命令。

use std::path::PathBuf;

use personal_desk::{
    launcher::{self, AppTool},
    tags::{RecordTags, Scope},
};

use crate::state::development_db;

#[derive(serde::Serialize)]
pub struct ToolData {
    items: Vec<AppTool>,
    favorites: Vec<String>,
    tags: Vec<String>,
    assignments: Vec<RecordTags>,
    view: String,
}

#[tauri::command]
pub fn tools_list() -> Result<Vec<AppTool>, String> {
    development_db()?
        .app_tools()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tools_save_classified(entry: AppTool, tags: Vec<String>) -> Result<(), String> {
    development_db()?
        .save_app_tool_classified(&entry, &tags)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tools_delete(id: String) -> Result<(), String> {
    development_db()?
        .delete_app_tool(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tools_launch(path: PathBuf) -> Result<(), String> {
    launcher::launch(&path).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_data() -> Result<ToolData, String> {
    let database = development_db()?;
    let favorites = database
        .favorites()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(|(scope, id)| (scope == "tools").then_some(id))
        .collect();
    let view = database
        .settings()
        .map_err(|error| error.to_string())?
        .get("view.tools")
        .cloned()
        .filter(|value| value == "list" || value == "grid")
        .unwrap_or_else(|| "grid".into());
    Ok(ToolData {
        items: database.app_tools().map_err(|error| error.to_string())?,
        favorites,
        tags: database
            .tags(Scope::Tools)
            .map_err(|error| error.to_string())?,
        assignments: database
            .tagged_records(Scope::Tools)
            .map_err(|error| error.to_string())?,
        view,
    })
}

#[tauri::command]
pub fn tool_favorite(id: String, value: bool) -> Result<(), String> {
    development_db()?
        .set_favorite("tools", &id, value)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_view(mode: String) -> Result<(), String> {
    if mode != "grid" && mode != "list" {
        return Err("视图模式无效".into());
    }
    development_db()?
        .set_setting("view.tools", &mode)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_tags_set(id: String, tags: Vec<String>) -> Result<(), String> {
    development_db()?
        .set_record_tags(Scope::Tools, &id, &tags)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn tool_pick() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("选择快捷程序")
        .add_filter("Windows 程序", &["exe"])
        .pick_file()
        .await
        .map(|file| file.path().to_owned())
}

#[tauri::command]
pub fn tool_tag_create(name: String) -> Result<(), String> {
    development_db()?
        .create_tag(Scope::Tools, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_tag_rename(original: String, name: String) -> Result<(), String> {
    development_db()?
        .rename_tag(Scope::Tools, &original, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_tag_delete(name: String) -> Result<(), String> {
    development_db()?
        .delete_tag(Scope::Tools, &name)
        .map_err(|error| error.to_string())
}
