//! 联系人相关命令（含照片、聊天记录导入导出）。

use std::path::PathBuf;

use crate::{
    contacts::{self, Contact, ContactSearchItem, MAX_CONTACT_BYTES, MAX_FILE_BYTES},
    tags::{RecordTags, Scope},
};
use base64::Engine;

use crate::state::development_db;

/// 联系人页的聚合数据；包含联系人、标签、分类分配、置顶、视图和扩展字段开关。
#[derive(serde::Serialize)]
pub struct ContactData {
    items: Vec<Contact>,
    tags: Vec<String>,
    assignments: Vec<RecordTags>,
    favorites: Vec<String>,
    view: String,
    extra: bool,
}

/// 批量导入聊天记录的结果；包含新增数量和跳过数量。
#[derive(serde::Serialize)]
pub struct ChatImportResult {
    added: usize,
    skipped: usize,
}

/// 读取联系人概要列表（旧版接口；前端当前使用 contact_data）。
#[tauri::command]
pub fn contacts_list() -> Result<Vec<Contact>, String> {
    development_db()?
        .contact_overviews()
        .map_err(|error| error.to_string())
}

/// 全局搜索使用的联系人字段，不读取照片或聊天记录。
#[tauri::command]
pub fn contact_search_items() -> Result<Vec<ContactSearchItem>, String> {
    development_db()?
        .contact_search_items()
        .map_err(|error| error.to_string())
}

/// 删除联系人及其聊天记录。
#[tauri::command]
pub fn contacts_delete(id: String) -> Result<(), String> {
    development_db()?
        .delete_contact(&id)
        .map_err(|error| error.to_string())
}

/// 读取联系人页完整数据（联系人、标签、分类分配、置顶、视图、扩展字段开关）。
#[tauri::command]
pub fn contact_data() -> Result<ContactData, String> {
    let database = development_db()?;
    Ok(ContactData {
        items: database
            .contact_overviews()
            .map_err(|error| error.to_string())?,
        tags: database
            .tags(Scope::Contacts)
            .map_err(|error| error.to_string())?,
        assignments: database
            .tagged_records(Scope::Contacts)
            .map_err(|error| error.to_string())?,
        favorites: database
            .favorites()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter_map(|(scope, id)| (scope == "contacts").then_some(id))
            .collect(),
        view: database
            .settings()
            .map_err(|error| error.to_string())?
            .get("view.contacts")
            .filter(|value| *value == "grid" || *value == "list")
            .cloned()
            .unwrap_or_else(|| "grid".into()),
        extra: database
            .settings()
            .map_err(|error| error.to_string())?
            .get("view.contacts.extra")
            .is_some_and(|value| value == "true"),
    })
}

/// 新增或更新联系人，并保存其分类标签。
#[tauri::command]
pub fn contact_save_classified(entry: Contact, tags: Vec<String>) -> Result<(), String> {
    contacts::validate(&entry).map_err(|error| error.to_string())?;
    let mut database = development_db()?;
    let result = if database
        .contact_exists(&entry.id)
        .map_err(|error| error.to_string())?
    {
        database.update_contact_details(&entry, &tags)
    } else {
        database.save_contact_with_tags(&entry, &tags)
    };
    result.map_err(|error| error.to_string())
}

/// 保存联系人页的视图模式（grid/list）。
#[tauri::command]
pub fn contact_view(mode: String) -> Result<(), String> {
    if mode != "grid" && mode != "list" {
        return Err("视图模式无效".into());
    }
    development_db()?
        .set_setting("view.contacts", &mode)
        .map_err(|error| error.to_string())
}

/// 设置列表视图是否显示证件号、地址、爱好等扩展字段。
#[tauri::command]
pub fn contact_extra(value: bool) -> Result<(), String> {
    development_db()?
        .set_setting("view.contacts.extra", if value { "true" } else { "false" })
        .map_err(|error| error.to_string())
}

/// 设置或取消一位联系人的置顶。
#[tauri::command]
pub fn contact_favorite(id: String, value: bool) -> Result<(), String> {
    development_db()?
        .set_favorite("contacts", &id, value)
        .map_err(|error| error.to_string())
}

/// 新建联系人分类标签。
#[tauri::command]
pub fn contact_tag_create(name: String) -> Result<(), String> {
    development_db()?
        .create_tag(Scope::Contacts, &name)
        .map_err(|error| error.to_string())
}

/// 重命名联系人分类标签。
#[tauri::command]
pub fn contact_tag_rename(original: String, name: String) -> Result<(), String> {
    development_db()?
        .rename_tag(Scope::Contacts, &original, &name)
        .map_err(|error| error.to_string())
}

