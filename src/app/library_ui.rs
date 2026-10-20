use super::*;
use crate::library::{Book, Page};
pub(super) struct LibraryState {
    pub book: Option<Book>,
    pub page: Option<Page>,
    pub dirty: bool,
    pub dirty_since: Option<Instant>,
    pub error: String,
    pub dialog: Option<Book>,
    pub page_filter: TagFilter,
    pub undo: Vec<(String, bool)>,
    pub writer: String,
    pub show_deleted: bool,
}
impl Default for LibraryState {
    fn default() -> Self {
        Self {
            book: None,
            page: None,
            dirty: false,
            dirty_since: None,
            error: String::new(),
            dialog: None,
            page_filter: TagFilter::All,
            undo: vec![],
            writer: uuid::Uuid::new_v4().to_string(),
            show_deleted: false,
        }
    }
}
impl PersonalDeskApp {
    pub(super) fn save_library_page(&mut self) -> bool {
        if !self.library.dirty {
            return true;
        }
        let Some(page) = self.library.page.as_mut() else {
            return true;
        };
        let result = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.save_book_page(page));
        match result {
            Ok(()) => {
                self.library.dirty = false;
                self.library.error.clear();
                if let Some(db) = self.db.as_mut() {
                    let _ = db.clear_book_draft(&self.library.writer, &page.id);
                }
                true
            }
            Err(error) => {
                self.library.error = error.to_string();
                false
            }
        }
    }
    fn select_book(&mut self, book: Book) {
        if !self.save_library_page() {
            return;
        }
        let pages = self
            .db
            .as_ref()
            .and_then(|db| db.book_pages(&book.id).ok())
            .unwrap_or_default();
        self.library.book = Some(book);
        self.library.page = pages.first().cloned();
        self.library.page_filter = TagFilter::All;
    }
    fn new_library_page(&mut self) {
        if !self.save_library_page() {
            return;
        }
        let Some(book) = &self.library.book else {
            return;
        };
        let mut page = Page::new(book.id.clone());
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.save_book_page(&mut page))
        {
            Ok(()) => self.library.page = Some(page),
            Err(error) => self.library.error = error.to_string(),
        }
    }
    pub(super) fn undo_library(&mut self) -> bool {
        if let Some((id, page)) = self.library.undo.last().cloned() {
            match self
                .db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| db.set_library_deleted(&id, page, false))
            {
                Ok(()) => {
                    self.library.undo.pop();
                    self.notice = "已撤回删除".into();
                    return true;
                }
                Err(error) => self.notice = error.to_string(),
            }
        }
        false
    }
    pub(super) fn recover_book_draft(&mut self, key: &str, draft: &Page) -> bool {
        if !self.save_library_page() {
            return false;
        }
        let same_revision = self
            .db
            .as_ref()
            .and_then(|db| db.book_pages(&draft.book_id).ok())
            .is_some_and(|pages| {
                pages
                    .iter()
                    .any(|page| page.id == draft.id && page.revision == draft.revision)
            });
        let result = (|| {
            let db = self.db.as_mut().ok_or(DeskError::UnsupportedFormat)?;
            if same_revision {
                db.save_book_draft(&self.library.writer, draft)?;
                self.library.page = Some(draft.clone());
                self.library.dirty = true;
                self.library.dirty_since = Some(Instant::now());
            } else {
                let mut recovered = Page::new(draft.book_id.clone());
                recovered.title = format!("{} · 恢复副本", draft.title);
                recovered.body = draft.body.clone();
                recovered.tags = draft.tags.clone();
                db.save_book_page(&mut recovered)?;
                self.library.page = Some(recovered);
                self.library.dirty = false;
            }
            db.clear_book_draft_key(key)
        })();
        match result {
            Ok(()) => true,
            Err(error) => {
                self.library.error = error.to_string();
                false
            }
        }
    }
    pub(super) fn render_library(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.library.show_deleted, false, "图书");
            ui.selectable_value(&mut self.library.show_deleted, true, "已删除");
        });
        if self.library.show_deleted {
            let items = self
                .db
                .as_ref()
                .and_then(|db| db.deleted_library().ok())
                .unwrap_or_default();
            for (id, title, page) in items {
                ui.horizontal(|ui| {
                    ui.label(title);
                    if ui
                        .button(if page { "恢复页面" } else { "恢复图书" })
                        .clicked()
                        && let Some(db) = self.db.as_mut()
                        && let Err(error) = db.set_library_deleted(&id, page, false)
                    {
                        self.library.error = error.to_string();
                    }
                });
            }
            return;
        }

        if let Some(book) = self.library.book.clone() {
            let mut back = false;
            let mut add = false;
            ui.horizontal(|ui| {
                back = ui.button("← 全部图书").clicked();
                ui.label(RichText::new(&book.title).strong());
                if ui.button("重命名").clicked() {
                    self.library.dialog = Some(book.clone());
                }
                add = ui.button("+ 新增页面").clicked();
                if ui.button("保存").clicked() {
                    self.save_library_page();
                }
                if !self.library.dirty {
                    ui.label(RichText::new("已保存").small().color(muted()));
                }
            });
            if back && self.save_library_page() {
                self.library.book = None;
                self.library.page = None;
                return;
            }
            if add {
                self.new_library_page();
            }
            let options = self.names_for_scope(TagScope::BookPages);
            ui.horizontal_wrapped(|ui| {
                ui.label("页面分类");
                egui::ComboBox::from_id_salt("page-filter")
                    .selected_text(match &self.library.page_filter {
                        TagFilter::All => "全部".into(),
                        TagFilter::Untagged => "未分类".into(),
                        TagFilter::Named(name) => name.clone(),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.library.page_filter, TagFilter::All, "全部");
                        ui.selectable_value(
                            &mut self.library.page_filter,
                            TagFilter::Untagged,
                            "未分类",
                        );
                        for name in &options {
                            ui.selectable_value(
                                &mut self.library.page_filter,
                                TagFilter::Named(name.clone()),
                                name,
                            );
                        }
                    });
                if ui.button("管理页面标签").clicked() {
                    self.open_tag_manager(TagScope::BookPages);
                }
            });
            let mut pages = self
                .db
                .as_ref()
                .and_then(|db| db.book_pages(&book.id).ok())
                .unwrap_or_default();
            pages.sort_by_key(|page| !self.is_favorite("book_pages", &page.id));
            let mut select = None;
            let mut remove = None;
            ui.horizontal_wrapped(|ui| {
                for page in &pages {
                    if !self.library.page_filter.matches(&page.tags)
                        || !matches_query(
                            &self.search_query.to_lowercase(),
                            &[&page.title, &page.body, &page.tags.join(" ")],
                        )
                    {
                        continue;
                    }
                    let response = ui.selectable_label(
                        self.library
                            .page
                            .as_ref()
                            .is_some_and(|current| current.id == page.id),
                        &page.title,
                    );
                    if response.clicked() {
                        select = Some(page.clone());
                    }
                    response.context_menu(|ui| {
                        if ui
                            .button(if self.is_favorite("book_pages", &page.id) {
                                "取消收藏"
                            } else {
                                "收藏并置顶"
                            })
                            .clicked()
                        {
                            self.toggle_favorite("book_pages", &page.id);
                            ui.close();
                        }
                        if ui.button("删除页面").clicked() {
                            remove = Some(page.id.clone());
                            ui.close();
                        }
                    });
                }
            });
            if let Some(page) = select
                && self.save_library_page()
            {
                self.library.page = self
                    .db
                    .as_ref()
                    .and_then(|db| db.book_pages(&book.id).ok())
                    .and_then(|pages| pages.into_iter().find(|saved| saved.id == page.id));
            }
            if let Some(id) = remove
                && self.save_library_page()
            {
                match self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.set_library_deleted(&id, true, true))
                {
                    Ok(()) => {
                        self.library.undo.push((id.clone(), true));
                        if self.library.page.as_ref().is_some_and(|page| page.id == id) {
                            self.library.page = None;
                        }
                    }
                    Err(error) => self.library.error = error.to_string(),
                }
            }
            let drafts = self
                .db
                .as_ref()
                .and_then(|db| db.book_drafts(&book.id, &self.library.writer).ok())
                .unwrap_or_default();
            for (key, draft) in drafts {
                ui.horizontal(|ui| {
                    ui.label(format!("未保存草稿 · {}", draft.title));
                    if ui.button("恢复草稿").clicked() {
                        self.recover_book_draft(&key, &draft);
                    }
                });
            }
            if let Some(page) = self.library.page.as_mut() {
                let mut changed = false;
                egui::Frame::new()
                    .fill(surface())
                    .corner_radius(16)
                    .inner_margin(24)
                    .show(ui, |ui| {
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut page.title)
                                    .font(egui::FontId::proportional(26.0))
                                    .frame(egui::Frame::NONE)
                                    .desired_width(f32::INFINITY),
                            )
                            .changed();
                        changed |= crate::ui_components::classification_dropdown(
                            ui,
                            "active-page-tags",
                            &mut page.tags,
                            &options,
                            220.0,
                        );
                        ui.add_space(16.0);
                        changed |= ui
                            .add(
                                egui::TextEdit::multiline(&mut page.body)
                                    .id_salt(("book-page", &page.id))
                                    .font(egui::FontId::proportional(17.0))
                                    .frame(egui::Frame::NONE)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(20)
                                    .hint_text("开始写这一页…"),
                            )
                            .changed();
                    });
                if changed {
                    self.library.dirty = true;
                    self.library.dirty_since = Some(Instant::now());
                    if let Some(db) = self.db.as_mut()
                        && let Err(error) = db.save_book_draft(&self.library.writer, page)
                    {
                        self.library.error = format!("草稿未写入，请手动保存：{error}");
                    }
                }
            } else {
                if ui.button("+ 新增第一页").clicked() {
                    self.new_library_page();
                }
            }
        } else {
            let mut books = self
                .db
                .as_ref()
                .and_then(|db| db.books().ok())
                .unwrap_or_default();
            let mut select = None;
            let mut remove = None;
            let mut add = false;
            books.sort_by_key(|item| !self.is_favorite("books", &item.id));
            collection(ui, "book-gallery", self.view_mode(), |ui, layout| {
                for book in books
                    .iter()
                    .filter(|book| {
                        self.record_matches_filter(TagScope::Books, &book.id)
                            && matches_query(
                                &self.search_query.to_lowercase(),
                                &[&book.title, &book.tags.join(" ")],
                            )
                    })
                    .cloned()
                    .collect::<Vec<_>>()
                {
                    let count = self
                        .db
                        .as_ref()
                        .and_then(|db| db.book_pages(&book.id).ok())
                        .map_or(0, |pages| pages.len());
                    let response = tagged_entry(
                        ui,
                        &book.id,
                        &book.title,
                        (&format!("{count} 页"), &book.tags),
                        TileKind::Document,
                        layout,
                    );
                    self.record_classification_dropdown(ui, &response, TagScope::Books, &book.id);
                    self.favorite_control(ui, &response, "books", &book.id);
                    if response.clicked() {
                        select = Some(book.clone());
                    }
                    response.context_menu(|ui| {
                        if ui.button("重命名图书").clicked() {
                            self.library.dialog = Some(book.clone());
                            ui.close();
                        }
                        if ui.button("删除图书").clicked() {
                            remove = Some(book.id.clone());
                            ui.close();
                        }
                    });
                }
                add = entry(ui, "add-book", "点击添加图书", "", TileKind::Add, layout).clicked();
            });
            if let Some(book) = select {
                self.select_book(book);
            }
            if add {
                self.library.dialog = Some(Book::new());
            }
            if let Some(id) = remove
                && let Some(db) = self.db.as_mut()
            {
                match db.set_library_deleted(&id, false, true) {
                    Ok(()) => self.library.undo.push((id, false)),
                    Err(error) => self.library.error = error.to_string(),
                }
            }
        }
        if !self.library.error.is_empty() {
            ui.colored_label(Color32::from_rgb(185, 65, 71), &self.library.error);
        }
    }
    pub(super) fn render_book_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut book) = self.library.dialog.clone() else {
            return;
        };
        let mut save = false;
        let options = self.names_for_scope(TagScope::Books);
        let result = centered_dialog(ctx, "book-details", "图书", "", 480.0, |ui| {
            ui.label("图书名称");
            ui.text_edit_singleline(&mut book.title);
            crate::ui_components::classification_dropdown(
                ui,
                "book-dialog-tags",
                &mut book.tags,
                &options,
                ui.available_width(),
            );
            save = ui.add(primary_button("保存图书")).clicked();
        });
        self.library.dialog = Some(book.clone());
        if dialog_closed(&result) {
            self.library.dialog = None;
        } else if save {
            match self
                .db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| db.save_book(&book))
            {
                Ok(()) => {
                    self.library.dialog = None;
                    self.select_book(book);
                }
                Err(error) => self.library.error = error.to_string(),
            }
        }
    }
}
