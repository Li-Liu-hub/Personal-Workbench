use super::*;
use crate::library::Book;
#[derive(Default)]
pub(super) struct LibraryState {
    pub book: Option<Book>,
    pub dialog: Option<Book>,
    pub error: String,
}
impl PersonalDeskApp {
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
                    self.library.book = Some(book);
                }
                Err(error) => self.library.error = error.to_string(),
            }
        }
    }
}
