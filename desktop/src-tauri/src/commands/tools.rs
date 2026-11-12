//! 小工具（程序启动器）相关命令。

use std::path::PathBuf;

use crate::{
    launcher::{self, AppTool},
    tags::{RecordTags, Scope},
};

use crate::state::development_db;

/// 小工具页的聚合数据；包含记录、置顶 ID、标签、分类分配和视图模式。
#[derive(serde::Serialize)]
pub struct ToolData {
    items: Vec<AppTool>,
    favorites: Vec<String>,
    tags: Vec<String>,
    assignments: Vec<RecordTags>,
    view: String,
}

/// 读取全部小工具记录（旧版接口；前端当前使用 tool_data）。
#[tauri::command]
pub fn tools_list() -> Result<Vec<AppTool>, String> {
    development_db()?
        .app_tools()
        .map_err(|error| error.to_string())
}

/// 新增或更新小工具，并保存其分类标签。
#[tauri::command]
pub fn tools_save_classified(entry: AppTool, tags: Vec<String>) -> Result<(), String> {
    development_db()?
        .save_app_tool_classified(&entry, &tags)
        .map_err(|error| error.to_string())
}

/// 删除小工具记录；不会删除原程序文件。
#[tauri::command]
pub fn tools_delete(id: String) -> Result<(), String> {
    development_db()?
        .delete_app_tool(&id)
        .map_err(|error| error.to_string())
}

/// 启动指定的 EXE 或文件。
#[tauri::command]
pub fn tools_launch(path: PathBuf) -> Result<(), String> {
    launcher::launch(&path).map_err(|error| error.to_string())
}

/// 读取小工具页完整数据（记录、置顶、标签、视图）。
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

/// 设置或取消一条小工具的置顶。
#[tauri::command]
pub fn tool_favorite(id: String, value: bool) -> Result<(), String> {
    development_db()?
        .set_favorite("tools", &id, value)
        .map_err(|error| error.to_string())
}

/// 保存小工具页的视图模式（grid/list）。
#[tauri::command]
pub fn tool_view(mode: String) -> Result<(), String> {
    if mode != "grid" && mode != "list" {
        return Err("视图模式无效".into());
    }
    development_db()?
        .set_setting("view.tools", &mode)
        .map_err(|error| error.to_string())
}

/// 设置某个小工具的分类标签（旧版接口；前端当前通过保存接口一并提交）。
#[tauri::command]
pub fn tool_tags_set(id: String, tags: Vec<String>) -> Result<(), String> {
    development_db()?
        .set_record_tags(Scope::Tools, &id, &tags)
        .map_err(|error| error.to_string())
}

/// 打开程序选择框，返回所选 EXE 路径；取消时返回 null。
#[tauri::command]
pub async fn tool_pick() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("选择快捷程序")
        .add_filter("Windows 程序", &["exe"])
        .pick_file()
        .await
        .map(|file| file.path().to_owned())
}

/// 新建小工具分类标签。
#[tauri::command]
pub fn tool_tag_create(name: String) -> Result<(), String> {
    development_db()?
        .create_tag(Scope::Tools, &name)
        .map_err(|error| error.to_string())
}

/// 重命名小工具分类标签。
#[tauri::command]
pub fn tool_tag_rename(original: String, name: String) -> Result<(), String> {
    development_db()?
        .rename_tag(Scope::Tools, &original, &name)
        .map_err(|error| error.to_string())
}

/// 删除小工具分类标签。
#[tauri::command]
pub fn tool_tag_delete(name: String) -> Result<(), String> {
    development_db()?
        .delete_tag(Scope::Tools, &name)
        .map_err(|error| error.to_string())
}
