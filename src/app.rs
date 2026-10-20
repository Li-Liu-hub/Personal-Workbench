use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use eframe::egui::{self, Color32, RichText};
use serde::{Deserialize, Serialize};

mod browser_ui;
mod codex_ui;
mod contact_ui;
mod document_ui;
mod favorites_ui;
mod game_ui;
#[cfg(debug_assertions)]
mod interaction_probe;
mod inventory_ui;
mod library_ui;
mod module_tags;
mod native_codex_ui;
#[cfg(debug_assertions)]
mod preview;
mod terminal_ui;
mod theme_ui;
mod tool_ui;
mod update_ui;
#[cfg(test)]
mod view_tests;

use crate::{
    contacts::{self, ChatRecord, Contact},
    db::Database,
    documents::{self, DocumentFormatHandler, WordHandler},
    error::DeskError,
    export, files,
    model::{
        Credential, CredentialSummary, DocumentRecord, FileItem, InventoryItem, SearchResult,
        Section, Shortcut, Website,
    },
    search,
    tags::{self, Filter as TagFilter, Scope as TagScope},
    ui_components::{
        TileKind, accent, border, canvas, centered_dialog, collection, content_fill,
        credential_entry, credential_list_header, dialog_closed, entry, file_entry,
        file_list_header, ink, muted, navigation_item, primary_button, sidebar, surface,
        tag_choices, tag_filter_bar, tagged_entry, view_switcher, website_entry,
    },
    views::{self, Preferences, ViewMode},
    websites,
};

