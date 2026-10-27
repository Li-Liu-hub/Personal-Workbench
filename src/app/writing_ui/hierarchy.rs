use super::*;
use crate::library::Book;

#[derive(Clone)]
pub(super) struct BookDrag(pub String);
pub(super) enum LibraryDrop {
    Pages(Vec<String>, Option<String>),
    Book(String, Option<String>),
}

pub(super) fn is_within(books: &[Book], id: &str, ancestor: &str) -> bool {
    let mut current = Some(id);
    for _ in 0..=books.len() {
        let Some(id) = current else {
            return false;
        };
        if id == ancestor {
            return true;
        }
        current = books
            .iter()
            .find(|b| b.id == id)
            .and_then(|b| b.parent_id.as_deref());
    }
    false
}

fn highlight(ui: &egui::Ui, response: &egui::Response) {
    ui.painter().rect_stroke(
        response.rect,
        8,
        egui::Stroke::new(2., accent()),
        egui::StrokeKind::Inside,
    );
}
pub(super) fn page_drop(ui: &egui::Ui, response: &egui::Response, id: &str) -> Option<LibraryDrop> {
    if response
        .dnd_hover_payload::<WritingDrag>()
        .is_some_and(|p| p.0 != id)
    {
        highlight(ui, response);
    }
    response
        .dnd_release_payload::<WritingDrag>()
        .filter(|p| p.0 != id)
        .map(|p| LibraryDrop::Pages(vec![id.into(), p.0.clone()], None))
}
pub(super) fn book_drop(ui: &egui::Ui, response: &egui::Response, id: &str) -> Option<LibraryDrop> {
    if response.dnd_hover_payload::<WritingDrag>().is_some()
        || response
            .dnd_hover_payload::<BookDrag>()
            .is_some_and(|p| p.0 != id)
    {
        highlight(ui, response);
    }
    if response.dnd_hover_payload::<WritingDrag>().is_some() {
        return response
            .dnd_release_payload::<WritingDrag>()
            .map(|p| LibraryDrop::Pages(vec![p.0.clone()], Some(id.into())));
    }
    response
        .dnd_release_payload::<BookDrag>()
        .filter(|p| p.0 != id)
        .map(|p| LibraryDrop::Book(p.0.clone(), Some(id.into())))
}
impl PersonalDeskApp {
    // Catalog and editor use the same expansion state. Opening a nested document
    // keeps the directory that the user was browsing.
    pub(super) fn render_nested_book_catalog(
        &mut self,
        ui: &mut egui::Ui,
        books: &[Book],
        filter: &TagFilter,
    ) -> bool {
        for book in books {
            let all_books = self
                .db
                .as_ref()
                .and_then(|db| db.books().ok())
                .unwrap_or_default();
            let members = self.writing.catalog.iter().any(|page| {
                is_within(&all_books, &page.book_id, &book.id) && filter.matches(&page.tags)
            });
            let searching = !self.search_query.is_empty() || *filter != TagFilter::All;
            if searching && !members {
                continue;
            }
            let mut open = searching || self.writing.book_navigation.expanded.contains(&book.id);
            let arrow = if open { "▾" } else { "▸" };
            let response = ui.add_sized(
                [ui.available_width(), 34.],
                egui::Button::new(format!("{arrow} 图书 · {}", book.title))
                    .sense(egui::Sense::click_and_drag())
                    .wrap_mode(egui::TextWrapMode::Truncate),
            );
            self.favorite_control(ui, &response, "books", &book.id);
            if response.clicked() && !searching {
                open = !open;
                if open {
                    self.writing
                        .book_navigation
                        .expanded
                        .insert(book.id.clone());
                } else {
                    self.writing.book_navigation.expanded.remove(&book.id);
                }
            }
            response.dnd_set_drag_payload(BookDrag(book.id.clone()));
            let mut drop = book_drop(ui, &response, &book.id);
            response.context_menu(|ui| {
                if ui.button("重命名图书").clicked() {
                    self.library.dialog = Some(book.clone());
                    ui.close();
                }
                if ui.button("移到上一级").clicked() {
                    let parent = self
                        .parent_of(book.parent_id.as_deref())
                        .and_then(|book| book.parent_id);
                    drop = Some(LibraryDrop::Book(book.id.clone(), parent));
                    ui.close();
                }
                self.record_tag_menu(ui, TagScope::Books, &book.id);
                if ui.button("删除图书").clicked() {
                    self.request_library_deletion(&book.id, &book.title, false);
                    ui.close();
                }
            });
            if let Some(drop) = drop {
                self.apply_library_drop(drop);
                return true;
            }
            if open {
                let mut changed = false;
                ui.indent(("catalog-child-book", &book.id), |ui| {
                    let children = self.child_books(Some(&book.id));
                    if self.render_nested_book_catalog(ui, &children, filter) {
                        changed = true;
                        return;
                    }
                    let pages = self
                        .writing
                        .catalog
                        .iter()
                        .filter(|page| page.book_id == book.id && filter.matches(&page.tags))
                        .cloned()
                        .collect::<Vec<_>>();
                    if pages.is_empty() && children.is_empty() {
                        ui.label(RichText::new("暂无页面").small().color(muted()));
                    }
                    for page in pages {
                        let layout = crate::ui_components::EntryLayout::new(
                            crate::views::ViewMode::List,
                            ui.available_width(),
                        );
                        let response = tagged_entry(
                            ui,
                            &page.id,
                            &page.title,
                            (&page.preview, &page.tags),
                            TileKind::Document,
                            layout,
                        );
                        self.favorite_control(ui, &response, "book_pages", &page.id);
                        self.record_classification_dropdown(
                            ui,
                            &response,
                            TagScope::BookPages,
                            &page.id,
                        );
                        response.dnd_set_drag_payload(WritingDrag(page.id.clone()));
                        if let Some(drop) = page_drop(ui, &response, &page.id) {
                            self.apply_library_drop(drop);
                            changed = true;
                            return;
                        }
                        response.context_menu(|ui| {
                            self.record_tag_menu(ui, TagScope::BookPages, &page.id);
                            if ui.button("删除文档").clicked() {
                                self.request_library_deletion(&page.id, &page.title, true);
                                ui.close();
                            }
                        });
                        if response.clicked() {
                            self.open_book_directory_page(&page.id);
                            changed = true;
                            return;
                        }
                    }
                });
                if changed {
                    return true;
                }
            }
        }
        false
    }

