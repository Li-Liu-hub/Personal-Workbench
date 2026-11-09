use super::*;
use crate::library::{Page, WritingSummary};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

mod book_navigation;
mod floating;
mod hierarchy;

// The same insertion palette is used by the main and floating writers.
fn writing_format_toolbar(ui: &mut egui::Ui) -> Option<&'static str> {
    let mut insert = None;
    ui.horizontal_wrapped(|ui| {
        ui.menu_button("标题", |ui| {
            for (label, syntax) in [
                ("一级标题", "# "),
                ("二级标题", "## "),
                ("三级标题", "### "),
            ] {
                if ui.animated_button(label).clicked() {
                    insert = Some(syntax);
                    ui.close();
                }
            }
        });
        for (label, syntax, help) in [
            ("加粗", "**文字**", "插入加粗文字"),
            ("列表", "- ", "插入无序列表"),
            ("编号", "1. ", "插入有序列表"),
            ("待办", "- [ ] ", "插入待办事项"),
            ("引用", "> ", "插入引用"),
            (
                "代码块",
                "\n```\n代码\n```\n",
                "插入代码块，可在开头三个反引号后填写语言",
            ),
            ("链接", "[链接文字](https://)", "插入链接"),
            (
                "表格",
                "\n| 列一 | 列二 |\n| --- | --- |\n| 内容 | 内容 |\n",
                "插入表格",
            ),
        ] {
            if ui.animated_button(label).on_hover_text(help).clicked() {
                insert = Some(syntax);
            }
        }
    });
    insert
}

fn insert_writing_format(ctx: &egui::Context, id: egui::Id, body: &mut String, syntax: &str) {
    let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    let len = body.chars().count();
    let range = state.cursor.char_range().map(|r| r.as_sorted_char_range());
    let start = range.as_ref().map_or(len, |r| r.start.0.min(len));
    let end = range.as_ref().map_or(len, |r| r.end.0.min(len));
    let byte_at = |index: usize| {
        body.char_indices()
            .nth(index)
            .map_or(body.len(), |(i, _)| i)
    };
    let bytes = byte_at(start)..byte_at(end);
    body.replace_range(bytes, syntax);
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(start + syntax.chars().count()),
        )));
    state.store(ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
}

pub(super) struct WritingState {
    pub page: Option<Page>,
    pub dirty: bool,
    changed_at: f64,
    pub error: String,
    pub(super) preview: bool,
    writer: String,
    focus_body: bool,
    cache: CommonMarkCache,
    pub(super) catalog_key: Option<(u64, String)>,
    catalog: Vec<WritingSummary>,
    chars: usize,
    pub(super) floating: bool,
    floating_pinned: bool,
    floating_essay: bool,
    book_navigation: book_navigation::BookNavigation,
    book_directory_width: Option<f32>,
    pub(super) ssh_key_path: Option<std::path::PathBuf>,
    pub(super) publish_busy: bool,
    publish_requested: bool,
    pub(super) publishing_page_id: Option<String>,
    markdown_editor_rect: Option<egui::Rect>,
    markdown_editor_page_id: Option<String>,
    markdown_editor_markdown: Option<String>,
    markdown_editor_theme: Option<String>,
    markdown_editor_ready: bool,
    markdown_editor_error: Option<String>,
    #[cfg(target_os = "windows")]
    markdown_editor_webview: Option<wry::WebView>,
}

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum EditorMessage {
    Ready,
    Changed { page_id: String, markdown: String },
    Save { page_id: String, markdown: String },
}

impl Default for WritingState {
    fn default() -> Self {
        Self {
            page: None,
            dirty: false,
            changed_at: 0.,
            error: String::new(),
            preview: false,
            writer: uuid::Uuid::new_v4().to_string(),
            focus_body: false,
            cache: Default::default(),
            catalog_key: None,
            catalog: vec![],
            chars: 0,
            floating: false,
            floating_pinned: true,
            floating_essay: false,
            book_navigation: Default::default(),
            book_directory_width: None,
            ssh_key_path: None,
            publish_busy: false,
            publish_requested: false,
            publishing_page_id: None,
            markdown_editor_rect: None,
            markdown_editor_page_id: None,
            markdown_editor_markdown: None,
            markdown_editor_theme: None,
            markdown_editor_ready: false,
            markdown_editor_error: None,
            #[cfg(target_os = "windows")]
            markdown_editor_webview: None,
        }
    }
}
#[derive(Clone)]
struct WritingDrag(String);

