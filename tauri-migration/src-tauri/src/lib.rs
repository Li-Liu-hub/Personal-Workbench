use personal_desk::{db::Database, model::Credential};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

static DATABASE: OnceLock<Mutex<Database>> = OnceLock::new();
static DATABASE_ERROR: OnceLock<String> = OnceLock::new();

// The production path is enabled only for an explicitly built release candidate.
fn data_directory() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("PERSONAL_DESK_MIGRATION_TEST_DATA") {
        let path = PathBuf::from(path);
        return if path.is_absolute() {
            Ok(path)
        } else {
            Err("测试数据目录必须是绝对路径".into())
        };
    }
    #[cfg(not(debug_assertions))]
    {
        personal_desk::data_directory::resolve()
    }
    #[cfg(debug_assertions)]
    {
        Err("开发版仅允许使用显式指定的测试数据目录".into())
    }
}

fn initialize_database() {
    let result = data_directory().and_then(|directory| {
        Database::open(&directory).map_err(|error| error.to_string())
    });
    match result {
        Ok(database) => {
            let _ = DATABASE.set(Mutex::new(database));
        }
        Err(error) => {
            let _ = DATABASE_ERROR.set(error);
        }
    }
}

fn development_db() -> Result<MutexGuard<'static, Database>, String> {
    if let Some(error) = DATABASE_ERROR.get() {
        return Err(error.clone());
    }
    DATABASE
        .get()
        .ok_or_else(|| "数据库尚未初始化".to_string())?
        .lock()
        .map_err(|_| "数据库锁已损坏".to_string())
}

fn writing_assets_data_directory() -> Result<PathBuf, String> {
    let data = data_directory()?;
    if !personal_desk::writing_images::remote_assets_ready(&data) {
        return Err("写作图片正在后台加载，请稍候再试".into());
    }
    Ok(data)
}