/// 删除联系人分类标签。
#[tauri::command]
pub fn contact_tag_delete(name: String) -> Result<(), String> {
    development_db()?
        .delete_tag(Scope::Contacts, &name)
        .map_err(|error| error.to_string())
}

/// 选择照片并导入到联系人；取消时返回 false。
#[tauri::command]
pub async fn contact_import_photo(id: String) -> Result<bool, String> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("选择照片")
        .add_filter("照片", &["png", "jpg", "jpeg", "webp", "bmp"])
        .pick_file()
        .await
    else {
        return Ok(false);
    };
    let path = file.path().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let photo = contacts::import_photo(&path).map_err(|error| error.to_string())?;
        development_db()?
            .set_contact_photo(&id, &photo)
            .map_err(|error| error.to_string())?;
        Ok(true)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 选择单个聊天记录文件导入（旧版单文件入口；前端当前使用批量版）。
#[tauri::command]
pub async fn contact_import_chat(id: String) -> Result<Option<bool>, String> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("选择聊天记录")
        .add_filter(
            "聊天记录",
            &["txt", "md", "markdown", "json", "xlsx", "xls"],
        )
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let path = file.path().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let chat = contacts::import_chat(&path).map_err(|error| error.to_string())?;
        development_db()?
            .append_contact_chat(&id, chat)
            .map(Some)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 选择多个聊天记录文件批量导入，返回新增与跳过数量；取消时返回 null。
#[tauri::command]
pub async fn contact_import_chats(id: String) -> Result<Option<ChatImportResult>, String> {
    let Some(files) = rfd::AsyncFileDialog::new()
        .set_title("选择聊天记录（可多选）")
        .add_filter(
            "聊天记录",
            &["txt", "md", "markdown", "json", "xlsx", "xls"],
        )
        .pick_files()
        .await
    else {
        return Ok(None);
    };
    let paths: Vec<_> = files
        .into_iter()
        .map(|file| file.path().to_owned())
        .collect();
    tauri::async_runtime::spawn_blocking(move || {
        let mut chats = Vec::new();
        let mut bytes = 0;
        for path in paths {
            let chat = contacts::import_chat(&path).map_err(|error| {
                format!(
                    "{}：{error}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )
            })?;
            bytes += chat.original.len();
            if bytes > MAX_CONTACT_BYTES {
                return Err("本批原文件总量超过 32 MiB，请分批导入".into());
            }
            chats.push(chat);
        }
        let count = chats.len();
        let added = development_db()?
            .append_contact_chats(&id, chats)
            .map_err(|error| error.to_string())?;
        Ok(Some(ChatImportResult {
            added,
            skipped: count - added,
        }))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 读取联系人照片，返回 data URL；没有照片时返回 null。
#[tauri::command]
pub fn contact_photo(id: String) -> Result<Option<String>, String> {
    let photo = development_db()?
        .contact_photo(&id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "联系人已不存在".to_string())?;
    if photo.is_empty() {
        return Ok(None);
    }
    if photo.len() > MAX_FILE_BYTES {
        return Err("联系人照片超过大小限制".into());
    }
    let mime = match image::guess_format(&photo).map_err(|error| error.to_string())? {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        image::ImageFormat::Bmp => "image/bmp",
        _ => return Err("不支持的照片格式".into()),
    };
    Ok(Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(photo)
    )))
}

/// 移除联系人照片。
#[tauri::command]
pub fn contact_remove_photo(id: String) -> Result<(), String> {
    development_db()?
        .set_contact_photo(&id, &[])
        .map_err(|error| error.to_string())
}

/// 移除联系人下的一条聊天记录。
#[tauri::command]
pub fn contact_remove_chat(id: String, chat_id: String) -> Result<(), String> {
    development_db()?
        .remove_contact_chat(&id, &chat_id)
        .map_err(|error| error.to_string())
}

/// 导出联系人聊天记录原件；取消时返回 null。
#[tauri::command]
pub async fn contact_export_chat(id: String, chat_id: String) -> Result<Option<PathBuf>, String> {
    let chat = development_db()?
        .contact_chat(&id, &chat_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "聊天记录已不存在".to_string())?;
    let filename = PathBuf::from(&chat.title)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "聊天记录.txt".into());
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("导出聊天记录原件")
        .set_file_name(filename)
        .save_file()
        .await
    else {
        return Ok(None);
    };
    let path = file.path().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        contacts::export_chat(&chat, &path).map_err(|error| error.to_string())?;
        Ok(Some(path))
    })
    .await
    .map_err(|error| error.to_string())?
}
