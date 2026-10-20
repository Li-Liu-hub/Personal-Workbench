use super::*;
use crate::games::{self, Direction, Status};

impl PersonalDeskApp {
    pub(super) fn render_games(&mut self, ui: &mut egui::Ui) {
        if !self.snake_open {
            if matches_query(
                &self.search_query.to_lowercase(),
                &["无尽贪吃蛇", "snake", "小游戏"],
            ) {
                let mut open = false;
                collection(ui, "games-gallery", self.view_mode(), |ui, layout| {
                    open = entry(
                        ui,
                        "snake",
                        "无尽贪吃蛇",
                        "方向键 / WASD · 空格开始或暂停",
                        TileKind::Game,
                        layout,
                    )
                    .clicked();
                });
                if open {
                    self.snake_open = true;
                    self.snake = games::random_game();
                    self.snake_last_time = None;
                }
            }
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("返回小游戏").clicked() {
                self.snake.pause();
                self.snake_open = false;
                self.snake_last_time = None;
            }
            let label = match self.snake.status {
                Status::Ready => "开始游戏",
                Status::Running => "暂停",
                Status::Paused => "继续",
                _ => "本局已结束",
            };
            if ui
                .add_enabled(
                    !matches!(self.snake.status, Status::Over | Status::Filled),
                    primary_button(label),
                )
                .clicked()
            {
                if let Some(id) = ui.ctx().memory(|m| m.focused()) {
                    ui.ctx().memory_mut(|m| m.surrender_focus(id));
                }
                self.snake.toggle();
                self.snake_last_time = None;
            }
            if ui.button("重新开始").clicked() {
                self.snake = games::random_game();
                self.snake.toggle();
                self.snake_last_time = None;
            }
            ui.label(RichText::new(format!("得分  {}", self.snake.score)).strong());
            ui.label(format!("最高分  {}", self.snake_best));
        });
        if !self.snake_open {
            return;
        }
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let search_focused = self
            .search_field
            .is_some_and(|id| ctx.memory(|m| m.has_focus(id)));
        let blocked = self.detail_open() || search_focused || !ctx.input(|i| i.focused);
        if blocked {
            self.snake.pause();
            self.snake_last_time = None;
        } else {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
                self.snake.toggle();
                self.snake_last_time = None;
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.snake.pause();
                self.snake_last_time = None;
            }
            for (key, direction) in [
                (egui::Key::ArrowUp, Direction::Up),
                (egui::Key::W, Direction::Up),
                (egui::Key::ArrowDown, Direction::Down),
                (egui::Key::S, Direction::Down),
                (egui::Key::ArrowLeft, Direction::Left),
                (egui::Key::A, Direction::Left),
                (egui::Key::ArrowRight, Direction::Right),
                (egui::Key::D, Direction::Right),
            ] {
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                    self.snake.turn(direction);
                }
            }
            let delta = self
                .snake_last_time
                .map_or(0.0, |last| (now - last).max(0.0));
            self.snake_last_time = Some(now);
            self.snake.advance(delta);
        }
        if self.snake.score > self.snake_best {
            match self
                .db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| db.set_setting("games.snake.best", &self.snake.score.to_string()))
            {
                Ok(()) => self.snake_best = self.snake.score,
                Err(error) => self.notice = format!("最高分暂未保存：{error}"),
            }
        }
        if self.snake.status == Status::Running {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        ui.add_space(10.0);
        let cell = ((ctx.content_rect().height() - 430.0) / self.snake.height as f32)
            .min((ui.available_width() - 30.0) / self.snake.width as f32)
            .clamp(10.0, 23.0);
        let size = egui::vec2(
            self.snake.width as f32 * cell,
            self.snake.height as f32 * cell,
        );
        ui.vertical_centered(|ui| {
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
            if response.clicked() {
                response.request_focus();
            }
            let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
            painter.rect_filled(rect, 14, Color32::from_rgb(24, 32, 48));
            for x in 1..self.snake.width {
                let at = rect.left() + x as f32 * cell;
                painter.line_segment(
                    [egui::pos2(at, rect.top()), egui::pos2(at, rect.bottom())],
                    egui::Stroke::new(0.5, Color32::from_rgb(34, 44, 61)),
                );
            }
            for y in 1..self.snake.height {
                let at = rect.top() + y as f32 * cell;
                painter.line_segment(
                    [egui::pos2(rect.left(), at), egui::pos2(rect.right(), at)],
                    egui::Stroke::new(0.5, Color32::from_rgb(34, 44, 61)),
                );
            }
            let center = |c: games::Cell| {
                rect.min + egui::vec2((c.x as f32 + 0.5) * cell, (c.y as f32 + 0.5) * cell)
            };
            if let Some(food) = self.snake.food {
                painter.circle_filled(center(food), cell * 0.43, Color32::from_rgb(99, 51, 69));
                painter.circle_filled(center(food), cell * 0.27, Color32::from_rgb(255, 116, 133));
            }
            for (index, part) in self.snake.body.iter().enumerate().rev() {
                painter.rect_filled(
                    egui::Rect::from_center_size(center(*part), egui::vec2(cell - 2.0, cell - 2.0)),
                    4,
                    if index == 0 {
                        Color32::from_rgb(150, 240, 196)
                    } else {
                        Color32::from_rgb(73, 183, 140)
                    },
                );
            }
            if self.snake.status != Status::Running {
                painter.rect_filled(rect, 14, Color32::from_black_alpha(95));
                let title = match self.snake.status {
                    Status::Ready => "准备好了吗？",
                    Status::Paused => "已暂停",
                    Status::Over => "本局结束",
                    Status::Filled => "棋盘已填满！",
                    _ => "",
                };
                painter.text(
                    rect.center() - egui::vec2(0.0, 15.0),
                    egui::Align2::CENTER_CENTER,
                    title,
                    egui::FontId::proportional(25.0),
                    Color32::WHITE,
                );
                painter.text(
                    rect.center() + egui::vec2(0.0, 22.0),
                    egui::Align2::CENTER_CENTER,
                    if matches!(self.snake.status, Status::Over | Status::Filled) {
                        "点击重新开始，再来一局"
                    } else {
                        "空格开始 / 继续 · 方向键或 WASD 转向"
                    },
                    egui::FontId::proportional(13.0),
                    Color32::from_rgb(191, 204, 220),
                );
            }
        });
        ui.add_space(10.0);
        ui.label(
            RichText::new(
                "方向键 / WASD 转向 · 空格暂停或继续 · Esc 暂停 · 切换页面或离开窗口自动暂停",
            )
            .small()
            .color(muted()),
        );
    }
}
