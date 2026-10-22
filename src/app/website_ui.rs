use super::*;

impl PersonalDeskApp {
    pub(super) fn save_website_draft(&mut self) -> bool {
        let result = websites::prepare(self.website_draft.clone()).and_then(|item| {
            self.db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)?
                .save_website_classified(&item, &self.website_draft_tags)?;
            self.website_draft = item;
            Ok(())
        });
        match result {
            Ok(()) => {
                self.load_websites();
                self.notice = "网站已保存".into();
                self.website_error.clear();
                true
            }
            Err(error) => {
                self.website_error = error.to_string();
                false
            }
        }
    }
}

impl PersonalDeskApp {
    pub(super) fn render_websites(&mut self, ui: &mut egui::Ui) {
        let query = self.search_query.trim().to_lowercase();
        let mut visible = self
            .website_items
            .iter()
            .filter(|item| {
                self.record_matches_filter(TagScope::Websites, &item.id)
                    && matches_query(
                        &query,
                        &[&item.title, &item.url, &item.category, &item.notes],
                    )
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut selected = None;
        let mut delete = None;
        let mut open = None;
        let mut add = false;
        visible.sort_by_key(|item| !self.is_favorite("websites", &item.id));
        collection(ui, "website-gallery", self.view_mode(), |ui, side| {
            for item in &visible {
                let labels = self.record_labels(TagScope::Websites, &item.id);
                let (response, quick_open) = website_entry(ui, item, side, &labels);
                self.favorite_control(ui, &response, "websites", &item.id);
                self.record_classification_dropdown(ui, &response, TagScope::Websites, &item.id);
                if quick_open {
                    open = Some(item.clone());
                }
                if response.clicked() {
                    selected = Some(item.clone());
                }
                response.context_menu(|ui| {
                    self.record_tag_menu(ui, TagScope::Websites, &item.id);
                    if ui.button("打开网站").clicked() {
                        open = Some(item.clone());
                        ui.close();
                    }
                    if ui.button("复制网址").clicked() {
                        ui.ctx().copy_text(item.url.clone());
                        ui.close();
                    }
                    if ui.button("编辑网站").clicked() {
                        selected = Some(item.clone());
                        ui.close();
                    }
                    if ui.button("删除网站").clicked() {
                        delete = Some(item.id.clone());
                        ui.close();
                    }
                });
            }
            add = entry(
                ui,
                "add",
                "点击添加网站",
                "保存网址、分类和备注",
                TileKind::Add,
                side,
            )
            .clicked();
        });
        if let Some(item) = selected {
            self.website_draft_tags = self.record_labels(TagScope::Websites, &item.id);
            self.website_draft = item;
            self.website_editing = true;
            self.website_error.clear();
            self.website_dialog_open = true;
        }
        if let Some(item) = open {
            self.website_open_request = Some(item);
        }
        if let Some(id) = delete
            && let Some(db) = self.db.as_mut()
        {
            match db.delete_website(&id) {
                Ok(()) => {
                    self.load_websites();
                    self.notice = "网站收藏已删除".into();
                }
                Err(error) => self.notice = format!("删除失败：{error}"),
            }
        }
        if add {
            self.add_current_item();
        }
    }
}
