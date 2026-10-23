use std::{
    fs,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use eframe::egui::{self, Color32, RichText};

mod appearance;
mod essay_ui;
use appearance::configure_theme;
mod contact_ui;
mod credential_ui;
mod document_group_ui;
mod favorites_ui;
mod game_ui;
mod index_ui;
mod navigation_ui;
#[cfg(test)]
mod performance_tests;
mod record_dialogs;
mod reference_ui;
mod settings_ui;
mod study_ui;
mod website_ui;
#[cfg(test)]
mod writing_tests;
mod writing_ui;

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
    StudyGraded(Box<study_ui::GradedMessage>),
    SqlExecuted(study_ui::SqlMessage),
    Learning(study_ui::LearningMessage),
    ContactChats(String, std::result::Result<Vec<ChatRecord>, String>),
    Exported(std::result::Result<export::Report, String>),
}
impl BackgroundMessage {
    fn graded(message: study_ui::GradedMessage) -> Self {
        Self::StudyGraded(Box::new(message))
    }
}

#[derive(Clone)]
struct TagAssignment {
    scope: TagScope,
    id: String,
    selected: Vec<String>,
}

pub struct PersonalDeskApp {
    essay: essay_ui::EssayState,
    reference: reference_ui::ReferenceState,
    study: study_ui::StudyState,
    credential_layout: credential_ui::CredentialLayoutCache,
    favorites: std::collections::BTreeSet<(String, String)>,
    inline_inventory: std::collections::HashMap<String, InventoryItem>,
    contact_extra_columns: bool,
    inventory_quantities: std::collections::HashMap<String, String>,
    writing: writing_ui::WritingState,
    library: library_ui::LibraryState,
    photo_cache: std::collections::HashMap<String, egui::TextureHandle>,
    contact_scroll_id: Option<String>,
    updates: update_ui::UpdateState,
    #[cfg(debug_assertions)]
    preview_capture: Option<preview::Capture>,
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
    games: game_ui::GameState,
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
            Ok(mut db) => {
                let recovery_error = db.recover_writing_drafts().err();
                let notice = db.migration_notice.clone();
                (
                    Some(db),
                    String::new(),
                    recovery_error
                        .map(|e| format!("草稿恢复失败：{e}"))
                        .unwrap_or(notice),
                )
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
            essay: Default::default(),
            reference: Default::default(),
            study: Default::default(),
            credential_layout: Default::default(),
            favorites,
            inline_inventory: Default::default(),
            inventory_quantities: Default::default(),
            contact_extra_columns: settings
                .get("view.contacts.extra")
                .is_some_and(|v| v == "true"),
            writing: Default::default(),
            library: Default::default(),
            photo_cache: Default::default(),
            contact_scroll_id: None,
            #[cfg(debug_assertions)]
            preview_capture: None,
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
            games: Default::default(),
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
            app.initialize_study();
            app.initialize_games();
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

    fn run_search(&mut self) {}

    fn poll_background(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                BackgroundMessage::StudyGraded(message) => self.accept_study_grade(*message),
                BackgroundMessage::SqlExecuted(message) => self.accept_sql_execution(message),
                BackgroundMessage::Learning(message) => self.accept_learning(message),
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

    fn view_mode(&self) -> ViewMode {
        self.view_modes.mode(self.section)
    }

    fn set_view_mode(&mut self, mode: ViewMode) {
        if self.section == Section::Documents && !self.flush_writing() {
            return;
        }
        // Browsing is independent from editing. Invalid/incomplete row drafts stay in memory.
        if self.view_mode() == ViewMode::List
            && mode == ViewMode::Grid
            && (!self.inline_contacts.is_empty()
                || !self.inline_inventory.is_empty()
                || !self.inline_credentials.is_empty())
        {
            self.notice = "编辑草稿已保留，切回列表可继续修改".into();
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
                if self.section == Section::Games {
                    self.search_field = None;
                    self.render_header(ui, &ctx);
                    self.handle_workspace_shortcuts(&ctx);
                    ui.add_space(10.);
                    self.render_games(ui);
                    return;
                }
                if self.section == Section::Documents && self.writing.page.is_some() {
                    self.render_writing_editor(ui);
                    self.handle_workspace_shortcuts(&ctx);
                    return;
                }
                if matches!(self.section, Section::Practice | Section::Words) {
                    self.render_study(ui);
                    return;
                }
                if self.section == Section::Essays {
                    self.render_essays(ui);
                    return;
                }
                if self.section == Section::Reference {
                    self.render_reference(ui);
                    return;
                }
                self.search_field = None;
                self.render_header(ui, &ctx);

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
                        Section::Credentials => self.render_credentials(ui),
                        Section::Folders => self.render_folders(ui),
                        Section::Documents => self.render_writing_documents(ui),
                        Section::Inventory => self.render_inventory(ui),
                        Section::Websites => self.render_websites(ui),
                        Section::Contacts => self.render_contacts(ui),
                        Section::Games => self.render_games(ui),
                        Section::Practice | Section::Words => self.render_study(ui),
                        Section::Settings => self.render_settings(ui),
                        Section::Tools => self.render_tools(ui),
                        Section::Essays => unreachable!("essays own their scroll workspace"),
                        Section::Reference => unreachable!("reference owns its scroll workspace"),
                    });
            });
        self.render_detail_windows(&ctx);
    }

    fn detail_open(&self) -> bool {
        self.study.route_open
            || self.study.settings
            || self.theme_editor_open
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
    configured
        .or_else(crate::install::remembered_data_directory)
        .unwrap_or_else(|| {
            directories::BaseDirs::new()
                .map(|base| base.data_local_dir().join("PersonalDesk"))
                .unwrap_or_else(|| PathBuf::from("PersonalDesk-data"))
        })
}

impl eframe::App for PersonalDeskApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_background();
        self.study
            .ai
            .unload_if_idle(matches!(self.section, Section::Practice | Section::Words));
        if ctx.input(|input| input.viewport().close_requested()) {
            let mut records_saved =
                self.flush_writing() && self.flush_study_answer() && self.flush_games();
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
        ctx.request_repaint_after(
            if self.contact_import_busy || self.export_busy || self.study.busy {
                Duration::from_millis(100)
            } else {
                Duration::from_secs(5)
            },
        );
        self.render_workspace(ui);
        #[cfg(debug_assertions)]
        self.capture_preview(&ctx);
        if let Some(path) = self.tool_launch_request.take()
            && let Err(error) = crate::launcher::launch(&path)
        {
            self.notice = error.to_string();
        }
        if let Some(site) = self.website_open_request.take()
            && let Err(error) = open::that_detached(&site.url)
        {
            self.notice = format!("打开网站失败：{error}");
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

impl Drop for PersonalDeskApp {
    fn drop(&mut self) {
        self.study.ai.shutdown();
    }
}

#[cfg(test)]
mod editor_tests;
