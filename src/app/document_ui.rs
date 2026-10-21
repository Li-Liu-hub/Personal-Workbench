use super::*;
impl PersonalDeskApp {
    pub(super) fn render_document_toolbar(&mut self, ui: &mut egui::Ui) {
        let mut close = false;
        ui.horizontal_wrapped(|ui| {
            close = ui.button("← 全部文档").clicked();
            if let Some(book) = self.library.book.clone()
                && ui.button(&book.title).on_hover_text("重命名图书").clicked()
            {
                self.library.dialog = Some(book);
            }
            if ui.button("保存").clicked() {
                self.save_editor();
            }
            if ui.button("导出 Word").clicked() {
                self.export_word_dialog();
            }
            ui.label(
                RichText::new(if self.editor_dirty {
                    "正在保存…"
                } else {
                    "已保存"
                })
                .small()
                .color(muted()),
            );
        });
        if close && (!self.editor_dirty || self.save_editor()) && self.save_library_page() {
            self.editor_visible = false;
            self.library.book = None;
            self.library.page = None;
            self.document_scroll_epoch = self.document_scroll_epoch.wrapping_add(1);
        }
    }
    pub(super) fn render_document_editor_inline(&mut self, ui: &mut egui::Ui) {
        if let Some(path) = self.editor_path.clone() {
            let id = self
                .db
                .as_mut()
                .and_then(|db| db.document_record_key(&path).ok());
            if let Some(id) = id {
                let mut tags = self.record_labels(TagScope::Documents, &id);
                let options = self.names_for_scope(TagScope::Documents);
                ui.horizontal_wrapped(|ui| {
                    if crate::ui_components::classification_dropdown(
                        ui,
                        "editor-tags",
                        &mut tags,
                        &options,
                        240.0,
                    ) {
                        self.save_record_classification(TagScope::Documents, &id, &tags);
                    }
                    if ui.button("管理文档标签").clicked() {
                        self.open_tag_manager(TagScope::Documents);
                    }
                });
            }
        }
        let mut changed = false;
        ui.add_space(12.0);
        egui::Frame::new()
            .fill(surface())
            .corner_radius(16)
            .inner_margin(28)
            .show(ui, |ui| {
                let title = ui.add(
                    egui::TextEdit::singleline(&mut self.editor_title)
                        .font(egui::FontId::proportional(28.0))
                        .frame(egui::Frame::NONE)
                        .hint_text("文档标题")
                        .desired_width(f32::INFINITY),
                );
                if self.editor_focus_title {
                    title.request_focus();
                    self.editor_focus_title = false;
                }
                changed |= title.changed();
                ui.add_space(18.0);
                changed |= ui
                    .add(
                        egui::TextEdit::multiline(&mut self.editor_text)
                            .id_salt("document-body-inline")
                            .font(egui::FontId::proportional(17.0))
                            .frame(egui::Frame::NONE)
                            .desired_width(f32::INFINITY)
                            .desired_rows(26)
                            .hint_text("开始写正文…"),
                    )
                    .changed();
            });
        if changed {
            self.editor_dirty = true;
            self.dirty_since = Some(Instant::now());
            self.save_recovery_draft();
        }
    }
    pub(super) fn delete_file_reversible(
        &mut self,
        path: &std::path::Path,
    ) -> crate::error::Result<()> {
        let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let documents = self
            .db
            .as_ref()
            .ok_or(DeskError::UnsupportedFormat)?
            .document_records()?
            .into_iter()
            .filter(|record| {
                let record_path = record
                    .path
                    .canonicalize()
                    .unwrap_or_else(|_| record.path.clone());
                record_path == normalized || record_path.starts_with(&normalized)
            })
            .map(|record| {
                let tags = self.record_labels(TagScope::Documents, &record.path.to_string_lossy());
                crate::undo::Document { record, tags }
            })
            .collect();
        self.undo.delete(path, documents)
    }
    pub(super) fn undo_last_delete(&mut self) {
        if self.section == Section::Books && self.undo_library() {
            return;
        }
        let result = self.undo.undo(|documents| {
            let db = self.db.as_mut().ok_or(DeskError::UnsupportedFormat)?;
            for document in documents {
                db.set_document_title(&document.record.path, &document.record.title)?;
                let key = db.document_record_key(&document.record.path)?;
                db.set_record_tags(TagScope::Documents, &key, &document.tags)?;
                if let Ok((body, _)) = documents::read_text_with_fingerprint(&document.record.path)
                {
                    db.index_file(
                        "documents",
                        &document.record.path,
                        &document.record.title,
                        &body,
                    )?;
                }
            }
            Ok(())
        });
        match result {
            Ok(path) => {
                self.refresh_files();
                self.notice = format!(
                    "已恢复 {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            Err(error) => self.notice = error.to_string(),
        }
    }
}
