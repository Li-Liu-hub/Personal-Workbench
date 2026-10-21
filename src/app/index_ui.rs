use super::*;
impl PersonalDeskApp {
    #[cfg(test)]
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
}
