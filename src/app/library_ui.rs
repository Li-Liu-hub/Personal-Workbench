use super::*;
use crate::library::Book;
#[derive(Default)]
pub(super) struct LibraryState {
    pub book: Option<Book>,
    pub dialog: Option<Book>,
    pub error: String,
    pub deletion: Option<LibraryDeletion>,
}
#[derive(Clone)]
pub(super) struct LibraryDeletion {
    pub id: String,
    pub title: String,
    pub page: bool,
}
impl PersonalDeskApp {
    pub(super) fn request_library_deletion(&mut self, id: &str, title: &str, page: bool) {
        self.library.error.clear();
        self.library.deletion = Some(LibraryDeletion {
            id: id.into(),
            title: title.into(),
            page,
        });
    }

    pub(super) fn confirm_library_deletion(&mut self) -> bool {
        let Some(target) = self.library.deletion.clone() else {
            return false;
        };
        if !self.flush_writing() {
            self.library.error = self.writing.error.clone();
            return false;
        }
        let active_deleted = self.writing.page.as_ref().is_some_and(|p| {
            if target.page {
                p.id == target.id
            } else {
                self.book_is_within(&p.book_id, &target.id)
            }
        });
        let result = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| {
                let next = if target.page && active_deleted {
                    if let Some(page) = self.writing.page.as_ref() {
                        let pages = db.book_pages(&page.book_id)?;
                        pages
                            .iter()
                            .position(|p| p.id == target.id)
                            .and_then(|index| {
                                pages
                                    .get(index + 1)
                                    .or_else(|| index.checked_sub(1).and_then(|i| pages.get(i)))
                            })
                            .cloned()
                    } else {
                        None
                    }
                } else {
                    None
                };
                db.set_library_deleted(&target.id, target.page, true)?;
                Ok(next)
            });
        let next = match result {
            Ok(next) => next,
            Err(error) => {
                self.library.error = format!("删除失败：{error}");
                return false;
            }
        };
        if active_deleted {
            self.writing.page = None;
            self.writing.dirty = false;
            self.writing.floating = false;
            self.writing.error.clear();
            if let Some(page) = next {
                self.set_writing_page(page);
            }
        }
        if !target.page
            && self.library.book.as_ref().is_some_and(|b| {
                self.book_is_within(&b.id, &target.id)
                    || self
                        .db
                        .as_ref()
                        .and_then(|db| db.books().ok())
                        .is_some_and(|books| !books.iter().any(|book| book.id == b.id))
            })
        {
            self.library.book = None;
        }
        if !target.page
            && self.library.dialog.as_ref().is_some_and(|b| {
                self.book_is_within(&b.id, &target.id)
                    || self
                        .db
                        .as_ref()
                        .and_then(|db| db.books().ok())
                        .is_some_and(|books| !books.iter().any(|book| book.id == b.id))
            })
        {
            self.library.dialog = None;
        }
        self.writing.catalog_key = None;
        self.library.deletion = None;
        self.library.error.clear();
        self.notice = if target.page {
            "页面已删除"
        } else {
            "图书及其页面已删除"
        }
        .into();
        true
    }

    pub(super) fn render_library_deletion_dialog(&mut self, ctx: &egui::Context) {
        let Some(target) = self.library.deletion.clone() else {
            return;
        };
        let mut confirm = false;
        let mut cancel = false;
        let result = centered_dialog(
            ctx,
            "library-deletion",
            if target.page {
                "删除页面"
            } else {
                "删除图书"
            },
            "",
            440.,
            |ui| {
                ui.label(format!("确认删除《{}》？", target.title));
                if !target.page {
                    ui.label("这本图书、所有子图书及其中的页面也会一起删除。");
                }
                if !self.library.error.is_empty() {
                    ui.colored_label(Color32::from_rgb(184, 65, 71), &self.library.error);
                }
                ui.horizontal(|ui| {
                    cancel = ui.button("取消").clicked();
                    confirm = ui.button("确认删除").clicked();
                });
            },
        );
        if cancel || dialog_closed(&result) {
            self.library.deletion = None;
            self.library.error.clear();
        } else if confirm {
            self.confirm_library_deletion();
        }
    }

    pub(super) fn render_library(&mut self, ui: &mut egui::Ui) {
        self.render_writing_documents(ui);
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
            if !self.library.error.is_empty() {
                ui.colored_label(Color32::from_rgb(184, 65, 71), &self.library.error);
            }
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
                    self.enter_book(book);
                }
                Err(error) => self.library.error = error.to_string(),
            }
        }
    }
}
