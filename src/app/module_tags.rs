use super::*;

impl PersonalDeskApp {
    pub(super) fn record_classification_dropdown(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        scope: TagScope,
        id: &str,
    ) {
        if self.view_mode() == ViewMode::Grid {
            return;
        }
        let rect = response.rect;
        let area = if self.view_mode() == ViewMode::Grid {
            egui::Rect::from_min_size(
                rect.min + egui::vec2(20.0, 18.0),
                egui::vec2(
                    if matches!(scope, TagScope::Websites | TagScope::Documents) {
                        (rect.width() - 106.0).max(60.0)
                    } else {
                        rect.width() - 40.0
                    },
                    30.0,
                ),
            )
        } else {
            let x = if scope == TagScope::Websites {
                rect.right() - 218.0
            } else {
                rect.left() + 56.0 + (rect.width() - 76.0) * 0.76
            };
            egui::Rect::from_min_size(
                egui::pos2(x, rect.center().y - 14.0),
                egui::vec2(
                    if scope == TagScope::Websites {
                        130.0
                    } else {
                        (rect.right()
                            - x
                            - if scope == TagScope::Documents {
                                82.0
                            } else {
                                14.0
                            })
                        .max(60.0)
                    },
                    28.0,
                ),
            )
        };
        ui.painter()
            .rect_filled(area.expand(2.0), 4, crate::theme::surface());
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let normalized_id = if scope == TagScope::Documents {
            self.db
                .as_mut()
                .and_then(|db| db.document_record_key(std::path::Path::new(id)).ok())
                .unwrap_or_else(|| id.to_owned())
        } else {
            id.to_owned()
        };
        let id = normalized_id.as_str();
        let mut tags = self.record_labels(scope, id);
        if crate::ui_components::classification_dropdown(
            &mut child,
            ("record-category", scope.key(), id),
            &mut tags,
            &self.names_for_scope(scope),
            area.width(),
        ) {
            self.save_record_classification(scope, id, &tags);
        }
    }
    pub(super) fn save_record_classification(
        &mut self,
        scope: TagScope,
        id: &str,
        tags: &[String],
    ) {
        let result = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.set_record_tags(scope, id, tags));
        match result {
            Ok(()) => {
                if let Some(draft) = self.inline_credentials.get_mut(id) {
                    draft.tags = tags.to_vec();
                }
                if scope == TagScope::Inventory
                    && let Some(draft) = self.inline_inventory.get_mut(id)
                {
                    draft.tags = tags.to_vec();
                    draft.category = tags.join(" · ");
                }
                if scope == TagScope::Contacts && self.contact_draft.id == id {
                    self.contact_draft_tags = tags.to_vec();
                }
                if scope == TagScope::Credentials {
                    self.load_summaries();
                }
                if scope == TagScope::Inventory {
                    self.load_inventory();
                }
                if scope == TagScope::Websites {
                    self.load_websites();
                }
                self.load_tags();
                self.notice = "分类已保存".into();
            }
            Err(error) => self.notice = format!("保存分类失败：{error}"),
        }
    }
    pub(super) fn names_for_scope(&self, scope: TagScope) -> Vec<String> {
        self.db
            .as_ref()
            .and_then(|db| db.tags(scope).ok())
            .unwrap_or_default()
    }
    pub(super) fn record_labels(&self, scope: TagScope, id: &str) -> Vec<String> {
        self.db
            .as_ref()
            .and_then(|db| db.record_tags(scope, id).ok())
            .unwrap_or_default()
    }
    pub(super) fn record_matches_filter(&self, scope: TagScope, id: &str) -> bool {
        self.module_filters
            .get(scope.key())
            .is_none_or(|filter| filter.matches(&self.record_labels(scope, id)))
    }
    pub(super) fn record_tag_menu(&mut self, ui: &mut egui::Ui, scope: TagScope, id: &str) {
        if ui.button("设置标签").clicked() {
            let id = if scope == TagScope::Documents {
                match self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.document_record_key(std::path::Path::new(id)))
                {
                    Ok(id) => id,
                    Err(error) => {
                        self.notice = error.to_string();
                        ui.close();
                        return;
                    }
                }
            } else {
                id.to_owned()
            };
            self.tag_assignment = Some(TagAssignment {
                scope,
                selected: self.record_labels(scope, &id),
                id,
            });
            self.tag_error.clear();
            ui.close();
        }
    }
    pub(super) fn render_module_filters(&mut self, ui: &mut egui::Ui) {
        if self.section == Section::Books && self.library.book.is_some() {
            return;
        }
        if self.section == Section::Folders && self.current_directory.is_some() {
            ui.horizontal_wrapped(|ui| {
                for kind in files::FileFilter::ALL {
                    ui.selectable_value(&mut self.file_filter, kind, kind.label());
                }
            });
            ui.add_space(12.0);
            return;
        }
        if let Some(scope) = TagScope::for_section(self.section)
            && !matches!(scope, TagScope::Credentials | TagScope::Inventory)
        {
            let names = self.names_for_scope(scope);
            let filter = self.module_filters.entry(scope.key().into()).or_default();
            if tag_filter_bar(ui, filter, &names) {
                self.open_tag_manager(scope);
            }
        }
    }
    pub(super) fn file_transfer_controls(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        path: &std::path::Path,
    ) {
        if crate::ui_components::file_transfer_button(ui, response, self.view_mode()) {
            self.copy_file_request = Some(path.to_path_buf());
        }
        if response.drag_started_by(egui::PointerButton::Primary) && !self.detail_open() {
            self.drag_file_request = Some(path.to_path_buf());
        }
    }
}
