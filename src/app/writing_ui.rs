use super::*;
use crate::library::{Page, WritingSummary};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

pub(super) struct WritingState {
    pub page: Option<Page>,
    pub dirty: bool,
    changed_at: f64,
    pub error: String,
    pub(super) preview: bool,
    writer: String,
    focus_body: bool,
    cache: CommonMarkCache,
    catalog_key: Option<(u64, String)>,
    catalog: Vec<WritingSummary>,
    chars: usize,
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
        let result = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| {
                db.create_writing(
                    if log { &day } else { "未命名文档" },
                    self.library.book.as_ref().map(|b| b.id.as_str()),
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
    fn set_writing_page(&mut self, page: Page) {
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
            Ok(_) => {
                self.writing.catalog_key = None;
                true
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
        let books = if self.library.book.is_none() {
            self.db
                .as_ref()
                .and_then(|db| db.books().ok())
                .unwrap_or_default()
        } else {
            vec![]
        };
        let mut open = None;
        let mut remove = None;
        let mut add = false;
        let mut group = None;
        let mut book_open = None;
        collection(ui, "writing-documents", self.view_mode(), |ui, layout| {
            for book in &books {
                let members = self
                    .writing
                    .catalog
                    .iter()
                    .filter(|p| p.book_id == book.id && filter.matches(&p.tags))
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
                if let Some(payload) = response.dnd_release_payload::<WritingDrag>() {
                    group = Some((vec![payload.0.clone()], Some(book.id.clone())));
                }
                response.context_menu(|ui| {
                    if ui.button("重命名图书").clicked() {
                        self.library.dialog = Some(book.clone());
                        ui.close();
                    }
                    self.record_tag_menu(ui, TagScope::Books, &book.id);
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
                let drag = response.clone().interact(egui::Sense::click_and_drag());
                drag.dnd_set_drag_payload(WritingDrag(note.id.clone()));
                if let Some(payload) = drag.dnd_hover_payload::<WritingDrag>()
                    && payload.0 != note.id
                {
                    ui.painter().rect_stroke(
                        response.rect,
                        12,
                        egui::Stroke::new(2., accent()),
                        egui::StrokeKind::Inside,
                    );
                }
                if let Some(payload) = drag.dnd_release_payload::<WritingDrag>()
                    && payload.0 != note.id
                {
                    group = Some((vec![note.id.clone(), payload.0.clone()], None));
                }
                if response.clicked() {
                    open = Some(note.id.clone());
                }
                response.context_menu(|ui| {
                    self.record_tag_menu(ui, TagScope::BookPages, &note.id);
                    if ui.button("删除文档").clicked() {
                        remove = Some(note.id.clone());
                        ui.close();
                    }
                });
            }
            add = entry(ui, "add-writing", "新建文档", "", TileKind::Add, layout).clicked();
        });
        if let Some(book) = book_open {
            let first = self
                .db
                .as_ref()
                .and_then(|db| db.book_pages(&book.id).ok())
                .and_then(|pages| pages.first().map(|p| p.id.clone()));
            self.library.book = Some(book);
            if let Some(id) = first {
                self.open_writing(&id);
            } else {
                self.new_writing(false);
            }
        }
        if let Some((ids, target)) = group {
            self.group_written_documents(&ids, target.as_deref());
        }
        if let Some(id) = remove
            && let Some(db) = self.db.as_mut()
            && let Err(e) = db.set_library_deleted(&id, true, true)
        {
            self.notice = e.to_string();
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
        ui.horizontal(|ui| {
            ui.set_min_height(38.);
            back = ui.button("← 文档").clicked();
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(primary_button("新建")).clicked() {
                    self.new_writing(false);
                }
                if ui.button("生成日志").clicked() {
                    self.new_writing(true);
                }
                ui.selectable_value(&mut self.writing.preview, false, "写作");
                ui.selectable_value(&mut self.writing.preview, true, "预览");
                ui.menu_button("更多", |ui| {
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
        });
        if back && self.flush_writing() {
            self.writing.page = None;
            self.library.book = None;
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
        if self.writing.page.is_none() {
            return;
        }
        if let Some(book) = self.library.book.clone() {
            let pages = self
                .db
                .as_ref()
                .and_then(|db| db.book_pages(&book.id).ok())
                .unwrap_or_default();
            let mut selected = None;
            egui::ScrollArea::horizontal()
                .id_salt("writing-tabs")
                .max_height(34.)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for page in &pages {
                            if ui
                                .selectable_label(
                                    self.writing.page.as_ref().is_some_and(|p| p.id == page.id),
                                    &page.title,
                                )
                                .clicked()
                            {
                                selected = Some(page.id.clone());
                            }
                        }
                    });
                });
            if let Some(id) = selected {
                self.open_writing(&id);
            }
        }
        if !self.writing.error.is_empty() {
            ui.colored_label(Color32::from_rgb(184, 65, 71), &self.writing.error);
        }
        ui.add_space(12.);
        let mut changed = false;
        let available = ui.available_width();
        let width = available.min(820.);
        let id = self.writing.page.as_ref().unwrap().id.clone();
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
                                ui.set_width((width - 76.).max(220.));
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
