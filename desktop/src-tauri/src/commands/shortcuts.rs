//! 快捷路径相关命令。

use std::path::PathBuf;

use personal_desk::{
    files,
    model::Shortcut,
    tags::{RecordTags, Scope},
};

use crate::state::development_db;

#[derive(serde::Serialize)]
pub struct ShortcutData {
    items: Vec<Shortcut>,
    tags: Vec<String>,
    assignments: Vec<RecordTags>,
    favorites: Vec<String>,
    view: String,
    undo: bool,
}

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

#[tauri::command]
pub fn shortcut_add(name: String, path: PathBuf, is_dir: bool) -> Result<(), String> {
    development_db()?
        .save_shortcut(&name, &path, is_dir)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn shortcut_open(path: PathBuf) -> Result<(), String> {
    files::open_location(&path).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn shortcut_remove(id: String) -> Result<(), String> {
    development_db()?
        .remove_index("shortcuts", &id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn shortcut_undo() -> Result<bool, String> {
    development_db()?
        .undo_index_removal()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn shortcut_favorite(id: String, value: bool) -> Result<(), String> {
    development_db()?
        .set_favorite("shortcuts", &id, value)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn shortcut_tags(id: String, tags: Vec<String>) -> Result<(), String> {
    development_db()?
        .set_record_tags(Scope::Shortcuts, &id, &tags)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn shortcut_view(mode: String) -> Result<(), String> {
    if mode != "grid" && mode != "list" {
        return Err("视图模式无效".into());
    }
    development_db()?
        .set_setting("view.folders", &mode)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn shortcut_tag_create(name: String) -> Result<(), String> {
    development_db()?
        .create_tag(Scope::Shortcuts, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn shortcut_tag_rename(original: String, name: String) -> Result<(), String> {
    development_db()?
        .rename_tag(Scope::Shortcuts, &original, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn shortcut_tag_delete(name: String) -> Result<(), String> {
    development_db()?
        .delete_tag(Scope::Shortcuts, &name)
        .map_err(|error| error.to_string())
}
