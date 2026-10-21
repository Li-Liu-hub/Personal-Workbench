use super::*;

#[derive(Clone)]
pub(super) struct DocumentDrag(pub PathBuf);

impl PersonalDeskApp {
    pub(super) fn group_document_paths(&mut self, paths: &[PathBuf], book: Option<&str>) -> bool {
        if self.editor_dirty && !self.save_editor() {
            return false;
        }
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
        if crate::ui_components::file_transfer_button(ui, &response, self.view_mode()) {
            self.copy_file_request = Some(path.to_path_buf());
        }
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
    pub(super) fn document_outbound_drag(&mut self, ctx: &egui::Context) {
        if let Some(payload) = egui::DragAndDrop::payload::<DocumentDrag>(ctx)
            && let Some(at) = ctx.input(|i| i.pointer.hover_pos())
        {
            let title = self
                .db
                .as_ref()
                .and_then(|db| db.document_title(&payload.0).ok())
                .unwrap_or_else(|| "文档".into());
            egui::Area::new(egui::Id::unique("document-drag-preview"))
                .order(egui::Order::Tooltip)
                .fixed_pos(at + egui::vec2(14., 14.))
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(title);
                    });
                });
        }
        if ctx.input(|i| i.pointer.primary_down() && i.pointer.hover_pos().is_none())
            && let Some(payload) = egui::DragAndDrop::payload::<DocumentDrag>(ctx)
        {
            self.drag_file_request = Some(payload.0.clone());
            egui::DragAndDrop::clear_payload(ctx);
        }
    }
    pub(super) fn open_document_book(&mut self, book: crate::library::Book) {
        if (self.editor_dirty && !self.save_editor()) || !self.save_library_page() {
            return;
        }
        let first = self
            .db
            .as_ref()
            .and_then(|db| db.book_documents(&book.id).ok())
            .and_then(|items| items.first().cloned());
        self.library.book = Some(book.clone());
        self.editor_visible = false;
        self.library.page = self
            .db
            .as_ref()
            .and_then(|db| db.book_pages(&book.id).ok())
            .and_then(|p| p.first().cloned());
        self.document_scroll_epoch = self.document_scroll_epoch.wrapping_add(1);
        if let Some(doc) = first {
            self.open_editor(doc.path);
        }
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
    pub(super) fn render_book_tabs(&mut self, ui: &mut egui::Ui) {
        let Some(book) = self.library.book.clone() else {
            return;
        };
        let docs = self
            .db
            .as_ref()
            .and_then(|db| db.book_documents(&book.id).ok())
            .unwrap_or_default();
        let mut selected = None;
        let mut remove = None;
        egui::ScrollArea::horizontal()
            .id_salt("book-document-tabs")
            .max_height(36.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for doc in docs {
                        let response = ui.selectable_label(
                            self.editor_visible && self.editor_path.as_ref() == Some(&doc.path),
                            RichText::new(&doc.title).size(13.0),
                        );
                        if response.clicked() {
                            selected = Some(doc.path.clone());
                        }
                        response.context_menu(|ui| {
                            if ui.button("移出图书").clicked() {
                                remove = Some(doc.path.clone());
                                ui.close();
                            }
                        });
                    }
                    if ui.button("+").on_hover_text("新增文档").clicked() {
                        self.create_internal_document();
                    }
                });
            });
        if let Some(path) = selected {
            self.open_editor(path);
        }
        if let Some(path) = remove
            && (!self.editor_dirty || self.save_editor())
        {
            if let Some(db) = self.db.as_mut()
                && let Err(error) = db.ungroup_document(&path)
            {
                self.notice = error.to_string();
                return;
            }
            self.open_document_book(book);
        }
    }
    pub(super) fn render_book_documents(&mut self, ui: &mut egui::Ui) {
        let linked = self
            .library
            .book
            .as_ref()
            .and_then(|book| {
                self.db
                    .as_ref()
                    .and_then(|db| db.book_documents(&book.id).ok())
            })
            .is_some_and(|docs| !docs.is_empty());
        if linked {
            self.render_document_editor_inline(ui);
        } else {
            self.render_library(ui);
        }
    }
}
