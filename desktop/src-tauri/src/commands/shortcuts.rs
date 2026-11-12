//! 快捷路径相关命令。

use std::path::PathBuf;

use crate::{
    files,
    model::Shortcut,
    tags::{RecordTags, Scope},
};

use crate::state::development_db;

/// 快捷路径页的聚合数据；包含路径记录、标签、分类分配、置顶、视图和撤销状态。
#[derive(serde::Serialize)]
pub struct ShortcutData {
    items: Vec<Shortcut>,
    tags: Vec<String>,
    assignments: Vec<RecordTags>,
    favorites: Vec<String>,
    view: String,
    undo: bool,
}

/// 读取快捷路径页完整数据（路径、标签、置顶、视图、撤销状态）。
#[tauri::command]
pub fn shortcut_data() -> Result<ShortcutData, String> {
    let database = development_db()?;
    Ok(ShortcutData {
        items: database.shortcuts().map_err(|error| error.to_string())?,
        tags: database
            .tags(Scope::Shortcuts)
            .map_err(|error| error.to_string())?,
        assignments: database
            .tagged_records(Scope::Shortcuts)
            .map_err(|error| error.to_string())?,
        favorites: database
            .favorites()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter_map(|(scope, id)| (scope == "shortcuts").then_some(id))
            .collect(),
        view: database
            .settings()
            .map_err(|error| error.to_string())?
            .get("view.folders")
            .filter(|value| *value == "grid" || *value == "list")
            .cloned()
            .unwrap_or_else(|| "list".into()),
        undo: database
            .index_undo_available()
            .map_err(|error| error.to_string())?,
    })
}

/// 打开文件/文件夹选择框，返回所选路径；取消时返回 null。
#[tauri::command]
pub async fn shortcut_pick(is_dir: bool) -> Option<PathBuf> {
    let dialog = rfd::AsyncFileDialog::new().set_title("选择快捷路径");
    if is_dir {
        dialog.pick_folder().await
    } else {
        dialog.pick_file().await
    }
    .map(|file| file.path().to_owned())
}

/// 添加快捷路径索引。
#[tauri::command]
pub fn shortcut_add(name: String, path: PathBuf, is_dir: bool) -> Result<(), String> {
    development_db()?
        .save_shortcut(&name, &path, is_dir)
        .map_err(|error| error.to_string())
}

/// 在资源管理器中打开或定位快捷路径；文件会选中，文件夹会打开。
#[tauri::command]
pub fn shortcut_open(path: PathBuf) -> Result<(), String> {
    files::open_location(&path).map_err(|error| error.to_string())
}

/// 移除快捷路径索引；不会删除原文件或文件夹。
#[tauri::command]
pub fn shortcut_remove(id: String) -> Result<(), String> {
    development_db()?
        .remove_index("shortcuts", &id)
        .map_err(|error| error.to_string())
}

/// 撤销最近一次快捷路径移除。
#[tauri::command]
pub fn shortcut_undo() -> Result<bool, String> {
    development_db()?
        .undo_index_removal()
        .map_err(|error| error.to_string())
}

/// 设置或取消一条快捷路径的置顶。
#[tauri::command]
pub fn shortcut_favorite(id: String, value: bool) -> Result<(), String> {
    development_db()?
        .set_favorite("shortcuts", &id, value)
        .map_err(|error| error.to_string())
}

/// 设置某条快捷路径的分类标签。
#[tauri::command]
pub fn shortcut_tags(id: String, tags: Vec<String>) -> Result<(), String> {
    development_db()?
        .set_record_tags(Scope::Shortcuts, &id, &tags)
        .map_err(|error| error.to_string())
}

/// 保存快捷路径页的视图模式（grid/list）。
#[tauri::command]
pub fn shortcut_view(mode: String) -> Result<(), String> {
    if mode != "grid" && mode != "list" {
        return Err("视图模式无效".into());
    }
    development_db()?
        .set_setting("view.folders", &mode)
        .map_err(|error| error.to_string())
}

/// 新建快捷路径分类标签。
#[tauri::command]
pub fn shortcut_tag_create(name: String) -> Result<(), String> {
    development_db()?
        .create_tag(Scope::Shortcuts, &name)
        .map_err(|error| error.to_string())
}

/// 重命名快捷路径分类标签。
#[tauri::command]
pub fn shortcut_tag_rename(original: String, name: String) -> Result<(), String> {
    development_db()?
        .rename_tag(Scope::Shortcuts, &original, &name)
        .map_err(|error| error.to_string())
}

/// 删除快捷路径分类标签。
#[tauri::command]
pub fn shortcut_tag_delete(name: String) -> Result<(), String> {
    development_db()?
        .delete_tag(Scope::Shortcuts, &name)
        .map_err(|error| error.to_string())
}
