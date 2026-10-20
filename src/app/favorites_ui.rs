use super::*;
impl PersonalDeskApp {
    pub(super) fn favorite_key(scope: &str, id: &str) -> (String, String) {
        let id = if matches!(scope, "files" | "documents") {
            let path = std::path::Path::new(id);
            files::shell_path(&path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
                .to_string_lossy()
                .to_lowercase()
        } else {
            id.to_owned()
        };
        (scope.into(), id)
    }
    pub(super) fn is_favorite(&self, scope: &str, id: &str) -> bool {
        self.favorites.contains(&Self::favorite_key(scope, id))
    }
    pub(super) fn toggle_favorite(&mut self, scope: &str, id: &str) {
        let key = Self::favorite_key(scope, id);
        let value = !self.favorites.contains(&key);
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.set_favorite(&key.0, &key.1, value))
        {
            Ok(()) => {
                if value {
                    self.favorites.insert(key);
                } else {
                    self.favorites.remove(&key);
                }
            }
            Err(error) => self.notice = error.to_string(),
        }
    }
    pub(super) fn favorite_control(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        scope: &str,
        id: &str,
    ) {
        let rect = response.rect;
        let area = if self.view_mode() == ViewMode::Grid {
            egui::Rect::from_min_size(
                rect.right_bottom() - egui::vec2(36.0, 36.0),
                egui::vec2(24.0, 24.0),
            )
        } else {
            egui::Rect::from_min_size(rect.min + egui::vec2(3.0, 3.0), egui::vec2(16.0, 19.0))
        };
        if !area.intersects(ui.clip_rect()) {
            return;
        }
        let selected = self.is_favorite(scope, id);
        let clicked = ui
            .place(
                area,
                egui::Button::new(
                    RichText::new(if selected { "★" } else { "☆" }).color(if selected {
                        accent()
                    } else {
                        muted()
                    }),
                )
                .frame(false),
            )
            .on_hover_text(if selected {
                "取消收藏"
            } else {
                "收藏并置顶"
            })
            .clicked();
        if clicked {
            self.toggle_favorite(scope, id);
        }
    }
}
