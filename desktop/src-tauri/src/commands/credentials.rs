//! 账号密码与口令工具相关命令。

use crate::{
    model::{Credential, CredentialSearchItem},
    passwords,
    tags::Scope,
};

use crate::state::{development_db, with_vault};

#[tauri::command]
pub async fn credentials_list() -> Result<Vec<Credential>, String> {
    with_vault(|database| database.credentials()).await
}

/// 全局搜索使用的账号字段，不读取口令或备注。
#[tauri::command]
pub async fn credential_search_items() -> Result<Vec<CredentialSearchItem>, String> {
    with_vault(|database| database.credential_search_items()).await
}

#[tauri::command]
pub async fn credentials_save(entry: Credential) -> Result<(), String> {
    entry.validate().map_err(|error| error.to_string())?;
    with_vault(move |database| database.save_credential(&entry)).await
}

#[tauri::command]
pub async fn credentials_delete(id: String) -> Result<(), String> {
    with_vault(move |database| database.delete_credential(&id)).await
}

#[tauri::command]
pub async fn credential_favorites() -> Result<Vec<String>, String> {
    with_vault(|database| {
        Ok(database
            .favorites()?
            .into_iter()
            .filter_map(|(scope, id)| (scope == "credentials").then_some(id))
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn credential_favorite(id: String, value: bool) -> Result<(), String> {
    with_vault(move |database| database.set_favorite("credentials", &id, value)).await
}

#[tauri::command]
pub async fn credential_tags() -> Result<Vec<String>, String> {
    with_vault(|database| database.tags(Scope::Credentials)).await
}

#[tauri::command]
pub async fn credential_tag_create(name: String) -> Result<(), String> {
    with_vault(move |database| database.create_tag(Scope::Credentials, &name)).await
}

#[tauri::command]
pub async fn credential_tag_rename(original: String, name: String) -> Result<(), String> {
    with_vault(move |database| database.rename_tag(Scope::Credentials, &original, &name)).await
}

#[tauri::command]
pub async fn credential_tag_delete(name: String) -> Result<(), String> {
    with_vault(move |database| database.delete_tag(Scope::Credentials, &name)).await
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
