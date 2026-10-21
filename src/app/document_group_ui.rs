use super::*;

#[derive(Clone)]
pub(super) struct DocumentDrag(pub PathBuf);

impl PersonalDeskApp {
    pub(super) fn group_document_paths(&mut self, paths: &[PathBuf], book: Option<&str>) -> bool {
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.group_documents(paths, book))
        {
            Ok(_) => {
                self.notice = "已组成图书".into();
                true
            }
            Err(error) => {
                self.notice = error.to_string();
                false
            }
        }
    }
    pub(super) fn document_drag_target(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        path: &std::path::Path,
    ) {
        let response = response.interact(egui::Sense::click_and_drag());
        response.dnd_set_drag_payload(DocumentDrag(path.to_path_buf()));
        if let Some(payload) = response.dnd_hover_payload::<DocumentDrag>()
            && payload.0 != path
        {
            ui.painter().rect_stroke(
                response.rect,
                12,
                egui::Stroke::new(2., accent()),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(payload) = response.dnd_release_payload::<DocumentDrag>()
            && payload.0 != path
        {
            self.group_document_paths(&[path.to_path_buf(), payload.0.clone()], None);
        }
    }
    pub(super) fn open_document_book(&mut self, book: crate::library::Book) {
        self.library.book = Some(book);
    }
    pub(super) fn render_document_books(&mut self, ui: &mut egui::Ui) {
        let books = self
            .db
            .as_ref()
            .and_then(|db| db.books().ok())
            .unwrap_or_default();
        if books.is_empty() {
            return;
        }
        let mut selected = None;
        let mut drop = None;
        collection(ui, "document-books", self.view_mode(), |ui, layout| {
            for book in &books {
                let documents = self
                    .db
                    .as_ref()
                    .and_then(|db| db.book_documents(&book.id).ok())
                    .unwrap_or_default();
                let pages = self
                    .db
                    .as_ref()
                    .and_then(|db| db.book_pages(&book.id).ok())
                    .unwrap_or_default();
                let query = self.search_query.to_lowercase();
                if !matches_query(&query, &[&book.title, &book.tags.join(" ")])
                    && !documents.iter().any(|d| matches_query(&query, &[&d.title]))
                    && !pages
                        .iter()
                        .any(|p| matches_query(&query, &[&p.title, &p.body]))
                {
                    continue;
                }
                let filter = self
                    .module_filters
                    .get(TagScope::Documents.key())
                    .cloned()
                    .unwrap_or_default();
                if !filter.matches(&book.tags)
                    && !documents.iter().any(|d| {
                        self.record_matches_filter(TagScope::Documents, &d.path.to_string_lossy())
                    })
                {
                    continue;
                }
                let count = documents.len() + pages.len();
                let response = tagged_entry(
                    ui,
                    &book.id,
                    &book.title,
                    (&format!("{count} 份文档"), &book.tags),
                    TileKind::Folder,
                    layout,
                );
                self.favorite_control(ui, &response, "books", &book.id);
                if response.clicked() {
                    selected = Some(book.clone());
                }
                if response.dnd_hover_payload::<DocumentDrag>().is_some() {
                    ui.painter().rect_stroke(
                        response.rect,
                        12,
                        egui::Stroke::new(2., accent()),
                        egui::StrokeKind::Inside,
                    );
                }
                if let Some(payload) = response.dnd_release_payload::<DocumentDrag>() {
                    drop = Some((payload.0.clone(), book.id.clone()));
                }
                response.context_menu(|ui| {
                    if ui.button("重命名图书").clicked() {
                        self.library.dialog = Some(book.clone());
                        ui.close();
                    }
                    self.record_tag_menu(ui, TagScope::Books, &book.id);
                    if ui.button("移除图书（文档保留）").clicked() {
                        if let Some(db) = self.db.as_mut() {
                            match db.set_library_deleted(&book.id, false, true) {
                                Ok(()) => self.notice = "图书已移除，文档仍在列表中".into(),
                                Err(e) => self.notice = e.to_string(),
                            }
                        }
                        ui.close();
                    }
                });
            }
        });
        ui.add_space(12.);
        if let Some((path, id)) = drop {
            self.group_document_paths(&[path], Some(&id));
        }
        if let Some(book) = selected {
            self.open_document_book(book);
        }
    }
}
