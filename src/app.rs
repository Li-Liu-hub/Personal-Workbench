use std::{
    fs,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use eframe::egui::{self, Color32, RichText};

mod browser_ui;
mod contact_ui;
mod credential_ui;
mod document_group_ui;
mod favorites_ui;
mod game_ui;
mod index_ui;
#[cfg(test)]
mod performance_tests;

mod inventory_ui;
mod layout_ui;
mod library_ui;
mod module_tags;
#[cfg(debug_assertions)]
mod preview;

#[cfg(test)]
mod index_tests;
mod theme_ui;
mod tool_ui;
mod update_ui;
#[cfg(test)]
mod view_tests;

use crate::{
    contacts::{self, ChatRecord, Contact},
    db::Database,
    error::DeskError,
    export, files,
    model::{Credential, CredentialSummary, InventoryItem, Section, Website},
    tags::{self, Filter as TagFilter, Scope as TagScope},
    ui_components::{
        TileKind, accent, border, canvas, centered_dialog, collection, content_fill,
        credential_entry, credential_list_header, dialog_closed, entry, ink, muted,
        navigation_item, primary_button, sidebar, surface, tag_choices, tag_filter_bar,
        tagged_entry, view_switcher, website_entry,
    },
    views::{self, Preferences, ViewMode},
    websites,
};

enum BackgroundMessage {
    ContactChats(String, std::result::Result<Vec<ChatRecord>, String>),
    Exported(std::result::Result<export::Report, String>),
}

#[derive(Clone)]
struct TagAssignment {
    scope: TagScope,
    id: String,
    selected: Vec<String>,
}

pub struct PersonalDeskApp {
    credential_layout: credential_ui::CredentialLayoutCache,
    favorites: std::collections::BTreeSet<(String, String)>,
    inline_inventory: std::collections::HashMap<String, InventoryItem>,
    browser: crate::browser::Browser,
    browser_area: Option<egui::Rect>,
    library: library_ui::LibraryState,
    photo_cache: std::collections::HashMap<String, egui::TextureHandle>,
    contact_scroll_id: Option<String>,
    updates: update_ui::UpdateState,
    #[cfg(debug_assertions)]
    preview_capture: Option<preview::Capture>,
    native_host: usize,
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
    tag_manager: Option<TagScope>,
    tag_assignment: Option<TagAssignment>,
    tag_name: String,
    tag_original: Option<String>,
    tag_error: String,
    location_request: Option<PathBuf>,
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
    inventory_prices: std::collections::BTreeMap<String, String>,
    inline_contacts: std::collections::BTreeMap<String, Contact>,
    inline_contact_tags: std::collections::BTreeMap<String, Vec<String>>,
    inline_credentials: std::collections::BTreeMap<String, CredentialSummary>,
    selected_credential: Option<String>,
    credential_dialog_open: bool,
    inventory_dialog_open: bool,
    shortcut_dialog_open: bool,
    settings_detail: Option<String>,
    search_query: String,
    search_field: Option<egui::Id>,
    search_pending: Option<f64>,
    inventory_items: Vec<InventoryItem>,
    inventory_draft: InventoryItem,
    selected_inventory: Option<String>,
    sender: Sender<BackgroundMessage>,
    receiver: Receiver<BackgroundMessage>,
}

impl PersonalDeskApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_theme(&cc.egui_ctx);
        let mut app = Self::open_data_directory(data_directory());
        // Preview/test construction never changes the user's startup entry.
        if !cfg!(debug_assertions)
            && let Err(error) = std::env::current_exe()
                .map_err(|e| e.to_string())
                .and_then(|path| crate::autostart::refresh_if_enabled(&path))
        {
            app.notice = error;
        }
        app
    }

    #[cfg(debug_assertions)]
    pub fn desktop_visual_preview(cc: &eframe::CreationContext<'_>, base: PathBuf) -> Self {
        configure_theme(&cc.egui_ctx);
        let mut app = Self::open_data_directory(base);
        app.section = Section::Folders;
        app
    }

    #[cfg(debug_assertions)]
    pub fn layout_preview(cc: &eframe::CreationContext<'_>, base: PathBuf) -> Self {
        configure_theme(&cc.egui_ctx);
        let mut app = Self::open_data_directory(base);
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
            credential_layout: Default::default(),
            favorites,
            inline_inventory: Default::default(),
            browser: crate::browser::Browser::new(base.join("browser-profile")),
            browser_area: None,
            library: Default::default(),
            photo_cache: Default::default(),
            contact_scroll_id: None,
            #[cfg(debug_assertions)]
            preview_capture: None,
            native_host: 0,
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
            tag_manager: None,
            tag_assignment: None,
            tag_name: String::new(),
            tag_original: None,
            tag_error: String::new(),
            location_request: None,
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
            inventory_prices: Default::default(),
            inline_contacts: Default::default(),
            inline_contact_tags: Default::default(),
            inline_credentials: Default::default(),
            selected_credential: None,
            credential_dialog_open: false,
            inventory_dialog_open: false,
            shortcut_dialog_open: false,
            settings_detail: None,
            search_query: String::new(),
            search_field: None,
            search_pending: None,
            inventory_items: Vec::new(),
            inventory_draft: InventoryItem::new(),
            selected_inventory: None,
            sender,
            receiver,
        };
        if app.db.is_some() {
            app.load_summaries();
            app.load_inventory();
            app.load_websites();
            app.load_contacts();
            app.load_tags();
        }
        app
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

    fn run_search(&mut self) {}

    fn poll_background(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                BackgroundMessage::ContactChats(id, result) => {
                    self.contact_import_busy = false;
                    if self.contact_dialog_open && self.contact_draft.id == id {
                        match result.and_then(|chats| {
                            contacts::append_chats(&mut self.contact_draft, chats)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        }) {
                            Ok(()) => self.contact_error.clear(),
                            Err(e) => self.contact_error = e,
                        }
                    }
                }
                BackgroundMessage::Exported(result) => {
                    self.export_busy = false;
                    self.notice = match result {
                        Ok(report) => format!(
                            "已导出：{}，{} 项说明",
                            report.path.display(),
                            report.warnings
                        ),
                        Err(e) => e.to_string(),
                    };
                }
            }
        }
    }

    fn page_title(&self) -> String {
        match self.section {
            Section::Folders => "快捷路径".into(),
            Section::Documents => "文档索引".into(),
            Section::Books => "图书".into(),
            Section::Browser => "大模型聊天".into(),
            Section::Credentials => "账号密码".into(),
            Section::Inventory => "物品清单".into(),
            Section::Contacts => "联系人".into(),
            Section::Websites => "网站收藏".into(),
            Section::Tools => "小工具".into(),
            Section::Games => "小游戏".into(),
            Section::Settings => "设置".into(),
        }
    }

    fn handle_workspace_shortcuts(&mut self, ctx: &egui::Context) {
        if self.detail_open() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::K))
            && let Some(id) = self.search_field
        {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
            self.add_current_item();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.save_visible_rows();
        }
        if !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Z))
        {
            self.undo_index_removal();
        }
        if self.section == Section::Documents
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft))
        {
            self.library.book = None;
        }
    }

    fn save_visible_rows(&mut self) -> bool {
        let mut saved = true;
        match self.section {
            Section::Contacts => {
                for id in self.inline_contacts.keys().cloned().collect::<Vec<_>>() {
                    saved &= self.save_inline_contact(&id);
                }
            }
            Section::Inventory => {
                for id in self.inline_inventory.keys().cloned().collect::<Vec<_>>() {
                    saved &= self.save_inline_inventory(&id);
                }
            }
            Section::Credentials => {
                for id in self.inline_credentials.keys().cloned().collect::<Vec<_>>() {
                    saved &= self.save_inline_credential(&id);
                }
            }
            _ => {}
        }
        saved
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
        ui.add_space(24.0);
        let navigation_spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 4.0;
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
                    (Section::Documents, "文档索引", TileKind::Document, None),
                    (Section::Browser, "大模型聊天", TileKind::Website, None),
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
                        self.section = section;
                        self.notice.clear();
                        self.search_query.clear();

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

    fn add_label(&self) -> &'static str {
        match self.section {
            Section::Books => "新增图书",
            Section::Browser => "",
            Section::Credentials => "添加账号",
            Section::Folders => "添加快捷路径",
            Section::Documents => "添加文档索引",
            Section::Inventory => "添加物品",
            Section::Websites => "添加网站",
            Section::Contacts => "添加联系人",
            Section::Tools => "添加工具",
            Section::Settings => "",
            Section::Games => "",
        }
    }

    fn add_current_item(&mut self) {
        match self.section {
            Section::Books => self.library.dialog = Some(crate::library::Book::new()),
            Section::Browser => self.browser.open("https://chat.deepseek.com/"),
            Section::Credentials => {
                self.selected_credential = None;
                self.credential_draft = Credential::new();
                self.credential_dialog_open = true;
            }
            Section::Folders => self.shortcut_dialog_open = true,
            Section::Documents => self.pick_document_indices(),
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
                self.inventory_prices.remove(id);
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

    fn render_credentials(&mut self, ui: &mut egui::Ui) {
        let mut selected = None;
        let mut delete = None;
        let mut add = false;

        credential_list_header(ui, self.view_mode());
        let cells = self.credential_cells(ui);
        for (item, rect, side) in cells {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt((
                        "credential-cell",
                        item.as_ref().map(|v| v.id.as_str()).unwrap_or("add"),
                    ))
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            let ui = &mut child;
            if let Some(item) = item {
                let editing = self.inline_credentials.contains_key(&item.id);
                let mut draft = self
                    .inline_credentials
                    .get(&item.id)
                    .unwrap_or(&item)
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
                if response.details
                    || (self.view_mode() == ViewMode::Grid && response.response.clicked())
                {
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
            } else if entry(
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
        }
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
                            &item.brand,
                            &item.model,
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
                        format!(
                            "{} · {} 件{}",
                            [item.brand.as_str(), item.model.as_str()]
                                .into_iter()
                                .filter(|v| !v.is_empty())
                                .collect::<Vec<_>>()
                                .join(" "),
                            item.quantity,
                            if item.location.is_empty() {
                                String::new()
                            } else {
                                format!(" · {}", item.location)
                            }
                        )
                        .trim_start_matches(" · "),
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

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style())
            .fill(surface())
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.set_min_width((ui.available_width() - 1.0).max(0.0));
                ui.strong("启动设置");
                ui.add_space(8.0);
                let registered = crate::autostart::registered_command();
                let mut enabled = registered.as_ref().is_ok_and(|value| value.is_some());
                ui.horizontal_wrapped(|ui| {
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
                            Err(error) => {
                                enabled = registered.as_ref().is_ok_and(|value| value.is_some());
                                self.notice = error;
                            }
                        }
                    }
                    ui.label(
                        RichText::new(if enabled { "已开启" } else { "已关闭" }).color(muted()),
                    );
                    if let Ok(Some(value)) = &registered
                        && let Ok(path) = std::env::current_exe()
                        && *value != crate::autostart::command(&path)
                        && ui.button("更新启动路径").clicked()
                    {
                        self.notice = match crate::autostart::set(true, &path) {
                            Ok(()) => "启动路径已更新".into(),
                            Err(error) => error,
                        };
                    }
                });
                if let Err(error) = registered {
                    ui.colored_label(Color32::from_rgb(185, 65, 71), error);
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
        if self.view_mode() == ViewMode::List && mode == ViewMode::Grid && !self.save_visible_rows()
        {
            return;
        }
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
        self.paint_theme_background(ui);
        if self.db.is_some() {
            egui::Panel::left("navigation")
                .resizable(false)
                .show_separator_line(false)
                .exact_size(184.0)
                .frame(
                    egui::Frame::default()
                        .fill(sidebar())
                        .stroke(egui::Stroke::NONE)
                        .inner_margin(egui::Margin::symmetric(12, 14)),
                )
                .show(ui, |ui| self.render_sidebar(ui));
        }

        if self.db.is_none() {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.heading("无法打开本地数据");
                ui.label(&self.startup_error);
            });
            return;
        }
        egui::Panel::bottom("workspace-status")
            .exact_size(24.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(content_fill())
                    .inner_margin(egui::Margin::symmetric(24, 2)),
            )
            .show(ui, |ui| self.render_status(ui));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(content_fill())
                    .stroke(egui::Stroke::NONE)
                    .inner_margin(egui::Margin::symmetric(24, 14)),
            )
            .show(ui, |ui| {
                if self.section == Section::Browser {
                    self.render_browser(ui);
                    return;
                }
                self.search_field = None;
                self.render_header(ui, &ctx);
                if self.section == Section::Documents && self.library.book.is_some() {
                    self.render_index_book_toolbar(ui);
                }
                self.handle_workspace_shortcuts(&ctx);
                ui.add_space(12.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt((
                        "workspace-content",
                        views::key(self.section),
                        self.view_mode().value(),
                    ))
                    .show(ui, |ui| match self.section {
                        Section::Books => self.render_library(ui),
                        Section::Browser => self.render_browser(ui),
                        Section::Credentials => self.render_credentials(ui),
                        Section::Folders => self.render_folders(ui),
                        Section::Documents => self.render_documents(ui),
                        Section::Inventory => self.render_inventory(ui),
                        Section::Websites => self.render_websites(ui),
                        Section::Contacts => self.render_contacts(ui),
                        Section::Games => self.render_games(ui),
                        Section::Settings => self.render_settings(ui),
                        Section::Tools => self.render_tools(ui),
                    });
            });
        self.render_detail_windows(&ctx);
    }

    fn detail_open(&self) -> bool {
        self.theme_editor_open
            || self.tool_draft.is_some()
            || self.credential_dialog_open
            || self.inventory_dialog_open
            || self.library.dialog.is_some()
            || self.shortcut_dialog_open
            || self.settings_detail.is_some()
            || self.updates.open
            || self.website_dialog_open
            || self.export_dialog_open
            || self.tag_manager.is_some()
            || self.tag_assignment.is_some()
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
                    ui.spacing_mut().item_spacing.y = 4.0;
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
                        columns[0].label("品牌");
                        columns[0].text_edit_singleline(&mut self.inventory_draft.brand);
                        columns[1].label("型号");
                        columns[1].text_edit_singleline(&mut self.inventory_draft.model);
                    });
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
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(window) = frame.window_handle()
            && let RawWindowHandle::Win32(window) = window.as_raw()
        {
            self.native_host = window.hwnd.get() as usize;
        }
        let ctx = ui.ctx().clone();
        self.browser_area = None;
        self.poll_background();
        if ctx.input(|input| input.viewport().close_requested()) {
            let mut records_saved = true;
            if self.export_busy || self.contact_import_busy {
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
            for id in self.inline_contacts.keys().cloned().collect::<Vec<_>>() {
                records_saved &= self.save_inline_contact(&id);
            }
            for id in self.inline_credentials.keys().cloned().collect::<Vec<_>>() {
                records_saved &= self.save_inline_credential(&id);
            }
            for id in self.inline_inventory.keys().cloned().collect::<Vec<_>>() {
                records_saved &= self.save_inline_inventory(&id);
            }
            if !records_saved {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        ctx.request_repaint_after(if self.contact_import_busy || self.export_busy {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(5)
        });
        self.render_workspace(ui);
        #[cfg(debug_assertions)]
        self.capture_preview(&ctx);
        self.sync_browser(&ctx);
        if let Some(path) = self.tool_launch_request.take()
            && let Err(error) = crate::launcher::launch(&path)
        {
            self.notice = error.to_string();
        }
        if let Some(site) = self.website_open_request.take() {
            self.browser.open(&site.url);
            self.section = Section::Browser;
        }
        if let Some(path) = self.location_request.take()
            && let Err(error) = files::open_location(&path)
        {
            self.notice = error.to_string();
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
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.animation_time = 0.12;
        style.spacing.interact_size.y = 32.0;
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
        style.visuals.selection.bg_fill = Color32::from_rgb(210, 230, 255);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, accent());
        style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, ink());
        style.visuals.widgets.noninteractive.bg_fill = surface();
        style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, border());
        style.visuals.widgets.inactive.bg_fill = surface();
        style.visuals.widgets.inactive.weak_bg_fill = surface();
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, ink());
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border());
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(243, 244, 247);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(243, 244, 247);
        style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, accent());
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent());
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(225, 235, 250);
        style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(225, 235, 250);
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
            .push("chinese".into());
        if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
            fonts
                .font_data
                .insert("system-ui".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "system-ui".into());
        }
        ctx.set_fonts(fonts);
    }
}

#[cfg(test)]
mod editor_tests {
    use super::*;
    use std::time::Instant;

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
    fn every_application_dialog_is_centered_and_fits_after_resizing() {
        for id in [
            "credential-details",
            "inventory-details",
            "shortcut-details",
            "settings-details",
            "website-details",
            "export-details",
            "manage-tags",
            "assign-tags",
            "contact-details",
            "contact-chat",
            "theme-editor",
            "tool-details",
        ] {
            let directory = tempfile::tempdir().expect("isolated data");
            let mut app = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            match id {
                "credential-details" => app.credential_dialog_open = true,
                "inventory-details" => app.inventory_dialog_open = true,
                "shortcut-details" => app.shortcut_dialog_open = true,
                "website-details" => app.website_dialog_open = true,
                "export-details" => app.export_dialog_open = true,
                "theme-editor" => app.theme_editor_open = true,
                "tool-details" => {
                    app.tool_draft = Some(crate::launcher::AppTool::new(
                        directory.path().join("test.exe"),
                    ))
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
                if matches!(id, "credential-details" | "inventory-details") {
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
}
