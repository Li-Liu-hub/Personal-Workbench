//! 个人办公台 Tauri 后端入口：只负责应用生命周期与命令注册。
//!
//! 状态与数据目录解析在 `state`，各领域命令实现按目录拆分在 `commands`。

mod commands;
pub mod state;

use commands::{
    contacts, credentials, essays, export, inventory, mongo, settings, shortcuts, theme, tools,
    websites, writing,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data = state::data_directory();
    let _instance = data
        .as_ref()
        .ok()
        .and_then(|path| personal_desk::install::Instance::acquire(path));
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
            credentials::credentials_list,
            credentials::credentials_save,
            credentials::credentials_delete,
            credentials::credential_favorites,
            credentials::credential_favorite,
            credentials::credential_tags,
            credentials::credential_tag_create,
            credentials::credential_tag_rename,
            credentials::credential_tag_delete,
            credentials::credential_view_get,
            credentials::credential_view_set,
            credentials::copy_value,
            credentials::password_generate,
            inventory::inventory_list,
            inventory::inventory_save,
            inventory::inventory_delete,
            inventory::inventory_tags,
            inventory::inventory_favorites,
            inventory::inventory_favorite,
            inventory::inventory_view_get,
            inventory::inventory_view_set,
            inventory::inventory_tag_create,
            inventory::inventory_tag_rename,
            inventory::inventory_tag_delete,
            websites::websites_list,
            websites::websites_delete,
            websites::websites_open,
            websites::website_data,
            websites::website_view,
            websites::website_favorite,
            websites::website_tag_create,
            websites::website_tag_rename,
            websites::website_tag_delete,
            websites::website_save_classified,
            shortcuts::shortcut_data,
            shortcuts::shortcut_pick,
            shortcuts::shortcut_add,
            shortcuts::shortcut_open,
            shortcuts::shortcut_remove,
            shortcuts::shortcut_undo,
            shortcuts::shortcut_favorite,
            shortcuts::shortcut_tags,
            shortcuts::shortcut_view,
            shortcuts::shortcut_tag_create,
            shortcuts::shortcut_tag_rename,
            shortcuts::shortcut_tag_delete,
            tools::tools_list,
            tools::tools_save_classified,
            tools::tools_delete,
            tools::tools_launch,
            tools::tool_data,
            tools::tool_favorite,
            tools::tool_view,
            tools::tool_tags_set,
            tools::tool_pick,
            tools::tool_tag_create,
            tools::tool_tag_rename,
            tools::tool_tag_delete,
            essays::essay_today,
            essays::essay_reroll,
            essays::essay_start,
            essays::essay_history,
            essays::essay_page,
            writing::writing_list,
            writing::writing_create,
            writing::writing_save,
            writing::writing_delete,
            writing::writing_draft,
            writing::writing_recover_drafts,
            writing::writing_daily_log,
            writing::writing_tags,
            writing::writing_tag_create,
            writing::writing_tag_rename,
            writing::writing_tag_delete,
            writing::writing_save_copy,
            writing::writing_export_markdown,
            writing::writing_publish_blog,
            writing::writing_image_import,
            writing::writing_image_paste,
            writing::writing_image_read,
            writing::writing_move_book,
            writing::writing_move_page,
            writing::books_list,
            writing::books_save,
            contacts::contacts_list,
            contacts::contacts_delete,
            contacts::contact_data,
            contacts::contact_save_classified,
            contacts::contact_view,
            contacts::contact_extra,
            contacts::contact_favorite,
            contacts::contact_tag_create,
            contacts::contact_tag_rename,
            contacts::contact_tag_delete,
            contacts::contact_import_photo,
            contacts::contact_import_chat,
            contacts::contact_import_chats,
            contacts::contact_photo,
            contacts::contact_remove_photo,
            contacts::contact_remove_chat,
            contacts::contact_export_chat,
            settings::settings_info,
            settings::settings_check_update,
            settings::settings_install_update,
            settings::settings_autostart_get,
            settings::settings_autostart_set,
            settings::vault_mysql_info,
            settings::vault_mysql_save,
            settings::vault_mysql_test,
            settings::vault_mysql_test_saved,
            settings::vault_mysql_clear,
            settings::vault_sync_now,
            export::export_test_data,
            theme::theme_get,
            theme::theme_save,
            theme::theme_set_zoom,
            theme::theme_pick_image,
            theme::theme_image,
            mongo::mongo_info,
            mongo::mongo_save,
            mongo::mongo_test,
            mongo::mongo_test_saved,
            mongo::mongo_clear,
            mongo::writing_publish_mongo,
        ])
        .run(tauri::generate_context!())
        .expect("个人办公台启动失败");
}
