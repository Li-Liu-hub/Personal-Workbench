//! 物品清单相关命令。

use crate::{model::InventoryItem, tags::Scope};

use crate::state::development_db;

/// 读取全部物品记录。
#[tauri::command]
pub fn inventory_list() -> Result<Vec<InventoryItem>, String> {
    development_db()?
        .inventory_items()
        .map_err(|error| error.to_string())
}

/// 新增或更新一条物品记录。
#[tauri::command]
pub fn inventory_save(entry: InventoryItem) -> Result<(), String> {
    entry.validate().map_err(|error| error.to_string())?;
    development_db()?
        .save_inventory_item(&entry)
        .map_err(|error| error.to_string())
}

/// 删除指定物品记录。
#[tauri::command]
pub fn inventory_delete(id: String) -> Result<(), String> {
    development_db()?
        .delete_inventory_item(&id)
        .map_err(|error| error.to_string())
}

/// 读取物品的全部分类标签。
#[tauri::command]
pub fn inventory_tags() -> Result<Vec<String>, String> {
    development_db()?
        .tags(Scope::Inventory)
        .map_err(|error| error.to_string())
}

/// 读取物品的置顶记录 ID 列表。
#[tauri::command]
pub fn inventory_favorites() -> Result<Vec<String>, String> {
    Ok(development_db()?
        .favorites()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(|(scope, id)| (scope == "inventory").then_some(id))
        .collect())
}

/// 设置或取消一条物品记录的置顶。
#[tauri::command]
pub fn inventory_favorite(id: String, value: bool) -> Result<(), String> {
    development_db()?
        .set_favorite("inventory", &id, value)
        .map_err(|error| error.to_string())
}

/// 读取物品页的视图模式（grid/list）。
#[tauri::command]
pub fn inventory_view_get() -> Result<String, String> {
    Ok(development_db()?
        .settings()
        .map_err(|error| error.to_string())?
        .get("view.inventory")
        .filter(|value| *value == "grid" || *value == "list")
        .cloned()
        .unwrap_or_else(|| "grid".into()))
}

/// 保存物品页的视图模式（grid/list）。
#[tauri::command]
pub fn inventory_view_set(mode: String) -> Result<(), String> {
    if mode != "grid" && mode != "list" {
        return Err("视图模式无效".into());
    }
    development_db()?
        .set_setting("view.inventory", &mode)
        .map_err(|error| error.to_string())
}

/// 新建物品分类标签。
#[tauri::command]
pub fn inventory_tag_create(name: String) -> Result<(), String> {
    development_db()?
        .create_tag(Scope::Inventory, &name)
        .map_err(|error| error.to_string())
}

/// 重命名物品分类标签。
#[tauri::command]
pub fn inventory_tag_rename(original: String, name: String) -> Result<(), String> {
    development_db()?
        .rename_tag(Scope::Inventory, &original, &name)
        .map_err(|error| error.to_string())
}

/// 删除物品分类标签。
#[tauri::command]
pub fn inventory_tag_delete(name: String) -> Result<(), String> {
    development_db()?
        .delete_tag(Scope::Inventory, &name)
        .map_err(|error| error.to_string())
}
