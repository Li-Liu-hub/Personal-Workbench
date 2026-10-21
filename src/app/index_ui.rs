use super::*;
impl PersonalDeskApp {
    pub(super) fn add_document_indices(&mut self, paths: &[PathBuf]) {
        let mut added = vec![];
        for path in paths {
            if !path.is_file() {
                self.notice = "请选择已有文件".into();
                continue;
            }
            match self
                .db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| db.touch_document(path))
            {
                Ok(()) => added.push(path.clone()),
                Err(e) => {
                    self.notice = e.to_string();
                    return;
                }
            }
        }
        if let Some(book) = self.library.book.clone()
            && !added.is_empty()
        {
            self.group_document_paths(&added, Some(&book.id));
        }
        self.notice = format!("已添加 {} 个索引", added.len());
    }
    pub(super) fn pick_document_indices(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("选择文档文件")
            .pick_files()
        {
            self.add_document_indices(&paths);
        }
    }
    pub(super) fn request_location(&mut self, path: PathBuf) {
        match files::location_target(&path) {
            Ok(_) => self.location_request = Some(path),
            Err(e) => self.notice = e.to_string(),
        }
    }
    pub(super) fn index_undo_available(&self) -> bool {
        self.db
            .as_ref()
            .is_some_and(|db| db.index_undo_available().unwrap_or(false))
    }
    pub(super) fn undo_index_removal(&mut self) {
        self.notice = match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.undo_index_removal())
        {
            Ok(true) => "已恢复索引".into(),
            Ok(false) => "没有可恢复的索引".into(),
            Err(e) => e.to_string(),
        };
    }
    pub(super) fn remove_index(&mut self, scope: &str, key: &str) {
        self.notice = match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.remove_index(scope, key))
        {
            Ok(()) => "索引已移除".into(),
            Err(e) => e.to_string(),
        };
    }
    fn index_location_button(&self, ui: &mut egui::Ui, response: &egui::Response) -> bool {
        let rect = response.rect;
        let at = if self.view_mode() == ViewMode::Grid {
            egui::pos2(rect.right() - 78., rect.top() + 14.)
        } else {
            egui::pos2(rect.right() - 76., rect.center().y - 14.)
        };
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(response.id.with("location"))
                .max_rect(egui::Rect::from_min_size(at, egui::vec2(64., 28.)))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child
            .add_sized([64., 28.], egui::Button::new("位置"))
            .on_hover_text("打开文件所在位置")
            .clicked()
    }
    pub(super) fn render_documents(&mut self, ui: &mut egui::Ui) {
        let records = if let Some(book) = &self.library.book {
            self.db
                .as_ref()
                .and_then(|db| db.book_documents(&book.id).ok())
                .unwrap_or_default()
        } else {
            self.render_document_books(ui);
            self.db
                .as_ref()
                .and_then(|db| db.document_records().ok())
                .unwrap_or_default()
                .into_iter()
                .filter(|d| {
                    self.db
                        .as_ref()
                        .is_none_or(|db| db.document_book(&d.path).ok().flatten().is_none())
                })
                .collect()
        };
        let mut records = records
            .into_iter()
            .filter(|d| {
                let id = d.path.to_string_lossy();
                self.record_matches_filter(TagScope::Documents, &id)
                    && matches_query(
                        &self.search_query.to_lowercase(),
                        &[
                            &d.title,
                            &id,
                            &self.record_labels(TagScope::Documents, &id).join(" "),
                        ],
                    )
            })
            .collect::<Vec<_>>();
        records.sort_by_key(|d| !self.is_favorite("documents", &d.path.to_string_lossy()));
        let mut open = None;
        let mut remove = None;
        let mut add = false;
        collection(ui, "documents-gallery", self.view_mode(), |ui, layout| {
            for d in &records {
                let id = d.path.to_string_lossy();
                let labels = self.record_labels(TagScope::Documents, &id);
                let response = tagged_entry(
                    ui,
                    &id,
                    &d.title,
                    (&files::shell_path(&d.path).display().to_string(), &labels),
                    TileKind::Document,
                    layout,
                );
                self.favorite_control(ui, &response, "documents", &id);
                self.record_classification_dropdown(ui, &response, TagScope::Documents, &id);
                self.document_drag_target(ui, &response, &d.path);
                if self.index_location_button(ui, &response) || response.clicked() {
                    open = Some(d.path.clone());
                }
                response.context_menu(|ui| {
                    if ui.button("打开文件所在位置").clicked() {
                        open = Some(d.path.clone());
                        ui.close();
                    }
                    self.record_tag_menu(ui, TagScope::Documents, &id);
                    if ui.button("删除索引").clicked() {
                        remove = Some(id.to_string());
                        ui.close();
                    }
                });
            }
            add = entry(ui, "add", "添加文档索引", "", TileKind::Add, layout).clicked();
        });
        if let Some(path) = open {
            self.request_location(path);
        }
        if let Some(key) = remove {
            self.remove_index("documents", &key);
        }
        if add {
            self.pick_document_indices();
        }
    }
    pub(super) fn render_folders(&mut self, ui: &mut egui::Ui) {
        let mut items = self
            .db
            .as_ref()
            .and_then(|db| db.shortcuts().ok())
            .unwrap_or_default();
        items.retain(|s| {
            self.record_matches_filter(TagScope::Shortcuts, &s.id)
                && matches_query(
                    &self.search_query.to_lowercase(),
                    &[
                        &s.name,
                        &s.path.to_string_lossy(),
                        &self.record_labels(TagScope::Shortcuts, &s.id).join(" "),
                    ],
                )
        });
        items.sort_by_key(|s| !self.is_favorite("shortcuts", &s.id));
        let mut open = None;
        let mut remove = None;
        let mut add = false;
        collection(ui, "shortcuts-gallery", self.view_mode(), |ui, layout| {
            for item in &items {
                let labels = self.record_labels(TagScope::Shortcuts, &item.id);
                let response = tagged_entry(
                    ui,
                    &item.id,
                    &item.name,
                    (
                        &files::shell_path(&item.path).display().to_string(),
                        &labels,
                    ),
                    if item.is_dir {
                        TileKind::Folder
                    } else {
                        TileKind::Document
                    },
                    layout,
                );
                self.favorite_control(ui, &response, "shortcuts", &item.id);
                self.record_classification_dropdown(ui, &response, TagScope::Shortcuts, &item.id);
                if self.index_location_button(ui, &response) || response.clicked() {
                    open = Some(item.path.clone());
                }
                response.context_menu(|ui| {
                    if ui.button("打开所在位置").clicked() {
                        open = Some(item.path.clone());
                        ui.close();
                    }
                    self.record_tag_menu(ui, TagScope::Shortcuts, &item.id);
                    if ui.button("删除索引").clicked() {
                        remove = Some(item.id.clone());
                        ui.close();
                    }
                });
            }
            add = entry(ui, "add", "添加快捷路径", "", TileKind::Add, layout).clicked();
        });
        if let Some(path) = open {
            self.request_location(path);
        }
        if let Some(key) = remove {
            self.remove_index("shortcuts", &key);
        }
        if add {
            self.shortcut_dialog_open = true;
        }
    }
    pub(super) fn render_index_book_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("← 全部文档").clicked() {
                self.library.book = None;
            }
            if let Some(book) = self.library.book.clone()
                && ui.button(&book.title).clicked()
            {
                self.library.dialog = Some(book);
            }
        });
    }
}