#[derive(Clone)]
struct RecoveryItem {
    id: String,
    original_path: String,
    draft_path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct RecoveryManifest {
    original_path: String,
}

enum BackgroundMessage {
    Terminal(
        PathBuf,
        Box<std::result::Result<crate::terminal::Terminal, String>>,
    ),
    ContactChats(String, std::result::Result<Vec<ChatRecord>, String>),
    Indexed(std::result::Result<usize, String>),
    Exported(std::result::Result<export::Report, String>),
    Dropped(PathBuf, files::DropReport),
    Copied(std::result::Result<PathBuf, String>),
    Moved(std::result::Result<PathBuf, String>),
}

#[derive(Clone)]
struct TagAssignment {
    scope: TagScope,
    id: String,
    selected: Vec<String>,
}

pub struct PersonalDeskApp {
    favorites: std::collections::BTreeSet<(String, String)>,
    inline_inventory: std::collections::HashMap<String, InventoryItem>,
    browser: crate::browser::Browser,
    browser_area: Option<egui::Rect>,
    terminal: Option<crate::terminal::Terminal>,
    terminal_directory: PathBuf,
    terminal_error: String,
    terminal_focused: bool,
    terminal_inline: bool,
    terminal_command: String,
    terminal_command_focus: bool,
    terminal_starting: bool,
    library: library_ui::LibraryState,
    photo_cache: std::collections::HashMap<String, egui::TextureHandle>,
    contact_scroll_id: Option<String>,
    undo: crate::undo::Journal,
    file_operations: usize,
    updates: update_ui::UpdateState,
    #[cfg(debug_assertions)]
    preview_capture: Option<preview::Capture>,
    #[cfg(debug_assertions)]
    interaction_probe: Option<interaction_probe::Probe>,
    desktop: crate::native_embed::DesktopDock,
    codex: crate::codex_client::CodexState,
    theme: crate::theme::ThemeSettings,
    theme_saved: crate::theme::ThemeSettings,
    theme_applied: Option<crate::theme::ThemeSettings>,
    theme_background: Option<egui::TextureHandle>,
    theme_background_key: Option<String>,
    theme_editor_open: bool,
    theme_error: String,
    tool_draft: Option<crate::launcher::AppTool>,
    tool_error: String,
    tool_launch_request: Option<PathBuf>,
    website_open_request: Option<Website>,
    snake: crate::games::Snake,
    snake_open: bool,
    snake_last_time: Option<f64>,
    snake_best: u32,
    view_modes: Preferences,
    contact_items: Vec<Contact>,
    contact_draft: Contact,
    contact_draft_tags: Vec<String>,
    contact_dialog_open: bool,
    contact_chat_open: Option<String>,
    contact_sheet: usize,
    contact_error: String,
    contact_import_busy: bool,
    db: Option<Database>,
    account_tags: Vec<String>,
    inventory_tags: Vec<String>,
    account_filter: TagFilter,
    inventory_filter: TagFilter,
    module_filters: std::collections::BTreeMap<String, TagFilter>,
    file_filter: files::FileFilter,
    tag_manager: Option<TagScope>,
    tag_assignment: Option<TagAssignment>,
    tag_name: String,
    tag_original: Option<String>,
    tag_error: String,
    system_open_request: Option<PathBuf>,
    open_with_request: Option<PathBuf>,
    copy_file_request: Option<PathBuf>,
    drag_file_request: Option<PathBuf>,
    drop_busy: bool,
    drop_report: Option<files::DropReport>,
    native_drop_position: Option<egui::Pos2>,
    folder_drop_targets: Vec<(egui::Rect, PathBuf)>,
    data_dir: PathBuf,
    startup_error: String,
    section: Section,
    website_items: Vec<Website>,
    website_draft: Website,
    website_draft_tags: Vec<String>,
    website_dialog_open: bool,
    website_editing: bool,
    website_error: String,
    export_dialog_open: bool,
    export_busy: bool,
    notice: String,
    credential_draft: Credential,
    credentials: Vec<CredentialSummary>,
    inline_credentials: std::collections::BTreeMap<String, CredentialSummary>,
    selected_credential: Option<String>,
    credential_dialog_open: bool,
    inventory_dialog_open: bool,
    editor_visible: bool,
    editor_title: String,
    editor_focus_title: bool,
    shortcut_dialog_open: bool,
    folder_dialog_open: bool,
    file_dialog_open: bool,
    settings_detail: Option<String>,
    editor_path: Option<PathBuf>,
    editor_text: String,
    editor_dirty: bool,
    editor_fingerprint: Option<[u8; 32]>,
    editor_conflict: bool,
    recovery_id: Option<String>,
    recovery_items: Vec<RecoveryItem>,
    dirty_since: Option<Instant>,
    search_query: String,
    search_field: Option<egui::Id>,
    search_results: Vec<SearchResult>,
    search_pending: Option<f64>,
    inventory_items: Vec<InventoryItem>,
    inventory_draft: InventoryItem,
    selected_inventory: Option<String>,
    folder_name: String,
    current_directory: Option<PathBuf>,
    current_root: Option<PathBuf>,
    file_items: Vec<FileItem>,
    selected_file: Option<PathBuf>,
    rename_text: String,
    sender: Sender<BackgroundMessage>,
    receiver: Receiver<BackgroundMessage>,
}

impl PersonalDeskApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_theme(&cc.egui_ctx);
        Self::open_data_directory(data_directory())
    }

    #[cfg(debug_assertions)]
    pub fn desktop_visual_preview(cc: &eframe::CreationContext<'_>, base: PathBuf) -> Self {
        configure_theme(&cc.egui_ctx);
        let mut app = Self::open_data_directory(base);
        app.section = Section::Codex;
        app
    }

    #[cfg(debug_assertions)]
    pub fn layout_preview(cc: &eframe::CreationContext<'_>, base: PathBuf) -> Self {
        configure_theme(&cc.egui_ctx);
        let mut app = Self::open_data_directory(base.clone());
        let folder = base.join("开发文档");
        fs::create_dir_all(folder.join("资料")).expect("preview folder");
        for name in [
            "项目笔记.md",
            "开发计划.txt",
            "需求清单.md",
            "接口说明.md",
            "会议记录.txt",
        ] {
            fs::write(folder.join(name), "界面预览用示例文档").expect("preview document");
        }
        app.db
            .as_mut()
            .unwrap()
            .save_shortcut("开发文档", &folder, true)
            .unwrap();
        app.navigate(folder.clone(), folder);
        app.notice.clear();
        app.prepare_preview_capture();
        app
    }
    fn open_data_directory(base: PathBuf) -> Self {
        let (sender, receiver) = mpsc::channel();
        let (db, startup_error, migration_notice) = match Database::open(&base) {
            Ok(db) => {
                let notice = db.migration_notice.clone();
                (Some(db), String::new(), notice)
            }
            Err(error) => (None, error.to_string(), String::new()),
        };
        let settings = db
            .as_ref()
            .and_then(|db| db.settings().ok())
            .unwrap_or_default();
        let theme = crate::theme::ThemeSettings::load(&settings);
        let favorites = db
            .as_ref()
            .and_then(|db| db.favorites().ok())
            .unwrap_or_default()
            .into_iter()
            .collect();
        let mut app = Self {
            favorites,
            inline_inventory: Default::default(),
            browser: crate::browser::Browser::new(base.join("browser-profile")),
            browser_area: None,
            terminal: None,
            terminal_directory: base.clone(),
            terminal_error: String::new(),
            terminal_focused: false,
            terminal_inline: false,
            terminal_command: String::new(),
            terminal_command_focus: false,
            terminal_starting: false,
            library: Default::default(),
            photo_cache: Default::default(),
            contact_scroll_id: None,
            undo: crate::undo::Journal::open(&base),
            file_operations: 0,
            #[cfg(debug_assertions)]
            preview_capture: None,
            #[cfg(debug_assertions)]
            interaction_probe: None,
            desktop: crate::native_embed::DesktopDock::default(),
            codex: crate::codex_client::CodexState::load(&base, &settings),
            theme: theme.clone(),
            theme_saved: theme,
            theme_applied: None,
            theme_background: None,
            theme_background_key: None,
            theme_editor_open: false,
            theme_error: String::new(),
            tool_draft: None,
            tool_error: String::new(),
            tool_launch_request: None,
            website_open_request: None,
            snake: crate::games::random_game(),
            snake_open: false,
            snake_last_time: None,
            snake_best: db
                .as_ref()
                .and_then(|db| db.settings().ok())
                .and_then(|s| s.get("games.snake.best").and_then(|s| s.parse().ok()))
                .unwrap_or(0),
            view_modes: Preferences::load(
                &db.as_ref()
                    .and_then(|db| db.settings().ok())
                    .unwrap_or_default(),
            ),
            updates: update_ui::UpdateState::new(
                db.as_ref()
                    .and_then(|db| db.settings().ok())
                    .and_then(|s| s.get("updates.source").cloned()),
            ),
            contact_items: Vec::new(),
            contact_draft: Contact::new(),
            contact_draft_tags: vec![],
            contact_dialog_open: false,
            contact_chat_open: None,
            contact_sheet: 0,
            contact_error: String::new(),
            contact_import_busy: false,
            db,
            account_tags: Vec::new(),
            inventory_tags: Vec::new(),
            account_filter: TagFilter::All,
            inventory_filter: TagFilter::All,
            module_filters: Default::default(),
            file_filter: Default::default(),
            tag_manager: None,
            tag_assignment: None,
            tag_name: String::new(),
            tag_original: None,
            tag_error: String::new(),
            system_open_request: None,
            open_with_request: None,
            copy_file_request: None,
            drag_file_request: None,
            drop_busy: false,
            drop_report: None,
            native_drop_position: None,
            folder_drop_targets: Vec::new(),
            data_dir: base,
            startup_error,
            section: Section::Folders,
            website_items: Vec::new(),
            website_draft: Website::new(),
            website_draft_tags: Vec::new(),
            website_dialog_open: false,
            website_editing: false,
            website_error: String::new(),
            export_dialog_open: false,
            export_busy: false,
            notice: migration_notice,
            credential_draft: Credential::new(),
            credentials: Vec::new(),
            inline_credentials: Default::default(),
            selected_credential: None,
            credential_dialog_open: false,
            inventory_dialog_open: false,
            editor_visible: false,
            editor_title: String::new(),
            editor_focus_title: false,
            shortcut_dialog_open: false,
            folder_dialog_open: false,
            file_dialog_open: false,
            settings_detail: None,
            editor_path: None,
            editor_text: String::new(),
            editor_dirty: false,
            editor_fingerprint: None,
            editor_conflict: false,
            recovery_id: None,
            recovery_items: Vec::new(),
            dirty_since: None,
            search_query: String::new(),
            search_field: None,
            search_results: Vec::new(),
            search_pending: None,
            inventory_items: Vec::new(),
            inventory_draft: InventoryItem::new(),
            selected_inventory: None,
            folder_name: String::new(),
            current_directory: None,
            current_root: None,
            file_items: Vec::new(),
            selected_file: None,
            rename_text: String::new(),
            sender,
            receiver,
        };
        if app.db.is_some() {
            app.load_recovery_drafts();
            app.load_summaries();
            app.load_inventory();
            app.load_websites();
            app.load_contacts();
            app.load_tags();
            if app.notice.is_empty() && !app.recovery_items.is_empty() {
                app.notice = format!("发现 {} 份可恢复草稿", app.recovery_items.len());
            }
        }
        app
    }

    fn save_recovery_draft(&mut self) {
        let Some(original_path) = self.editor_path.clone() else {
            return;
        };
        let id = self
            .recovery_id
            .get_or_insert_with(|| uuid::Uuid::new_v4().to_string())
            .clone();
        let directory = self.data_dir.join("recovery");
        let draft_path = directory.join(format!("{id}.recovery.txt"));
        let manifest_path = directory.join(format!("{id}.json"));
        if !manifest_path.exists() {
            let manifest = RecoveryManifest {
                original_path: original_path.to_string_lossy().into_owned(),
            };
            let result = serde_json::to_vec(&manifest)
                .map_err(DeskError::from)
                .and_then(|bytes| {
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&manifest_path)?;
                    file.write_all(&bytes)?;
                    file.sync_all()?;
                    Ok(())
                });
            if let Err(error) = result {
                self.notice = format!("恢复草稿信息写入失败：{error}");
                return;
            }
        }
        let expected = documents::fingerprint(&draft_path).ok();
        if let Err(error) =
            documents::save_text_atomic_checked(&draft_path, &self.editor_text, expected)
        {
            self.notice = format!("恢复草稿写入失败：{error}");
            return;
        }
        self.load_recovery_drafts();
    }

    fn load_recovery_drafts(&mut self) {
        let directory = self.data_dir.join("recovery");
        let mut items = Vec::new();
        if let Ok(entries) = fs::read_dir(&directory) {
            for entry in entries.flatten() {
                let manifest_path = entry.path();
                if manifest_path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let Some(id) = manifest_path.file_stem().and_then(|stem| stem.to_str()) else {
                    continue;
                };
                if uuid::Uuid::parse_str(id).is_err() {
                    continue;
                }
                let Ok(bytes) = fs::read(&manifest_path) else {
                    continue;
                };
                let Ok(manifest) = serde_json::from_slice::<RecoveryManifest>(&bytes) else {
                    continue;
                };
                let draft_path = directory.join(format!("{id}.recovery.txt"));
                if draft_path.is_file() {
                    items.push(RecoveryItem {
                        id: id.to_owned(),
                        original_path: manifest.original_path,
                        draft_path,
                    });
                }
            }
        }
        self.recovery_items = items;
    }

    fn discard_recovery_draft(&mut self, id: &str) {
        if uuid::Uuid::parse_str(id).is_err() {
            return;
        }
        let directory = self.data_dir.join("recovery");
        let _ = fs::remove_file(directory.join(format!("{id}.recovery.txt")));
        let _ = fs::remove_file(directory.join(format!("{id}.json")));
        self.recovery_items.retain(|item| item.id != id);
    }

    fn discard_active_recovery_draft(&mut self) {
        if let Some(id) = self.recovery_id.take() {
            self.discard_recovery_draft(&id);
        }
    }

    fn restore_recovery_draft(&mut self, item: RecoveryItem) {
        let text = match fs::read_to_string(&item.draft_path) {
            Ok(text) => text,
            Err(error) => {
                self.notice = format!("无法读取恢复草稿：{error}");
                return;
            }
        };
        let original = PathBuf::from(&item.original_path);
        let extension = original
            .extension()
            .and_then(|ext| ext.to_str())
            .filter(|ext| matches!(ext.to_ascii_lowercase().as_str(), "md" | "txt"))
            .unwrap_or("md");
        let stem = original
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "恢复文档".into());
        let short_id = item.id.replace('-', "");
        let path = self.data_dir.join("documents").join(format!(
            "{stem} - 恢复-{}.{}",
            &short_id[..8],
            extension
        ));
        match documents::save_text_new(&path, &text) {
            Ok(()) => {
                if let Some(db) = self.db.as_mut() {
                    let _ = db.touch_document(&path);
                }
                self.discard_recovery_draft(&item.id);
                self.notice = "草稿已恢复为新的文档，原文件未覆盖".into();
                self.open_editor(path);
            }
            Err(error) => self.notice = format!("无法创建恢复文档：{error}"),
        }
    }

    fn save_editor(&mut self) -> bool {
        if !self.editor_dirty {
            return true;
        }
        let Some(path) = self.editor_path.clone() else {
            return false;
        };
        match documents::save_text_atomic_checked(&path, &self.editor_text, self.editor_fingerprint)
        {
            Ok(()) => {
                self.editor_fingerprint =
                    Some(documents::fingerprint_bytes(self.editor_text.as_bytes()));
                if let Some(db) = self.db.as_mut()
                    && let Err(error) = db.set_document_title(&path, &self.editor_title)
                {
                    self.notice = format!("正文已保存，标题保存失败：{error}");
                    self.dirty_since = Some(Instant::now());
                    self.save_recovery_draft();
                    return false;
                }
                self.editor_dirty = false;
                self.editor_conflict = false;
                self.dirty_since = None;
                if self.editor_title.trim().is_empty() {
                    self.editor_title = "未命名文档".into();
                }
                self.notice = format!("已保存：{}", self.editor_title.trim());
                self.discard_active_recovery_draft();
                let managed = documents::default_documents_dir(&self.data_dir)
                    .canonicalize()
                    .is_ok_and(|root| path.canonicalize().is_ok_and(|file| file.starts_with(root)));
                if managed
                    && let Some(db) = self.db.as_mut()
                    && let Err(error) = db.index_file(
                        "managed-documents",
                        &path,
                        &self.editor_title,
                        &self.editor_text,
                    )
                {
                    self.notice = format!("文档已保存，搜索索引更新失败：{error}");
                }
                true
            }
            Err(error) => {
                self.notice = format!("保存失败：{error}");
                self.editor_conflict = matches!(&error, DeskError::ConcurrentModification);
                self.dirty_since = Some(Instant::now());
                self.save_recovery_draft();
                false
            }
        }
    }

    fn open_editor(&mut self, path: PathBuf) {
        if self.editor_dirty && !self.save_editor() {
            return;
        }
        match documents::read_text_with_fingerprint(&path) {
            Ok((contents, fingerprint)) => {
                self.editor_visible = true;
                self.editor_title = self
                    .db
                    .as_ref()
                    .and_then(|db| db.document_title(&path).ok())
                    .unwrap_or_else(|| "未命名文档".into());
                self.editor_path = Some(path.clone());
                self.editor_text = contents;
                self.editor_dirty = false;
                self.editor_fingerprint = Some(fingerprint);
                self.editor_conflict = false;
                self.dirty_since = None;
                self.section = Section::Documents;
                if let Some(db) = self.db.as_mut() {
                    let _ = db.touch_document(&path);
                }
            }
            Err(error) => self.notice = format!("无法打开文档：{error}"),
        }
    }

    fn reload_editor_from_disk(&mut self) {
        let Some(path) = self.editor_path.clone() else {
            return;
        };
        match documents::read_text_with_fingerprint(&path) {
            Ok((contents, fingerprint)) => {
                self.editor_text = contents;
                self.editor_fingerprint = Some(fingerprint);
                self.editor_dirty = false;
                self.editor_conflict = false;
                self.dirty_since = None;
                self.discard_active_recovery_draft();
                self.notice = "已重新载入磁盘版本".into();
            }
            Err(error) => self.notice = format!("无法重新载入：{error}"),
        }
    }

    fn save_editor_as(&mut self) {
        let suggested_name = "文档.md";
        let Some(path) = rfd::FileDialog::new()
            .set_title("导出文本副本")
            .set_file_name(suggested_name)
            .add_filter("Markdown", &["md"])
            .add_filter("纯文本", &["txt"])
            .save_file()
        else {
            return;
        };
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "md" | "txt") {
            self.notice = "导出文本仅支持 Markdown 或 TXT 文件".into();
            return;
        }
        match documents::save_text_new(&path, &self.editor_text) {
            Ok(()) => {
                self.notice = format!("已导出副本：{}；原文档仍保留在办公台内", path.display())
            }
            Err(error) => self.notice = format!("导出失败：{error}"),
        }
    }

    fn add_shortcut(&mut self, path: PathBuf) {
        let path = match path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                self.notice = format!("无法访问所选路径：{e}");
                return;
            }
        };
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned();
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.save_shortcut(&name, &path, path.is_dir()))
        {
            Ok(()) => {
                self.notice = format!("已添加：{name}");
            }
            Err(rusqlite_error) => self.notice = format!("添加快捷路径失败：{rusqlite_error}"),
        }
    }

    fn navigate(&mut self, root: PathBuf, current: PathBuf) {
        match (root.canonicalize(), current.canonicalize()) {
            (Ok(root), Ok(path)) if path.starts_with(&root) && path.is_dir() => {
                match files::list_directory(&path) {
                    Ok(items) => {
                        self.current_root = Some(root);
                        self.current_directory = Some(path);
                        self.file_items = items;
                        self.section = Section::Folders;
                        self.selected_file = None;
                        self.search_query.clear();
                        self.search_results.clear();
                    }
                    Err(e) => self.notice = e.to_string(),
                }
            }
            (Err(e), _) | (_, Err(e)) => self.notice = format!("路径已失效：{e}"),
            _ => self.notice = "该位置不在所选文件夹中".into(),
        }
    }

    fn refresh_files(&mut self) {
        if let Some(path) = self.current_directory.clone() {
            match files::list_directory(&path) {
                Ok(items) => self.file_items = items,
                Err(e) => self.notice = e.to_string(),
            }
        }
    }

    fn load_summaries(&mut self) {
        match self
            .db
            .as_ref()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.credential_summaries())
        {
            Ok(items) => self.credentials = items,
            Err(e) => self.notice = format!("读取账号条目失败：{e}"),
        }
    }

    fn load_inventory(&mut self) {
        match self
            .db
            .as_ref()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.inventory_items())
        {
            Ok(items) => self.inventory_items = items,
            Err(error) => self.notice = format!("读取物品清单失败：{error}"),
        }
    }

    fn load_websites(&mut self) {
        if let Some(db) = self.db.as_ref() {
            match db.websites() {
                Ok(items) => self.website_items = items,
                Err(error) => self.notice = format!("读取网站失败：{error}"),
            }
        }
    }

    fn load_tags(&mut self) {
        if let Some(db) = self.db.as_ref() {
            match (db.tags(TagScope::Credentials), db.tags(TagScope::Inventory)) {
                (Ok(accounts), Ok(inventory)) => {
                    self.account_tags = accounts;
                    self.inventory_tags = inventory;
                }
                _ => self.notice = "标签读取失败，请稍后重试".into(),
            }
        }
    }

    fn open_tag_manager(&mut self, scope: TagScope) {
        self.tag_manager = Some(scope);
        self.tag_name.clear();
        self.tag_original = None;
        self.tag_error.clear();
    }

    fn sync_tag_edit(&mut self, scope: TagScope, original: &str, replacement: Option<&str>) {
        let rewrite = |values: &mut Vec<String>| {
            for value in values.iter_mut() {
                if value == original
                    && let Some(name) = replacement
                {
                    *value = name.to_owned();
                }
            }
            if replacement.is_none() {
                values.retain(|value| value != original);
            }
        };
        let filter = match scope {
            TagScope::Credentials => {
                rewrite(&mut self.credential_draft.tags);
                &mut self.account_filter
            }
            TagScope::Inventory => {
                rewrite(&mut self.inventory_draft.tags);
                &mut self.inventory_filter
            }
            _ => self.module_filters.entry(scope.key().into()).or_default(),
        };
        if *filter == TagFilter::Named(original.to_owned()) {
            *filter = replacement
                .map(|name| TagFilter::Named(name.to_owned()))
                .unwrap_or_default();
        }
        if let Some(assignment) = self.tag_assignment.as_mut()
            && assignment.scope == scope
        {
            rewrite(&mut assignment.selected);
        }
        if scope == TagScope::Contacts {
            rewrite(&mut self.contact_draft_tags);
        }
        if scope == TagScope::Inventory {
            for draft in self.inline_inventory.values_mut() {
                rewrite(&mut draft.tags);
                draft.category = draft.tags.join(" · ");
            }
        }
        if scope == TagScope::Books {
            if let Some(book) = self.library.book.as_mut() {
                rewrite(&mut book.tags);
            }
            if let Some(book) = self.library.dialog.as_mut() {
                rewrite(&mut book.tags);
            }
        }
        if scope == TagScope::BookPages {
            if let Some(page) = self.library.page.as_mut() {
                rewrite(&mut page.tags);
            }
            if self.library.page_filter == TagFilter::Named(original.to_owned()) {
                self.library.page_filter = replacement
                    .map(|name| TagFilter::Named(name.into()))
                    .unwrap_or_default();
            }
        }
        self.load_tags();
        self.load_summaries();
        self.load_inventory();
    }

    fn render_tag_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut assignment) = self.tag_assignment.clone() {
            let names = self.names_for_scope(assignment.scope);
            let mut save = false;
            let dialog = centered_dialog(
                ctx,
                "assign-tags",
                "设置标签",
                "选择分类后保存，清空选择后归入未分类；不会删除原记录。",
                460.0,
                |ui| {
                    if tag_choices(ui, &mut assignment.selected, &names) {
                        self.open_tag_manager(assignment.scope);
                    }
                    if !self.tag_error.is_empty() {
                        ui.colored_label(Color32::from_rgb(184, 65, 71), &self.tag_error);
                    }
                    save = ui
                        .add_sized(
                            [ui.available_width(), 38.0],
                            primary_button(if assignment.scope == TagScope::Inventory {
                                "保存分类"
                            } else {
                                "保存标签"
                            }),
                        )
                        .clicked();
                },
            );
            let mut close = dialog_closed(&dialog);
            if save {
                let result = self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| {
                        db.set_record_tags(assignment.scope, &assignment.id, &assignment.selected)
                    });
                match result {
                    Ok(()) => {
                        if assignment.scope == TagScope::Inventory
                            && self.inventory_draft.id == assignment.id
                        {
                            self.inventory_draft.tags = assignment.selected.clone();
                            self.inventory_draft.category = assignment.selected.join(" · ");
                        }
                        if assignment.scope == TagScope::Credentials
                            && self.credential_draft.id == assignment.id
                        {
                            self.credential_draft.tags = assignment.selected.clone();
                        }
                        if assignment.scope == TagScope::Contacts
                            && self.contact_draft.id == assignment.id
                        {
                            self.contact_draft_tags = assignment.selected.clone();
                        }
                        if assignment.scope == TagScope::Inventory
                            && let Some(draft) = self.inline_inventory.get_mut(&assignment.id)
                        {
                            draft.tags = assignment.selected.clone();
                            draft.category = draft.tags.join(" · ");
                        }
                        self.load_tags();
                        self.load_summaries();
                        self.load_inventory();
                        self.notice = "标签已保存".into();
                        close = true;
                    }
                    Err(error) => self.tag_error = error.to_string(),
                }
            }
            self.tag_assignment = (!close).then_some(assignment);
        }
        if let Some(scope) = self.tag_manager {
            let names = self.names_for_scope(scope);
            let mut submit = false;
            let mut delete = None;
            let dialog = centered_dialog(
                ctx,
                "manage-tags",
                &format!("{}标签", scope.label()),
                "创建自己的分类；删除标签只会移除分类，不会删除卡片。",
                480.0,
                |ui| {
                    ui.label(if self.tag_original.is_some() {
                        "重命名标签"
                    } else {
                        "新增标签"
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.tag_name)
                            .hint_text("例如：工作、游戏、学习"),
                    );
                    ui.horizontal(|ui| {
                        submit = ui
                            .add(primary_button(if self.tag_original.is_some() {
                                "保存名称"
                            } else {
                                "创建标签"
                            }))
                            .clicked();
                        if self.tag_original.is_some() && ui.button("取消重命名").clicked() {
                            self.tag_original = None;
                            self.tag_name.clear();
                        }
                    });
                    if !self.tag_error.is_empty() {
                        ui.colored_label(Color32::from_rgb(184, 65, 71), &self.tag_error);
                    }
                    ui.separator();
                    if names.is_empty() {
                        ui.label("暂无标签，先创建一个分类吧。");
                    }
                    for name in names {
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - 170.0).max(40.0);
                            ui.add_sized([width, 30.0], egui::Label::new(&name).truncate())
                                .on_hover_text(&name);
                            if ui.button("重命名").clicked() {
                                self.tag_name = name.clone();
                                self.tag_original = Some(name.clone());
                                self.tag_error.clear();
                            }
                            if ui.button("删除标签").clicked() {
                                delete = Some(name);
                            }
                        });
                    }
                },
            );
            if submit {
                let result = tags::name(&self.tag_name).and_then(|name| {
                    let db = self.db.as_mut().ok_or(DeskError::UnsupportedFormat)?;
                    if let Some(original) = self.tag_original.as_ref() {
                        db.rename_tag(scope, original, &name)?;
                    } else {
                        db.create_tag(scope, &name)?;
                    }
                    Ok(name)
                });
                match result {
                    Ok(name) => {
                        if let Some(original) = self.tag_original.take() {
                            self.sync_tag_edit(scope, &original, Some(&name));
                        } else {
                            if let Some(assignment) = self.tag_assignment.as_mut()
                                && assignment.scope == scope
                            {
                                assignment.selected.push(name.clone());
                            }
                            if scope == TagScope::Credentials && self.credential_dialog_open {
                                self.credential_draft.tags.push(name.clone());
                            }
                            if scope == TagScope::Inventory && self.inventory_dialog_open {
                                self.inventory_draft.tags.push(name.clone());
                            }
                            self.load_tags();
                        }
                        self.tag_name.clear();
                        self.tag_error.clear();
                        self.notice = "标签已保存".into();
                    }
                    Err(error) => self.tag_error = error.to_string(),
                }
            }
            if let Some(name) = delete {
                match self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.delete_tag(scope, &name))
                {
                    Ok(()) => {
                        self.sync_tag_edit(scope, &name, None);
                        self.tag_error.clear();
                        if self.tag_original.as_deref() == Some(&name) {
                            self.tag_original = None;
                            self.tag_name.clear();
                        }
                        self.notice = "标签已删除，卡片仍保留".into();
                    }
                    Err(error) => self.tag_error = error.to_string(),
                }
            }
            if dialog_closed(&dialog) {
                self.tag_manager = None;
            }
        }
        if self.drop_report.is_some() {
            let report = self.drop_report.as_ref().expect("drop report");
            let dialog = centered_dialog(
                ctx,
                "drop-results",
                "拖入结果",
                "成功复制的项目已保留；以下项目未能复制，源文件均未移动。",
                520.0,
                |ui| {
                    ui.label(format!(
                        "已复制 {} 项，未成功 {} 项",
                        report.copied.len(),
                        report.failures.len()
                    ));
                    for (path, reason) in &report.failures {
                        ui.label(path.display().to_string());
                        ui.colored_label(Color32::from_rgb(184, 65, 71), reason);
                        ui.separator();
                    }
                },
            );
            if dialog_closed(&dialog) {
                self.drop_report = None;
            }
        }
    }

    fn save_website_draft(&mut self) -> bool {
        let result = websites::prepare(self.website_draft.clone()).and_then(|item| {
            self.db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)?
                .save_website_classified(&item, &self.website_draft_tags)?;
            self.website_draft = item;
            Ok(())
        });
        match result {
            Ok(()) => {
                self.load_websites();
                self.notice = "网站已保存".into();
                self.website_error.clear();
                true
            }
            Err(error) => {
                self.website_error = error.to_string();
                false
            }
        }
    }

    fn start_export(&mut self, scope: export::Scope) -> bool {
        if self.export_busy {
            return false;
        }
        if matches!(scope, export::Scope::All | export::Scope::Documents)
            && self.editor_dirty
            && !self.save_editor()
        {
            return false;
        }
        let filename = format!(
            "个人办公台-{}-{}.zip",
            scope.label(),
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        );
        let Some(mut path) = rfd::FileDialog::new()
            .set_title("导出办公台数据")
            .set_file_name(&filename)
            .add_filter("ZIP 压缩文件", &["zip"])
            .save_file()
        else {
            return false;
        };
        if path
            .extension()
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("zip"))
        {
            path.set_extension("zip");
        }
        let data_dir = self.data_dir.clone();
        let sender = self.sender.clone();
        self.export_busy = true;
        self.notice = format!("正在导出{}…", scope.label());
        thread::spawn(move || {
            let report = export::write(&data_dir, &path, scope).map_err(|error| error.to_string());
            let _ = sender.send(BackgroundMessage::Exported(report));
        });
        true
    }

    fn save_inventory_item(&mut self) -> bool {
        self.inventory_draft.category = self.inventory_draft.tags.join(" · ");
        if self.inventory_draft.name.trim().is_empty() {
            self.notice = "请先填写物品名称".into();
            return false;
        }
        let result = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.save_inventory_item(&self.inventory_draft));
        match result {
            Ok(()) => {
                self.notice = format!("已保存物品：{}", self.inventory_draft.name.trim());
                self.inline_inventory.remove(&self.inventory_draft.id);
                self.selected_inventory = Some(self.inventory_draft.id.clone());
                self.load_inventory();
                self.load_tags();
                true
            }
            Err(error) => {
                self.notice = format!("保存物品失败：{error}");
                false
            }
        }
    }

    fn run_search(&mut self) {
        if self.search_query.trim().is_empty() {
            self.search_results.clear();
            return;
        }
        let Some(db) = self.db.as_ref() else {
            return;
        };
        match search::search(db, &self.search_query) {
            Ok(rows) => self.search_results = rows,
            Err(e) => {
                self.notice = format!("搜索失败：{e}");
                self.search_results.clear();
            }
        }
    }

    fn start_index(&mut self, id: String, root: PathBuf) {
        let data = self.data_dir.clone();
        let tx = self.sender.clone();
        self.notice = "正在建立文件索引…".into();
        thread::spawn(move || {
            let result = search::index_folder_at(&data, &id, &root).map_err(|e| e.to_string());
            let _ = tx.send(BackgroundMessage::Indexed(result));
        });
    }

    fn start_copy(&mut self, source: PathBuf, destination: PathBuf) {
        self.file_operations += 1;
        let tx = self.sender.clone();
        self.notice = "正在复制文件…".into();
        thread::spawn(move || {
            let result = files::copy_path(&source, &destination).map_err(|e| e.to_string());
            let _ = tx.send(BackgroundMessage::Copied(result));
        });
    }

    fn start_move(&mut self, source: PathBuf, destination: PathBuf) {
        self.file_operations += 1;
        let tx = self.sender.clone();
        self.notice = "正在移动文件；跨磁盘时会先复制并校验…".into();
        thread::spawn(move || {
            let result = files::move_path(&source, &destination).map_err(|e| e.to_string());
            let _ = tx.send(BackgroundMessage::Moved(result));
        });
    }

    fn poll_background(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            if matches!(
                &message,
                BackgroundMessage::Copied(_) | BackgroundMessage::Moved(_)
            ) {
                self.file_operations = self.file_operations.saturating_sub(1);
            }
            match message {
                BackgroundMessage::Terminal(directory, result) => {
                    self.terminal_starting = false;
                    match *result {
                        Ok(terminal) => {
                            self.terminal = Some(terminal);
                            self.terminal_directory = directory;
                            self.terminal_error.clear();
                            self.terminal_command_focus = true;
                        }
                        Err(error) => self.terminal_error = error,
                    }
                }
                BackgroundMessage::ContactChats(id, result) => {
                    self.contact_import_busy = false;
                    if self.contact_dialog_open && self.contact_draft.id == id {
                        match result.and_then(|chats| {
                            contacts::append_chats(&mut self.contact_draft, chats)
                                .map_err(|e| e.to_string())
                        }) {
                            Ok(count) => {
                                self.contact_error.clear();
                                self.notice =
                                    format!("已加入 {count} 份聊天记录，保存联系人后生效");
                            }
                            Err(error) => self.contact_error = error,
                        }
                    } else {
                        self.notice = "联系人已关闭，本次导入未写入；原文件保留".into();
                    }
                }
                BackgroundMessage::Dropped(destination, report) => {
                    self.drop_busy = false;
                    if self.current_directory.as_ref().is_some_and(|current| {
                        current.canonicalize().ok() == destination.canonicalize().ok()
                    }) {
                        self.refresh_files();
                    }
                    self.notice = format!(
                        "已复制 {} 项，失败 {} 项",
                        report.copied.len(),
                        report.failures.len()
                    );
                    if !report.failures.is_empty() {
                        self.drop_report = Some(report);
                    }
                }

                BackgroundMessage::Exported(result) => {
                    self.export_busy = false;
                    self.notice = match result {
                        Ok(report) if report.warnings == 0 => {
                            format!("导出完成：{}", report.path.display())
                        }
                        Ok(report) => format!(
                            "导出完成，有 {} 项说明，请查看压缩包内的导出清单：{}",
                            report.warnings,
                            report.path.display()
                        ),
                        Err(error) => format!("导出失败：{error}"),
                    };
                }
                BackgroundMessage::Indexed(Ok(count)) => {
                    self.notice = format!("已索引 {count} 个文件")
                }
                BackgroundMessage::Indexed(Err(error)) => {
                    self.notice = format!("文件索引失败：{error}")
                }
                BackgroundMessage::Copied(Ok(path)) => {
                    self.notice = format!("已复制到 {}", path.display());
                    self.refresh_files();
                }
                BackgroundMessage::Copied(Err(error)) => self.notice = format!("复制失败：{error}"),
                BackgroundMessage::Moved(Ok(path)) => {
                    self.notice = format!("已移动到 {}", path.display());
                    self.refresh_files();
                }
                BackgroundMessage::Moved(Err(error)) => {
                    self.notice = format!("移动失败：{error}");
                    self.refresh_files();
                }
            }
        }
    }

    fn page_title(&self) -> String {
        match self.section {
            Section::Folders => self
                .current_directory
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "快捷路径".into()),
            Section::Browser => "大模型聊天".into(),
            Section::Books => "图书".into(),
            Section::Terminal => "PowerShell".into(),
            Section::Credentials => "账号密码".into(),
            Section::Documents => "文档写作".into(),
            Section::Inventory => "物品清单".into(),
            Section::Websites => "网站收藏".into(),
            Section::Contacts => "联系人".into(),
            Section::Games => "小游戏".into(),
            Section::Settings => "设置".into(),
            Section::Tools => "小工具".into(),
            Section::Codex => "Codex".into(),
        }
    }

    fn render_header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut search_id = None;
        let title = self.page_title();
        ui.horizontal(|ui| {
            ui.set_min_height(42.0);
            ui.add_sized(
                [180.0_f32.min(ui.available_width() * 0.24), 42.0],
                egui::Label::new(RichText::new(&title).size(25.0).strong().color(ink())).truncate(),
            )
            .on_hover_text(&title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !self.add_label().is_empty()
                    && ui
                        .add(primary_button(&format!("+  {}", self.add_label())))
                        .clicked()
                {
                    self.add_current_item();
                }
                if self.section == Section::Folders && ui.button("PowerShell").clicked() {
                    let previous = self.section;
                    self.open_terminal(
                        self.current_directory
                            .clone()
                            .unwrap_or_else(|| self.data_dir.clone()),
                    );
                    self.section = previous;
                    self.terminal_inline = true;
                }
                if (self.undo.available()
                    || (self.section == Section::Books && !self.library.undo.is_empty()))
                    && ui.button("撤回 Ctrl+Z").clicked()
                {
                    self.undo_last_delete();
                }
                if ui
                    .add_enabled(
                        !self.export_busy,
                        egui::Button::new(if self.export_busy {
                            "导出中…"
                        } else {
                            "导出"
                        }),
                    )
                    .clicked()
                {
                    self.export_dialog_open = true;
                }
                if !matches!(
                    self.section,
                    Section::Codex | Section::Games | Section::Terminal | Section::Browser
                ) {
                    self.render_view_switcher(ui);
                }
            });
        });
        if self.section != Section::Codex {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let hint = match self.section {
                    Section::Folders => "搜索文件名、路径或已索引内容  Ctrl+K",
                    Section::Credentials => "搜索名称、账号、密码、备注或标签  Ctrl+K",
                    Section::Documents => "搜索文档标题或已索引内容  Ctrl+K",
                    _ => "搜索当前页面  Ctrl+K",
                };
                let width = (ui.available_width() - 155.0).clamp(100.0, 620.0);
                let response = ui.add_sized(
                    [width, 38.0],
                    egui::TextEdit::singleline(&mut self.search_query)
                        .id_salt("global-search")
                        .margin(egui::Margin::symmetric(12, 10))
                        .hint_text(hint),
                );
                search_id = Some(response.id);
                self.search_field = Some(response.id);
                if response.changed() {
                    self.search_results.clear();
                    self.search_pending = Some(ui.input(|input| input.time) + 0.3);
                    ctx.request_repaint_after(Duration::from_millis(310));
                }
                if ui.button("搜索").clicked()
                    || (response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)))
                {
                    self.search_pending = None;
                    self.run_search();
                }
                if ui
                    .add_enabled(!self.search_query.is_empty(), egui::Button::new("清空"))
                    .clicked()
                {
                    self.search_query.clear();
                    self.search_results.clear();
                    self.search_pending = None;
                }
            });
            if self
                .search_pending
                .is_some_and(|deadline| ctx.input(|input| input.time) >= deadline)
            {
                self.search_pending = None;
                if matches!(self.section, Section::Folders | Section::Documents) {
                    self.run_search();
                }
            }
        }
        if !self.detail_open()
            && !ctx.egui_wants_keyboard_input()
            && !self.terminal_focused
            && matches!(self.section, Section::Folders | Section::Documents)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::C))
        {
            self.copy_file_request = self.selected_file.clone();
        }
        let ctrl_k = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::K));
        let ctrl_n = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N));
        let ctrl_s = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S));
        if ctrl_k
            && !self.detail_open()
            && let Some(id) = search_id
        {
            ctx.memory_mut(|memory| memory.request_focus(id));
        }
        if ctrl_n && !self.detail_open() {
            self.add_current_item();
        }
        if ctrl_s && self.section == Section::Books {
            self.save_library_page();
        }
        if ctrl_s && self.editor_visible {
            self.save_editor();
        }
    }

    fn create_internal_document(&mut self) {
        if self.editor_dirty && !self.save_editor() {
            return;
        }
        match documents::create_managed_document(&self.data_dir) {
            Ok(path) => {
                let result = self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.set_document_title(&path, "未命名文档"));
                if let Err(error) = result {
                    self.notice = format!("文档已创建，但记录保存失败：{error}");
                    return;
                }
                self.open_editor(path);
                self.editor_focus_title = true;
                self.search_query.clear();
                self.search_results.clear();
                self.notice = "可以直接填写标题和正文，修改后会自动保存".into();
            }
            Err(error) => self.notice = format!("创建文档失败：{error}"),
        }
    }

    fn export_word_dialog(&mut self) {
        let handler = WordHandler;
        let Some(path) = rfd::FileDialog::new()
            .set_title("导出 Word 副本")
            .set_file_name("文档.docx")
            .add_filter(handler.label(), &[handler.extension()])
            .save_file()
        else {
            return;
        };
        match handler.export_text(&path, &self.editor_text) {
            Ok(()) => self.notice = format!("已导出 Word 副本：{}", path.display()),
            Err(error) => self.notice = format!("导出失败：{error}"),
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 11, accent());
            for (x, y) in [(10.0, 10.0), (21.0, 10.0), (10.0, 21.0), (21.0, 21.0)] {
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(rect.min + egui::vec2(x, y), egui::vec2(7.0, 7.0)),
                    2,
                    crate::theme::button_text(),
                );
            }
            ui.label(
                RichText::new("个人办公台")
                    .size(18.0)
                    .strong()
                    .color(crate::theme::sidebar_text()),
            );
        });
        ui.add_space(28.0);
        let navigation_spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 6.0;
        egui::ScrollArea::vertical()
            .id_salt("navigation-scroll")
            .max_height((ui.available_height() - 56.0).max(160.0))
            .show(ui, |ui| {
                let mut navigation = vec![
                    (Section::Folders, "快捷路径", TileKind::Folder, None),
                    (
                        Section::Credentials,
                        "账号密码",
                        TileKind::Credential,
                        Some(self.credentials.len()),
                    ),
                    (Section::Documents, "文档写作", TileKind::Document, None),
                    (Section::Books, "图书", TileKind::Document, None),
                    (Section::Browser, "大模型聊天", TileKind::Website, None),
                    (Section::Terminal, "PowerShell", TileKind::Setting, None),
                    (
                        Section::Contacts,
                        "联系人",
                        TileKind::Contact,
                        Some(self.contact_items.len()),
                    ),
                    (
                        Section::Inventory,
                        "物品清单",
                        TileKind::Item,
                        Some(self.inventory_items.len()),
                    ),
                    (
                        Section::Websites,
                        "网站收藏",
                        TileKind::Website,
                        Some(self.website_items.len()),
                    ),
                    (Section::Tools, "小工具", TileKind::Setting, None),
                    (Section::Codex, "Codex", TileKind::Document, None),
                    (Section::Games, "小游戏", TileKind::Game, Some(1)),
                    (Section::Settings, "设置", TileKind::Setting, None),
                ];
                navigation.sort_by_key(|(section, _, _, _)| {
                    !self.is_favorite("modules", views::key(*section))
                });
                for (section, title, kind, count) in navigation {
                    let selected = section == self.section;
                    let label = if self.is_favorite("modules", views::key(section)) {
                        format!("★ {title}")
                    } else {
                        title.to_string()
                    };
                    let response = navigation_item(ui, &label, kind, selected, count);
                    response.context_menu(|ui| {
                        if ui
                            .button(if self.is_favorite("modules", views::key(section)) {
                                "取消收藏"
                            } else {
                                "收藏并置顶"
                            })
                            .clicked()
                        {
                            self.toggle_favorite("modules", views::key(section));
                            ui.close();
                        }
                    });
                    if response.clicked() {
                        if self.editor_dirty && !self.save_editor() {
                            continue;
                        }
                        if !self.save_library_page() {
                            continue;
                        }
                        if self.section == Section::Documents && section != Section::Documents {
                            self.editor_visible = false;
                        }
                        self.section = section;
                        if section == Section::Terminal && self.terminal.is_none() {
                            self.open_terminal(
                                self.current_directory
                                    .clone()
                                    .unwrap_or_else(|| self.data_dir.clone()),
                            );
                        }
                        self.notice.clear();
                        self.selected_file = None;
                        self.search_query.clear();
                        self.search_results.clear();
                        if section == Section::Folders {
                            self.current_directory = None;
                            self.current_root = None;
                        }
                        if section == Section::Credentials {
                            self.load_summaries();
                        }
                        if section == Section::Inventory {
                            self.load_inventory();
                        }
                        if section == Section::Websites {
                            self.load_websites();
                        }
                        if section == Section::Contacts {
                            self.load_contacts();
                        }
                    }
                }
            });
        ui.add_space(12.0);
        if ui
            .button(format!("更新  v{}", crate::updater::VERSION))
            .clicked()
        {
            self.updates.open = true;
        }
        ui.spacing_mut().item_spacing.y = navigation_spacing;
    }

    fn open_shortcut(&mut self, shortcut: Shortcut) {
        let path = shortcut.path;
        if shortcut.is_dir {
            self.navigate(path.clone(), path);
        } else {
            self.request_file_open(path);
        }
    }

    fn file_open_menu(&mut self, ui: &mut egui::Ui, path: &std::path::Path) {
        if ui.button("复制").clicked() {
            self.copy_file_request = Some(path.to_path_buf());
            ui.close();
        }
        if ui.button("用默认程序打开").clicked() {
            self.request_file_open(path.to_path_buf());
            ui.close();
        }
        if ui.button("选择程序打开…").clicked() {
            match files::open_with_target(path) {
                Ok(path) => self.open_with_request = Some(path),
                Err(error) => self.notice = error.to_string(),
            }
            ui.close();
        }
    }

    fn request_file_open(&mut self, path: PathBuf) {
        match path.canonicalize() {
            Ok(path) if path.is_file() => self.system_open_request = Some(path),
            _ => self.notice = "文件不存在或当前无法访问，请刷新后重试".into(),
        }
    }

    fn start_drop(&mut self, paths: Vec<PathBuf>, destination: PathBuf) {
        if self.drop_busy {
            self.notice = "上一批文件仍在复制，请稍后再拖入".into();
            return;
        }
        if paths.is_empty() {
            self.notice = "请从资源管理器拖入本机文件或文件夹".into();
            return;
        }
        let sender = self.sender.clone();
        self.drop_busy = true;
        self.search_query.clear();
        self.search_results.clear();
        self.notice = format!("正在复制 {} 项…", paths.len());
        thread::spawn(move || {
            let report = files::copy_dropped(&paths, &destination);
            let _ = sender.send(BackgroundMessage::Dropped(destination, report));
        });
    }

    fn handle_external_drop(&mut self, ctx: &egui::Context, content: egui::Rect) {
        let (hovered, dropped, fallback_position) = ctx.input_mut(|input| {
            (
                !input.raw.hovered_files.is_empty(),
                std::mem::take(&mut input.raw.dropped_files),
                input.pointer.latest_pos(),
            )
        });
        if !hovered && dropped.is_empty() {
            return;
        }
        let position = self.native_drop_position.or(fallback_position);
        let target = if self.section == Section::Folders
            && !self.detail_open()
            && position.is_some_and(|position| content.contains(position))
        {
            position
                .and_then(|position| {
                    self.folder_drop_targets
                        .iter()
                        .find(|(rect, _)| rect.contains(position))
                })
                .map(|(rect, path)| (*rect, path.clone()))
                .or_else(|| self.current_directory.clone().map(|path| (content, path)))
        } else {
            None
        };
        if hovered {
            // OLE DragOver does not generate normal cursor movement events.
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if hovered && let Some((target_rect, destination)) = &target {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::unique("external-drop-preview"),
            ));
            painter.rect_stroke(
                target_rect.shrink(3.0),
                16,
                egui::Stroke::new(2.0, accent()),
                egui::StrokeKind::Inside,
            );
            let banner = egui::Rect::from_center_size(
                content.center(),
                egui::vec2(300.0_f32.min(content.width() - 24.0), 52.0),
            );
            painter.rect_filled(banner, 14, Color32::from_rgb(237, 233, 255));
            let label = if self.drop_busy {
                "正在复制…".to_owned()
            } else {
                format!(
                    "复制到 {}",
                    destination
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )
            };
            painter.text(
                banner.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(15.0),
                accent(),
            );
        }
        if dropped.is_empty() {
            return;
        }
        let Some((_, destination)) = target else {
            if !self.detail_open() {
                self.notice = "请拖到文件夹上，或先打开目标文件夹".into();
            }
            return;
        };
        let paths = dropped
            .into_iter()
            .map(|file| file.path().to_path_buf())
            .filter(|path| path.is_absolute())
            .collect();
        self.start_drop(paths, destination);
    }

    fn open_path(&mut self, path: PathBuf) {
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                self.notice = format!("无法访问文件：{e}");
                return;
            }
        };
        let path = canonical;
        let ext = path
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "md" | "txt" | "markdown") {
            self.open_editor(path);
        } else if let Err(e) = files::open_in_system(&path) {
            self.notice = e.to_string();
        }
    }

    fn add_label(&self) -> &'static str {
        match self.section {
            Section::Books => "新增图书",
            Section::Browser | Section::Terminal => "",
            Section::Credentials => "添加账号",
            Section::Folders if self.current_directory.is_some() => "新建文件夹",
            Section::Folders => "添加快捷路径",
            Section::Documents => "新建文档",
            Section::Inventory => "添加物品",
            Section::Websites => "添加网站",
            Section::Contacts => "添加联系人",
            Section::Tools => "添加工具",
            Section::Codex => "",
            Section::Settings => "",
            Section::Games => "",
        }
    }

    fn add_current_item(&mut self) {
        match self.section {
            Section::Books => self.library.dialog = Some(crate::library::Book::new()),
            Section::Browser => self.browser.open("https://chat.deepseek.com/"),
            Section::Terminal => self.open_terminal(
                self.current_directory
                    .clone()
                    .unwrap_or_else(|| self.data_dir.clone()),
            ),
            Section::Credentials => {
                self.selected_credential = None;
                self.credential_draft = Credential::new();
                self.credential_dialog_open = true;
            }
            Section::Folders if self.current_directory.is_some() => {
                self.folder_name.clear();
                self.folder_dialog_open = true;
            }
            Section::Folders => self.shortcut_dialog_open = true,
            Section::Documents => self.create_internal_document(),
            Section::Inventory => {
                self.selected_inventory = None;
                self.inventory_draft = InventoryItem::new();
                self.inventory_dialog_open = true;
            }
            Section::Websites => {
                self.website_draft = Website::new();
                self.website_draft_tags.clear();
                self.website_editing = false;
                self.website_error.clear();
                self.website_dialog_open = true;
            }
            Section::Tools => self.add_tool(),
            Section::Codex => {}
            Section::Settings => {}
            Section::Games => {}
            Section::Contacts => {
                self.contact_draft = Contact::new();
                self.contact_error.clear();
                self.contact_chat_open = None;
                self.contact_dialog_open = true;
            }
        }
    }

    fn delete_credential_record(&mut self, id: &str) {
        let result = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.delete_credential(id));
        match result {
            Ok(()) => {
                self.inline_credentials.remove(id);
                if self.selected_credential.as_deref() == Some(id) {
                    self.credential_dialog_open = false;
                    self.selected_credential = None;
                    self.credential_draft = Credential::new();
                }
                self.load_summaries();
                self.notice = "账号记录已删除".into();
            }
            Err(error) => self.notice = format!("删除失败：{error}"),
        }
    }

    fn delete_inventory_record(&mut self, id: &str) {
        let result = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.delete_inventory_item(id));
        match result {
            Ok(()) => {
                self.inline_inventory.remove(id);
                if self.selected_inventory.as_deref() == Some(id) {
                    self.inventory_dialog_open = false;
                    self.selected_inventory = None;
                    self.inventory_draft = InventoryItem::new();
                }
                self.load_inventory();
                self.notice = "物品记录已删除".into();
            }
            Err(error) => self.notice = format!("删除失败：{error}"),
        }
    }

    fn delete_document(&mut self, path: &std::path::Path) {
        let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let editing = self.editor_path.as_ref().is_some_and(|current| {
            current.canonicalize().unwrap_or_else(|_| current.clone()) == normalized
        });
        if editing && self.editor_dirty && !self.save_editor() {
            return;
        }
        let aliases = self
            .db
            .as_ref()
            .and_then(|db| db.documents().ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|record| record.canonicalize().unwrap_or_else(|_| record.clone()) == normalized)
            .collect::<Vec<_>>();
        if path.exists()
            && let Err(error) = self.delete_file_reversible(path)
        {
            self.notice = format!("文档删除失败：{error}");
            return;
        }
        let result = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| {
                for alias in aliases {
                    db.forget_document(&alias)?;
                }
                db.forget_document(path)?;
                db.forget_document(&normalized)
            });
        if let Err(error) = result {
            self.notice = format!("文件已暂存，可撤回；列表更新失败：{error}");
            return;
        }
        if editing {
            self.discard_active_recovery_draft();
            self.editor_path = None;
            self.editor_text.clear();
            self.editor_dirty = false;
            self.editor_visible = false;
        }
        self.search_results.retain(|result| {
            result.path.as_deref().is_none_or(|value| {
                std::path::Path::new(value) != path && std::path::Path::new(value) != normalized
            })
        });
        self.notice = "文档已删除，Ctrl+Z 可恢复".into();
    }

    fn render_credentials(&mut self, ui: &mut egui::Ui) {
        if tag_filter_bar(ui, &mut self.account_filter, &self.account_tags) {
            self.open_tag_manager(TagScope::Credentials);
        }
        let query = self.search_query.trim().to_lowercase();
        let mut visible = self
            .credentials
            .iter()
            .filter(|item| {
                self.account_filter.matches(&item.tags)
                    && matches_query(
                        &query,
                        &[
                            &item.title,
                            &item.username,
                            &item.website,
                            &item.password,
                            &item.notes,
                            &item.tags.join(" "),
                        ],
                    )
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut selected = None;
        let mut delete = None;
        let mut add = false;
        visible.sort_by_key(|item| !self.is_favorite("credentials", &item.id));
        credential_list_header(ui, self.view_mode());
        collection(ui, "credential-gallery", self.view_mode(), |ui, side| {
            for item in &visible {
                let editing = self.inline_credentials.contains_key(&item.id);
                let mut draft = self
                    .inline_credentials
                    .get(&item.id)
                    .unwrap_or(item)
                    .clone();
                let response = credential_entry(ui, &mut draft, side, editing, &self.account_tags);
                self.favorite_control(ui, &response.response, "credentials", &item.id);
                if response.classification_changed {
                    self.save_record_classification(TagScope::Credentials, &item.id, &draft.tags);
                }
                if response.changed {
                    self.inline_credentials.insert(item.id.clone(), draft);
                }
                if response.cancel {
                    self.inline_credentials.remove(&item.id);
                } else if response.save {
                    self.save_inline_credential(&item.id);
                }
                if response.details {
                    selected = Some(item.id.clone());
                }
                response.response.context_menu(|ui| {
                    if ui.button("查看详情").clicked() {
                        selected = Some(item.id.clone());
                        ui.close();
                    }
                    if ui.button("复制账号").clicked() {
                        ui.ctx().copy_text(item.username.clone());
                        ui.close();
                    }
                    if ui.button("复制密码").clicked() {
                        ui.ctx().copy_text(item.password.clone());
                        ui.close();
                    }
                    if ui.button("设置标签").clicked() {
                        self.tag_assignment = Some(TagAssignment {
                            scope: TagScope::Credentials,
                            id: item.id.clone(),
                            selected: item.tags.clone(),
                        });
                        self.tag_error.clear();
                        ui.close();
                    }
                    if ui.button("删除账号").clicked() {
                        delete = Some(item.id.clone());
                        ui.close();
                    }
                });
            }
            if entry(
                ui,
                "add",
                "点击添加账号",
                "保存网站或软件账号",
                TileKind::Add,
                side,
            )
            .clicked()
            {
                add = true;
            }
        });
        if let Some(id) = delete {
            self.delete_credential_record(&id);
        } else if let Some(id) = selected {
            let entry = match self.db.as_ref() {
                Some(db) => db.credential(&id),
                None => return,
            };
            match entry {
                Ok(Some(item)) => {
                    self.credential_draft = item;
                    if let Some(draft) = self.inline_credentials.get(&id) {
                        self.credential_draft.title = draft.title.clone();
                        self.credential_draft.username = draft.username.clone();
                        self.credential_draft.password = draft.password.clone();
                    }
                    self.selected_credential = Some(id);
                    self.credential_dialog_open = true;
                }
                Ok(None) => self.notice = "该账号记录已不存在".into(),
                Err(error) => self.notice = format!("读取账号失败：{error}"),
            }
        }
        if add {
            self.add_current_item();
        }
    }

    fn render_websites(&mut self, ui: &mut egui::Ui) {
        let query = self.search_query.trim().to_lowercase();
        let mut visible = self
            .website_items
            .iter()
            .filter(|item| {
                self.record_matches_filter(TagScope::Websites, &item.id)
                    && matches_query(
                        &query,
                        &[&item.title, &item.url, &item.category, &item.notes],
                    )
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut selected = None;
        let mut delete = None;
        let mut open = None;
        let mut add = false;
        visible.sort_by_key(|item| !self.is_favorite("websites", &item.id));
        collection(ui, "website-gallery", self.view_mode(), |ui, side| {
            for item in &visible {
                let labels = self.record_labels(TagScope::Websites, &item.id);
                let (response, quick_open) = website_entry(ui, item, side, &labels);
                self.favorite_control(ui, &response, "websites", &item.id);
                self.record_classification_dropdown(ui, &response, TagScope::Websites, &item.id);
                if quick_open {
                    open = Some(item.clone());
                }
                if response.clicked() {
                    selected = Some(item.clone());
                }
                response.context_menu(|ui| {
                    self.record_tag_menu(ui, TagScope::Websites, &item.id);
                    if ui.button("打开网站").clicked() {
                        open = Some(item.clone());
                        ui.close();
                    }
                    if ui.button("复制网址").clicked() {
                        ui.ctx().copy_text(item.url.clone());
                        ui.close();
                    }
                    if ui.button("编辑网站").clicked() {
                        selected = Some(item.clone());
                        ui.close();
                    }
                    if ui.button("删除网站").clicked() {
                        delete = Some(item.id.clone());
                        ui.close();
                    }
                });
            }
            add = entry(
                ui,
                "add",
                "点击添加网站",
                "保存网址、分类和备注",
                TileKind::Add,
                side,
            )
            .clicked();
        });
        if let Some(item) = selected {
            self.website_draft_tags = self.record_labels(TagScope::Websites, &item.id);
            self.website_draft = item;
            self.website_editing = true;
            self.website_error.clear();
            self.website_dialog_open = true;
        }
        if let Some(item) = open {
            self.website_open_request = Some(item);
        }
        if let Some(id) = delete
            && let Some(db) = self.db.as_mut()
        {
            match db.delete_website(&id) {
                Ok(()) => {
                    self.load_websites();
                    self.notice = "网站收藏已删除".into();
                }
                Err(error) => self.notice = format!("删除失败：{error}"),
            }
        }
        if add {
            self.add_current_item();
        }
    }

    fn render_inventory(&mut self, ui: &mut egui::Ui) {
        if tag_filter_bar(ui, &mut self.inventory_filter, &self.inventory_tags) {
            self.open_tag_manager(TagScope::Inventory);
        }
        let query = self.search_query.trim().to_lowercase();
        let mut visible = self
            .inventory_items
            .iter()
            .filter(|item| {
                self.inventory_filter.matches(&item.tags)
                    && matches_query(
                        &query,
                        &[
                            &item.name,
                            &item.category,
                            &item.location,
                            &item.notes,
                            &item.tags.join(" "),
                            &item.quantity.to_string(),
                        ],
                    )
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut selected = None;
        let mut delete = None;
        let mut add = false;
        visible.sort_by_key(|item| !self.is_favorite("inventory", &item.id));
        if self.view_mode() == ViewMode::List {
            self.render_inventory_rows(ui, &visible);
            return;
        }
        collection(ui, "inventory-gallery", self.view_mode(), |ui, side| {
            for item in &visible {
                let response = tagged_entry(
                    ui,
                    &item.id,
                    &item.name,
                    (
                        &format!(
                            "{} 件 · {}",
                            item.quantity,
                            if item.tags.is_empty() {
                                "未分类".into()
                            } else {
                                item.tags.join(" · ")
                            }
                        ),
                        &item.tags,
                    ),
                    TileKind::Item,
                    side,
                );
                self.favorite_control(ui, &response, "inventory", &item.id);
                self.record_classification_dropdown(ui, &response, TagScope::Inventory, &item.id);
                if response.clicked() {
                    selected = Some(item.clone());
                }
                response.context_menu(|ui| {
                    if ui.button("设置分类").clicked() {
                        self.tag_assignment = Some(TagAssignment {
                            scope: TagScope::Inventory,
                            id: item.id.clone(),
                            selected: item.tags.clone(),
                        });
                        self.tag_error.clear();
                        ui.close();
                    }
                    if ui.button("删除物品记录").clicked() {
                        delete = Some(item.id.clone());
                        ui.close();
                    }
                });
            }
            if entry(
                ui,
                "add",
                "点击添加物品",
                "记录拥有的物品",
                TileKind::Add,
                side,
            )
            .clicked()
            {
                add = true;
            }
        });
        if let Some(id) = delete {
            self.delete_inventory_record(&id);
        } else if let Some(item) = selected {
            self.selected_inventory = Some(item.id.clone());
            self.inventory_draft = item;
            self.inventory_dialog_open = true;
        }
        if add {
            self.add_current_item();
        }
    }

    fn render_documents(&mut self, ui: &mut egui::Ui) {
        if self.editor_visible {
            self.render_document_editor_inline(ui);
            return;
        }
        let query = self.search_query.trim().to_lowercase();
        let mut records = self
            .db
            .as_ref()
            .and_then(|db| db.document_records().ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|record| {
                matches_query(&query, &[&record.title, &record.path.to_string_lossy()])
            })
            .collect::<Vec<_>>();
        let mut seen = std::collections::HashSet::new();
        records.retain(|record| {
            seen.insert(
                record
                    .path
                    .canonicalize()
                    .unwrap_or_else(|_| record.path.clone()),
            )
        });
        for result in self
            .search_results
            .iter()
            .filter(|result| result.category == "已索引文件")
        {
            if let Some(path) = result.path.as_ref().map(PathBuf::from)
                && matches!(
                    path.extension()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .as_str(),
                    "md" | "txt" | "markdown" | "docx"
                )
                && !records.iter().any(|current| {
                    current
                        .path
                        .canonicalize()
                        .unwrap_or_else(|_| current.path.clone())
                        == path.canonicalize().unwrap_or_else(|_| path.clone())
                })
            {
                let title = result.title.clone();
                records.push(DocumentRecord { path, title });
            }
        }
        let mut selected = None;
        let mut delete = None;
        let mut restore = None;
        let mut discard = None;
        let mut add = false;
        records.sort_by_key(|item| !self.is_favorite("documents", &item.path.to_string_lossy()));
        collection(ui, "documents-gallery", self.view_mode(), |ui, side| {
            for record in &records {
                let path = &record.path;
                let id = path.to_string_lossy();
                if !self.record_matches_filter(TagScope::Documents, &id) {
                    continue;
                }
                let labels = self.record_labels(TagScope::Documents, &id);
                let preview = std::fs::File::open(path)
                    .ok()
                    .and_then(|file| {
                        use std::io::Read;
                        let mut bytes = vec![];
                        file.take(2048).read_to_end(&mut bytes).ok()?;
                        String::from_utf8(bytes).ok()
                    })
                    .unwrap_or_default()
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .take(4)
                    .collect::<Vec<_>>()
                    .join(" ");
                let response = crate::ui_components::document_entry(
                    ui,
                    &id,
                    &record.title,
                    &preview,
                    &labels,
                    side.with_file_controls(),
                );
                self.favorite_control(ui, &response, "documents", &id);
                self.record_classification_dropdown(ui, &response, TagScope::Documents, &id);
                self.file_transfer_controls(ui, &response, path);
                let pointer = ui.input(|input| {
                    (
                        input.pointer.primary_clicked(),
                        input.pointer.secondary_clicked(),
                        input.pointer.interact_pos(),
                    )
                });
                let layer_matches = pointer
                    .2
                    .is_some_and(|at| ui.ctx().layer_id_at(at) == Some(response.layer_id));
                let primary = response.enabled()
                    && !self.detail_open()
                    && layer_matches
                    && ui.input(|input| {
                        input.pointer.primary_clicked()
                            && input.pointer.interact_pos().is_some_and(|at| {
                                response.rect.intersect(ui.clip_rect()).contains(at)
                                    && if self.view_mode() == ViewMode::Grid {
                                        at.y >= response.rect.top() + 60.0
                                            && !(at.x > response.rect.right() - 40.0
                                                && at.y > response.rect.bottom() - 40.0)
                                    } else {
                                        at.x > response.rect.left() + 24.0
                                            && at.x
                                                < response.rect.left() + response.rect.width() * 0.5
                                    }
                            })
                    });
                if response.clicked() || primary {
                    self.selected_file = Some(path.clone());
                    selected = Some(path.clone());
                }
                let secondary = response.enabled()
                    && !self.detail_open()
                    && pointer.1
                    && layer_matches
                    && pointer
                        .2
                        .is_some_and(|at| response.rect.intersect(ui.clip_rect()).contains(at));
                egui::Popup::context_menu(&response)
                    .open_memory(secondary.then_some(egui::SetOpenCommand::Bool(true)))
                    .show(|ui| {
                        self.record_tag_menu(ui, TagScope::Documents, &id);
                        self.file_open_menu(ui, path);
                        if ui.button("删除文档（可撤回）").clicked() {
                            delete = Some(path.clone());
                            ui.close();
                        }
                    });
            }
            for item in &self.recovery_items {
                if !matches_query(&query, &[&item.original_path]) {
                    continue;
                }
                let response = entry(
                    ui,
                    &item.id,
                    "恢复未保存的草稿",
                    &item.original_path,
                    TileKind::Document,
                    side,
                );
                if response.clicked() {
                    restore = Some(item.clone());
                }
                response.context_menu(|ui| {
                    if ui.button("删除恢复草稿").clicked() {
                        discard = Some(item.id.clone());
                        ui.close();
                    }
                });
            }
            if entry(
                ui,
                "add",
                "点击添加文档",
                "直接在程序内开始写作",
                TileKind::Add,
                side,
            )
            .clicked()
            {
                add = true;
            }
        });
        if let Some(path) = delete {
            self.delete_document(&path);
        } else if let Some(path) = selected {
            self.open_path(path);
        }
        if let Some(item) = restore {
            self.restore_recovery_draft(item);
        }
        if let Some(id) = discard {
            self.discard_recovery_draft(&id);
        }
        if add {
            self.add_current_item();
        }
    }

    fn render_folder_toolbar(&mut self, ui: &mut egui::Ui) {
        if let (Some(root), Some(current)) =
            (self.current_root.clone(), self.current_directory.clone())
        {
            ui.horizontal_wrapped(|ui| {
                if ui.button("所有快捷路径").clicked() {
                    self.current_root = None;
                    self.current_directory = None;
                    self.search_query.clear();
                    self.search_results.clear();
                }
                if ui
                    .add_enabled(current != root, egui::Button::new("上一级"))
                    .clicked()
                    && let Some(parent) = current.parent()
                {
                    self.navigate(root.clone(), parent.to_path_buf());
                }
                if ui.button("添加文件").clicked()
                    && let Some(paths) = rfd::FileDialog::new().set_title("添加文件").pick_files()
                {
                    self.start_drop(paths, current.clone());
                }
                if ui.button("刷新").clicked() {
                    self.refresh_files();
                }
                if ui.button("在资源管理器打开").clicked()
                    && let Err(error) = files::open_in_system(&current)
                {
                    self.notice = error.to_string();
                }
                if ui.button("更新内容索引").clicked() {
                    let shortcut = self
                        .db
                        .as_ref()
                        .and_then(|db| db.shortcuts().ok())
                        .unwrap_or_default()
                        .into_iter()
                        .find(|shortcut| {
                            shortcut.path.canonicalize().is_ok_and(|path| path == root)
                        });
                    if let Some(shortcut) = shortcut {
                        self.start_index(shortcut.id, root.clone());
                    }
                }
            });
            let location = files::shell_path(&current).display().to_string();
            ui.add(egui::Label::new(RichText::new(&location).small().color(muted())).truncate())
                .on_hover_text(location);
            ui.add_space(12.0);
        }
    }

    fn render_folders(&mut self, ui: &mut egui::Ui) {
        if let (Some(root), Some(current)) =
            (self.current_root.clone(), self.current_directory.clone())
        {
            let query = self.search_query.trim().to_lowercase();
            let mut items = self
                .file_items
                .iter()
                .filter(|item| matches_query(&query, &[&item.name, &item.path.to_string_lossy()]))
                .cloned()
                .collect::<Vec<_>>();
            for result in self
                .search_results
                .iter()
                .filter(|result| result.category == "已索引文件")
            {
                if let Some(path) = result
                    .path
                    .as_ref()
                    .map(PathBuf::from)
                    .map(|path| path.canonicalize().unwrap_or(path))
                    && path.starts_with(&current)
                    && !items.iter().any(|item| item.path == path)
                {
                    items.push(FileItem {
                        name: result.title.clone(),
                        path,
                        is_dir: false,
                        size: 0,
                        modified: String::new(),
                    });
                }
            }
            items.retain(|item| self.file_filter.matches(&item.path, item.is_dir));
            items.sort_by_key(|item| !self.is_favorite("files", &item.path.to_string_lossy()));
            let mut selected = None;
            let mut delete = None;
            let mut add = false;
            file_list_header(ui, self.view_mode());
            collection(ui, "folder-contents", self.view_mode(), |ui, side| {
                for item in &items {
                    let response = file_entry(ui, item, side);
                    self.favorite_control(ui, &response, "files", &item.path.to_string_lossy());
                    self.file_transfer_controls(ui, &response, &item.path);
                    if item.is_dir {
                        self.folder_drop_targets
                            .push((response.rect.intersect(ui.clip_rect()), item.path.clone()));
                    }
                    if response.clicked() && !item.is_dir {
                        self.selected_file = Some(item.path.clone());
                    }
                    if (item.is_dir && response.clicked())
                        || (!item.is_dir && response.double_clicked())
                    {
                        selected = Some(item.clone());
                    }
                    if self.selected_file.as_ref() == Some(&item.path) {
                        ui.painter().rect_stroke(
                            response.rect,
                            16,
                            egui::Stroke::new(2.0, accent()),
                            egui::StrokeKind::Inside,
                        );
                    }
                    response.context_menu(|ui| {
                        if !item.is_dir {
                            self.file_open_menu(ui, &item.path);
                        }
                        if ui.button("重命名").clicked() {
                            self.selected_file = Some(item.path.clone());
                            self.rename_text = item.name.clone();
                            self.file_dialog_open = true;
                            ui.close();
                        }
                        if ui.button("复制到其他文件夹").clicked()
                            && let Some(destination) = files::choose_folder()
                        {
                            self.start_copy(item.path.clone(), destination);
                            ui.close();
                        }
                        if ui.button("移动到其他文件夹").clicked()
                            && let Some(destination) = files::choose_folder()
                        {
                            self.start_move(item.path.clone(), destination);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("删除（可撤回）").clicked() {
                            delete = Some(item.path.clone());
                            ui.close();
                        }
                    });
                }
                if entry(
                    ui,
                    "add",
                    "点击添加文件夹",
                    "在当前目录新建文件夹",
                    TileKind::Add,
                    side,
                )
                .clicked()
                {
                    add = true;
                }
            });
            if let Some(path) = delete {
                if path.is_file() {
                    self.delete_document(&path);
                    self.refresh_files();
                } else {
                    match self.delete_file_reversible(&path) {
                        Ok(()) => {
                            self.refresh_files();
                            self.search_results.clear();
                            self.notice = "已删除，Ctrl+Z 可恢复".into();
                        }
                        Err(error) => self.notice = format!("删除失败：{error}"),
                    }
                }
            } else if let Some(item) = selected {
                if item.is_dir {
                    self.navigate(root, item.path);
                } else {
                    self.request_file_open(item.path);
                }
            }
            if add {
                self.add_current_item();
            }
        } else {
            let query = self.search_query.trim().to_lowercase();
            let mut shortcuts = self
                .db
                .as_ref()
                .and_then(|db| db.shortcuts().ok())
                .unwrap_or_default()
                .into_iter()
                .filter(|item| {
                    self.record_matches_filter(TagScope::Shortcuts, &item.id)
                        && matches_query(&query, &[&item.name, &item.path.to_string_lossy()])
                })
                .collect::<Vec<_>>();
            let mut selected = None;
            let mut delete = None;
            let mut open_file = None;
            let mut delete_file = None;
            let mut add = false;
            shortcuts.sort_by_key(|item| !self.is_favorite("shortcuts", &item.id));
            collection(ui, "shortcut-gallery", self.view_mode(), |ui, side| {
                for shortcut in &shortcuts {
                    let detail = if shortcut.path.exists() {
                        files::shell_path(&shortcut.path).display().to_string()
                    } else {
                        "路径已失效 · 右键可删除入口".into()
                    };
                    let labels = self.record_labels(TagScope::Shortcuts, &shortcut.id);
                    let response = tagged_entry(
                        ui,
                        &shortcut.id,
                        &shortcut.name,
                        (&detail, &labels),
                        if shortcut.is_dir {
                            TileKind::Folder
                        } else {
                            TileKind::Document
                        },
                        side.with_file_controls(),
                    );
                    self.favorite_control(ui, &response, "shortcuts", &shortcut.id);
                    self.file_transfer_controls(ui, &response, &shortcut.path);
                    if shortcut.is_dir {
                        self.folder_drop_targets.push((
                            response.rect.intersect(ui.clip_rect()),
                            shortcut.path.clone(),
                        ));
                    }
                    if response.clicked() && !shortcut.is_dir {
                        self.selected_file = Some(shortcut.path.clone());
                    }
                    if (shortcut.is_dir && response.clicked())
                        || (!shortcut.is_dir && response.double_clicked())
                    {
                        selected = Some(shortcut.clone());
                    }
                    response.context_menu(|ui| {
                        if !shortcut.is_dir {
                            self.file_open_menu(ui, &shortcut.path);
                        }
                        self.record_tag_menu(ui, TagScope::Shortcuts, &shortcut.id);
                        if shortcut.is_dir && ui.button("复制").clicked() {
                            self.copy_file_request = Some(shortcut.path.clone());
                            ui.close();
                        }
                        if ui.button("删除快捷入口").clicked() {
                            delete = Some(shortcut.id.clone());
                            ui.close();
                        }
                    });
                }
                for result in self
                    .search_results
                    .iter()
                    .filter(|result| result.category == "已索引文件")
                    .cloned()
                    .collect::<Vec<_>>()
                {
                    if let Some(path) = &result.path {
                        if self
                            .module_filters
                            .get(TagScope::Shortcuts.key())
                            .is_some_and(|f| matches!(f, TagFilter::Named(_)))
                        {
                            continue;
                        }
                        if shortcuts
                            .iter()
                            .any(|shortcut| shortcut.path == std::path::Path::new(path))
                        {
                            continue;
                        }
                        let response = entry(
                            ui,
                            path,
                            &result.title,
                            &result.detail,
                            TileKind::Document,
                            side,
                        );
                        self.file_transfer_controls(ui, &response, std::path::Path::new(path));
                        if response.clicked() {
                            self.selected_file = Some(PathBuf::from(path));
                        }
                        if response.double_clicked() {
                            open_file = Some(PathBuf::from(path));
                        }
                        response.context_menu(|ui| {
                            self.file_open_menu(ui, std::path::Path::new(path));
                            if ui.button("删除文件（可撤回）").clicked() {
                                delete_file = Some(PathBuf::from(path));
                                ui.close();
                            }
                        });
                    }
                }
                if entry(
                    ui,
                    "add",
                    "点击添加快捷路径",
                    "选择本机文件夹或文件",
                    TileKind::Add,
                    side,
                )
                .clicked()
                {
                    add = true;
                }
            });
            if let Some(id) = delete {
                let result = self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.remove_shortcut(&id));
                self.notice = match result {
                    Ok(()) => "快捷入口已删除，原始文件仍保留".into(),
                    Err(error) => format!("删除失败：{error}"),
                };
            } else if let Some(shortcut) = selected {
                self.open_shortcut(shortcut);
            }
            if let Some(path) = open_file {
                self.request_file_open(path);
            }
            if let Some(path) = delete_file {
                self.delete_document(&path);
            }
            if add {
                self.add_current_item();
            }
        }
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let mut enabled = crate::autostart::enabled();
            if ui.checkbox(&mut enabled, "开机自启动").changed() {
                match std::env::current_exe()
                    .map_err(|e| e.to_string())
                    .and_then(|path| crate::autostart::set(enabled, &path))
                {
                    Ok(()) => {
                        self.notice = if enabled {
                            "已开启开机自启动".into()
                        } else {
                            "已关闭开机自启动".into()
                        }
                    }
                    Err(error) => self.notice = error,
                }
            }
            if ui
                .add_enabled(self.undo.available(), egui::Button::new("撤回最近删除"))
                .clicked()
            {
                self.undo_last_delete();
            }
            if ui.button("系统回收站").clicked() {
                let _ = open::that_detached("shell:RecycleBinFolder");
            }
        });
        ui.add_space(16.0);
        collection(ui, "settings-gallery", self.view_mode(), |ui, side| {
            if entry(
                ui,
                "edit-theme",
                "编辑主题",
                "颜色、背景图片与透明度",
                TileKind::Setting,
                side,
            )
            .clicked()
            {
                self.theme_editor_open = true;
                self.theme_error.clear();
            }
            if entry(
                ui,
                "export-all",
                "导出全部数据",
                "账号、联系人、聊天与文档",
                TileKind::Setting,
                side,
            )
            .clicked()
            {
                self.export_dialog_open = true;
            }
            if entry(
                ui,
                "data",
                "本地数据",
                "查看数据保存位置",
                TileKind::Setting,
                side,
            )
            .clicked()
            {
                self.settings_detail = Some("本地数据".into());
            }
        });
    }

    fn view_mode(&self) -> ViewMode {
        self.view_modes.mode(self.section)
    }

    fn set_view_mode(&mut self, mode: ViewMode) {
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.set_setting(views::key(self.section), mode.value()))
        {
            Ok(()) => self.view_modes.set(self.section, mode),
            Err(error) => self.notice = format!("视图未保存：{error}"),
        }
    }

    fn render_view_switcher(&mut self, ui: &mut egui::Ui) {
        let mut mode = self.view_mode();
        if view_switcher(ui, &mut mode) {
            self.set_view_mode(mode);
        }
    }

    fn render_workspace(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.detail_open()
            && !self.terminal_focused
            && !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Z))
        {
            self.undo_last_delete();
        }
        self.folder_drop_targets.clear();
        self.paint_theme_background(ui);
        if self.db.is_some() {
            egui::Panel::left("navigation")
                .resizable(false)
                .show_separator_line(false)
                .exact_size(204.0)
                .frame(
                    egui::Frame::default()
                        .fill(sidebar())
                        .stroke(egui::Stroke::NONE)
                        .inner_margin(egui::Margin::symmetric(16, 18)),
                )
                .show(ui, |ui| self.render_sidebar(ui));
        }
        if self.db.is_some() && self.section == Section::Codex && !self.desktop.basic_chat {
            egui::CentralPanel::default()
                .frame(egui::Frame::default().fill(content_fill()).inner_margin(8))
                .show(ui, |ui| self.render_codex(ui));
            self.render_detail_windows(&ctx);
            return;
        }
        if self.db.is_none() {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.heading("无法打开本地数据");
                ui.label(&self.startup_error);
            });
            return;
        }
        let content_panel = egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(content_fill())
                    .stroke(egui::Stroke::NONE)
                    .inner_margin(egui::Margin::symmetric(28, 24)),
            )
            .show(ui, |ui| {
                if self.section == Section::Codex {
                    self.render_codex(ui);
                    return;
                }
                if self.section == Section::Browser {
                    self.render_browser(ui);
                    return;
                }
                if self.section == Section::Terminal {
                    self.render_terminal(ui);
                    return;
                }
                if self.section == Section::Folders && self.terminal_inline {
                    egui::Panel::bottom("folder-terminal")
                        .resizable(true)
                        .default_size(260.0)
                        .min_size(180.0)
                        .show(ui, |ui| self.render_terminal(ui));
                }
                self.render_header(ui, &ctx);
                ui.add_space(20.0);
                self.render_module_filters(ui);
                if !self.notice.is_empty() {
                    ui.add(
                        egui::Label::new(RichText::new(&self.notice).small().color(muted()))
                            .truncate(),
                    )
                    .on_hover_text(&self.notice);
                    ui.add_space(6.0);
                }
                if self.section == Section::Folders {
                    self.render_folder_toolbar(ui);
                }
                egui::ScrollArea::vertical()
                    .id_salt((
                        "workspace-content",
                        views::key(self.section),
                        self.view_mode().value(),
                    ))
                    .show(ui, |ui| match self.section {
                        Section::Books => self.render_library(ui),
                        Section::Browser => self.render_browser(ui),
                        Section::Terminal => self.render_terminal(ui),
                        Section::Credentials => self.render_credentials(ui),
                        Section::Folders => self.render_folders(ui),
                        Section::Documents => self.render_documents(ui),
                        Section::Inventory => self.render_inventory(ui),
                        Section::Websites => self.render_websites(ui),
                        Section::Contacts => self.render_contacts(ui),
                        Section::Games => self.render_games(ui),
                        Section::Settings => self.render_settings(ui),
                        Section::Tools => self.render_tools(ui),
                        Section::Codex => self.render_codex(ui),
                    });
            });
        self.handle_external_drop(&ctx, content_panel.response.rect);
        self.render_detail_windows(&ctx);
    }

    fn detail_open(&self) -> bool {
        self.theme_editor_open
            || self.tool_draft.is_some()
            || (self.codex.has_request() || self.codex.settings_open)
            || self.credential_dialog_open
            || self.inventory_dialog_open
            || (self.editor_visible && self.section != Section::Documents)
            || self.library.dialog.is_some()
            || self.shortcut_dialog_open
            || self.folder_dialog_open
            || self.file_dialog_open
            || self.settings_detail.is_some()
            || self.updates.open
            || self.website_dialog_open
            || self.export_dialog_open
            || self.tag_manager.is_some()
            || self.tag_assignment.is_some()
            || self.drop_report.is_some()
            || self.contact_dialog_open
            || self.contact_chat_open.is_some()
    }

    fn render_detail_windows(&mut self, ctx: &egui::Context) {
        if self.website_dialog_open {
            let mut save = false;
            let mut browse = false;
            let dialog = centered_dialog(
                ctx,
                "website-details",
                if self.website_editing {
                    "网站详情"
                } else {
                    "添加网站"
                },
                "保存常用网址，需要时一键打开。",
                520.0,
                |ui| {
                    ui.label("名称");
                    ui.text_edit_singleline(&mut self.website_draft.title);
                    ui.label("网址");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.website_draft.url)
                            .hint_text("https://example.com"),
                    );
                    ui.label("分类");
                    let options = self.names_for_scope(TagScope::Websites);
                    crate::ui_components::classification_dropdown(
                        ui,
                        "website-dialog-category",
                        &mut self.website_draft_tags,
                        &options,
                        ui.available_width(),
                    );
                    if ui.button("管理分类").clicked() {
                        self.open_tag_manager(TagScope::Websites);
                    }
                    ui.label("备注");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.website_draft.notes)
                            .desired_width(f32::INFINITY)
                            .desired_rows(3),
                    );
                    if !self.website_error.is_empty() {
                        ui.colored_label(Color32::from_rgb(184, 65, 71), &self.website_error);
                    }
                    ui.horizontal_wrapped(|ui| {
                        save = ui.add(primary_button("保存网站")).clicked();
                        browse = ui.button("打开网站").clicked();
                        if ui.button("复制网址").clicked() {
                            ctx.copy_text(self.website_draft.url.clone());
                        }
                    });
                },
            );
            if dialog_closed(&dialog) || (save && self.save_website_draft()) {
                self.website_dialog_open = false;
            }
            if browse {
                match crate::browser::address(&self.website_draft.url) {
                    Ok(_) => {
                        self.website_open_request = Some(self.website_draft.clone());
                        self.website_dialog_open = false;
                    }
                    Err(error) => self.website_error = error,
                }
            }
        }
        if self.export_dialog_open {
            let scope = export::Scope::for_section(self.section);
            let mut selection = None;
            let dialog = centered_dialog(
                ctx,
                "export-details",
                "导出数据",
                "导出为 ZIP 压缩包，可在其他位置保存和查看。",
                480.0,
                |ui| {
                    if scope != export::Scope::All
                        && ui
                            .add_sized(
                                [ui.available_width(), 40.0],
                                egui::Button::new(format!("导出{}", scope.label())),
                            )
                            .clicked()
                    {
                        selection = Some(scope);
                    }
                    if ui
                        .add_sized([ui.available_width(), 40.0], primary_button("导出全部数据"))
                        .clicked()
                    {
                        selection = Some(export::Scope::All);
                    }
                },
            );
            if dialog_closed(&dialog) {
                self.export_dialog_open = false;
            }
            if let Some(scope) = selection
                && self.start_export(scope)
            {
                self.export_dialog_open = false;
            }
        }
        if self.credential_dialog_open {
            let mut open = true;
            let mut save = false;
            let dialog = centered_dialog(
                ctx,
                "credential-details",
                if self.selected_credential.is_some() {
                    "账号详情"
                } else {
                    "添加账号"
                },
                "直接查看、编辑和复制账号信息。",
                520.0,
                |ui| {
                    ui.style_mut()
                        .text_styles
                        .insert(egui::TextStyle::Body, egui::FontId::proportional(16.0));
                    ui.style_mut()
                        .text_styles
                        .insert(egui::TextStyle::Button, egui::FontId::proportional(16.0));
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.label("名称");
                    ui.text_edit_singleline(&mut self.credential_draft.title);
                    ui.horizontal(|ui| {
                        ui.label("用户名");
                        if ui.small_button("复制账号").clicked() {
                            ctx.copy_text(self.credential_draft.username.clone());
                            self.notice = "账号已复制到剪贴板".into();
                        }
                    });
                    ui.text_edit_singleline(&mut self.credential_draft.username);
                    ui.label("网址或应用");
                    ui.text_edit_singleline(&mut self.credential_draft.website);
                    ui.label("密码");
                    ui.text_edit_singleline(&mut self.credential_draft.password);
                    if ui.button("复制密码").clicked() {
                        ctx.copy_text(self.credential_draft.password.clone());
                        self.notice = "密码已复制到剪贴板".into();
                    }
                    if tag_choices(ui, &mut self.credential_draft.tags, &self.account_tags) {
                        self.open_tag_manager(TagScope::Credentials);
                    }
                    ui.label("备注");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.credential_draft.notes)
                            .desired_width(f32::INFINITY)
                            .desired_rows(2),
                    );
                    if ui
                        .add_sized([ui.available_width(), 38.0], primary_button("保存账号"))
                        .clicked()
                    {
                        save = true;
                    }
                },
            );
            if dialog_closed(&dialog) {
                open = false;
            }
            if save && self.save_credential_draft() {
                open = false;
            }
            self.credential_dialog_open = open;
            if !open {
                self.credential_draft = Credential::new();
                self.selected_credential = None;
            }
        }
        if self.inventory_dialog_open {
            let mut open = true;
            let mut save = false;
            let dialog = centered_dialog(
                ctx,
                "inventory-details",
                if self.selected_inventory.is_some() {
                    "物品详情"
                } else {
                    "添加物品"
                },
                "记下拥有的物品，也记住它们放在哪里。",
                440.0,
                |ui| {
                    if ui.add(primary_button("保存物品")).clicked() {
                        save = true;
                    }
                    ui.label("物品名称");
                    ui.text_edit_singleline(&mut self.inventory_draft.name);
                    ui.columns(2, |columns| {
                        columns[0].label("分类");
                        columns[0].label(if self.inventory_draft.tags.is_empty() {
                            "未分类".into()
                        } else {
                            self.inventory_draft.tags.join(" · ")
                        });
                        columns[1].label("数量");
                        let quantity_width = columns[1].available_width();
                        columns[1].add_sized(
                            [quantity_width, 36.0],
                            egui::DragValue::new(&mut self.inventory_draft.quantity)
                                .range(0..=100_000),
                        );
                    });
                    if tag_choices(ui, &mut self.inventory_draft.tags, &self.inventory_tags) {
                        self.open_tag_manager(TagScope::Inventory);
                    }
                    ui.label("存放位置");
                    ui.text_edit_singleline(&mut self.inventory_draft.location);
                    ui.columns(2, |columns| {
                        columns[0].label("购买日期");
                        columns[0].add(
                            egui::TextEdit::singleline(&mut self.inventory_draft.purchase_date)
                                .hint_text("YYYY-MM-DD"),
                        );
                        let mut filled = self.inventory_draft.purchase_price_cents.is_some();
                        if columns[1].checkbox(&mut filled, "购买价格").changed() {
                            self.inventory_draft.purchase_price_cents = filled.then_some(0);
                        }
                        if let Some(cents) = self.inventory_draft.purchase_price_cents.as_mut() {
                            let mut price = *cents as f64 / 100.0;
                            if columns[1]
                                .add(
                                    egui::DragValue::new(&mut price)
                                        .prefix("¥ ")
                                        .fixed_decimals(2)
                                        .range(0.0..=1_000_000_000_000.0)
                                        .speed(1.0),
                                )
                                .changed()
                            {
                                *cents = (price * 100.0).round() as i64;
                            }
                        }
                    });
                    ui.label("丢弃日期");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.inventory_draft.disposal_date)
                            .hint_text("YYYY-MM-DD"),
                    );
                    ui.label("备注");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.inventory_draft.notes)
                            .desired_width(f32::INFINITY)
                            .desired_rows(3),
                    );
                },
            );
            if dialog_closed(&dialog) {
                open = false;
            }
            if save && self.save_inventory_item() {
                open = false;
            }
            self.inventory_dialog_open = open;
        }
        if self.editor_visible && self.section != Section::Documents {
            let mut open = true;
            let dialog = centered_dialog(
                ctx,
                "document-editor",
                "文档写作",
                "在这里专注记录。修改后自动保存，也可使用 Ctrl+S。",
                820.0,
                |ui| {
                    let title = ui.add(
                        egui::TextEdit::singleline(&mut self.editor_title)
                            .hint_text("给文档起个名字")
                            .font(egui::FontId::proportional(24.0))
                            .frame(egui::Frame::NONE)
                            .margin(egui::Margin::symmetric(0, 10))
                            .desired_width(f32::INFINITY),
                    );
                    if self.editor_focus_title {
                        title.request_focus();
                        self.editor_focus_title = false;
                    }
                    if title.changed() {
                        self.editor_dirty = true;
                        self.dirty_since = Some(Instant::now());
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(!self.editor_conflict, primary_button("保存文档"))
                            .clicked()
                        {
                            self.save_editor();
                        }
                        if ui.button("导出文本副本").clicked() {
                            self.save_editor_as();
                        }
                        if ui.button("导出 Word 副本").clicked() {
                            self.export_word_dialog();
                        }
                        if self.editor_conflict {
                            ui.colored_label(
                                Color32::from_rgb(180, 75, 70),
                                "文件已被外部修改，自动保存已暂停",
                            );
                            if ui.button("重新载入磁盘版本").clicked() {
                                self.reload_editor_from_disk();
                            }
                        } else {
                            ui.label(if self.editor_dirty {
                                "未保存"
                            } else {
                                "已保存"
                            });
                        }
                    });
                    ui.add_space(4.0);
                    egui::Frame::new()
                        .fill(canvas())
                        .corner_radius(12)
                        .inner_margin(16)
                        .show(ui, |ui| {
                            let response = ui.add(
                                egui::TextEdit::multiline(&mut self.editor_text)
                                    .hint_text("在这里开始编写文档……")
                                    .desired_width(f32::INFINITY)
                                    .frame(egui::Frame::NONE)
                                    .font(egui::FontId::proportional(16.0))
                                    .desired_rows(
                                        ((ctx.content_rect().height() - 360.0) / 24.0)
                                            .clamp(8.0, 18.0)
                                            as usize,
                                    ),
                            );
                            if response.changed() {
                                self.editor_dirty = true;
                                self.dirty_since = Some(Instant::now());
                            }
                        });
                },
            );
            if dialog_closed(&dialog) {
                open = false;
            }
            if !open && self.editor_dirty && !self.save_editor() {
                open = true;
            }
            self.editor_visible = open;
        }
        if self.shortcut_dialog_open {
            let mut open = true;
            let mut path = None;
            let dialog = centered_dialog(
                ctx,
                "shortcut-details",
                "添加快捷路径",
                "把常用文件夹和文件放在伸手可及的地方。",
                440.0,
                |ui| {
                    ui.horizontal(|ui| {
                        if ui.add(primary_button("选择文件夹")).clicked() {
                            path = files::choose_folder();
                        }
                        if ui.button("选择文件").clicked() {
                            path = files::choose_file();
                        }
                    });
                },
            );
            if dialog_closed(&dialog) {
                open = false;
            }
            if let Some(path) = path {
                self.add_shortcut(path);
                open = false;
            }
            self.shortcut_dialog_open = open;
        }
        if self.folder_dialog_open {
            let mut open = true;
            let mut create = false;
            let dialog = centered_dialog(
                ctx,
                "new-folder",
                "新建文件夹",
                "在当前目录创建一个新的文件夹。",
                420.0,
                |ui| {
                    ui.label("文件夹名称");
                    ui.text_edit_singleline(&mut self.folder_name);
                    if ui
                        .add_sized([ui.available_width(), 38.0], primary_button("创建文件夹"))
                        .clicked()
                    {
                        create = true;
                    }
                },
            );
            if dialog_closed(&dialog) {
                open = false;
            }
            if create && let Some(parent) = self.current_directory.as_ref() {
                match files::create_folder(parent, &self.folder_name) {
                    Ok(_) => {
                        self.refresh_files();
                        self.notice = "文件夹已创建".into();
                        open = false;
                    }
                    Err(error) => self.notice = format!("创建失败：{error}"),
                }
            }
            self.folder_dialog_open = open;
        }
        if self.file_dialog_open {
            let mut open = true;
            let mut rename = false;
            let dialog = centered_dialog(
                ctx,
                "rename-file",
                "重命名",
                "为当前文件或文件夹设置新的名称。",
                420.0,
                |ui| {
                    ui.label("新的名称");
                    ui.text_edit_singleline(&mut self.rename_text);
                    if ui
                        .add_sized([ui.available_width(), 38.0], primary_button("保存名称"))
                        .clicked()
                    {
                        rename = true;
                    }
                },
            );
            if dialog_closed(&dialog) {
                open = false;
            }
            if rename && let Some(path) = self.selected_file.as_ref() {
                match files::rename_path(path, &self.rename_text) {
                    Ok(_) => {
                        self.refresh_files();
                        open = false;
                    }
                    Err(error) => self.notice = format!("重命名失败：{error}"),
                }
            }
            self.file_dialog_open = open;
        }
        if let Some(title) = self.settings_detail.clone() {
            let mut open = true;
            let dialog = centered_dialog(
                ctx,
                "settings-details",
                &title,
                "按自己的习惯使用个人办公台。",
                460.0,
                |ui| {
                    if title == "本地数据" {
                        ui.label(format!("数据目录：{}", self.data_dir.display()));
                        if ui.button("打开数据目录").clicked()
                            && let Err(error) = files::open_in_system(&self.data_dir)
                        {
                            self.notice = error.to_string();
                        }
                    }
                },
            );
            if dialog_closed(&dialog) {
                open = false;
            }
            if !open {
                self.settings_detail = None;
            }
        }
        self.render_tag_dialogs(ctx);
        self.render_contact_dialogs(ctx);
        self.render_tool_dialog(ctx);
        self.render_theme_dialog(ctx);
        self.render_codex_requests(ctx);
        self.render_update_dialog(ctx);
        self.render_book_dialog(ctx);
    }

    fn save_inline_credential(&mut self, id: &str) -> bool {
        let Some(draft) = self.inline_credentials.get(id) else {
            return false;
        };
        if draft.title.trim().is_empty() {
            self.notice = "请填写账号名称".into();
            return false;
        }
        let result = (|| {
            let db = self.db.as_mut().ok_or(DeskError::UnsupportedFormat)?;
            let Some(mut record) = db.credential(id)? else {
                return Err(DeskError::Database(rusqlite::Error::QueryReturnedNoRows));
            };
            record.title = draft.title.clone();
            record.username = draft.username.clone();
            record.password = draft.password.clone();
            db.save_credential(&record)
        })();
        match result {
            Ok(()) => {
                self.inline_credentials.remove(id);
                self.load_summaries();
                self.notice = "账号已保存".into();
                true
            }
            Err(error) => {
                self.notice = format!("保存账号失败：{error}");
                false
            }
        }
    }

    fn save_credential_draft(&mut self) -> bool {
        if self.credential_draft.title.trim().is_empty() {
            self.notice = "请填写账号名称".into();
            return false;
        }
        let result = match self.db.as_mut() {
            Some(db) => db.save_credential(&self.credential_draft),
            None => return false,
        };
        match result {
            Ok(()) => {
                self.inline_credentials.remove(&self.credential_draft.id);
                self.load_summaries();
                self.load_tags();
                self.notice = "账号已保存".into();
                true
            }
            Err(error) => {
                self.notice = format!("保存账号失败：{error}");
                false
            }
        }
    }
}

