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
