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
                if scope == TagScope::BookPages
                    && let Some(page) = self.writing.page.as_mut()
                    && page.id == id
                {
                    page.tags = tags.to_vec();
                }
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
            .and_then(|db| db.cached_tag_names(scope).ok())
            .unwrap_or_default()
    }
    pub(super) fn record_labels(&self, scope: TagScope, id: &str) -> Vec<String> {
        self.db
            .as_ref()
            .and_then(|db| db.cached_record_tags(scope, id).ok())
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
        if let Some(scope) = TagScope::for_section(self.section) {
            let names = self.names_for_scope(scope);
            let filter = match scope {
                TagScope::Credentials => &mut self.account_filter,
                TagScope::Inventory => &mut self.inventory_filter,
                _ => self.module_filters.entry(scope.key().into()).or_default(),
            };
            if tag_filter_bar(ui, filter, &names) {
                self.open_tag_manager(scope);
            }
        }
    }
}

impl PersonalDeskApp {
    pub(super) fn load_tags(&mut self) {
        if let Some(db) = self.db.as_ref() {
            match (db.tags(TagScope::Credentials), db.tags(TagScope::Inventory)) {
                (Ok(accounts), Ok(inventory)) => {
                    self.account_tags = accounts;
                    self.inventory_tags = inventory;
                }
                _ => self.notice = "标签读取失败，请稍后重试".into(),
            }
        }
    }

    pub(super) fn open_tag_manager(&mut self, scope: TagScope) {
        if scope == TagScope::BookPages && !self.flush_writing() {
            return;
        }
        self.tag_manager = Some(scope);
        self.tag_name.clear();
        self.tag_original = None;
        self.tag_error.clear();
    }

    pub(super) fn sync_tag_edit(
        &mut self,
        scope: TagScope,
        original: &str,
        replacement: Option<&str>,
    ) {
        let rewrite = |values: &mut Vec<String>| {
            for value in values.iter_mut() {
                if value == original
                    && let Some(name) = replacement
                {
                    *value = name.to_owned();
                }
            }
            if replacement.is_none() {
                values.retain(|value| value != original);
            }
        };
        let filter = match scope {
            TagScope::Credentials => {
                rewrite(&mut self.credential_draft.tags);
                &mut self.account_filter
            }
            TagScope::Inventory => {
                rewrite(&mut self.inventory_draft.tags);
                &mut self.inventory_filter
            }
            _ => self.module_filters.entry(scope.key().into()).or_default(),
        };
        if *filter == TagFilter::Named(original.to_owned()) {
            *filter = replacement
                .map(|name| TagFilter::Named(name.to_owned()))
                .unwrap_or_default();
        }
        if let Some(assignment) = self.tag_assignment.as_mut()
            && assignment.scope == scope
        {
            rewrite(&mut assignment.selected);
        }
        if scope == TagScope::Contacts {
            rewrite(&mut self.contact_draft_tags);
        }
        if scope == TagScope::Inventory {
            for draft in self.inline_inventory.values_mut() {
                rewrite(&mut draft.tags);
                draft.category = draft.tags.join(" · ");
            }
        }
        if scope == TagScope::Books {
            if let Some(book) = self.library.book.as_mut() {
                rewrite(&mut book.tags);
            }
            if let Some(book) = self.library.dialog.as_mut() {
                rewrite(&mut book.tags);
            }
        }
        if scope == TagScope::BookPages
            && let Some(page) = self.writing.page.as_mut()
        {
            rewrite(&mut page.tags);
        }
        self.load_tags();
        self.load_summaries();
        self.load_inventory();
    }