#[tauri::command]
fn credentials_list() -> Result<Vec<Credential>, String> {
    development_db()?.credentials().map_err(|e| e.to_string())
}
#[tauri::command]
fn credentials_save(entry: Credential) -> Result<(), String> {
    if entry.id.is_empty() || entry.title.trim().is_empty() { return Err("名称和编号不能为空".into()); }
    development_db()?.save_credential(&entry).map_err(|e| e.to_string())
}
#[tauri::command]
fn credentials_delete(id: String) -> Result<(), String> {
    development_db()?.delete_credential(&id).map_err(|e| e.to_string())
}
#[tauri::command]
fn inventory_list() -> Result<Vec<personal_desk::model::InventoryItem>, String> {
    development_db()?.inventory_items().map_err(|e| e.to_string())
}
#[tauri::command]
fn inventory_save(entry: personal_desk::model::InventoryItem) -> Result<(), String> {
    if entry.id.is_empty() || entry.name.trim().is_empty() { return Err("请填写物品名称".into()); }
    if entry.quantity < 0 { return Err("数量不能小于零".into()); }
    development_db()?.save_inventory_item(&entry).map_err(|e| e.to_string())
}
#[tauri::command]
fn inventory_delete(id: String) -> Result<(), String> {
    development_db()?.delete_inventory_item(&id).map_err(|e| e.to_string())
}
#[tauri::command]
fn websites_list() -> Result<Vec<personal_desk::model::Website>, String> {
    development_db()?.websites().map_err(|e| e.to_string())
}
#[tauri::command]
fn websites_save(entry: personal_desk::model::Website) -> Result<(), String> {
    let entry = personal_desk::websites::prepare(entry).map_err(|e| e.to_string())?;
    let mut db = development_db()?;
    let existing = db.tagged_records(personal_desk::tags::Scope::Websites).map_err(|e| e.to_string())?.into_iter().find(|r| r.id == entry.id).map(|r| r.tags).unwrap_or_default();
    let tags = if existing.join(" · ") == entry.category { existing } else {
        entry.category.split('·').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect()
    };
    db.save_website_classified(&entry, &tags).map_err(|e| e.to_string())
}
#[tauri::command]
fn websites_open(url: String) -> Result<(), String> {
    let site = personal_desk::websites::prepare(personal_desk::model::Website { url, ..Default::default() }).map_err(|e| e.to_string())?;
    open::that_detached(site.url).map_err(|e| format!("打开网站失败：{e}"))
}
#[tauri::command]
fn websites_delete(id: String) -> Result<(), String> {
    development_db()?.delete_website(&id).map_err(|e| e.to_string())
}
#[tauri::command]
fn tools_list() -> Result<Vec<personal_desk::launcher::AppTool>, String> {
    development_db()?.app_tools().map_err(|e| e.to_string())
}
#[tauri::command]
fn tools_save(entry: personal_desk::launcher::AppTool) -> Result<(), String> {
    development_db()?.save_app_tool(&entry).map_err(|e| e.to_string())
}
#[tauri::command]
fn tools_save_classified(entry: personal_desk::launcher::AppTool, tags: Vec<String>) -> Result<(), String> {
    development_db()?.save_app_tool_classified(&entry, &tags).map_err(|e| e.to_string())
}
#[tauri::command]
fn tools_delete(id: String) -> Result<(), String> {
    development_db()?.delete_app_tool(&id).map_err(|e| e.to_string())
}
#[tauri::command]
fn tools_launch(path: PathBuf) -> Result<(), String> {
    personal_desk::launcher::launch(&path).map_err(|e| e.to_string())
}
#[tauri::command]
fn writing_list() -> Result<Vec<personal_desk::library::Page>, String> {
    development_db()?.writing_pages().map_err(|e| e.to_string())
}
#[tauri::command]
fn writing_create(title: String, book_id: Option<String>) -> Result<personal_desk::library::Page, String> {
    development_db()?.create_writing(&title, book_id.as_deref(), None).map_err(|e| e.to_string())
}
#[tauri::command]
fn writing_save(mut page: personal_desk::library::Page) -> Result<personal_desk::library::Page, String> {
    let mut db = development_db()?;
    db.save_book_draft("tauri-migration", &page).map_err(|e| e.to_string())?;
    db.finish_writing_save("tauri-migration", &mut page).map_err(|e| e.to_string())?;
    Ok(page)
}
#[tauri::command]
fn books_list() -> Result<Vec<personal_desk::library::Book>, String> {
    development_db()?.books().map_err(|e| e.to_string())
}
#[tauri::command]
fn books_save(book: personal_desk::library::Book) -> Result<(), String> {
    development_db()?.save_book(&book).map_err(|e| e.to_string())
}
#[tauri::command]
fn contacts_list() -> Result<Vec<personal_desk::contacts::Contact>, String> {
    development_db()?.contact_overviews().map_err(|e| e.to_string())
}
#[tauri::command]
fn contacts_save(entry: personal_desk::contacts::Contact) -> Result<(), String> {
    let mut db = development_db()?;
    let tags = db.tagged_records(personal_desk::tags::Scope::Contacts).map_err(|e| e.to_string())?
        .into_iter().find(|r| r.id == entry.id).map(|r| r.tags).unwrap_or_default();
    if db.contact(&entry.id).map_err(|e| e.to_string())?.is_some() {
        db.update_contact_fields(&entry, &tags).map_err(|e| e.to_string())
    } else {
        db.save_contact_with_tags(&entry, &tags).map_err(|e| e.to_string())
    }
}
#[tauri::command]
fn contacts_delete(id: String) -> Result<(), String> {
    development_db()?.delete_contact(&id).map_err(|e| e.to_string())
}
#[tauri::command]
fn credential_favorites() -> Result<Vec<String>, String> {
    Ok(development_db()?.favorites().map_err(|e| e.to_string())?.into_iter()
        .filter_map(|(scope, id)| (scope == "credentials").then_some(id)).collect())
}
#[tauri::command]
fn credential_favorite(id: String, value: bool) -> Result<(), String> {
    development_db()?.set_favorite("credentials", &id, value).map_err(|e| e.to_string())
}
#[tauri::command]
fn credential_tags() -> Result<Vec<String>, String> {
    development_db()?.tags(personal_desk::tags::Scope::Credentials).map_err(|e| e.to_string())
}
#[tauri::command]
fn credential_tag_create(name: String) -> Result<(), String> {
    development_db()?.create_tag(personal_desk::tags::Scope::Credentials, &name).map_err(|e| e.to_string())
}
#[tauri::command]
fn credential_tag_rename(original: String, name: String) -> Result<(), String> {
    development_db()?.rename_tag(personal_desk::tags::Scope::Credentials, &original, &name).map_err(|e| e.to_string())
}
#[tauri::command]
fn credential_tag_delete(name: String) -> Result<(), String> {
    development_db()?.delete_tag(personal_desk::tags::Scope::Credentials, &name).map_err(|e| e.to_string())
}
#[tauri::command]
fn copy_value(value: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(value).map_err(|e| e.to_string())
}
#[tauri::command]
fn password_generate() -> Result<String, String> {
    personal_desk::passwords::generate().map_err(|e| e.to_string())
}
#[tauri::command]
fn credential_view_get() -> Result<String, String> {
    let value = development_db()?.settings().map_err(|e| e.to_string())?.get("view.credentials").cloned();
    Ok(if value.as_deref() == Some("list") { "list" } else { "grid" }.into())
}
#[tauri::command]
fn credential_view_set(mode: String) -> Result<(), String> {
    if mode != "grid" && mode != "list" { return Err("视图模式无效".into()); }
    development_db()?.set_setting("view.credentials", &mode).map_err(|e| e.to_string())
}
#[derive(serde::Serialize)]
struct ToolData {
    items: Vec<personal_desk::launcher::AppTool>,
    favorites: Vec<String>,
    tags: Vec<String>,
    assignments: Vec<personal_desk::tags::RecordTags>,
    view: String,
}
#[tauri::command]
fn tool_data() -> Result<ToolData, String> {
    let db = development_db()?;
    let favorites = db.favorites().map_err(|e| e.to_string())?.into_iter().filter_map(|(scope,id)| (scope == "tools").then_some(id)).collect();
    let view = db.settings().map_err(|e| e.to_string())?.get("view.tools").cloned().filter(|v| v == "list" || v == "grid").unwrap_or_else(|| "grid".into());
    Ok(ToolData { items:db.app_tools().map_err(|e| e.to_string())?, favorites, tags:db.tags(personal_desk::tags::Scope::Tools).map_err(|e| e.to_string())?, assignments:db.tagged_records(personal_desk::tags::Scope::Tools).map_err(|e| e.to_string())?, view })
}
#[tauri::command]
fn tool_favorite(id:String, value:bool) -> Result<(),String> {
    development_db()?.set_favorite("tools", &id, value).map_err(|e| e.to_string())
}
#[tauri::command]
fn tool_view(mode:String) -> Result<(),String> {
    if mode != "grid" && mode != "list" { return Err("视图模式无效".into()); }
    development_db()?.set_setting("view.tools", &mode).map_err(|e| e.to_string())
}
#[tauri::command]
fn tool_tags_set(id:String, tags:Vec<String>) -> Result<(),String> {
    development_db()?.set_record_tags(personal_desk::tags::Scope::Tools, &id, &tags).map_err(|e| e.to_string())
}
#[tauri::command]
async fn tool_pick() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new().set_title("选择快捷程序").add_filter("Windows 程序", &["exe"]).pick_file().await.map(|file| file.path().to_owned())
}
#[tauri::command]
fn tool_tag_create(name:String) -> Result<(),String> {
    development_db()?.create_tag(personal_desk::tags::Scope::Tools, &name).map_err(|e|e.to_string())
}
#[tauri::command]
fn tool_tag_rename(original:String,name:String) -> Result<(),String> {
    development_db()?.rename_tag(personal_desk::tags::Scope::Tools, &original, &name).map_err(|e|e.to_string())
}
#[tauri::command]
fn tool_tag_delete(name:String) -> Result<(),String> {
    development_db()?.delete_tag(personal_desk::tags::Scope::Tools, &name).map_err(|e|e.to_string())
}
#[tauri::command]
fn inventory_tags() -> Result<Vec<String>, String> {
    development_db()?.tags(personal_desk::tags::Scope::Inventory).map_err(|e| e.to_string())
}
#[tauri::command]
fn inventory_favorites() -> Result<Vec<String>, String> {
    Ok(development_db()?.favorites().map_err(|e| e.to_string())?.into_iter().filter_map(|(scope,id)| (scope == "inventory").then_some(id)).collect())
}
#[tauri::command]
fn inventory_favorite(id: String, value: bool) -> Result<(), String> {
    development_db()?.set_favorite("inventory", &id, value).map_err(|e| e.to_string())
}
#[tauri::command]
fn inventory_view_get() -> Result<String, String> {
    Ok(development_db()?.settings().map_err(|e| e.to_string())?.get("view.inventory").filter(|v| *v == "grid" || *v == "list").cloned().unwrap_or_else(|| "grid".into()))
}
#[tauri::command]
fn inventory_view_set(mode: String) -> Result<(), String> {
    if mode != "grid" && mode != "list" { return Err("视图模式无效".into()); }
    development_db()?.set_setting("view.inventory", &mode).map_err(|e| e.to_string())
}
#[tauri::command]
fn inventory_tag_create(name: String) -> Result<(), String> { development_db()?.create_tag(personal_desk::tags::Scope::Inventory, &name).map_err(|e| e.to_string()) }
#[tauri::command]
fn inventory_tag_rename(original: String, name: String) -> Result<(), String> { development_db()?.rename_tag(personal_desk::tags::Scope::Inventory, &original, &name).map_err(|e| e.to_string()) }
#[tauri::command]
fn inventory_tag_delete(name: String) -> Result<(), String> { development_db()?.delete_tag(personal_desk::tags::Scope::Inventory, &name).map_err(|e| e.to_string()) }
#[derive(serde::Serialize)]
struct WebsiteData { tags: Vec<String>, assignments: Vec<personal_desk::tags::RecordTags>, favorites: Vec<String>, view:String }
#[tauri::command]
fn website_data() -> Result<WebsiteData, String> {
    let db = development_db()?;
    Ok(WebsiteData { tags: db.tags(personal_desk::tags::Scope::Websites).map_err(|e|e.to_string())?, assignments: db.tagged_records(personal_desk::tags::Scope::Websites).map_err(|e|e.to_string())?, favorites: db.favorites().map_err(|e|e.to_string())?.into_iter().filter_map(|(scope,id)| (scope == "websites").then_some(id)).collect(), view:db.settings().map_err(|e|e.to_string())?.get("view.websites").filter(|v| *v=="grid" || *v=="list").cloned().unwrap_or_else(||"grid".into()) })
}
#[tauri::command]
fn website_view(mode:String)->Result<(),String>{if mode!="grid" && mode!="list" {return Err("视图模式无效".into())} development_db()?.set_setting("view.websites",&mode).map_err(|e|e.to_string())}
#[tauri::command]
fn website_favorite(id:String, value:bool) -> Result<(),String> { development_db()?.set_favorite("websites",&id,value).map_err(|e|e.to_string()) }
#[tauri::command]
fn website_tag_create(name:String) -> Result<(),String> { development_db()?.create_tag(personal_desk::tags::Scope::Websites,&name).map_err(|e|e.to_string()) }
#[tauri::command]
fn website_tag_rename(original:String,name:String) -> Result<(),String> { development_db()?.rename_tag(personal_desk::tags::Scope::Websites,&original,&name).map_err(|e|e.to_string()) }
#[tauri::command]
fn website_tag_delete(name:String) -> Result<(),String> { development_db()?.delete_tag(personal_desk::tags::Scope::Websites,&name).map_err(|e|e.to_string()) }
#[tauri::command]
fn website_save_classified(entry: personal_desk::model::Website, tags:Vec<String>) -> Result<(),String> {
 let entry = personal_desk::websites::prepare(entry).map_err(|e|e.to_string())?;
 development_db()?.save_website_classified(&entry,&tags).map_err(|e|e.to_string())
}
#[derive(serde::Serialize)]
struct ShortcutData { items:Vec<personal_desk::model::Shortcut>, tags:Vec<String>, assignments:Vec<personal_desk::tags::RecordTags>, favorites:Vec<String>, view:String, undo:bool }
#[tauri::command]
fn shortcut_data() -> Result<ShortcutData,String> {
 let db=development_db()?;
 Ok(ShortcutData { items:db.shortcuts().map_err(|e|e.to_string())?, tags:db.tags(personal_desk::tags::Scope::Shortcuts).map_err(|e|e.to_string())?, assignments:db.tagged_records(personal_desk::tags::Scope::Shortcuts).map_err(|e|e.to_string())?, favorites:db.favorites().map_err(|e|e.to_string())?.into_iter().filter_map(|(scope,id)| (scope=="shortcuts").then_some(id)).collect(), view:db.settings().map_err(|e|e.to_string())?.get("view.folders").filter(|v| *v=="grid" || *v=="list").cloned().unwrap_or_else(||"list".into()), undo:db.index_undo_available().map_err(|e|e.to_string())? })
}
#[tauri::command]
async fn shortcut_pick(is_dir:bool) -> Option<PathBuf> { let dialog=rfd::AsyncFileDialog::new().set_title("选择快捷路径"); if is_dir {dialog.pick_folder().await} else {dialog.pick_file().await}.map(|x|x.path().to_owned()) }
#[tauri::command]
fn shortcut_add(name:String,path:PathBuf,is_dir:bool) -> Result<(),String> { if name.trim().is_empty() || (is_dir && !path.is_dir()) || (!is_dir && !path.is_file()) {return Err("请输入名称并选择存在的路径".into())} development_db()?.save_shortcut(&name,&path,is_dir).map_err(|e|e.to_string()) }
#[tauri::command]
fn shortcut_open(path:PathBuf) -> Result<(),String> { personal_desk::files::open_location(&path).map_err(|e|e.to_string()) }
#[tauri::command]
fn shortcut_remove(id:String) -> Result<(),String> { development_db()?.remove_index("shortcuts",&id).map_err(|e|e.to_string()) }
#[tauri::command]
fn shortcut_undo() -> Result<bool,String> { development_db()?.undo_index_removal().map_err(|e|e.to_string()) }
#[tauri::command]
fn shortcut_favorite(id:String,value:bool) -> Result<(),String> { development_db()?.set_favorite("shortcuts",&id,value).map_err(|e|e.to_string()) }
#[tauri::command]
fn shortcut_tags(id:String,tags:Vec<String>) -> Result<(),String> { development_db()?.set_record_tags(personal_desk::tags::Scope::Shortcuts,&id,&tags).map_err(|e|e.to_string()) }
#[tauri::command]
fn shortcut_view(mode:String) -> Result<(),String> { if mode!="grid" && mode!="list" {return Err("视图模式无效".into())} development_db()?.set_setting("view.folders",&mode).map_err(|e|e.to_string()) }
#[tauri::command]
fn shortcut_tag_create(name:String) -> Result<(),String> { development_db()?.create_tag(personal_desk::tags::Scope::Shortcuts,&name).map_err(|e|e.to_string()) }
#[tauri::command]
fn shortcut_tag_rename(original:String,name:String) -> Result<(),String> { development_db()?.rename_tag(personal_desk::tags::Scope::Shortcuts,&original,&name).map_err(|e|e.to_string()) }
#[tauri::command]
fn shortcut_tag_delete(name:String) -> Result<(),String> { development_db()?.delete_tag(personal_desk::tags::Scope::Shortcuts,&name).map_err(|e|e.to_string()) }
#[tauri::command]
fn essay_today() -> Result<personal_desk::essays::Assignment,String> { development_db()?.ensure_essay_day(chrono::Local::now().date_naive()).map_err(|e|e.to_string()) }
#[tauri::command]
fn essay_reroll(day:String,expected:String) -> Result<personal_desk::essays::Assignment,String> { development_db()?.reroll_essay(&day,&expected).map_err(|e|e.to_string()) }
#[tauri::command]
fn essay_start(day:String) -> Result<personal_desk::library::Page,String> { development_db()?.start_essay(&day).map_err(|e|e.to_string()) }
#[tauri::command]
fn essay_history(query:String) -> Result<Vec<personal_desk::essays::Summary>,String> { development_db()?.essay_summaries(&query).map_err(|e|e.to_string()) }
#[tauri::command]
fn essay_page(id:String) -> Result<Option<personal_desk::library::Page>,String> { development_db()?.writing_page(&id).map_err(|e|e.to_string()) }
#[tauri::command]
fn writing_delete(id:String, page:bool) -> Result<(),String> { development_db()?.set_library_deleted(&id,page,true).map_err(|e|e.to_string()) }
#[derive(serde::Serialize)]
struct ContactData { items:Vec<personal_desk::contacts::Contact>, tags:Vec<String>, assignments:Vec<personal_desk::tags::RecordTags>, favorites:Vec<String>, view:String, extra:bool }
#[tauri::command]
fn contact_data() -> Result<ContactData,String> { let db=development_db()?; Ok(ContactData {items:db.contact_overviews().map_err(|e|e.to_string())?, tags:db.tags(personal_desk::tags::Scope::Contacts).map_err(|e|e.to_string())?, assignments:db.tagged_records(personal_desk::tags::Scope::Contacts).map_err(|e|e.to_string())?, favorites:db.favorites().map_err(|e|e.to_string())?.into_iter().filter_map(|(scope,id)| (scope=="contacts").then_some(id)).collect(), view:db.settings().map_err(|e|e.to_string())?.get("view.contacts").filter(|v| *v=="grid" || *v=="list").cloned().unwrap_or_else(||"grid".into()), extra:db.settings().map_err(|e|e.to_string())?.get("view.contacts.extra").is_some_and(|v|v=="true")}) }
#[tauri::command]
fn contact_save_classified(entry:personal_desk::contacts::Contact,tags:Vec<String>) -> Result<(),String> { let mut db=development_db()?; if db.contact(&entry.id).map_err(|e|e.to_string())?.is_some() {db.update_contact_details(&entry,&tags)} else {db.save_contact_with_tags(&entry,&tags)}.map_err(|e|e.to_string()) }
#[tauri::command]
fn contact_view(mode:String)->Result<(),String>{if mode!="grid"&&mode!="list"{return Err("视图模式无效".into())}development_db()?.set_setting("view.contacts",&mode).map_err(|e|e.to_string())}
#[tauri::command]
fn contact_extra(value:bool)->Result<(),String>{development_db()?.set_setting("view.contacts.extra",if value {"true"}else{"false"}).map_err(|e|e.to_string())}
#[tauri::command]
fn contact_favorite(id:String,value:bool)->Result<(),String>{development_db()?.set_favorite("contacts",&id,value).map_err(|e|e.to_string())}
#[tauri::command]
fn contact_tag_create(name:String)->Result<(),String>{development_db()?.create_tag(personal_desk::tags::Scope::Contacts,&name).map_err(|e|e.to_string())}
#[tauri::command]
fn contact_tag_rename(original:String,name:String)->Result<(),String>{development_db()?.rename_tag(personal_desk::tags::Scope::Contacts,&original,&name).map_err(|e|e.to_string())}
#[tauri::command]
fn contact_tag_delete(name:String)->Result<(),String>{development_db()?.delete_tag(personal_desk::tags::Scope::Contacts,&name).map_err(|e|e.to_string())}
#[derive(serde::Serialize)]
struct MigrationSettings { version: &'static str, data_directory: PathBuf, releases: serde_json::Value }
#[tauri::command]
fn settings_info() -> Result<MigrationSettings,String> {
 let data_directory=data_directory()?;
 Ok(MigrationSettings {version:personal_desk::updater::VERSION,data_directory,releases:serde_json::from_str(include_str!("../../../assets/release_history.json")).map_err(|e|e.to_string())?})
}
#[derive(serde::Serialize)]
struct UpdateInfo { version: Option<String>, notes: Option<String> }
#[tauri::command]
fn settings_check_update() -> Result<UpdateInfo,String> {
 let result=personal_desk::updater::check(&personal_desk::updater::default_source(), personal_desk::updater::VERSION)?;
 Ok(match result { Some(release)=>UpdateInfo{version:Some(release.manifest.version),notes:Some(release.manifest.notes)},None=>UpdateInfo{version:None,notes:None} })
}
#[tauri::command]
fn settings_install_update() -> Result<(),String> {
 let release=personal_desk::updater::check(&personal_desk::updater::default_source(),personal_desk::updater::VERSION)?
  .ok_or_else(||"当前已是最新版本".to_string())?;
 personal_desk::updater::launch(&release,&data_directory()?)
}
#[tauri::command]
fn settings_autostart_get() -> Result<bool,String> {
 Ok(personal_desk::autostart::registered_command()?.is_some())
}
#[tauri::command]
fn settings_autostart_set(enabled:bool) -> Result<(),String> {
 let executable=std::env::current_exe().map_err(|e|e.to_string())?;
 personal_desk::autostart::set(enabled,&executable)
}
#[tauri::command]
fn vault_mysql_info() -> Result<personal_desk::vault_config::VaultMysqlSummary,String> {
 personal_desk::vault_config::VaultMysqlConfig::summary().map_err(|e|e.to_string())
}
#[tauri::command]
fn vault_mysql_save(host:String,port:u16,user:String,password:String,database:String) -> Result<(),String> {
 personal_desk::vault_config::VaultMysqlConfig::new(host,port,user,password,database)
  .and_then(|config|config.save()).map_err(|e|e.to_string())
}
#[tauri::command]
fn vault_mysql_test(host:String,port:u16,user:String,password:String,database:String) -> Result<(),String> {
 personal_desk::vault_config::VaultMysqlConfig::new(host,port,user,password,database)
  .and_then(|config|config.test_connection()).map_err(|e|e.to_string())
}
#[tauri::command]
fn vault_mysql_test_saved() -> Result<(),String> {
 let config = personal_desk::vault_config::VaultMysqlConfig::load()
  .map_err(|e|e.to_string())?
  .ok_or_else(||"尚未保存直连配置".to_string())?;
 config.test_connection().map_err(|e|e.to_string())
}
#[tauri::command]
fn vault_mysql_clear() -> Result<(),String> {
 personal_desk::vault_config::VaultMysqlConfig::clear().map_err(|e|e.to_string())
}
#[tauri::command]
fn vault_sync_now() -> Result<bool,String> {
 development_db()?.sync_remote_now().map_err(|e|e.to_string())
}
#[derive(serde::Serialize)]
struct ExportResult { path:PathBuf, warnings:usize }
#[tauri::command]
async fn export_test_data(scope:String) -> Result<Option<ExportResult>,String> {
 let scope=match scope.as_str() { "credentials"=>personal_desk::export::Scope::Credentials, "inventory"=>personal_desk::export::Scope::Inventory, "folders"=>personal_desk::export::Scope::Folders, "tools"=>personal_desk::export::Scope::Tools, "all"=>personal_desk::export::Scope::All, "essays"=>personal_desk::export::Scope::Essays, "documents"=>personal_desk::export::Scope::Documents, "contacts"=>personal_desk::export::Scope::Contacts, "websites"=>personal_desk::export::Scope::Websites, _=>return Err("导出范围无效".into()) };
 let data=data_directory()?;
 let Some(path)=rfd::AsyncFileDialog::new().set_title("导出测试数据").set_file_name("个人办公台数据.zip").add_filter("ZIP 压缩包", &["zip"]).save_file().await else {return Ok(None)};
 let mut destination=path.path().to_owned();
 if destination.extension().is_none_or(|ext| !ext.eq_ignore_ascii_case("zip")) {
  destination.set_extension("zip");
  if destination.exists() {return Err("补全扩展名后的文件已存在，请在保存窗口直接选择该文件".into())}
 }
 tauri::async_runtime::spawn_blocking(move || personal_desk::export::write(&data,&destination,scope).map(|r|Some(ExportResult{path:r.path,warnings:r.warnings})).map_err(|e|e.to_string())).await.map_err(|e|e.to_string())?
}
#[tauri::command]
async fn contact_import_photo(id:String) -> Result<bool,String> {
 let Some(file)=rfd::AsyncFileDialog::new().set_title("选择照片").add_filter("照片",&["png","jpg","jpeg","webp","bmp"]).pick_file().await else {return Ok(false)};
 let path=file.path().to_owned();
 tauri::async_runtime::spawn_blocking(move || {let photo=personal_desk::contacts::import_photo(&path).map_err(|e|e.to_string())?; development_db()?.set_contact_photo(&id,&photo).map_err(|e|e.to_string())?; Ok(true)}).await.map_err(|e|e.to_string())?
}
#[tauri::command]
async fn contact_import_chat(id:String) -> Result<Option<bool>,String> {
 let Some(file)=rfd::AsyncFileDialog::new().set_title("选择聊天记录").add_filter("聊天记录",&["txt","md","markdown","json","xlsx","xls"]).pick_file().await else {return Ok(None)};
 let path=file.path().to_owned();
 tauri::async_runtime::spawn_blocking(move || {let chat=personal_desk::contacts::import_chat(&path).map_err(|e|e.to_string())?;development_db()?.append_contact_chat(&id,chat).map(Some).map_err(|e|e.to_string())}).await.map_err(|e|e.to_string())?
}
#[derive(serde::Serialize)]
struct ChatImportResult { added:usize, skipped:usize }
#[tauri::command]
async fn contact_import_chats(id:String) -> Result<Option<ChatImportResult>,String> {
 let Some(files)=rfd::AsyncFileDialog::new().set_title("选择聊天记录（可多选）").add_filter("聊天记录",&["txt","md","markdown","json","xlsx","xls"]).pick_files().await else {return Ok(None)};
 let paths:Vec<_>=files.into_iter().map(|file|file.path().to_owned()).collect();
 tauri::async_runtime::spawn_blocking(move || {
  let mut chats=Vec::new();let mut bytes=0;
  for path in paths {
   let chat=personal_desk::contacts::import_chat(&path).map_err(|e|format!("{}：{e}",path.file_name().unwrap_or_default().to_string_lossy()))?;
   bytes+=chat.original.len();
   if bytes>personal_desk::contacts::MAX_CONTACT_BYTES{return Err("本批原文件总量超过 32 MiB，请分批导入".into())}
   chats.push(chat);
  }
  let count=chats.len();let added=development_db()?.append_contact_chats(&id,chats).map_err(|e|e.to_string())?;
  Ok(Some(ChatImportResult{added,skipped:count-added}))
 }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
fn contact_photo(id:String)->Result<Option<String>,String>{
 use base64::Engine;
 let contact=development_db()?.contact(&id).map_err(|e|e.to_string())?.ok_or_else(||"联系人已不存在".to_string())?;
 if contact.photo.is_empty(){return Ok(None)}
 if contact.photo.len()>personal_desk::contacts::MAX_FILE_BYTES{return Err("联系人照片超过大小限制".into())}
 let mime=match image::guess_format(&contact.photo).map_err(|e|e.to_string())?{image::ImageFormat::Png=>"image/png",image::ImageFormat::Jpeg=>"image/jpeg",image::ImageFormat::WebP=>"image/webp",image::ImageFormat::Bmp=>"image/bmp",_=>return Err("不支持的照片格式".into())};
 Ok(Some(format!("data:{mime};base64,{}",base64::engine::general_purpose::STANDARD.encode(contact.photo))))
}
#[tauri::command]
fn contact_remove_photo(id:String)->Result<(),String>{development_db()?.set_contact_photo(&id,&[]).map_err(|e|e.to_string())}
#[tauri::command]
fn contact_remove_chat(id:String,chat_id:String)->Result<(),String>{development_db()?.remove_contact_chat(&id,&chat_id).map_err(|e|e.to_string())}
#[tauri::command]
async fn contact_export_chat(id:String,chat_id:String)->Result<Option<PathBuf>,String>{
 let contact=development_db()?.contact(&id).map_err(|e|e.to_string())?.ok_or_else(||"联系人已不存在".to_string())?;
 let chat=contact.chats.into_iter().find(|c|c.id==chat_id).ok_or_else(||"聊天记录已不存在".to_string())?;
 let filename=PathBuf::from(&chat.title).file_name().map(|n|n.to_string_lossy().into_owned()).filter(|n|!n.is_empty()).unwrap_or_else(||"聊天记录.txt".into());
 let Some(file)=rfd::AsyncFileDialog::new().set_title("导出聊天记录原件").set_file_name(filename).save_file().await else{return Ok(None)};
 let path=file.path().to_owned();
 tauri::async_runtime::spawn_blocking(move || {personal_desk::contacts::export_chat(&chat,&path).map_err(|e|e.to_string())?;Ok(Some(path))}).await.map_err(|e|e.to_string())?
}
#[tauri::command]
fn writing_draft(page:personal_desk::library::Page) -> Result<(),String> { development_db()?.save_book_draft("tauri-migration",&page).map_err(|e|e.to_string()) }
#[tauri::command]
fn writing_recover_drafts() -> Result<(),String> { development_db()?.recover_writing_drafts().map_err(|e|e.to_string()) }
#[tauri::command]
fn theme_get() -> Result<personal_desk::theme::ThemeSettings,String> { let db=development_db()?;Ok(personal_desk::theme::ThemeSettings::load(&db.settings().map_err(|e|e.to_string())?)) }
#[tauri::command]
fn theme_save(mut theme:personal_desk::theme::ThemeSettings) -> Result<personal_desk::theme::ThemeSettings,String> {
 theme.validate().map_err(|e|e.to_string())?;
 if let Some(path)=theme.background.clone().filter(|p|std::path::Path::new(p).is_absolute()) {
  let data=data_directory()?;
  theme.background=Some(personal_desk::theme::import_image(&data,std::path::Path::new(&path)).map_err(|e|e.to_string())?);
 }
 let json=serde_json::to_string(&theme).map_err(|e|e.to_string())?;development_db()?.set_setting("appearance.theme",&json).map_err(|e|e.to_string())?;Ok(theme)
}
#[tauri::command]
async fn theme_pick_image() -> Option<PathBuf> { rfd::AsyncFileDialog::new().set_title("选择主题背景图片").add_filter("图片",&["png","jpg","jpeg","webp","bmp"]).pick_file().await.map(|f|f.path().to_owned()) }
#[tauri::command]
fn theme_image(value:String) -> Result<String,String> {
 use std::io::Read;
 use base64::Engine;
 let data=data_directory()?;
 let path=personal_desk::theme::background_path(&data,&value).map_err(|e|e.to_string())?;
 personal_desk::theme::load_image(&path).map_err(|e|e.to_string())?;
 let mut bytes=Vec::new();std::fs::File::open(path).map_err(|e|e.to_string())?.take(20*1024*1024+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
 if bytes.len()>20*1024*1024 {return Err("图片超过 20 MiB".into())}
 let mime=match image::guess_format(&bytes).map_err(|e|e.to_string())? {image::ImageFormat::Png=>"image/png",image::ImageFormat::Jpeg=>"image/jpeg",image::ImageFormat::WebP=>"image/webp",image::ImageFormat::Bmp=>"image/bmp",_=>return Err("不支持的图片格式".into())};
 Ok(format!("data:{mime};base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)))
}
#[tauri::command]
fn writing_move_book(id:String,parent:Option<String>)->Result<(),String>{development_db()?.move_book(&id,parent.as_deref()).map_err(|e|e.to_string())}
#[tauri::command]
fn writing_move_page(id:String,target:Option<String>)->Result<(),String>{development_db()?.move_writing_page(&id,target.as_deref()).map_err(|e|e.to_string())}
#[tauri::command]
async fn writing_image_import(document_id:String)->Result<Option<String>,String>{
 let data=writing_assets_data_directory()?;
 let Some(file)=rfd::AsyncFileDialog::new().set_title("插入图片").add_filter("图片",&["png","jpg","jpeg","webp","bmp"]).pick_file().await else{return Ok(None)};
 let source=file.path().to_owned();
 tauri::async_runtime::spawn_blocking(move||personal_desk::writing_images::import_document(&data,&document_id,&source).map(Some).map_err(|e|e.to_string())).await.map_err(|e|e.to_string())?
}
#[tauri::command]
async fn writing_image_paste(document_id:String,bytes:Vec<u8>)->Result<String,String>{
 if bytes.len()>personal_desk::contacts::MAX_FILE_BYTES{return Err("图片不能超过 8 MiB".into())}
 let data=writing_assets_data_directory()?;
 tauri::async_runtime::spawn_blocking(move||personal_desk::writing_images::import_document_bytes(&data,&document_id,&bytes).map_err(|e|e.to_string())).await.map_err(|e|e.to_string())?
}
#[tauri::command]
async fn writing_image_read(reference:String)->Result<String,String>{
 use base64::Engine;
 let data=writing_assets_data_directory()?;
 tauri::async_runtime::spawn_blocking(move||{let (bytes,mime)=personal_desk::writing_images::read(&data,&reference).map_err(|e|e.to_string())?;Ok(format!("data:{mime};base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)))}).await.map_err(|e|e.to_string())?
}
#[tauri::command]
fn writing_daily_log(book_id:Option<String>) -> Result<personal_desk::library::Page,String> {
 let day=chrono::Local::now().format("%Y-%m-%d").to_string();
 development_db()?.create_writing(&day,book_id.as_deref(),Some(&day)).map_err(|e|e.to_string())
}
#[tauri::command]
fn writing_tags()->Result<Vec<String>,String>{development_db()?.tags(personal_desk::tags::Scope::BookPages).map_err(|e|e.to_string())}
#[tauri::command]
fn writing_tag_create(name:String)->Result<(),String>{development_db()?.create_tag(personal_desk::tags::Scope::BookPages,&name).map_err(|e|e.to_string())}
#[tauri::command]
fn writing_tag_rename(original:String,name:String)->Result<(),String>{development_db()?.rename_tag(personal_desk::tags::Scope::BookPages,&original,&name).map_err(|e|e.to_string())}
#[tauri::command]
fn writing_tag_delete(name:String)->Result<(),String>{development_db()?.delete_tag(personal_desk::tags::Scope::BookPages,&name).map_err(|e|e.to_string())}
#[tauri::command]
fn writing_save_copy(mut page:personal_desk::library::Page)->Result<personal_desk::library::Page,String>{
 page.id=uuid::Uuid::new_v4().to_string();page.revision=0;page.title=format!("{}（副本）",page.title);development_db()?.save_book_page(&mut page).map_err(|e|e.to_string())?;Ok(page)
}
#[tauri::command]
async fn writing_export_markdown(page:personal_desk::library::Page)->Result<Option<PathBuf>,String>{
 let data=writing_assets_data_directory()?;
 let filename=format!("{}.md",page.title.replace(['\\','/',':','*','?','\"','<','>','|'],"_"));
 let Some(file)=rfd::AsyncFileDialog::new().set_title("导出 Markdown 文档").set_file_name(filename).add_filter("Markdown",&["md"]).save_file().await else{return Ok(None)};
 let mut destination=file.path().to_owned();if destination.extension().is_none_or(|e|!e.eq_ignore_ascii_case("md")){destination.set_extension("md");if destination.exists(){return Err("补全扩展名后的文件已存在，请直接选择该文件".into())}}
 tauri::async_runtime::spawn_blocking(move||{personal_desk::writing_images::export_markdown(&data,&page,&destination).map_err(|e|e.to_string())?;Ok(Some(destination))}).await.map_err(|e|e.to_string())?
}
#[tauri::command]
fn writing_publish_blog(page:personal_desk::library::Page)->Result<String,String>{
 if page.title.trim().is_empty()||page.title.chars().count()>200{return Err("博客标题不能为空，且不能超过 200 个字符".into())}
 if page.body.len()>200_000{return Err("博客正文超过 200 KB，请拆分文档后再发布".into())}
 if page.tags.len()>personal_desk::blog_publish::MAX_TAGS||page.tags.iter().any(|tag|tag.trim().chars().count()>50){return Err("博客标签最多 8 个，每个标签不能超过 50 个字符".into())}
 let mut db=development_db()?;
 let key_path=db.settings().map_err(|e|e.to_string())?.get("blog.ssh_key_path").map(PathBuf::from).filter(|path|path.is_file())
  .or_else(personal_desk::blog_publish::default_ssh_key)
  .ok_or_else(||"没有找到博客 SSH 私钥".to_string())?;
 db.set_setting("blog.ssh_key_path",&key_path.to_string_lossy()).map_err(|e|e.to_string())?;
 let post_id=match db.blog_post_id(&page.id).map_err(|e|e.to_string())?{
  Some(id)=>id,
  None=>{let id=uuid::Uuid::new_v4().simple().to_string()[..16].to_owned();db.link_blog_post(&page.id,&id).map_err(|e|e.to_string())?;id}
 };
 drop(db);
 let now=chrono::Local::now();
 let payload=personal_desk::blog_publish::BlogPostPayload{page_id:page.id,post_id,title:page.title,content:page.body,tags:page.tags,tenant_id:"0000000000000000".into(),published_at:now.timestamp_millis(),published_date:now.format("%Y-%m-%d").to_string()};
 personal_desk::blog_publish::publish_over_ssh(&key_path,&payload)
}
#[tauri::command]
fn theme_set_zoom(zoom:f32)->Result<personal_desk::theme::ThemeSettings,String>{
 let mut db=development_db()?;let mut theme=personal_desk::theme::ThemeSettings::load(&db.settings().map_err(|e|e.to_string())?);theme.zoom=zoom;theme.validate().map_err(|e|e.to_string())?;db.set_setting("appearance.theme",&serde_json::to_string(&theme).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;Ok(theme)
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data = data_directory();
    let _instance = data
        .as_ref()
        .ok()
        .and_then(|path| personal_desk::install::Instance::acquire(path));
    if data.is_ok() && _instance.is_none() {
        return;
    }
    initialize_database();
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(2));
        if let Ok(mut database) = development_db() {
            let _ = database.sync_remote_if_due();
        }
    });
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![credentials_list, credentials_save, credentials_delete, inventory_list, inventory_save, inventory_delete, inventory_tags, inventory_favorites, inventory_favorite, inventory_view_get, inventory_view_set, inventory_tag_create, inventory_tag_rename, inventory_tag_delete, websites_list, websites_save, websites_delete, websites_open, website_data, website_view, website_favorite, website_tag_create, website_tag_rename, website_tag_delete, website_save_classified, shortcut_data, shortcut_pick, shortcut_add, shortcut_open, shortcut_remove, shortcut_undo, shortcut_favorite, shortcut_tags, shortcut_view, shortcut_tag_create, shortcut_tag_rename, shortcut_tag_delete, essay_today, essay_reroll, essay_start, essay_history, essay_page, tools_list, tools_save, tools_save_classified, tools_delete, tools_launch, writing_list, writing_create, writing_daily_log, writing_tags, writing_tag_create, writing_tag_rename, writing_tag_delete, writing_save_copy, writing_save, writing_delete, writing_draft, writing_recover_drafts, writing_export_markdown, writing_publish_blog, writing_image_import, writing_image_paste, writing_image_read, writing_move_book, writing_move_page, books_list, books_save, contacts_list, contacts_save, contacts_delete, contact_data, contact_save_classified, contact_view, contact_extra, contact_favorite, contact_tag_create, contact_tag_rename, contact_tag_delete, contact_import_photo, contact_import_chat, contact_import_chats, contact_photo, contact_remove_photo, contact_remove_chat, contact_export_chat, settings_info, settings_check_update, settings_install_update, settings_autostart_get, settings_autostart_set, vault_mysql_info, vault_mysql_save, vault_mysql_test, vault_mysql_test_saved, vault_mysql_clear, vault_sync_now, export_test_data, theme_get, theme_save, theme_set_zoom, theme_pick_image, theme_image, credential_favorites, credential_favorite, credential_tags, credential_tag_create, credential_tag_rename, credential_tag_delete, copy_value, credential_view_get, credential_view_set, password_generate, tool_data, tool_favorite, tool_view, tool_tags_set, tool_pick, tool_tag_create, tool_tag_rename, tool_tag_delete])
        .run(tauri::generate_context!())
        .expect("error while running migration development build");
}
