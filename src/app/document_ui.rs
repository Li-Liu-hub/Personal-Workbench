use super::*;
impl PersonalDeskApp {
    pub(super) fn render_document_toolbar(&mut self, ui: &mut egui::Ui) {
        let mut close = false;
        ui.horizontal(|ui| {
            ui.set_min_height(38.0);
            close = ui.button("← 全部文档").clicked();
            if let Some(book) = self.library.book.clone() {
                let width = (ui.available_width() - 320.0).clamp(70.0, 300.0);
                let title = crate::ui_components::toolbar_title(ui, &book.title, width, 14.0);
                if title.on_hover_text("重命名图书").clicked() {
                    self.library.dialog = Some(book);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("更多", |ui| {
                    if ui.button("导出 Word").clicked() {
                        ui.close();
                        self.export_word_dialog();
                    }
                    if ui.button("管理文档标签").clicked() {
                        self.open_tag_manager(TagScope::Documents);
                        ui.close();
                    }
                });
                self.render_document_category_menu(ui);
                if ui.button("保存").on_hover_text("Ctrl+S").clicked() {
                    self.save_editor();
                    self.save_library_page();
                }
                ui.label(
                    RichText::new(if self.editor_conflict {
                        "保存冲突"
                    } else if self.editor_dirty || self.library.dirty {
                        "有修改"
                    } else {
                        "已保存"
                    })
                    .small()
                    .color(muted()),
                );
            });
        });
        if close && (!self.editor_dirty || self.save_editor()) && self.save_library_page() {
            self.editor_visible = false;
            self.library.book = None;
            self.library.page = None;
            self.document_scroll_epoch = self.document_scroll_epoch.wrapping_add(1);
        }
    }

    fn render_document_category_menu(&mut self, ui: &mut egui::Ui) {
        let Some(path) = self.editor_path.clone().filter(|_| self.editor_visible) else {
            return;
        };
        let Some(id) = self
            .db
            .as_mut()
            .and_then(|db| db.document_record_key(&path).ok())
        else {
            return;
        };
        let mut selected = self.record_labels(TagScope::Documents, &id);
        let before = selected.clone();
        ui.menu_button("分类", |ui| {
            ui.set_min_width(170.0);
            if ui.selectable_label(selected.is_empty(), "未分类").clicked() {
                selected.clear();
            }
            for name in self.names_for_scope(TagScope::Documents) {
                let mut checked = selected.contains(&name);
                if ui.checkbox(&mut checked, &name).changed() {
                    if checked {
                        selected.push(name);
                    } else {
                        selected.retain(|tag| tag != &name);
                    }
                }
            }
            ui.separator();
            if ui.button("管理文档标签").clicked() {
                self.open_tag_manager(TagScope::Documents);
                ui.close();
            }
        });
        if selected != before {
            self.save_record_classification(TagScope::Documents, &id, &selected);
        }
    }

    pub(super) fn render_document_editor_inline(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let width = ui.available_width().min(920.0);
        let inset = ((ui.available_width() - width) / 2.0).max(0.0);
        let height = (ui.ctx().content_rect().bottom() - ui.cursor().top() - 38.0).max(380.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(inset);
            ui.vertical(|ui| {
                ui.set_width(width);
                egui::Frame::new()
                    .fill(surface())
                    .stroke(egui::Stroke::new(1.0, border()))
                    .corner_radius(10)
                    .inner_margin(egui::Margin::symmetric(32, 24))
                    .show(ui, |ui| {
                        ui.set_width((width - 64.0).max(100.0));
                        ui.set_min_height((height - 48.0).max(300.0));
                        let title = ui.add(
                            egui::TextEdit::singleline(&mut self.editor_title)
                                .font(egui::FontId::proportional(25.0))
                                .frame(egui::Frame::NONE)
                                .hint_text("文档标题")
                                .desired_width(f32::INFINITY),
                        );
                        if self.editor_focus_title {
                            title.request_focus();
                            self.editor_focus_title = false;
                        }
                        changed |= title.changed();
                        ui.add_space(16.0);
                        changed |= ui
                            .add(
                                egui::TextEdit::multiline(&mut self.editor_text)
                                    .id_salt("document-body-inline")
                                    .font(egui::FontId::proportional(16.0))
                                    .frame(egui::Frame::NONE)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(24)
                                    .hint_text("开始写正文…"),
                            )
                            .changed();
                    });
            });
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
