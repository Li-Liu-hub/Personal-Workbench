//! 文档写作、图书、图片与博客/文档库发布相关命令。

use std::path::PathBuf;

use crate::{
    blog_publish::{self, BlogPostPayload},
    db::writing_reads::{self, WritingDirectory, WritingRecent, WritingResults, WritingScope},
    library::{Book, Page},
    tags::Scope,
    writing_images,
};
use base64::Engine;
use chrono::Local;

use crate::state::{data_directory, development_db};

/// 草稿归属标识。
///
/// `"tauri-migration"` 是 Tauri 迁移时期写入数据库的历史草稿归属值，
/// 旧草稿仍按该值关联与恢复，必须保持原样，不能随目录改名（tauri-migration → desktop）。
const DRAFT_OWNER: &str = "tauri-migration";

/// 读取全部文档页面。
#[tauri::command]
pub fn writing_list() -> Result<Vec<Page>, String> {
    development_db()?
        .writing_pages()
        .map_err(|error| error.to_string())
}

/// 读取目录条目和图书分组，正文在打开文档时读取。
#[tauri::command]
pub fn writing_directory() -> Result<WritingDirectory, String> {
    development_db()?
        .writing_directory()
        .map_err(|error| error.to_string())
}

/// 读取最近编辑的文档与随笔，每类最多四篇。
#[tauri::command]
pub fn writing_recent() -> Result<Vec<WritingRecent>, String> {
    development_db()?
        .writing_recent(4)
        .map_err(|error| error.to_string())
}

/// 按文档 ID 读取一篇文档。已删除或隐藏的页面返回 null。
#[tauri::command]
pub fn writing_page(id: String) -> Result<Option<Page>, String> {
    development_db()?
        .writing_page(&id)
        .map_err(|error| error.to_string())
}

