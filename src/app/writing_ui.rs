use super::*;
use crate::library::{Page, WritingSummary};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

mod book_navigation;
mod floating;
mod hierarchy;

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
                if ui.button("删除图书").clicked() {
                    self.request_library_deletion(&book.id, &book.title, false);
                }
            });
            ui.add_space(8.);
        }
        if ui.button("创建图书").clicked() {
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
                    if ui.button("重命名图书").clicked() {
                        self.library.dialog = Some(book.clone());
                        ui.close();
                    }
                    if book.parent_id.is_some() && ui.button("移到上一级").clicked() {
                        let parent = self
                            .parent_of(book.parent_id.as_deref())
                            .and_then(|b| b.parent_id);
                        drop_action = Some(hierarchy::LibraryDrop::Book(book.id.clone(), parent));
                        ui.close();
                    }
                    self.record_tag_menu(ui, TagScope::Books, &book.id);
                    ui.separator();
                    if ui.button("删除图书").clicked() {
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
                    if ui.button("删除文档").clicked() {
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
        let mut remove = None;
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
                && ui.button("删除页面").clicked()
                && let Some(page) = self.writing.page.as_ref()
            {
                remove = Some((page.id.clone(), page.title.clone(), true));
            }
            if self.section != Section::Essays && ui.add(primary_button("新建")).clicked() {
                self.new_writing(false);
            }
            if self.section != Section::Essays && ui.button("生成日志").clicked() {
                self.new_writing(true);
            }
            float = ui
                .button("悬浮窗")
                .on_hover_text("置顶小窗，方便对照简历或聊天记录写总结")
                .clicked();
            crate::ui_components::chrome::segmented(
                ui,
                &mut self.writing.preview,
                &[(false, "写作"), (true, "预览")],
            );
            self.render_writing_zoom(ui);
            ui.menu_button("更多", |ui| {
                if let Some(book) = self.library.book.as_ref() {
                    if ui.button("删除图书").clicked() {
                        remove = Some((book.id.clone(), book.title.clone(), false));
                        ui.close();
                    }
                    ui.separator();
                }
                save = ui.button("保存 · Ctrl+S").clicked();
                export = ui.button("导出 Markdown").clicked();
                if !self.writing.error.is_empty() {
                    copy = ui.button("另存为新文档").clicked();
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
                if ui.button("管理标签").clicked() {
                    self.open_tag_manager(TagScope::BookPages);
                    ui.close();
                }
                ui.separator();
                for (label, syntax) in [
                    ("标题", "# "),
                    ("加粗", "**文字**"),
                    ("列表", "- "),
                    ("待办", "- [ ] "),
                    ("引用", "> "),
                ] {
                    if ui.button(label).clicked() {
                        format = Some(syntax);
                        ui.close();
                    }
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
        if self.writing.page.is_none() {
            return;
        }
        if self.writing.floating {
            ui.add_space(30.);
            ui.heading(&self.writing.page.as_ref().unwrap().title);
            ui.label("正在悬浮窗中写作，可切换到简历或其他应用。内容自动保存到这份文档。");
            ui.horizontal(|ui| {
                if ui.button("显示悬浮窗").clicked() {
                    ui.ctx().send_viewport_cmd_to(
                        floating::viewport_id(),
                        egui::ViewportCommand::Minimized(false),
                    );
                    ui.ctx().send_viewport_cmd_to(
                        floating::viewport_id(),
                        egui::ViewportCommand::Focus,
                    );
                }
                if ui.button("回到主窗口").clicked() {
                    self.close_floating_writer();
                }
            });
            return;
        }
        if !self.writing.error.is_empty() {
            ui.colored_label(Color32::from_rgb(184, 65, 71), &self.writing.error);
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
            ui.ctx().memory_mut(|m| m.request_focus(body_id));
            ui.ctx()
                .input_mut(|i| i.events.push(egui::Event::Paste(syntax.into())));
        }
        egui::ScrollArea::vertical()
            .id_salt(("writing-paper", &id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.add_space(((available - width) / 2.).max(0.));
                    egui::Frame::new()
                        .fill(surface())
                        .corner_radius(14)
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