    pub(in crate::app) fn child_books(&self, parent: Option<&str>) -> Vec<Book> {
        self.db
            .as_ref()
            .and_then(|db| db.books().ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|b| b.parent_id.as_deref() == parent)
            .collect()
    }
    pub(in crate::app) fn parent_of(&self, id: Option<&str>) -> Option<Book> {
        self.db
            .as_ref()?
            .books()
            .ok()?
            .into_iter()
            .find(|b| Some(b.id.as_str()) == id)
    }
    pub(in crate::app) fn book_is_within(&self, id: &str, ancestor: &str) -> bool {
        let mut current = Some(id.to_owned());
        let books = self
            .db
            .as_ref()
            .and_then(|db| db.books().ok())
            .unwrap_or_default();
        for _ in 0..=books.len() {
            let Some(id) = current else {
                return false;
            };
            if id == ancestor {
                return true;
            }
            current = books
                .iter()
                .find(|b| b.id == id)
                .and_then(|b| b.parent_id.clone());
        }
        false
    }
    pub(in crate::app) fn enter_book(&mut self, book: Book) -> bool {
        if !self.flush_writing() {
            return false;
        }
        let first = self
            .db
            .as_ref()
            .and_then(|db| db.book_pages(&book.id).ok())
            .and_then(|pages| pages.first().map(|p| p.id.clone()));
        self.writing.page = None;
        self.writing.floating = false;
        self.library.book = Some(book);
        if let Some(id) = first {
            self.open_writing(&id);
        }
        true
    }
    pub(in crate::app) fn open_parent_book(&mut self) {
        if !self.flush_writing() {
            return;
        }
        let parent = self
            .library
            .book
            .as_ref()
            .and_then(|b| self.parent_of(b.parent_id.as_deref()));
        self.writing.page = None;
        self.writing.floating = false;
        self.library.book = parent;
    }
    pub(in crate::app) fn new_child_book(&mut self) {
        if !self.flush_writing() {
            return;
        }
        let mut book = Book::new();
        book.parent_id = self.library.book.as_ref().map(|b| b.id.clone());
        self.library.error.clear();
        self.library.dialog = Some(book);
    }
    pub(super) fn apply_library_drop(&mut self, action: LibraryDrop) -> bool {
        match action {
            LibraryDrop::Pages(ids, target) => {
                self.group_written_documents(&ids, target.as_deref())
            }
            LibraryDrop::Book(id, parent) => {
                if !self.flush_writing() {
                    return false;
                }
                match self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.move_book(&id, parent.as_deref()))
                {
                    Ok(()) => {
                        self.writing.catalog_key = None;
                        self.notice = "图书已移动".into();
                        true
                    }
                    Err(e) => {
                        self.notice = e.to_string();
                        false
                    }
                }
            }
        }
    }
}
