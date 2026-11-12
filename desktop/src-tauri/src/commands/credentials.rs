//! 账号密码与口令工具相关命令。

use crate::{
    model::{Credential, CredentialSearchItem},
    passwords,
    tags::Scope,
};

use crate::state::development_db;

#[tauri::command]
pub fn credentials_list() -> Result<Vec<Credential>, String> {
    development_db()?
        .credentials()
        .map_err(|error| error.to_string())
}

/// 全局搜索使用的账号字段，不读取口令或备注。
#[tauri::command]
pub fn credential_search_items() -> Result<Vec<CredentialSearchItem>, String> {
    development_db()?
        .credential_search_items()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn credentials_save(entry: Credential) -> Result<(), String> {
    entry.validate().map_err(|error| error.to_string())?;
    development_db()?
        .save_credential(&entry)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn credentials_delete(id: String) -> Result<(), String> {
    development_db()?
        .delete_credential(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn credential_favorites() -> Result<Vec<String>, String> {
    Ok(development_db()?
        .favorites()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(|(scope, id)| (scope == "credentials").then_some(id))
        .collect())
}

#[tauri::command]
pub fn credential_favorite(id: String, value: bool) -> Result<(), String> {
    development_db()?
        .set_favorite("credentials", &id, value)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn credential_tags() -> Result<Vec<String>, String> {
    development_db()?
        .tags(Scope::Credentials)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn credential_tag_create(name: String) -> Result<(), String> {
    development_db()?
        .create_tag(Scope::Credentials, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn credential_tag_rename(original: String, name: String) -> Result<(), String> {
    development_db()?
        .rename_tag(Scope::Credentials, &original, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn credential_tag_delete(name: String) -> Result<(), String> {
    development_db()?
        .delete_tag(Scope::Credentials, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn copy_value(value: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    clipboard.set_text(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn password_generate() -> Result<String, String> {
    passwords::generate().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn credential_view_get() -> Result<String, String> {
    let value = development_db()?
        .settings()
        .map_err(|error| error.to_string())?
        .get("view.credentials")
        .cloned();
    Ok(if value.as_deref() == Some("list") {
        "list"
    } else {
        "grid"
    }
    .into())
}

#[tauri::command]
pub fn credential_view_set(mode: String) -> Result<(), String> {
    if mode != "grid" && mode != "list" {
        return Err("视图模式无效".into());
    }
    development_db()?
        .set_setting("view.credentials", &mode)
        .map_err(|error| error.to_string())
}
