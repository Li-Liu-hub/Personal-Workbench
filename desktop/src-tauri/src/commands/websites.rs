//! 网站收藏相关命令。

use crate::{
    model::Website,
    tags::{RecordTags, Scope},
    websites,
};

use crate::state::development_db;

/// 网站页的聚合数据；包含分类标签、记录分类分配、置顶 ID 和视图模式。
#[derive(serde::Serialize)]
pub struct WebsiteData {
    tags: Vec<String>,
    assignments: Vec<RecordTags>,
    favorites: Vec<String>,
    view: String,
}

/// 读取全部网站收藏记录。
#[tauri::command]
pub fn websites_list() -> Result<Vec<Website>, String> {
    development_db()?
        .websites()
        .map_err(|error| error.to_string())
}

/// 删除指定网站收藏。
#[tauri::command]
pub fn websites_delete(id: String) -> Result<(), String> {
    development_db()?
        .delete_website(&id)
        .map_err(|error| error.to_string())
}

/// 在系统默认浏览器中打开网站地址。
#[tauri::command]
pub fn websites_open(url: String) -> Result<(), String> {
    let site = websites::prepare(Website {
        url,
        ..Default::default()
    })
    .map_err(|error| error.to_string())?;
    open::that_detached(site.url).map_err(|error| format!("打开网站失败：{error}"))
}

/// 读取网站页完整数据（标签、分类分配、置顶、视图）。
#[tauri::command]
pub fn website_data() -> Result<WebsiteData, String> {
    let database = development_db()?;
    Ok(WebsiteData {
        tags: database
            .tags(Scope::Websites)
            .map_err(|error| error.to_string())?,
        assignments: database
            .tagged_records(Scope::Websites)
            .map_err(|error| error.to_string())?,
        favorites: database
            .favorites()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter_map(|(scope, id)| (scope == "websites").then_some(id))
            .collect(),
        view: database
            .settings()
            .map_err(|error| error.to_string())?
            .get("view.websites")
            .filter(|value| *value == "grid" || *value == "list")
            .cloned()
            .unwrap_or_else(|| "grid".into()),
    })
}

/// 保存网站页的视图模式（grid/list）。
#[tauri::command]
pub fn website_view(mode: String) -> Result<(), String> {
    if mode != "grid" && mode != "list" {
        return Err("视图模式无效".into());
    }
    development_db()?
        .set_setting("view.websites", &mode)
        .map_err(|error| error.to_string())
}

/// 设置或取消一条网站收藏的置顶。
#[tauri::command]
pub fn website_favorite(id: String, value: bool) -> Result<(), String> {
    development_db()?
        .set_favorite("websites", &id, value)
        .map_err(|error| error.to_string())
}

/// 新建网站分类标签。
#[tauri::command]
pub fn website_tag_create(name: String) -> Result<(), String> {
    development_db()?
        .create_tag(Scope::Websites, &name)
        .map_err(|error| error.to_string())
}

/// 重命名网站分类标签。
#[tauri::command]
pub fn website_tag_rename(original: String, name: String) -> Result<(), String> {
    development_db()?
        .rename_tag(Scope::Websites, &original, &name)
        .map_err(|error| error.to_string())
}

/// 删除网站分类标签。
#[tauri::command]
pub fn website_tag_delete(name: String) -> Result<(), String> {
    development_db()?
        .delete_tag(Scope::Websites, &name)
        .map_err(|error| error.to_string())
}

/// 新增或更新网站收藏，并保存其分类标签。
#[tauri::command]
pub fn website_save_classified(entry: Website, tags: Vec<String>) -> Result<(), String> {
    development_db()?
        .save_website_classified(&entry, &tags)
        .map_err(|error| error.to_string())
}