/// 搜索目录中的标题、标签与正文，返回匹配页面的 ID。
#[tauri::command]
pub async fn writing_matches(query: String) -> Result<Vec<String>, String> {
    let data = data_directory()?;
    tauri::async_runtime::spawn_blocking(move || writing_reads::matching_ids(&data, &query))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

/// 全局写作搜索，返回相关度最高的 24 条摘要和匹配总数。
#[tauri::command]
pub async fn writing_search(
    query: String,
    scope: Option<WritingScope>,
) -> Result<WritingResults, String> {
    let data = data_directory()?;
    tauri::async_runtime::spawn_blocking(move || writing_reads::search(&data, &query, scope))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

/// 新建文档页面，可指定所属图书。
#[tauri::command]
pub fn writing_create(title: String, book_id: Option<String>) -> Result<Page, String> {
    development_db()?
        .create_writing(&title, book_id.as_deref(), None)
        .map_err(|error| error.to_string())
}

/// 保存文档内容，返回更新后的页面。
#[tauri::command]
pub fn writing_save(mut page: Page) -> Result<Page, String> {
    let mut database = development_db()?;
    database
        .save_book_draft(DRAFT_OWNER, &page)
        .map_err(|error| error.to_string())?;
    database
        .finish_writing_save(DRAFT_OWNER, &mut page)
        .map_err(|error| error.to_string())?;
    Ok(page)
}

/// 软删除指定文档或图书（page=true 删除文档，false 删除整本图书）。
#[tauri::command]
pub fn writing_delete(id: String, page: bool) -> Result<(), String> {
    development_db()?
        .set_library_deleted(&id, page, true)
        .map_err(|error| error.to_string())
}

/// 保存编辑中的临时草稿。
#[tauri::command]
pub fn writing_draft(page: Page) -> Result<(), String> {
    development_db()?
        .save_book_draft(DRAFT_OWNER, &page)
        .map_err(|error| error.to_string())
}

/// 恢复上次未保存的草稿。
#[tauri::command]
pub fn writing_recover_drafts() -> Result<(), String> {
    development_db()?
        .recover_writing_drafts()
        .map_err(|error| error.to_string())
}

/// 读取全部图书。
#[tauri::command]
pub fn books_list() -> Result<Vec<Book>, String> {
    development_db()?.books().map_err(|error| error.to_string())
}

/// 新增或更新图书。
#[tauri::command]
pub fn books_save(book: Book) -> Result<(), String> {
    development_db()?
        .save_book(&book)
        .map_err(|error| error.to_string())
}

/// 获取或创建今天的日志页，可指定所属图书。
#[tauri::command]
pub fn writing_daily_log(book_id: Option<String>) -> Result<Page, String> {
    let day = Local::now().format("%Y-%m-%d").to_string();
    development_db()?
        .create_writing(&day, book_id.as_deref(), Some(&day))
        .map_err(|error| error.to_string())
}

/// 读取文档的全部分类标签。
#[tauri::command]
pub fn writing_tags() -> Result<Vec<String>, String> {
    development_db()?
        .tags(Scope::BookPages)
        .map_err(|error| error.to_string())
}

/// 新建文档分类标签。
#[tauri::command]
pub fn writing_tag_create(name: String) -> Result<(), String> {
    development_db()?
        .create_tag(Scope::BookPages, &name)
        .map_err(|error| error.to_string())
}

/// 重命名文档分类标签。
#[tauri::command]
pub fn writing_tag_rename(original: String, name: String) -> Result<(), String> {
    development_db()?
        .rename_tag(Scope::BookPages, &original, &name)
        .map_err(|error| error.to_string())
}

/// 删除文档分类标签。
#[tauri::command]
pub fn writing_tag_delete(name: String) -> Result<(), String> {
    development_db()?
        .delete_tag(Scope::BookPages, &name)
        .map_err(|error| error.to_string())
}

/// 将文档另存为一个副本，返回新页面。
#[tauri::command]
pub fn writing_save_copy(mut page: Page) -> Result<Page, String> {
    page.id = uuid::Uuid::new_v4().to_string();
    page.revision = 0;
    page.title = format!("{}（副本）", page.title);
    development_db()?
        .save_book_page(&mut page)
        .map_err(|error| error.to_string())?;
    Ok(page)
}

/// 将文档及其受管图片导出为 Markdown 文件；取消时返回 null。
#[tauri::command]
pub async fn writing_export_markdown(page: Page) -> Result<Option<PathBuf>, String> {
    let data = data_directory()?;
    let filename = format!(
        "{}.md",
        page.title
            .replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_")
    );
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("导出 Markdown 文档")
        .set_file_name(filename)
        .add_filter("Markdown", &["md"])
        .save_file()
        .await
    else {
        return Ok(None);
    };
    let mut destination = file.path().to_owned();
    if destination
        .extension()
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("md"))
    {
        destination.set_extension("md");
        if destination.exists() {
            return Err("补全扩展名后的文件已存在，请直接选择该文件".into());
        }
    }
    tauri::async_runtime::spawn_blocking(move || {
        writing_images::export_markdown(&data, &page, &destination)
            .map_err(|error| error.to_string())?;
        Ok(Some(destination))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 通过 SSH 将文档发布或更新到博客，返回博客文章 ID。
#[tauri::command]
pub fn writing_publish_blog(page: Page) -> Result<String, String> {
    blog_publish::validate_post(&page.title, &page.body, &page.tags)
        .map_err(|error| error.to_string())?;
    let mut database = development_db()?;
    let key_path = database
        .settings()
        .map_err(|error| error.to_string())?
        .get("blog.ssh_key_path")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(blog_publish::default_ssh_key)
        .ok_or_else(|| "没有找到博客 SSH 私钥".to_string())?;
    database
        .set_setting("blog.ssh_key_path", &key_path.to_string_lossy())
        .map_err(|error| error.to_string())?;
    let post_id = match database
        .blog_post_id(&page.id)
        .map_err(|error| error.to_string())?
    {
        Some(id) => id,
        None => {
            let id = uuid::Uuid::new_v4().simple().to_string()[..16].to_owned();
            database
                .link_blog_post(&page.id, &id)
                .map_err(|error| error.to_string())?;
            id
        }
    };
    drop(database);
    let now = Local::now();
    let payload = BlogPostPayload {
        page_id: page.id,
        post_id,
        title: page.title,
        content: page.body,
        tags: page.tags,
        tenant_id: "0000000000000000".into(),
        published_at: now.timestamp_millis(),
        published_date: now.format("%Y-%m-%d").to_string(),
    };
    blog_publish::publish_over_ssh(&key_path, &payload)
}

/// 选择图片并导入到指定文档，返回图片引用；取消时返回 null。
#[tauri::command]
pub async fn writing_image_import(document_id: String) -> Result<Option<String>, String> {
    let data = data_directory()?;
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("插入图片")
        .add_filter("图片", &["png", "jpg", "jpeg", "webp", "bmp"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let source = file.path().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        writing_images::import_document(&data, &document_id, &source)
            .map(Some)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 将剪贴板图片字节导入到指定文档，返回图片引用。
#[tauri::command]
pub async fn writing_image_paste(document_id: String, bytes: Vec<u8>) -> Result<String, String> {
    let data = data_directory()?;
    tauri::async_runtime::spawn_blocking(move || {
        writing_images::import_document_bytes(&data, &document_id, &bytes)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 读取文档图片引用，返回 data URL。
#[tauri::command]
pub async fn writing_image_read(reference: String) -> Result<String, String> {
    let data = data_directory()?;
    tauri::async_runtime::spawn_blocking(move || {
        let (bytes, mime) =
            writing_images::read(&data, &reference).map_err(|error| error.to_string())?;
        Ok(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 将图书移动到另一个父图书；parent 为 null 时移到根级。
#[tauri::command]
pub fn writing_move_book(id: String, parent: Option<String>) -> Result<(), String> {
    development_db()?
        .move_book(&id, parent.as_deref())
        .map_err(|error| error.to_string())
}

/// 将文档移动到另一本图书；target 为 null 时移到文档根目录。
#[tauri::command]
pub fn writing_move_page(id: String, target: Option<String>) -> Result<(), String> {
    development_db()?
        .move_writing_page(&id, target.as_deref())
        .map_err(|error| error.to_string())
}