impl PersonalDeskApp {
    pub(super) fn new_writing(&mut self, log: bool) -> bool {
        self.new_writing_for_day(log, chrono::Local::now().date_naive())
    }
    pub(super) fn new_writing_for_day(&mut self, log: bool, day: chrono::NaiveDate) -> bool {
        if !self.flush_writing() {
            return false;
        }
        let day = day.format("%Y-%m-%d").to_string();
        let book_id = self
            .writing
            .page
            .as_ref()
            .filter(|page| page.book_id != crate::db::WRITING_ROOT)
            .map(|page| page.book_id.clone())
            .or_else(|| self.library.book.as_ref().map(|book| book.id.clone()));
        let result = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| {
                db.create_writing(
                    if log { &day } else { "未命名文档" },
                    book_id.as_deref(),
                    log.then_some(day.as_str()),
                )
            });
        match result {
            Ok(page) => {
                self.set_writing_page(page);
                self.section = Section::Documents;
                true
            }
            Err(e) => {
                self.notice = e.to_string();
                false
            }
        }
    }
    pub(super) fn set_writing_page(&mut self, page: Page) {
        self.writing.floating = false;
        self.library.book = if page.book_id == crate::db::WRITING_ROOT {
            None
        } else {
            self.db
                .as_ref()
                .and_then(|db| db.books().ok())
                .and_then(|books| books.into_iter().find(|b| b.id == page.book_id))
        };
        self.writing.chars = page.body.chars().count();
        self.writing.page = Some(page);
        self.writing.dirty = false;
        self.writing.error.clear();
        self.writing.preview = false;
        self.writing.focus_body = true;
        self.writing.cache = Default::default();
        self.writing.markdown_editor_page_id = None;
        self.writing.markdown_editor_markdown = None;
    }

    pub(super) fn handle_markdown_editor_message(&mut self, message: EditorMessage, now: f64) {
        match message {
            EditorMessage::Ready => {
                self.writing.markdown_editor_ready = true;
            }
            EditorMessage::Changed { page_id, markdown } => {
                self.apply_markdown_editor_update(page_id, markdown, false, now);
            }
            EditorMessage::Save { page_id, markdown } => {
                self.apply_markdown_editor_update(page_id, markdown, true, now);
            }
        }
    }

    fn apply_markdown_editor_update(&mut self, page_id: String, markdown: String, save: bool, now: f64) {
        if !self
            .writing
            .page
            .as_ref()
            .is_some_and(|page| page.id == page_id)
        {
            return;
        }
        let changed = self
            .writing
            .page
            .as_ref()
            .is_some_and(|page| page.body != markdown);
        if changed {
            if let Some(page) = self.writing.page.as_mut() {
                page.body = markdown.clone();
            }
            self.mark_writing_changed(now);
        }
        self.writing.markdown_editor_markdown = Some(markdown);
        if save {
            self.flush_writing();
            if self.writing.publish_requested {
                self.writing.publish_requested = false;
                self.start_blog_publish();
            }
        }
    }

    pub(super) fn request_blog_publish(&mut self) {
        if self.writing.publish_busy {
            return;
        }
        #[cfg(target_os = "windows")]
        if !self.writing.preview
            && self.writing.markdown_editor_ready
            && self.writing.markdown_editor_webview.is_some()
            && self.writing.page.as_ref().is_some_and(|page| {
                self.writing.markdown_editor_page_id.as_deref() == Some(page.id.as_str())
            })
        {
            self.writing.publish_requested = true;
            if let Some(view) = self.writing.markdown_editor_webview.as_ref()
                && view
                    .evaluate_script("window.personalDesk?.flush();")
                    .is_ok()
            {
                return;
            }
            self.writing.publish_requested = false;
            self.notice = "无法读取写作编辑器中的最新内容，请稍后再试。".into();
            return;
        }
        self.start_blog_publish();
    }

    fn start_blog_publish(&mut self) {
        if self.writing.publish_busy || !self.flush_writing() {
            return;
        }
        let Some(page) = self.writing.page.clone() else {
            self.notice = "请先打开要发布的文档。".into();
            return;
        };
        if page.title.trim().is_empty() || page.title.chars().count() > 200 {
            self.notice = "博客标题不能为空，且不能超过 200 个字符。".into();
            return;
        }
        if page.body.len() > 200_000 {
            self.notice = "博客正文超过 200 KB，请拆分文档后再发布。".into();
            return;
        }
        if page.tags.len() > super::blog_publish::MAX_TAGS
            || page.tags.iter().any(|tag| tag.trim().chars().count() > 50)
        {
            self.notice = "博客标签最多 8 个，每个标签不能超过 50 个字符。".into();
            return;
        }

        let stored_key = self
            .writing
            .ssh_key_path
            .clone()
            .filter(|path| path.is_file())
            .or_else(super::blog_publish::default_ssh_key);
        let key_path = stored_key.or_else(|| {
            rfd::FileDialog::new()
                .set_title("选择用于连接博客服务器的 SSH 私钥")
                .pick_file()
        });
        let Some(key_path) = key_path else {
            self.notice = "选择 SSH 私钥后才能发布到个人博客。".into();
            return;
        };
        if self.writing.ssh_key_path.as_deref() != Some(key_path.as_path()) {
            if let Err(error) = self
                .db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| db.set_setting("blog.ssh_key_path", &key_path.to_string_lossy()))
            {
                self.notice = format!("SSH 私钥路径未保存：{error}");
                return;
            }
            self.writing.ssh_key_path = Some(key_path.clone());
        }

        let post_id = match self
            .db
            .as_ref()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.blog_post_id(&page.id))
        {
            Ok(Some(id)) => id,
            Ok(None) => {
                let id = uuid::Uuid::new_v4().simple().to_string()[..16].to_owned();
                if let Err(error) = self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.link_blog_post(&page.id, &id))
                {
                    self.notice = format!("博客文章关联保存失败：{error}");
                    return;
                }
                id
            }
            Err(error) => {
                self.notice = format!("读取博客文章关联失败：{error}");
                return;
            }
        };

        let local_now = chrono::Local::now();
        let payload = super::blog_publish::BlogPostPayload {
            page_id: page.id.clone(),
            post_id,
            title: page.title,
            content: page.body,
            tags: page.tags,
            tenant_id: "0000000000000000".into(),
            published_at: local_now.timestamp_millis(),
            published_date: local_now.format("%Y-%m-%d").to_string(),
        };
        let page_id = page.id;
        let sender = self.sender.clone();
        self.writing.publish_busy = true;
        self.writing.publishing_page_id = Some(page_id.clone());
        self.notice = "正在通过 SSH 将文章写入个人博客数据库…".into();
        std::thread::spawn(move || {
            let result = super::blog_publish::publish_over_ssh(&key_path, &payload);
            let _ = sender.send(super::BackgroundMessage::BlogPublished { page_id, result });
        });
    }

    pub(super) fn reset_markdown_editor_bounds(&mut self) {
        // The editor is a native child window. Its previous-frame rectangle must not
        // survive when the active workspace page no longer contains the editor.
        self.writing.markdown_editor_rect = None;
    }

    #[cfg(target_os = "windows")]
    pub(super) fn sync_markdown_editor_webview(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        use std::{borrow::Cow, io::Read};
        use wry::{
            NewWindowResponse, Rect, WebViewBuilder,
            dpi::{LogicalPosition, LogicalSize},
            http::Response,
        };

        let Some(rect) = self.writing.markdown_editor_rect.filter(|rect| rect.is_positive()) else {
            if let Some(view) = self.writing.markdown_editor_webview.as_ref() {
                let _ = view.set_visible(false);
            }
            return;
        };
        let Some(window) = frame.winit_window() else {
            return;
        };
        let bounds = Rect {
            position: LogicalPosition::new(rect.left() as f64, rect.top() as f64).into(),
            size: LogicalSize::new(rect.width() as f64, rect.height() as f64).into(),
        };

        if self.writing.markdown_editor_webview.is_none() && self.writing.markdown_editor_error.is_none() {
            let sender = self.sender.clone();
            let ipc_ctx = ctx.clone();
            let builder = WebViewBuilder::new()
                .with_custom_protocol("desk-editor".into(), |_id, request| {
                    let name = match request.uri().path() {
                        "/" | "/index.html" => "index.html",
                        "/editor.js" => "editor.js",
                        "/style.css" => "style.css",
                        _ => {
                            return Response::builder()
                                .status(404)
                                .body(Cow::Borrowed(&[][..]))
                                .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])));
                        }
                    };
                    let mut archive = match zip::ZipArchive::new(std::io::Cursor::new(
                        include_bytes!("../../assets/markdown-editor/editor-bundle.zip").as_slice(),
                    )) {
                        Ok(archive) => archive,
                        Err(_) => {
                            return Response::builder()
                                .status(500)
                                .body(Cow::Borrowed(&[][..]))
                                .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])));
                        }
                    };
                    let mut bytes = Vec::new();
                    let read_ok = if let Ok(mut file) = archive.by_name(name) {
                        file.read_to_end(&mut bytes).is_ok()
                    } else {
                        false
                    };
                    if !read_ok {
                        return Response::builder()
                            .status(404)
                            .body(Cow::Borrowed(&[][..]))
                            .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])));
                    }
                    let mime = match name {
                        "index.html" => "text/html; charset=utf-8",
                        "editor.js" => "text/javascript; charset=utf-8",
                        _ => "text/css; charset=utf-8",
                    };
                    let mut response = Response::builder()
                        .status(200)
                        .header("Content-Type", mime)
                        .header("Cache-Control", "no-store");
                    if name == "index.html" {
                        response = response.header(
                            "Content-Security-Policy",
                            "default-src 'self' data: file:; script-src 'self'; style-src 'self' 'unsafe-inline' data:; img-src 'self' data: file: https:; font-src 'self' data:; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'",
                        );
                    }
                    response
                        .body(Cow::Owned(bytes))
                        .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])))
                })
                .with_ipc_handler(move |request| {
                    if let Ok(message) = serde_json::from_str::<EditorMessage>(request.body()) {
                        let _ = sender.send(super::BackgroundMessage::MarkdownEditor(message));
                    }
                    ipc_ctx.request_repaint();
                })
                .with_navigation_handler(|url| {
                    url.starts_with("desk-editor://localhost/")
                        || url.starts_with("http://desk-editor.localhost/")
                        || url.starts_with("https://desk-editor.localhost/")
                })
                .with_new_window_req_handler(|url, _| {
                    if url.starts_with("https://") || url.starts_with("http://") {
                        let _ = open::that_detached(&url);
                    }
                    NewWindowResponse::Deny
                })
                .with_url("desk-editor://localhost/index.html")
                .with_bounds(bounds)
                .with_clipboard(true)
                .with_focused(true);

            match builder.build_as_child(window.as_ref()) {
                Ok(view) => {
                    self.writing.markdown_editor_webview = Some(view);
                    self.writing.markdown_editor_ready = false;
                    self.writing.markdown_editor_error = None;
                }
                Err(error) => {
                    self.writing.markdown_editor_error = Some(error.to_string());
                    self.notice = "写作编辑器无法启动。".into();
                    ctx.request_repaint();
                    return;
                }
            }
        }

        let Some(view) = self.writing.markdown_editor_webview.as_ref() else {
            return;
        };
        let _ = view.set_visible(true);
        if let Err(error) = view.set_bounds(bounds) {
            self.writing.markdown_editor_error = Some(error.to_string());
            self.writing.markdown_editor_rect = None;
            ctx.request_repaint();
            return;
        }
        if !self.writing.markdown_editor_ready {
            return;
        }
        let theme = serde_json::json!({
            "accent": self.theme.accent,
            "surface": self.theme.surface,
            "text": self.theme.text,
            "muted": self.theme.muted,
        });
        if let Ok(theme) = serde_json::to_string(&theme)
            && self.writing.markdown_editor_theme.as_deref() != Some(theme.as_str())
            && view
                .evaluate_script(&format!("window.personalDesk?.setTheme({theme});"))
                .is_ok()
        {
            self.writing.markdown_editor_theme = Some(theme);
        }
        let Some(page) = self.writing.page.as_ref() else {
            return;
        };
        let markdown = page.body.clone();
        let needs_load = self.writing.markdown_editor_page_id.as_deref() != Some(page.id.as_str())
            || self.writing.markdown_editor_markdown.as_deref() != Some(markdown.as_str());
        if needs_load {
            let payload = serde_json::json!({ "id": page.id, "markdown": markdown });
            if let Ok(payload) = serde_json::to_string(&payload) {
                if view
                    .evaluate_script(&format!("window.personalDesk?.loadDocument({payload});"))
                    .is_ok()
                {
                    self.writing.markdown_editor_page_id = Some(page.id.clone());
                    self.writing.markdown_editor_markdown = Some(page.body.clone());
                }
            }
        }
    }
    pub(super) fn open_writing(&mut self, id: &str) -> bool {
        if !self.flush_writing() {
            return false;
        }
        match self
            .db
            .as_ref()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.writing_page(id))
        {
            Ok(Some(page)) => {
                self.set_writing_page(page);
                true
            }
            Ok(None) => {
                self.notice = "文档已不存在".into();
                false
            }
            Err(e) => {
                self.notice = e.to_string();
                false
            }
        }
    }
    pub(super) fn mark_writing_changed(&mut self, now: f64) {
        self.writing.dirty = true;
        self.writing.changed_at = now;
        let Some(page) = self.writing.page.as_ref() else {
            return;
        };
        self.writing.chars = page.body.chars().count();
        let result = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.save_book_draft(&self.writing.writer, page));
        if let Err(e) = result {
            self.writing.error = format!("草稿保存失败：{e}");
        }
    }
    pub(super) fn flush_writing(&mut self) -> bool {
        if !self.writing.dirty {
            return true;
        }
        let Some(page) = self.writing.page.as_mut() else {
            return true;
        };
        if page.title.trim().is_empty() {
            page.title = "未命名文档".into();
        }
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.finish_writing_save(&self.writing.writer, page))
        {
            Ok(()) => {
                self.writing.dirty = false;
                self.writing.error.clear();
                true
            }
            Err(e) => {
                self.writing.error = format!("保存失败：{e}");
                self.notice = self.writing.error.clone();
                false
            }
        }
    }
    fn refresh_writing_catalog(&mut self) {
        let Some(db) = self.db.as_ref() else { return };
        let key = (db.change_count(), self.search_query.clone());
        if self.writing.catalog_key.as_ref() == Some(&key) {
            return;
        }
        match db.writing_summaries(&self.search_query) {
            Ok(items) => {
                self.writing.catalog = items;
                self.writing.catalog_key = Some(key);
            }
            Err(e) => self.notice = e.to_string(),
        }
    }
    pub(super) fn group_written_documents(&mut self, ids: &[String], target: Option<&str>) -> bool {
        if !self.flush_writing() {
            return false;
        }
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.group_writing(ids, target))
        {
            Ok(book) => {
                self.writing.catalog_key = None;
                // Follow the dragged document into its destination, with fresh revision data.
                // Otherwise it vanishes from the old catalog while the editor still points there.
                if let Some(id) = ids.last()
                    && self.open_writing(id)
                {
                    self.notice = format!("已放入《{}》", book.title);
                    true
                } else {
                    false
                }
            }
            Err(e) => {
                self.notice = e.to_string();
                false
            }
        }
    }
    pub(super) fn render_writing_documents(&mut self, ui: &mut egui::Ui) {
        self.refresh_writing_catalog();
        let filter = self
            .module_filters
            .get(TagScope::BookPages.key())
            .cloned()
            .unwrap_or_default();
        let mut notes = self
            .writing
            .catalog
            .iter()
            .filter(|p| {
                filter.matches(&p.tags)
                    && p.book_id
                        == self
                            .library
                            .book
                            .as_ref()
                            .map(|b| b.id.as_str())
                            .unwrap_or(crate::db::WRITING_ROOT)
            })
            .cloned()
            .collect::<Vec<_>>();
        notes.sort_by_key(|p| !self.is_favorite("book_pages", &p.id));
        let all_books = self
            .db
            .as_ref()
            .and_then(|db| db.books().ok())
            .unwrap_or_default();
        let books = all_books
            .iter()
            .filter(|b| b.parent_id.as_deref() == self.library.book.as_ref().map(|b| b.id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let mut open = None;
        let mut remove = None;
        let mut add = false;
        let mut book_open = None;
        let mut drop_action = None;
        if let Some(book) = self.library.book.clone() {
            ui.horizontal(|ui| {
                if ui
                    .button(if book.parent_id.is_some() {
                        "← 上一级"
                    } else {
                        "← 全部图书"
                    })
                    .clicked()
                {
                    self.open_parent_book();
                }
                ui.label(&book.title);
                if ui.animated_button("删除图书").clicked() {
                    self.request_library_deletion(&book.id, &book.title, false);
                }
            });
            ui.add_space(8.);
        }
        if ui.animated_button("创建图书").clicked() {
            self.new_child_book();
        }
        ui.label(
            RichText::new("文档拖到文档上合成图书；拖到图书上放入其中，图书也可拖入图书。")
                .small()
                .color(muted()),
        );
        if self.library.book.is_some() && self.render_nested_book_catalog(ui, &books, &filter) {
            return;
        }
        let at_root = self.library.book.is_none();
        collection(ui, "writing-documents", self.view_mode(), |ui, layout| {
            for book in books.iter().filter(|_| at_root) {
                let members = self
                    .writing
                    .catalog
                    .iter()
                    .filter(|p| {
                        hierarchy::is_within(&all_books, &p.book_id, &book.id)
                            && filter.matches(&p.tags)
                    })
                    .collect::<Vec<_>>();
                if (!self.search_query.is_empty() || filter != TagFilter::All) && members.is_empty()
                {
                    continue;
                }
                let response = tagged_entry(
                    ui,
                    &book.id,
                    &book.title,
                    (&format!("{} 份文档", members.len()), &book.tags),
                    TileKind::Folder,
                    layout,
                );
                self.favorite_control(ui, &response, "books", &book.id);
                if response.clicked() {
                    book_open = Some(book.clone());
                }
                response.dnd_set_drag_payload(hierarchy::BookDrag(book.id.clone()));
                if let Some(action) = hierarchy::book_drop(ui, &response, &book.id) {
                    drop_action = Some(action);
                }
                response.context_menu(|ui| {
                    if ui.animated_button("重命名图书").clicked() {
                        self.library.dialog = Some(book.clone());
                        ui.close();
                    }
                    if book.parent_id.is_some() && ui.animated_button("移到上一级").clicked() {
                        let parent = self
                            .parent_of(book.parent_id.as_deref())
                            .and_then(|b| b.parent_id);
                        drop_action = Some(hierarchy::LibraryDrop::Book(book.id.clone(), parent));
                        ui.close();
                    }
                    self.record_tag_menu(ui, TagScope::Books, &book.id);
                    ui.separator();
                    if ui.animated_button("删除图书").clicked() {
                        remove = Some((book.id.clone(), book.title.clone(), false));
                        ui.close();
                    }
                });
            }
            for note in &notes {
                let preview = if note.preview.is_empty() {
                    note.updated_at.clone()
                } else {
                    note.preview.replace(['\n', '\r'], " ")
                };
                let response = tagged_entry(
                    ui,
                    &note.id,
                    &note.title,
                    (&preview, &note.tags),
                    TileKind::Document,
                    layout,
                );
                self.favorite_control(ui, &response, "book_pages", &note.id);
                self.record_classification_dropdown(ui, &response, TagScope::BookPages, &note.id);
                response.dnd_set_drag_payload(WritingDrag(note.id.clone()));
                if let Some(action) = hierarchy::page_drop(ui, &response, &note.id) {
                    drop_action = Some(action);
                }
                if response.clicked() {
                    open = Some(note.id.clone());
                }
                response.context_menu(|ui| {
                    self.record_tag_menu(ui, TagScope::BookPages, &note.id);
                    if ui.animated_button("删除文档").clicked() {
                        remove = Some((note.id.clone(), note.title.clone(), true));
                        ui.close();
                    }
                });
            }
            add = entry(ui, "add-writing", "新建文档", "", TileKind::Add, layout).clicked();
        });
        if let Some(action) = drop_action {
            self.apply_library_drop(action);
            return;
        }
        if let Some(book) = book_open {
            self.enter_book(book);
        }
        if let Some((id, title, page)) = remove {
            self.request_library_deletion(&id, &title, page);
        }
        if let Some(id) = open {
            self.open_writing(&id);
        }
        if add {
            self.new_writing(false);
        }
    }
    pub(super) fn render_writing_editor(&mut self, ui: &mut egui::Ui) {
        self.search_field = None;
        let mut back = false;
        let mut format = None;
        let mut save = false;
        let mut export = false;
        let mut copy = false;
        let mut float = false;
        let mut publish = false;
        let mut remove = None;
        self.writing.markdown_editor_rect = None;
        let publish_ready = !self.writing.publish_busy
            && !self.writing.floating
            && (self.writing.preview
                || self.writing.markdown_editor_error.is_some()
                || !cfg!(target_os = "windows")
                || (self.writing.markdown_editor_ready
                    && self.writing.page.as_ref().is_some_and(|page| {
                        self.writing.markdown_editor_page_id.as_deref() == Some(page.id.as_str())
                    })));
        ui.horizontal_wrapped(|ui| {
            ui.set_min_height(38.);
            back = ui
                .button(if self.section == Section::Essays {
                    "← 随笔"
                } else if self.section == Section::Books {
                    "← 图书"
                } else {
                    "← 文档"
                })
                .clicked();
            if let Some(book) = self.library.book.as_ref() {
                ui.add_sized(
                    [ui.available_width().min(150.), 24.],
                    egui::Label::new(RichText::new(&book.title).color(ink())).truncate(),
                )
                .on_hover_text(&book.title);
            }
            let status = if !self.writing.error.is_empty() {
                "保存失败"
            } else if self.writing.dirty {
                "正在保存…"
            } else {
                "已保存"
            };
            ui.label(RichText::new(status).small().color(muted()));
            ui.label(
                RichText::new(format!("{} 字", self.writing.chars))
                    .small()
                    .color(muted()),
            );
            if self.section != Section::Essays
                && ui.animated_button("删除页面").clicked()
                && let Some(page) = self.writing.page.as_ref()
            {
                remove = Some((page.id.clone(), page.title.clone(), true));
            }
            if self.section != Section::Essays && ui.add(primary_button("新建")).clicked() {
                self.new_writing(false);
            }
            if self.section != Section::Essays && ui.animated_button("生成日志").clicked() {
                self.new_writing(true);
            }
            float = ui
                .button("悬浮窗")
                .on_hover_text("置顶小窗，方便对照其他应用内容写总结")
                .clicked();
            crate::ui_components::chrome::segmented(
                ui,
                &mut self.writing.preview,
                &[
                    (
                        false,
                        if cfg!(target_os = "windows") {
                            "实时预览"
                        } else {
                            "写作"
                        },
                    ),
                    (true, "预览"),
                ],
            );
            self.render_writing_zoom(ui);
            if self.section != Section::Essays {
                publish = ui
                    .add_enabled(
                        publish_ready,
                        egui::Button::new(if self.writing.publish_busy {
                            "正在发布…"
                        } else if !publish_ready {
                            "编辑器准备中…"
                        } else {
                            "发布到博客"
                        }),
                    )
                    .on_hover_text("点击后立即公开当前文档；通过本机 SSH 密钥写入个人博客数据库。")
                    .clicked();
            }
            ui.menu_button("更多", |ui| {
                if let Some(book) = self.library.book.as_ref() {
                    if ui.animated_button("删除图书").clicked() {
                        remove = Some((book.id.clone(), book.title.clone(), false));
                        ui.close();
                    }
                    ui.separator();
                }
                save = ui.animated_button("保存 · Ctrl+S").clicked();
                export = ui.animated_button("导出 Markdown").clicked();
                if !self.writing.error.is_empty() {
                    copy = ui.animated_button("另存为新文档").clicked();
                }
                let options = self.names_for_scope(TagScope::BookPages);
                if let Some(page) = self.writing.page.as_mut()
                    && crate::ui_components::classification_dropdown(
                        ui,
                        "writing-tags",
                        &mut page.tags,
                        &options,
                        160.,
                    )
                {
                    self.mark_writing_changed(ui.input(|i| i.time));
                }
                if ui.animated_button("管理标签").clicked() {
                    self.open_tag_manager(TagScope::BookPages);
                    ui.close();
                }
            });
        });
        if let Some((id, title, page)) = remove {
            self.request_library_deletion(&id, &title, page);
        }
        if back && self.flush_writing() {
            self.essay.active = None;
            self.writing.page = None;
            return;
        }
        if save {
            self.flush_writing();
        }
        if copy {
            self.save_writing_copy();
        }
        if export {
            self.export_writing();
        }
        if float {
            self.open_floating_writer();
        }
        if publish {
            self.request_blog_publish();
        }
        if self.writing.page.is_none() {
            return;
        }
        if self.writing.floating {
            ui.add_space(30.);
            ui.heading(&self.writing.page.as_ref().unwrap().title);
            ui.label("正在悬浮窗中写作，可切换到其他应用。内容自动保存到这份文档。");
            ui.horizontal(|ui| {
                if ui.animated_button("显示悬浮窗").clicked() {
                    ui.ctx().send_viewport_cmd_to(
                        floating::viewport_id(),
                        egui::ViewportCommand::Minimized(false),
                    );
                    ui.ctx().send_viewport_cmd_to(
                        floating::viewport_id(),
                        egui::ViewportCommand::Focus,
                    );
                }
                if ui.animated_button("回到主窗口").clicked() {
                    self.close_floating_writer();
                }
            });
            return;
        }
        if let Some(syntax) = writing_format_toolbar(ui) {
            format = Some(syntax);
        }
        if !self.writing.error.is_empty() {
            ui.colored_label(Color32::from_rgb(184, 65, 71), &self.writing.error);
        }
        if let Some(error) = &self.writing.markdown_editor_error {
            ui.colored_label(
                Color32::from_rgb(184, 65, 71),
                format!("实时预览编辑器启动失败，暂用兼容编辑器：{error}"),
            );
        }
        if self.section == Section::Essays
            && let Some(a) = self.essay.active.as_ref()
        {
            ui.add_space(8.);
            ui.label(
                RichText::new(format!("{} · {}", a.day, a.topic.title))
                    .small()
                    .color(muted()),
            );
            ui.label(RichText::new(&a.topic.prompt).color(muted()));
        }
        ui.add_space(12.);
        if self.section != Section::Essays
            && let Some(book) = self.library.book.clone()
        {
            self.render_book_workspace(ui, &book, format);
        } else {
            self.render_writing_paper(ui, format);
        }
    }

    fn render_writing_paper(&mut self, ui: &mut egui::Ui, format: Option<&str>) {
        if cfg!(target_os = "windows")
            && !self.writing.preview
            && self.writing.markdown_editor_error.is_none()
        {
            self.render_markdown_editor_paper(ui, format);
            return;
        }
        let Some(page) = self.writing.page.as_ref() else {
            return;
        };
        let mut changed = false;
        let available = ui.available_width();
        let width = available.min(820.);
        let id = page.id.clone();
        let body_id = egui::Id::unique(("writing-body", &id));
        if let Some(syntax) = format {
            self.writing.preview = false;
            insert_writing_format(
                ui.ctx(),
                body_id,
                &mut self.writing.page.as_mut().unwrap().body,
                syntax,
            );
            changed = true;
        }
        egui::ScrollArea::vertical()
            .id_salt(("writing-paper", &id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.add_space(((available - width) / 2.).max(0.));
                    crate::ui_components::surface_frame()
                        .inner_margin(egui::Margin::symmetric(38, 30))
                        .show(ui, |ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.set_width((width - 76.).max(120.));
                                ui.set_min_height(
                                    (ui.ctx().content_rect().height() - 200.).max(300.),
                                );
                                let page = self.writing.page.as_mut().unwrap();
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut page.title)
                                        .id_salt(("writing-title", &id))
                                        .font(egui::FontId::proportional(28.))
                                        .frame(egui::Frame::NONE)
                                        .margin(0)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("未命名文档"),
                                );
                                changed |= response.changed();
                                ui.add_space(22.);
                                if self.writing.preview {
                                    CommonMarkViewer::new().show(
                                        ui,
                                        &mut self.writing.cache,
                                        &page.body,
                                    );
                                } else {
                                    let mut layouter =
                                        |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| {
                                            let mut job = egui::text::LayoutJob::simple(
                                                text.as_str().into(),
                                                egui::FontId::proportional(17.),
                                                ink(),
                                                width,
                                            );
                                            for section in &mut job.sections {
                                                section.format.line_height = Some(28.);
                                            }
                                            ui.painter().layout_job(job)
                                        };
                                    let response = ui.add(
                                        egui::TextEdit::multiline(&mut page.body)
                                            .id(body_id)
                                            .font(egui::FontId::proportional(17.))
                                            .frame(egui::Frame::NONE)
                                            .margin(0)
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(18)
                                            .hint_text("写点什么…")
                                            .layouter(&mut layouter),
                                    );
                                    changed |= response.changed();
                                    if self.writing.focus_body {
                                        response.request_focus();
                                        self.writing.focus_body = false;
                                    }
                                }
                            });
                        });
                });
            });
        let now = ui.input(|i| i.time);
        if changed {
            self.mark_writing_changed(now);
            ui.ctx().request_repaint_after(Duration::from_millis(550));
        }
        if self.writing.dirty {
            if now - self.writing.changed_at >= 0.5 {
                self.flush_writing();
            } else {
                ui.ctx().request_repaint_after(Duration::from_millis(550));
            }
        }
    }

    fn render_markdown_editor_paper(&mut self, ui: &mut egui::Ui, format: Option<&str>) {
        let Some(page) = self.writing.page.as_ref() else {
            return;
        };
        let available = ui.available_size();
        let width = available.x.min(980.);
        let id = page.id.clone();
        let mut changed = false;
        if let Some(syntax) = format
            && let Ok(payload) = serde_json::to_string(syntax)
            && let Some(view) = self.writing.markdown_editor_webview.as_ref()
        {
            let _ = view.evaluate_script(&format!("window.personalDesk?.insertFormat({payload});"));
        }
        ui.horizontal_top(|ui| {
            ui.add_space(((available.x - width) / 2.).max(0.));
            ui.allocate_ui_with_layout(
                egui::vec2(width, available.y.max(260.)),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(width);
                    ui.add_space(8.);
                    if let Some(page) = self.writing.page.as_mut() {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut page.title)
                                .id_salt(("writing-title", &id))
                                .font(egui::FontId::proportional(28.))
                                .frame(egui::Frame::NONE)
                                .margin(0)
                                .desired_width(f32::INFINITY)
                                .hint_text("未命名文档"),
                        );
                        changed = response.changed();
                    }
                    ui.add_space(14.);
                    let editor_area = ui.available_size();
                    let response = ui.allocate_response(
                        egui::vec2(width, editor_area.y.max(230.)),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(response.rect, 8., Color32::WHITE);
                    self.writing.markdown_editor_rect = Some(response.rect);
                },
            );
        });
        let now = ui.input(|input| input.time);
        if changed {
            self.mark_writing_changed(now);
            ui.ctx().request_repaint_after(Duration::from_millis(550));
        }
        if self.writing.dirty && now - self.writing.changed_at >= 0.5 {
            self.flush_writing();
        }
    }
    pub(super) fn save_writing_copy(&mut self) {
        if !self.flush_writing() {
            return;
        }
        let Some(mut page) = self.writing.page.clone() else {
            return;
        };
        page.id = uuid::Uuid::new_v4().to_string();
        page.revision = 0;
        page.title = format!("{}（副本）", page.title);
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.save_book_page(&mut page))
        {
            Ok(()) => self.set_writing_page(page),
            Err(e) => self.writing.error = e.to_string(),
        }
    }
    fn export_writing(&mut self) {
        if !self.flush_writing() {
            return;
        }
        let Some(page) = self.writing.page.as_ref() else {
            return;
        };
        let name = page
            .title
            .replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_");
        if let Some(path) = rfd::FileDialog::new()
            .set_title("导出文档")
            .set_file_name(format!("{name}.md"))
            .add_filter("Markdown", &["md"])
            .save_file()
        {
            let result = (|| -> crate::error::Result<()> {
                use std::io::Write;
                let parent = path.parent().ok_or(DeskError::UnsupportedFormat)?;
                let mut file = tempfile::NamedTempFile::new_in(parent)?;
                file.write_all(format!("# {}\n\n{}", page.title, page.body).as_bytes())?;
                file.as_file().sync_all()?;
                file.persist(&path).map_err(|e| e.error)?;
                Ok(())
            })();
            self.notice = match result {
                Ok(()) => "文档已导出".into(),
                Err(e) => e.to_string(),
            };
        }
    }
}