    pub(super) fn render_tag_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut assignment) = self.tag_assignment.clone() {
            let names = self.names_for_scope(assignment.scope);
            let mut save = false;
            let dialog = centered_dialog(
                ctx,
                "assign-tags",
                "设置标签",
                "选择分类后保存，清空选择后归入未分类；不会删除原记录。",
                460.0,
                |ui| {
                    if tag_choices(ui, &mut assignment.selected, &names) {
                        self.open_tag_manager(assignment.scope);
                    }
                    if !self.tag_error.is_empty() {
                        ui.colored_label(Color32::from_rgb(184, 65, 71), &self.tag_error);
                    }
                    save = ui
                        .add_sized(
                            [ui.available_width(), 38.0],
                            primary_button(if assignment.scope == TagScope::Inventory {
                                "保存分类"
                            } else {
                                "保存标签"
                            }),
                        )
                        .clicked();
                },
            );
            let mut close = dialog_closed(&dialog);
            if save {
                let result = self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| {
                        db.set_record_tags(assignment.scope, &assignment.id, &assignment.selected)
                    });
                match result {
                    Ok(()) => {
                        if assignment.scope == TagScope::Inventory
                            && self.inventory_draft.id == assignment.id
                        {
                            self.inventory_draft.tags = assignment.selected.clone();
                            self.inventory_draft.category = assignment.selected.join(" · ");
                        }
                        if assignment.scope == TagScope::Credentials
                            && self.credential_draft.id == assignment.id
                        {
                            self.credential_draft.tags = assignment.selected.clone();
                        }
                        if assignment.scope == TagScope::Contacts
                            && self.contact_draft.id == assignment.id
                        {
                            self.contact_draft_tags = assignment.selected.clone();
                        }
                        if assignment.scope == TagScope::Inventory
                            && let Some(draft) = self.inline_inventory.get_mut(&assignment.id)
                        {
                            draft.tags = assignment.selected.clone();
                            draft.category = draft.tags.join(" · ");
                        }
                        self.load_tags();
                        self.load_summaries();
                        self.load_inventory();
                        self.notice = "标签已保存".into();
                        close = true;
                    }
                    Err(error) => self.tag_error = error.to_string(),
                }
            }
            self.tag_assignment = (!close).then_some(assignment);
        }
        if let Some(scope) = self.tag_manager {
            let names = self.names_for_scope(scope);
            let mut submit = false;
            let mut delete = None;
            let dialog = centered_dialog(
                ctx,
                "manage-tags",
                &format!("{}标签", scope.label()),
                "创建自己的分类；删除标签只会移除分类，不会删除卡片。",
                480.0,
                |ui| {
                    ui.label(if self.tag_original.is_some() {
                        "重命名标签"
                    } else {
                        "新增标签"
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.tag_name)
                            .hint_text("例如：工作、游戏、学习"),
                    );
                    ui.horizontal(|ui| {
                        submit = ui
                            .add(primary_button(if self.tag_original.is_some() {
                                "保存名称"
                            } else {
                                "创建标签"
                            }))
                            .clicked();
                        if self.tag_original.is_some() && ui.button("取消重命名").clicked() {
                            self.tag_original = None;
                            self.tag_name.clear();
                        }
                    });
                    if !self.tag_error.is_empty() {
                        ui.colored_label(Color32::from_rgb(184, 65, 71), &self.tag_error);
                    }
                    ui.separator();
                    if names.is_empty() {
                        ui.label("暂无标签，先创建一个分类吧。");
                    }
                    for name in names {
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - 170.0).max(40.0);
                            ui.add_sized([width, 30.0], egui::Label::new(&name).truncate())
                                .on_hover_text(&name);
                            if ui.button("重命名").clicked() {
                                self.tag_name = name.clone();
                                self.tag_original = Some(name.clone());
                                self.tag_error.clear();
                            }
                            if ui.button("删除标签").clicked() {
                                delete = Some(name);
                            }
                        });
                    }
                },
            );
            if submit {
                let result = tags::name(&self.tag_name).and_then(|name| {
                    let db = self.db.as_mut().ok_or(DeskError::UnsupportedFormat)?;
                    if let Some(original) = self.tag_original.as_ref() {
                        db.rename_tag(scope, original, &name)?;
                    } else {
                        db.create_tag(scope, &name)?;
                    }
                    Ok(name)
                });
                match result {
                    Ok(name) => {
                        if let Some(original) = self.tag_original.take() {
                            self.sync_tag_edit(scope, &original, Some(&name));
                        } else {
                            if let Some(assignment) = self.tag_assignment.as_mut()
                                && assignment.scope == scope
                            {
                                assignment.selected.push(name.clone());
                            }
                            if scope == TagScope::Credentials && self.credential_dialog_open {
                                self.credential_draft.tags.push(name.clone());
                            }
                            if scope == TagScope::Inventory && self.inventory_dialog_open {
                                self.inventory_draft.tags.push(name.clone());
                            }
                            self.load_tags();
                        }
                        self.tag_name.clear();
                        self.tag_error.clear();
                        self.notice = "标签已保存".into();
                    }
                    Err(error) => self.tag_error = error.to_string(),
                }
            }
            if let Some(name) = delete {
                match self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.delete_tag(scope, &name))
                {
                    Ok(()) => {
                        self.sync_tag_edit(scope, &name, None);
                        self.tag_error.clear();
                        if self.tag_original.as_deref() == Some(&name) {
                            self.tag_original = None;
                            self.tag_name.clear();
                        }
                        self.notice = "标签已删除，卡片仍保留".into();
                    }
                    Err(error) => self.tag_error = error.to_string(),
                }
            }
            if dialog_closed(&dialog) {
                self.tag_manager = None;
            }
        }
    }
}
