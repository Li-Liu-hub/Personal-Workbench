//! 个人办公台 Tauri 后端入口：应用生命周期、命令注册与核心库模块。
//!
//! 业务与数据逻辑在 `core`，状态与数据目录解析在 `state`，
//! 各领域命令实现按目录拆分在 `commands`。

mod commands;
pub mod core;
pub mod state;

/// 让 `crate::db`、`crate::model` 等路径继续指向核心库模块。
pub use self::core::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data = state::data_directory();
    let _instance = data
        .as_ref()
        .ok()
        .and_then(|path| crate::install::Instance::acquire(path));
    if data.is_ok() && _instance.is_none() {
        return;
    }
    state::initialize_database();
    state::spawn_remote_sync();
    tauri::Builder::default()
        .setup(|app| {
            state::build_main_window(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::credentials::credentials_list,
            commands::credentials::credential_search_items,
            commands::credentials::credentials_save,
            commands::credentials::credentials_delete,
            commands::credentials::credential_favorites,
            commands::credentials::credential_favorite,
            commands::credentials::credential_tags,
            commands::credentials::credential_tag_create,
            commands::credentials::credential_tag_rename,
            commands::credentials::credential_tag_delete,
            commands::credentials::credential_view_get,
            commands::credentials::credential_view_set,
            commands::credentials::copy_value,
            commands::credentials::password_generate,
            commands::inventory::inventory_list,
            commands::inventory::inventory_save,
            commands::inventory::inventory_delete,
            commands::inventory::inventory_tags,
            commands::inventory::inventory_favorites,
            commands::inventory::inventory_favorite,
            commands::inventory::inventory_view_get,
            commands::inventory::inventory_view_set,
            commands::inventory::inventory_tag_create,
            commands::inventory::inventory_tag_rename,
            commands::inventory::inventory_tag_delete,
            commands::websites::websites_list,
            commands::websites::websites_delete,
            commands::websites::websites_open,
            commands::websites::website_data,
            commands::websites::website_view,
            commands::websites::website_favorite,
            commands::websites::website_tag_create,
            commands::websites::website_tag_rename,
            commands::websites::website_tag_delete,
            commands::websites::website_save_classified,
            commands::shortcuts::shortcut_data,
            commands::shortcuts::shortcut_pick,
            commands::shortcuts::shortcut_add,
            commands::shortcuts::shortcut_open,
            commands::shortcuts::shortcut_remove,
            commands::shortcuts::shortcut_undo,
            commands::shortcuts::shortcut_favorite,
            commands::shortcuts::shortcut_tags,
            commands::shortcuts::shortcut_view,
            commands::shortcuts::shortcut_tag_create,
            commands::shortcuts::shortcut_tag_rename,
            commands::shortcuts::shortcut_tag_delete,
            commands::tools::tools_list,
            commands::tools::tools_save_classified,
            commands::tools::tools_delete,
            commands::tools::tools_launch,
            commands::tools::tool_data,
            commands::tools::tool_favorite,
            commands::tools::tool_view,
            commands::tools::tool_tags_set,
            commands::tools::tool_pick,
            commands::tools::tool_tag_create,
            commands::tools::tool_tag_rename,
            commands::tools::tool_tag_delete,
            commands::essays::essay_today,
            commands::essays::essay_reroll,
            commands::essays::essay_start,
            commands::essays::essay_history,
            commands::essays::essay_page,
            commands::writing::writing_list,
            commands::writing::writing_directory,
            commands::writing::writing_recent,
            commands::writing::writing_page,
            commands::writing::writing_matches,
            commands::writing::writing_search,
            commands::writing::writing_create,
            commands::writing::writing_save,
            commands::writing::writing_delete,
            commands::writing::writing_draft,
            commands::writing::writing_recover_drafts,
            commands::writing::writing_daily_log,
            commands::writing::writing_tags,
            commands::writing::writing_tag_create,
            commands::writing::writing_tag_rename,
            commands::writing::writing_tag_delete,
            commands::writing::writing_save_copy,
            commands::writing::writing_export_markdown,
            commands::writing::writing_publish_blog,
            commands::writing::writing_image_import,
            commands::writing::writing_image_paste,
            commands::writing::writing_image_read,
            commands::writing::writing_move_book,
            commands::writing::writing_move_page,
            commands::writing::books_list,
            commands::writing::books_save,
            commands::contacts::contacts_list,
            commands::contacts::contact_search_items,
            commands::contacts::contacts_delete,
            commands::contacts::contact_data,
            commands::contacts::contact_save_classified,
            commands::contacts::contact_view,
            commands::contacts::contact_extra,
            commands::contacts::contact_favorite,
            commands::contacts::contact_tag_create,
            commands::contacts::contact_tag_rename,
            commands::contacts::contact_tag_delete,
            commands::contacts::contact_import_photo,
            commands::contacts::contact_import_chat,
            commands::contacts::contact_import_chats,
            commands::contacts::contact_photo,
            commands::contacts::contact_remove_photo,
            commands::contacts::contact_remove_chat,
            commands::contacts::contact_export_chat,
            commands::settings::settings_info,
            commands::settings::settings_check_update,
            commands::settings::settings_install_update,
            commands::settings::settings_autostart_get,
            commands::settings::settings_autostart_set,
            commands::settings::vault_mysql_info,
            commands::settings::vault_mysql_save,
            commands::settings::vault_mysql_test,
            commands::settings::vault_mysql_test_saved,
            commands::settings::vault_mysql_clear,
            commands::settings::vault_sync_now,
            commands::theme::theme_get,
            commands::theme::theme_save,
            commands::theme::theme_set_zoom,
            commands::theme::theme_pick_image,
            commands::theme::theme_image,
            commands::mongo::mongo_info,
            commands::mongo::mongo_save,
            commands::mongo::mongo_test,
            commands::mongo::mongo_test_saved,
            commands::mongo::mongo_clear,
            commands::mongo::writing_publish_mongo,
        ])
        .run(tauri::generate_context!())
        .expect("个人办公台启动失败");
}