pub fn data_directory() -> PathBuf {
    // A local deployment may pin its existing data directory across Windows
    // packaged and unpackaged launches; the file contains a path, never account data.
    let configured = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .map(|parent| parent.join("personal-desk-data-path.txt"))
        })
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|text| PathBuf::from(text.trim().trim_start_matches('\u{feff}')))
        .filter(|path| path.is_absolute());
    configured.unwrap_or_else(|| {
        directories::BaseDirs::new()
            .map(|base| base.data_local_dir().join("PersonalDesk"))
            .unwrap_or_else(|| PathBuf::from("PersonalDesk-data"))
    })
}

impl eframe::App for PersonalDeskApp {
    #[cfg(debug_assertions)]
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        self.probe_input_events(input);
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.editor_dirty && !self.save_editor() {
            self.save_recovery_draft();
        }
        self.save_library_page();
        if let Err(error) = self.desktop.detach() {
            self.notice = error;
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(window) = frame.window_handle()
            && let RawWindowHandle::Win32(window) = window.as_raw()
        {
            self.desktop.bind_parent(window.hwnd.get() as usize);
        }
        let ctx = ui.ctx().clone();
        self.native_drop_position =
            if ctx.input(|i| !i.raw.hovered_files.is_empty() || !i.raw.dropped_files.is_empty()) {
                crate::file_drop::client_position(self.desktop.parent, ctx.pixels_per_point())
            } else {
                None
            };
        self.browser_area = None;
        if self.section != Section::Terminal
            && !(self.section == Section::Folders && self.terminal_inline)
        {
            self.terminal_focused = false;
        }
        self.poll_background();
        if let Some(terminal) = self.terminal.as_mut() {
            terminal.poll();
        }
        if self.library.dirty
            && self
                .library
                .dirty_since
                .is_some_and(|time| time.elapsed() > Duration::from_millis(500))
        {
            self.save_library_page();
        }
        if ctx.input(|input| input.viewport().close_requested()) {
            let mut records_saved = true;
            if self.file_operations > 0
                || self.drop_busy
                || self.export_busy
                || self.contact_import_busy
            {
                records_saved = false;
                self.notice = "当前文件操作完成后即可退出".into();
            }
            let blank_contact = Contact {
                id: self.contact_draft.id.clone(),
                ..Default::default()
            };
            if self.contact_dialog_open
                && (self.contact_draft != blank_contact || !self.contact_draft_tags.is_empty())
            {
                records_saved &= self.save_contact_draft();
            }
            if self.inventory_dialog_open && !self.inventory_draft.name.trim().is_empty() {
                records_saved &= self.save_inventory_item();
            }
            if self.credential_dialog_open && !self.credential_draft.title.trim().is_empty() {
                records_saved &= self.save_credential_draft();
            }
            if self.website_dialog_open
                && (!self.website_draft.url.trim().is_empty()
                    || !self.website_draft.title.trim().is_empty())
            {
                records_saved &= self.save_website_draft();
            }
            for id in self.inline_credentials.keys().cloned().collect::<Vec<_>>() {
                records_saved &= self.save_inline_credential(&id);
            }
            for id in self.inline_inventory.keys().cloned().collect::<Vec<_>>() {
                records_saved &= self.save_inline_inventory(&id);
            }
            let documents_saved = !self.editor_dirty || self.save_editor();
            let pages_saved = self.save_library_page();
            if !records_saved || !documents_saved || !pages_saved {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        self.codex.poll();
        if self.editor_dirty
            && !self.editor_conflict
            && self
                .dirty_since
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(2))
        {
            self.save_editor();
        }
        ctx.request_repaint_after(Duration::from_secs(1));
        self.render_workspace(ui);
        #[cfg(debug_assertions)]
        self.capture_preview(&ctx);
        self.sync_native_codex();
        self.sync_browser(&ctx);
        #[cfg(debug_assertions)]
        self.advance_interaction_probe(&ctx);
        if let Some(path) = self.tool_launch_request.take()
            && let Err(error) = crate::launcher::launch(&path)
        {
            self.notice = error.to_string();
        }
        if let Some(site) = self.website_open_request.take() {
            self.browser.open(&site.url);
            self.section = Section::Browser;
        }
        if let Some(path) = self.system_open_request.take()
            && let Err(error) = files::open_in_system(&path)
        {
            self.notice = format!("无法打开文件：{error}");
        }
        if let Some(path) = self.open_with_request.take()
            && let Err(error) = files::open_with_dialog(&path)
        {
            self.notice = error.to_string();
        }
        if let Some(path) = self.copy_file_request.take() {
            self.notice = match crate::file_transfer::copy(&path) {
                Ok(()) => "已复制".into(),
                Err(error) => error.to_string(),
            };
        }
        if let Some(path) = self.drag_file_request.take() {
            match crate::file_transfer::drag(&path, self.desktop.parent) {
                Ok(true) => {
                    self.notice = "已复制".into();
                    self.refresh_files();
                }
                Ok(false) => {}
                Err(error) => self.notice = error.to_string(),
            }
            // The native OLE loop consumes the release event. Clear egui's drag
            // state so a cancelled transfer cannot leave the file stuck dragging.
            ctx.stop_dragging();
            ctx.input_mut(|input| input.pointer = Default::default());
        }
    }
}

fn matches_query(query: &str, fields: &[&str]) -> bool {
    query.is_empty()
        || fields
            .iter()
            .any(|value| value.to_lowercase().contains(query))
}

fn configure_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    ctx.style_mut_of(egui::Theme::Light, |style| {
        style.visuals = egui::Visuals::light();
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.0));
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 9.0);
        style.spacing.interact_size.y = 36.0;
        style.visuals.panel_fill = canvas();
        style.visuals.window_fill = surface();
        style.visuals.window_corner_radius = egui::CornerRadius::same(14);
        style.visuals.window_stroke = egui::Stroke::new(1.0, border());
        style.visuals.menu_corner_radius = egui::CornerRadius::same(10);
        style.visuals.extreme_bg_color = canvas();
        style.visuals.text_edit_bg_color = Some(Color32::from_rgb(249, 250, 253));
        style.visuals.faint_bg_color = canvas();
        style.visuals.weak_text_color = Some(muted());
        style.visuals.hyperlink_color = accent();
        style.visuals.selection.bg_fill = Color32::from_rgb(223, 218, 250);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, accent());
        style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, ink());
        style.visuals.widgets.noninteractive.bg_fill = surface();
        style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, border());
        style.visuals.widgets.inactive.bg_fill = surface();
        style.visuals.widgets.inactive.weak_bg_fill = surface();
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, ink());
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border());
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(244, 241, 255);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(244, 241, 255);
        style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, accent());
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent());
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(232, 226, 252);
        style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(232, 226, 252);
        style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, accent());
        style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, accent());
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(9);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(9);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(9);
    });
    #[cfg(windows)]
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\msyh.ttc") {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "chinese".into(),
            egui::FontData::from_owned(bytes)
                .tweak(egui::FontTweak {
                    hinting: Some(true),
                    hinting_target: egui::epaint::text::HintingTarget::Smooth(
                        egui::epaint::text::SmoothHinting {
                            light: false,
                            symmetric_rendering: false,
                            preserve_linear_metrics: false,
                        },
                    ),
                    subpixel_binning: Some(false),
                    ..Default::default()
                })
                .into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "chinese".into());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "chinese".into());
        ctx.set_fonts(fonts);
    }
}

