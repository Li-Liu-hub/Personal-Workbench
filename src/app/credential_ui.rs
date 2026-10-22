use super::*;
use crate::ui_components::{EntryLayout, credential_height};
use std::hash::{Hash, Hasher};

#[derive(Default)]
pub(super) struct CredentialLayoutCache {
    key: Option<u64>,
    cells: Vec<(Option<usize>, egui::Rect)>,
    height: f32,
}

impl PersonalDeskApp {
    /// Cache only indices and geometry, never another copy of account values.
    /// Rebuild after search, edits, favorites, resize, zoom or data changes.
    pub(super) fn credential_cells(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Vec<(Option<CredentialSummary>, egui::Rect, EntryLayout)> {
        let mode = self.view_mode();
        let width = ui.available_width();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (
            width.to_bits(),
            ui.ctx().pixels_per_point().to_bits(),
            mode.value(),
            &self.search_query,
            &self.account_filter,
            &self.credentials,
            &self.inline_credentials,
            &self.favorites,
        )
            .hash(&mut hash);
        let key = hash.finish();
        if self.credential_layout.key != Some(key) {
            let query = self.search_query.trim().to_lowercase();
            let mut indices = self
                .credentials
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    self.account_filter.matches(&item.tags)
                        && (matches_query(
                            &query,
                            &[
                                &item.title,
                                &item.username,
                                &item.password,
                                &item.website,
                                &item.notes,
                            ],
                        ) || item
                            .tags
                            .iter()
                            .any(|tag| tag.to_lowercase().contains(&query)))
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            indices.sort_by_key(|&index| {
                !self.is_favorite("credentials", &self.credentials[index].id)
            });
            let gap = if mode == ViewMode::Grid { 16. } else { 7. };
            let columns = if mode == ViewMode::Grid {
                ((width + gap) / 246.).floor().max(1.) as usize
            } else {
                1
            };
            let cell_width = (width - gap * (columns - 1) as f32) / columns as f32;
            let layout = EntryLayout::new(mode, cell_width);
            let mut cells = Vec::with_capacity(indices.len() + 1);
            let mut y = 0.;
            let items = indices
                .into_iter()
                .map(Some)
                .chain(std::iter::once(None))
                .collect::<Vec<_>>();
            for row in items.chunks(columns) {
                let height = row
                    .iter()
                    .map(|index| match index {
                        Some(index) => {
                            let item = &self.credentials[*index];
                            let draft = self.inline_credentials.get(&item.id);
                            credential_height(ui, draft.unwrap_or(item), layout, draft.is_some())
                        }
                        None => {
                            if mode == ViewMode::Grid {
                                cell_width
                            } else {
                                56.
                            }
                        }
                    })
                    .fold(0., f32::max);
                for (column, &index) in row.iter().enumerate() {
                    cells.push((
                        index,
                        egui::Rect::from_min_size(
                            egui::pos2(column as f32 * (cell_width + gap), y),
                            egui::vec2(cell_width, height),
                        ),
                    ));
                }
                y += height + gap;
            }
            self.credential_layout = CredentialLayoutCache {
                key: Some(key),
                cells,
                height: (y - gap).max(0.),
            };
        }
        let (_, area) = ui.allocate_space(egui::vec2(width, self.credential_layout.height));
        self.credential_layout
            .cells
            .iter()
            .filter_map(|(index, rect)| {
                let rect = rect.translate(area.min.to_vec2());
                // Rendering and cloning are restricted to the visible viewport.
                rect.intersects(ui.clip_rect()).then(|| {
                    (
                        index.map(|index| self.credentials[index].clone()),
                        rect,
                        EntryLayout::new(mode, rect.width()).with_min_height(rect.height()),
                    )
                })
            })
            .collect()
    }
}

impl PersonalDeskApp {
    pub(super) fn delete_credential_record(&mut self, id: &str) {
        let result = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.delete_credential(id));
        match result {
            Ok(()) => {
                self.inline_credentials.remove(id);
                if self.selected_credential.as_deref() == Some(id) {
                    self.credential_dialog_open = false;
                    self.selected_credential = None;
                    self.credential_draft = Credential::new();
                }
                self.load_summaries();
                self.notice = "账号记录已删除".into();
            }
            Err(error) => self.notice = format!("删除失败：{error}"),
        }
    }
}

impl PersonalDeskApp {
    pub(super) fn render_credentials(&mut self, ui: &mut egui::Ui) {
        let mut selected = None;
        let mut delete = None;
        let mut add = false;

        credential_list_header(ui, self.view_mode());
        let cells = self.credential_cells(ui);
        for (item, rect, side) in cells {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt((
                        "credential-cell",
                        item.as_ref().map(|v| v.id.as_str()).unwrap_or("add"),
                    ))
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            let ui = &mut child;
            if let Some(item) = item {
                let editing = self.inline_credentials.contains_key(&item.id);
                let mut draft = self
                    .inline_credentials
                    .get(&item.id)
                    .unwrap_or(&item)
                    .clone();
                let response = credential_entry(ui, &mut draft, side, editing, &self.account_tags);
                self.favorite_control(ui, &response.response, "credentials", &item.id);
                if response.classification_changed {
                    self.save_record_classification(TagScope::Credentials, &item.id, &draft.tags);
                }
                if response.changed {
                    self.inline_credentials.insert(item.id.clone(), draft);
                }
                if response.cancel {
                    self.inline_credentials.remove(&item.id);
                } else if response.save {
                    self.save_inline_credential(&item.id);
                }
                if response.details
                    || (self.view_mode() == ViewMode::Grid && response.response.clicked())
                {
                    selected = Some(item.id.clone());
                }
                response.response.context_menu(|ui| {
                    if ui.button("查看详情").clicked() {
                        selected = Some(item.id.clone());
                        ui.close();
                    }
                    if ui.button("复制账号").clicked() {
                        ui.ctx().copy_text(item.username.clone());
                        ui.close();
                    }
                    if ui.button("复制密码").clicked() {
                        ui.ctx().copy_text(item.password.clone());
                        ui.close();
                    }
                    if ui.button("设置标签").clicked() {
                        self.tag_assignment = Some(TagAssignment {
                            scope: TagScope::Credentials,
                            id: item.id.clone(),
                            selected: item.tags.clone(),
                        });
                        self.tag_error.clear();
                        ui.close();
                    }
                    if ui.button("删除账号").clicked() {
                        delete = Some(item.id.clone());
                        ui.close();
                    }
                });
            } else if entry(
                ui,
                "add",
                "点击添加账号",
                "保存网站或软件账号",
                TileKind::Add,
                side,
            )
            .clicked()
            {
                add = true;
            }
        }
        if let Some(id) = delete {
            self.delete_credential_record(&id);
        } else if let Some(id) = selected {
            let entry = match self.db.as_ref() {
                Some(db) => db.credential(&id),
                None => return,
            };
            match entry {
                Ok(Some(item)) => {
                    self.credential_draft = item;
                    if let Some(draft) = self.inline_credentials.get(&id) {
                        self.credential_draft.title = draft.title.clone();
                        self.credential_draft.username = draft.username.clone();
                        self.credential_draft.password = draft.password.clone();
                    }
                    self.selected_credential = Some(id);
                    self.credential_dialog_open = true;
                }
                Ok(None) => self.notice = "该账号记录已不存在".into(),
                Err(error) => self.notice = format!("读取账号失败：{error}"),
            }
        }
        if add {
            self.add_current_item();
        }
    }
}
