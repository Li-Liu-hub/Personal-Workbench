//! 联系人相关命令（含照片、聊天记录导入导出）。

use std::path::PathBuf;

use base64::Engine;
use personal_desk::{
    contacts::{self, Contact, MAX_CONTACT_BYTES, MAX_FILE_BYTES},
    tags::{RecordTags, Scope},
};

use crate::state::development_db;

#[derive(serde::Serialize)]
pub struct ContactData {
    items: Vec<Contact>,
    tags: Vec<String>,
    assignments: Vec<RecordTags>,
    favorites: Vec<String>,
    view: String,
    extra: bool,
}

#[derive(serde::Serialize)]
pub struct ChatImportResult {
    added: usize,
    skipped: usize,
}

#[tauri::command]
pub fn contacts_list() -> Result<Vec<Contact>, String> {
    development_db()?
        .contact_overviews()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn contacts_delete(id: String) -> Result<(), String> {
    development_db()?
        .delete_contact(&id)
        .map_err(|error| error.to_string())
}

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

#[tauri::command]
pub fn contact_save_classified(entry: Contact, tags: Vec<String>) -> Result<(), String> {
    contacts::validate(&entry).map_err(|error| error.to_string())?;
    let mut database = development_db()?;
    let result = if database
        .contact(&entry.id)
        .map_err(|error| error.to_string())?
        .is_some()
    {
        database.update_contact_details(&entry, &tags)
    } else {
        database.save_contact_with_tags(&entry, &tags)
    };
    result.map_err(|error| error.to_string())
}

#[tauri::command]
pub fn contact_view(mode: String) -> Result<(), String> {
    if mode != "grid" && mode != "list" {
        return Err("视图模式无效".into());
    }
    development_db()?
        .set_setting("view.contacts", &mode)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn contact_extra(value: bool) -> Result<(), String> {
    development_db()?
        .set_setting("view.contacts.extra", if value { "true" } else { "false" })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn contact_favorite(id: String, value: bool) -> Result<(), String> {
    development_db()?
        .set_favorite("contacts", &id, value)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn contact_tag_create(name: String) -> Result<(), String> {
    development_db()?
        .create_tag(Scope::Contacts, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn contact_tag_rename(original: String, name: String) -> Result<(), String> {
    development_db()?
        .rename_tag(Scope::Contacts, &original, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn contact_tag_delete(name: String) -> Result<(), String> {
    development_db()?
        .delete_tag(Scope::Contacts, &name)
        .map_err(|error| error.to_string())
}

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

#[tauri::command]
pub fn contact_photo(id: String) -> Result<Option<String>, String> {
    let contact = development_db()?
        .contact(&id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "联系人已不存在".to_string())?;
    if contact.photo.is_empty() {
        return Ok(None);
    }
    if contact.photo.len() > MAX_FILE_BYTES {
        return Err("联系人照片超过大小限制".into());
    }
    let mime = match image::guess_format(&contact.photo).map_err(|error| error.to_string())? {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        image::ImageFormat::Bmp => "image/bmp",
        _ => return Err("不支持的照片格式".into()),
    };
    Ok(Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(contact.photo)
    )))
}

#[tauri::command]
pub fn contact_remove_photo(id: String) -> Result<(), String> {
    development_db()?
        .set_contact_photo(&id, &[])
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn contact_remove_chat(id: String, chat_id: String) -> Result<(), String> {
    development_db()?
        .remove_contact_chat(&id, &chat_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn contact_export_chat(id: String, chat_id: String) -> Result<Option<PathBuf>, String> {
    let contact = development_db()?
        .contact(&id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "联系人已不存在".to_string())?;
    let chat = contact
        .chats
        .into_iter()
        .find(|chat| chat.id == chat_id)
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