#[cfg(test)]
mod editor_tests {
    use super::*;

    #[test]
    fn contact_import_is_persisted_after_save_and_never_applied_to_a_different_contact() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("聊天.xlsx");
        fs::write(&path, contacts::tests::workbook_fixture()).unwrap();
        let mut app = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
        app.section = Section::Contacts;
        app.add_current_item();
        assert!(app.contact_dialog_open);
        app.contact_draft.name = "示例联系人".into();
        app.start_contact_import(vec![path.clone(), path.clone()]);
        assert!(!app.save_contact_draft());
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.contact_import_busy && Instant::now() < deadline {
            app.poll_background();
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!app.contact_import_busy);
        assert_eq!(app.contact_draft.chats.len(), 1);
        assert!(app.save_contact_draft());
        assert_eq!(app.contact_items.len(), 1);
        assert!(app.contact_items[0].matches("下周一起看书"));
        app.start_contact_import(vec![path]);
        app.contact_dialog_open = false;
        app.add_current_item();
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.contact_import_busy && Instant::now() < deadline {
            app.poll_background();
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!app.contact_import_busy);
        assert!(app.contact_draft.chats.is_empty());
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        app.contact_dialog_open = false;
        app.search_query = "下周一起看书".into();
        let output = input_frame(&mut app, &ctx, 1.0, vec![], vec![]);
        assert!(
            output
                .shapes
                .iter()
                .any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="示例联系人"))
        );
        let restored = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
        assert_eq!(restored.contact_items.len(), 1);
        assert_eq!(restored.contact_items[0].chats.len(), 1);
    }

    pub(super) fn input_frame(
        app: &mut PersonalDeskApp,
        ctx: &egui::Context,
        time: f64,
        events: Vec<egui::Event>,
        dropped_files: Vec<egui::DroppedFileHandle>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                time: Some(time),
                events,
                dropped_files,
                ..Default::default()
            },
            |ui| app.render_workspace(ui),
        );
        output.textures_delta.clear();
        output
    }

    pub(super) fn pointer(position: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    #[derive(Debug)]
    struct LocalDrop(PathBuf);
    impl egui::DroppedFile for LocalDrop {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> std::result::Result<Vec<u8>, String> {
            fs::read(&self.0).map_err(|error| error.to_string())
        }
    }

    #[test]
    fn browser_files_only_request_system_open_after_a_double_click() {
        for mode in [ViewMode::Grid, ViewMode::List] {
            for filename in ["open-me.txt", "open-me.bin"] {
                let directory = tempfile::tempdir().unwrap();
                let folder = directory.path().join("folder");
                fs::create_dir(&folder).unwrap();
                let file = folder.join(filename);
                fs::write(&file, "synthetic").unwrap();
                let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
                app.navigate(folder.clone(), folder);
                app.set_view_mode(mode);
                let ctx = egui::Context::default();
                configure_theme(&ctx);
                let output = input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
                let position = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::epaint::Shape::Text(text) if text.galley.text() == filename => {
                            Some(text.pos + egui::vec2(8.0, 8.0))
                        }
                        _ => None,
                    })
                    .expect("file tile text");
                input_frame(&mut app, &ctx, 1.0, pointer(position, true), vec![]);
                input_frame(&mut app, &ctx, 1.03, pointer(position, false), vec![]);
                assert!(app.system_open_request.is_none());
                assert!(!app.editor_visible);
                input_frame(&mut app, &ctx, 1.12, pointer(position, true), vec![]);
                input_frame(&mut app, &ctx, 1.15, pointer(position, false), vec![]);
                assert_eq!(app.system_open_request, Some(file.canonicalize().unwrap()));
                assert!(!app.editor_visible);
            }
        }
    }

    #[test]
    fn external_drop_uses_live_position_and_accepts_folder_tiles_in_both_views() {
        for mode in [ViewMode::Grid, ViewMode::List] {
            for target in ["current", "child", "shortcut", "sidebar"] {
                let directory = tempfile::tempdir().unwrap();
                let folder = directory.path().join("destination");
                let child = folder.join("child");
                fs::create_dir_all(&child).unwrap();
                let source = directory.path().join("拖入.txt");
                fs::write(&source, "original").unwrap();
                let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
                if target == "shortcut" {
                    app.db
                        .as_mut()
                        .unwrap()
                        .save_shortcut("destination", &folder, true)
                        .unwrap();
                } else {
                    app.navigate(folder.clone(), folder.clone());
                }
                app.set_view_mode(mode);
                let ctx = egui::Context::default();
                configure_theme(&ctx);
                for time in [0.0, 0.1, 0.2] {
                    input_frame(&mut app, &ctx, time, vec![], vec![]);
                }
                let at = match target {
                    "child" | "shortcut" => app.folder_drop_targets[0].0.center(),
                    "sidebar" => egui::pos2(70.0, 300.0),
                    _ => egui::pos2(900.0, 650.0),
                };
                app.native_drop_position = Some(at);
                // Mimic Windows OLE: normal pointer still describes the old position.
                let stale = if target == "sidebar" {
                    egui::pos2(900.0, 650.0)
                } else {
                    egui::pos2(70.0, 300.0)
                };
                input_frame(
                    &mut app,
                    &ctx,
                    1.0,
                    vec![egui::Event::PointerMoved(stale)],
                    vec![std::sync::Arc::new(LocalDrop(source.clone()))],
                );
                if target == "sidebar" {
                    assert!(!app.drop_busy);
                    assert!(!folder.join("拖入.txt").exists());
                } else {
                    assert!(app.drop_busy, "{target} {mode:?}");
                    let deadline = Instant::now() + Duration::from_secs(3);
                    while app.drop_busy && Instant::now() < deadline {
                        app.poll_background();
                        thread::sleep(Duration::from_millis(5));
                    }
                    assert!(!app.drop_busy);
                    assert!(app.drop_report.is_none());
                    let destination = if target == "child" { &child } else { &folder };
                    assert_eq!(
                        fs::read_to_string(destination.join("拖入.txt")).unwrap(),
                        "original"
                    );
                    if target == "child" {
                        assert!(!folder.join("拖入.txt").exists());
                    }
                    if target == "current" {
                        assert!(app.file_items.iter().any(|file| file.name == "拖入.txt"));
                    }
                }
                assert_eq!(fs::read_to_string(&source).unwrap(), "original");
            }
        }
    }

    #[test]
    fn native_drop_events_only_copy_into_the_open_folder_content_area() {
        for mode in [ViewMode::Grid, ViewMode::List] {
            for target in ["folder", "sidebar", "root", "modal", "other-page"] {
                let directory = tempfile::tempdir().unwrap();
                let folder = directory.path().join("folder");
                fs::create_dir(&folder).unwrap();
                let source = directory.path().join("drop-me.txt");
                fs::write(&source, "source retained").unwrap();
                let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
                if target != "root" {
                    app.navigate(folder.clone(), folder.clone());
                }
                if target == "modal" {
                    app.folder_dialog_open = true;
                }
                if target == "other-page" {
                    app.section = Section::Inventory;
                }
                app.set_view_mode(mode);
                let ctx = egui::Context::default();
                configure_theme(&ctx);
                for time in [0.0, 0.1, 0.2] {
                    input_frame(&mut app, &ctx, time, vec![], vec![]);
                }
                let position = if target == "sidebar" {
                    egui::pos2(70.0, 300.0)
                } else {
                    egui::pos2(700.0, 550.0)
                };
                input_frame(
                    &mut app,
                    &ctx,
                    1.0,
                    vec![egui::Event::PointerMoved(position)],
                    vec![std::sync::Arc::new(LocalDrop(source.clone()))],
                );
                if target == "folder" {
                    assert!(app.drop_busy);
                    for _ in 0..200 {
                        app.poll_background();
                        if !app.drop_busy {
                            break;
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    assert!(!app.drop_busy);
                    assert!(app.drop_report.is_none());
                    assert_eq!(
                        fs::read_to_string(folder.join("drop-me.txt")).unwrap(),
                        "source retained"
                    );
                    assert!(app.file_items.iter().any(|item| item.name == "drop-me.txt"));
                    input_frame(&mut app, &ctx, 2.0, vec![], vec![]);
                    assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
                } else {
                    assert!(!app.drop_busy);
                    assert_eq!(fs::read_dir(&folder).unwrap().count(), 0);
                }
                assert_eq!(fs::read_to_string(source).unwrap(), "source retained");
            }
        }
    }

    #[test]
    fn tag_filters_combine_with_search_and_manager_edits_keep_pending_selections_in_sync() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
        let mut account = Credential::new();
        account.title = "work-card".into();
        account.username = "alice".into();
        account.tags = vec!["工作".into()];
        app.db.as_mut().unwrap().save_credential(&account).unwrap();
        let mut other = Credential::new();
        other.title = "game-card".into();
        other.username = "bob".into();
        other.tags = vec!["游戏".into()];
        app.db.as_mut().unwrap().save_credential(&other).unwrap();
        app.load_summaries();
        app.load_tags();
        app.section = Section::Credentials;
        app.account_filter = TagFilter::Named("游戏".into());
        app.search_query = "bob".into();
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let output = input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(texts.contains(&"game-card"));
        assert!(!texts.contains(&"work-card"));
        app.credential_draft = other.clone();
        app.tag_assignment = Some(TagAssignment {
            scope: TagScope::Credentials,
            id: other.id,
            selected: vec!["游戏".into()],
        });
        app.db
            .as_mut()
            .unwrap()
            .rename_tag(TagScope::Credentials, "游戏", "休闲")
            .unwrap();
        app.sync_tag_edit(TagScope::Credentials, "游戏", Some("休闲"));
        assert_eq!(app.account_filter, TagFilter::Named("休闲".into()));
        assert_eq!(app.credential_draft.tags, vec!["休闲"]);
        assert_eq!(app.tag_assignment.as_ref().unwrap().selected, vec!["休闲"]);
        app.db
            .as_mut()
            .unwrap()
            .delete_tag(TagScope::Credentials, "休闲")
            .unwrap();
        app.sync_tag_edit(TagScope::Credentials, "休闲", None);
        assert_eq!(app.account_filter, TagFilter::All);
        assert!(app.credential_draft.tags.is_empty());
        assert_eq!(app.credentials.len(), 2);
    }

    #[test]
    fn account_password_stays_readable_without_a_visibility_switch_at_display_scales() {
        for scale in [1.0, 1.25, 1.5] {
            let directory = tempfile::tempdir().unwrap();
            let mut app = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
            app.credential_draft.title = "测试账号".into();
            app.credential_draft.password = "SYNTHETIC-visible-password".into();
            assert!(app.save_credential_draft());
            app.selected_credential = Some(app.credential_draft.id.clone());
            app.credential_dialog_open = true;
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            ctx.set_pixels_per_point(scale);
            for time in [0.0, 1.0, 31.0, 61.0] {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000.0, 680.0),
                        )),
                        time: Some(time),
                        ..Default::default()
                    },
                    |ui| app.render_workspace(ui),
                );
                output.textures_delta.clear();
                if time == 0.0 {
                    continue;
                }
                let mut password_visible = false;
                for shape in output.shapes {
                    if let egui::epaint::Shape::Text(text) = shape.shape {
                        assert!(!["显示密码", "隐藏密码"].contains(&text.galley.text()));
                        if text.galley.text() == "SYNTHETIC-visible-password" {
                            password_visible = true;
                            assert!(
                                text.galley.job.sections.iter().all(|section| section
                                    .format
                                    .font_id
                                    .size
                                    >= 16.0)
                            );
                        }
                    }
                }
                assert!(
                    password_visible,
                    "password must stay visible at {scale}x after {time} seconds"
                );
            }
        }
    }

    #[test]
    fn closing_editor_saves_changes_and_keeps_dialog_open_on_conflict() {
        for external_change in [false, true] {
            let directory = tempfile::tempdir().expect("isolated data");
            let mut app = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
            app.create_internal_document();
            app.section = Section::Folders;
            let path = app.editor_path.clone().expect("new document");
            app.editor_text = "尚未自动保存的正文".into();
            app.editor_dirty = true;
            if external_change {
                fs::write(&path, "其他程序写入的内容").unwrap();
            }
            let ctx = egui::Context::default();
            for _ in 0..3 {
                let mut output =
                    ctx.run_ui(egui::RawInput::default(), |ui| app.render_workspace(ui));
                output.textures_delta.clear();
            }
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ui| app.render_workspace(ui),
            );
            output.textures_delta.clear();
            assert_eq!(app.editor_visible, external_change);
            assert_eq!(app.editor_dirty, external_change);
            assert_eq!(
                fs::read_to_string(path).unwrap(),
                if external_change {
                    "其他程序写入的内容"
                } else {
                    "尚未自动保存的正文"
                }
            );
        }
    }

    #[test]
    fn every_application_dialog_is_centered_and_fits_after_resizing() {
        for id in [
            "credential-details",
            "inventory-details",
            "document-editor",
            "shortcut-details",
            "new-folder",
            "rename-file",
            "settings-details",
            "website-details",
            "export-details",
            "manage-tags",
            "assign-tags",
            "drop-results",
            "contact-details",
            "contact-chat",
            "theme-editor",
            "tool-details",
            "codex-request",
            "codex-settings",
        ] {
            let directory = tempfile::tempdir().expect("isolated data");
            let mut app = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            match id {
                "credential-details" => app.credential_dialog_open = true,
                "inventory-details" => app.inventory_dialog_open = true,
                "document-editor" => {
                    app.create_internal_document();
                    app.section = Section::Folders;
                }
                "shortcut-details" => app.shortcut_dialog_open = true,
                "new-folder" => app.folder_dialog_open = true,
                "rename-file" => app.file_dialog_open = true,
                "website-details" => app.website_dialog_open = true,
                "export-details" => app.export_dialog_open = true,
                "codex-settings" => app.codex.settings_open = true,
                "theme-editor" => app.theme_editor_open = true,
                "tool-details" => {
                    app.tool_draft = Some(crate::launcher::AppTool::new(
                        directory.path().join("test.exe"),
                    ))
                }
                "codex-request" => {
                    app.codex
                        .requests
                        .push_back(crate::codex_client::ServerRequest {
                            id: serde_json::json!(1),
                            method: "item/commandExecution/requestApproval".into(),
                            params: serde_json::json!({"command":"test"}),
                            answers: Default::default(),
                        })
                }
                "contact-details" => app.contact_dialog_open = true,
                "contact-chat" => {
                    let path = directory.path().join("chat.xlsx");
                    fs::write(&path, contacts::tests::workbook_fixture()).unwrap();
                    let mut chat = contacts::import_chat(&path).unwrap();
                    chat.title = "很长的聊天记录文件名称".repeat(30);
                    app.contact_chat_open = Some(chat.id.clone());
                    app.contact_draft.chats.push(chat);
                }
                "manage-tags" => app.open_tag_manager(TagScope::Credentials),
                "assign-tags" => {
                    app.tag_assignment = Some(TagAssignment {
                        scope: TagScope::Credentials,
                        id: "synthetic".into(),
                        selected: vec![],
                    })
                }
                "drop-results" => {
                    app.drop_report = Some(files::DropReport {
                        copied: vec![],
                        failures: vec![(PathBuf::from("test.txt"), "无法读取".into())],
                    })
                }
                _ => app.settings_detail = Some("本地数据".into()),
            }
            for size in [
                [1280.0, 800.0],
                [1000.0, 680.0],
                [1440.0, 900.0],
                [1000.0, 680.0],
            ] {
                let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size.into());
                for _ in 0..8 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(viewport),
                            ..Default::default()
                        },
                        |ui| app.render_workspace(ui),
                    );
                    output.textures_delta.clear();
                }
                let rect = ctx
                    .memory(|memory| memory.area_rect(egui::Id::unique(id)))
                    .expect("dialog exists");
                if matches!(
                    id,
                    "credential-details" | "inventory-details" | "document-editor"
                ) {
                    assert!(
                        rect.height() > 400.0,
                        "{id} must not collapse its scrolling content: {rect:?}"
                    );
                }
                assert!(
                    viewport.contains_rect(rect),
                    "{id} overflows {size:?}: {rect:?}"
                );
                assert!(
                    (rect.center() - viewport.center()).length() < 2.0,
                    "{id} off center in {size:?}: {rect:?}"
                );
            }
        }
    }

    #[test]
    fn add_document_opens_a_blank_in_app_editor_and_preserves_title_and_body() {
        let directory = tempfile::tempdir().expect("temporary app data");
        let mut app = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
        app.section = Section::Documents;
        app.add_current_item();
        assert!(app.editor_visible);
        assert!(app.editor_text.is_empty());
        assert_eq!(app.editor_title, "未命名文档");
        let first_path = app.editor_path.clone().expect("managed document path");
        assert!(first_path.starts_with(directory.path().join("documents")));

        app.editor_title = "工作计划 / 2026 <日常>".into();
        app.editor_text = "直接在办公台里编写的正文。\n第二段内容。".into();
        app.editor_dirty = true;
        assert!(app.save_editor());
        let saved_title = app.editor_title.clone();
        let saved_body = app.editor_text.clone();
        app.search_query = "办公台里编写".into();
        app.run_search();
        assert!(
            app.search_results
                .iter()
                .any(|result| result.title == saved_title)
        );
        app.add_current_item();
        let second_path = app.editor_path.clone().expect("second managed document");
        assert_ne!(first_path, second_path);
        assert!(app.editor_text.is_empty());
        app.editor_title = saved_title.clone();
        app.editor_dirty = true;
        assert!(app.save_editor(), "duplicate titles must be supported");
        drop(app);

        let mut reopened = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
        reopened.open_path(first_path);
        assert!(reopened.editor_visible);
        assert_eq!(reopened.editor_title, saved_title);
        assert_eq!(reopened.editor_text, saved_body);
        assert_eq!(
            reopened
                .db
                .as_ref()
                .expect("database")
                .document_records()
                .expect("records")
                .len(),
            2
        );
    }
}
