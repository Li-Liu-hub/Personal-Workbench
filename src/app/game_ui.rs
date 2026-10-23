use super::*;
use crate::games::{
    self, Direction, Status,
    records::{Library, Record, Saved},
    tetris::Tetris,
};
use crate::ui_components::chrome;
mod board;
mod gallery;
#[cfg(test)]
mod tests;

pub(super) struct GameState {
    library: Option<Library>,
    selected: String,
    history: bool,
    history_limit: u32,
    history_rows: Vec<Record>,
    history_key: Option<(u64, u32)>,
    tetris_open: bool,
    tetris: Tetris,
    snake_run: String,
    tetris_run: String,
    saved_at: f64,
    dirty: bool,
    error: String,
}
impl Default for GameState {
    fn default() -> Self {
        Self {
            library: None,
            selected: String::new(),
            history: false,
            history_limit: 100,
            history_rows: vec![],
            history_key: None,
            tetris_open: false,
            tetris: Tetris::new(1, 0),
            snake_run: uuid::Uuid::new_v4().to_string(),
            tetris_run: uuid::Uuid::new_v4().to_string(),
            saved_at: 0.,
            dirty: false,
            error: String::new(),
        }
    }
}
fn seed() -> u64 {
    uuid::Uuid::new_v4().as_u128() as u64
}
fn ended(status: Status) -> bool {
    matches!(status, Status::Over | Status::Filled | Status::Cleared)
}
fn mode(stage: u8) -> String {
    if stage == 0 {
        "无尽模式".into()
    } else {
        format!("第 {stage} 关")
    }
}

impl PersonalDeskApp {
    #[cfg(debug_assertions)]
    pub(super) fn preview_game(&mut self, game: &str, stage: u8, play: bool) {
        self.section = Section::Games;
        self.search_query.clear();
        self.snake_open = false;
        self.games.tetris_open = false;
        self.games.selected = game.into();
        self.games.history = false;
        if play {
            self.start_game(game, stage);
        }
    }

    pub(super) fn initialize_games(&mut self) {
        match self
            .db
            .as_ref()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.game_library())
        {
            Ok(library) => {
                self.snake_best = library.snake_best;
                if let Some(saved) = &library.snake {
                    self.snake = saved.game.clone();
                    self.games.snake_run = saved.run_id.clone();
                }
                if let Some(saved) = &library.tetris {
                    self.games.tetris = saved.game.clone();
                    self.games.tetris_run = saved.run_id.clone();
                }
                self.games.library = Some(library);
            }
            Err(e) => self.games.error = e.to_string(),
        }
    }
    pub(super) fn flush_games(&mut self) -> bool {
        self.snake.pause();
        self.games.tetris.pause();
        self.snake_last_time = None;
        if !self.snake_open && !self.games.tetris_open && !self.games.dirty {
            return true;
        }
        self.save_games()
    }
    fn finish_game(&mut self, game: &str, abandon: bool) {
        let (run_id, stage, score, lines, status) = if game == "snake" {
            (
                self.games.snake_run.clone(),
                self.snake.level,
                self.snake.score,
                0,
                self.snake.status,
            )
        } else {
            (
                self.games.tetris_run.clone(),
                self.games.tetris.stage,
                self.games.tetris.score,
                self.games.tetris.lines,
                self.games.tetris.status,
            )
        };
        if !ended(status) && (!abandon || status == Status::Ready) {
            return;
        }
        if let Some(library) = &mut self.games.library {
            library.record(Record {
                run_id,
                game: game.into(),
                stage,
                score,
                lines,
                outcome: games::records::outcome(status).into(),
                at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
            });
        }
    }
    fn save_games(&mut self) -> bool {
        let Some(mut library) = self.games.library.clone() else {
            return self.games.error.is_empty();
        };
        if self.snake_open {
            let mut game = self.snake.clone();
            game.pause();
            library.snake = Some(Saved {
                run_id: self.games.snake_run.clone(),
                game,
            });
            library.snake_best = library.snake_best.max(self.snake.score);
        }
        if self.games.tetris_open {
            let mut game = self.games.tetris.clone();
            game.pause();
            library.tetris = Some(Saved {
                run_id: self.games.tetris_run.clone(),
                game,
            });
            library.tetris_best = library.tetris_best.max(self.games.tetris.score);
        }
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.save_game_library(&library))
        {
            Ok(()) => {
                self.snake_best = library.snake_best;
                self.games.library = Some(library);
                self.games.dirty = false;
                self.games.error.clear();
                true
            }
            Err(e) => {
                self.games.error = format!("存档未保存：{e}");
                false
            }
        }
    }
    fn start_game(&mut self, game: &str, stage: u8) {
        self.finish_game(game, true);
        if game == "snake" {
            self.snake = if stage == 0 {
                games::random_game()
            } else {
                games::Snake::stage(seed(), stage)
            };
            self.games.snake_run = uuid::Uuid::new_v4().to_string();
            self.snake_open = true;
            self.games.tetris_open = false;
        } else {
            self.games.tetris = Tetris::new(seed(), stage);
            self.games.tetris_run = uuid::Uuid::new_v4().to_string();
            self.games.tetris_open = true;
            self.snake_open = false;
        }
        self.games.history = false;
        self.snake_last_time = None;
        self.games.dirty = true;
        self.save_games();
    }
    pub(super) fn render_games(&mut self, ui: &mut egui::Ui) {
        if !self.games.error.is_empty() {
            ui.colored_label(Color32::from_rgb(190, 68, 68), &self.games.error);
        }
        if self.games.library.is_none() {
            return;
        }
        if !self.snake_open && !self.games.tetris_open {
            self.render_game_gallery(ui);
            return;
        }
        let snake = self.snake_open;
        let (status, stage, score, lines) = if snake {
            (self.snake.status, self.snake.level, self.snake.score, 0)
        } else {
            (
                self.games.tetris.status,
                self.games.tetris.stage,
                self.games.tetris.score,
                self.games.tetris.lines,
            )
        };
        ui.horizontal_wrapped(|ui| {
            if ui.button("‹ 关卡与记录").clicked() && self.flush_games() {
                self.games.selected = if snake { "snake" } else { "tetris" }.into();
                self.snake_open = false;
                self.games.tetris_open = false;
            }
            ui.label(
                RichText::new(if snake {
                    "贪吃蛇"
                } else {
                    "俄罗斯方块"
                })
                .strong(),
            );
            ui.label(mode(stage));
            ui.separator();
            let label = match status {
                Status::Ready => "开始",
                Status::Running => "暂停",
                Status::Paused => "继续",
                _ => "本局结束",
            };
            if ui
                .add_enabled(!ended(status), primary_button(label))
                .clicked()
            {
                if let Some(id) = ui.ctx().memory(|m| m.focused()) {
                    ui.ctx().memory_mut(|m| m.surrender_focus(id));
                }
                if snake {
                    self.snake.toggle();
                } else {
                    self.games.tetris.toggle();
                }
                self.snake_last_time = None;
                self.games.dirty = true;
            }
            if ui.button("重玩").clicked() {
                self.start_game(if snake { "snake" } else { "tetris" }, stage);
            }
            if status == Status::Cleared && stage < 12 && ui.add(primary_button("下一关")).clicked()
            {
                self.start_game(if snake { "snake" } else { "tetris" }, stage + 1);
            }
            ui.label(RichText::new(format!("得分 {score}")).strong());
            let library = self.games.library.as_ref().unwrap();
            ui.label(format!(
                "最高 {}",
                if snake {
                    library.snake_best.max(score)
                } else {
                    library.tetris_best.max(score)
                }
            ));
            if stage > 0 {
                ui.label(if snake {
                    format!("食物 {}/{}", score / 10, self.snake.goal() / 10)
                } else {
                    format!("消行 {lines}/{}", self.games.tetris.goal())
                });
            } else if !snake {
                ui.label(format!(
                    "消行 {lines} · 速度 {}",
                    self.games.tetris.speed_level()
                ));
            }
        });
        if !self.snake_open && !self.games.tetris_open {
            return;
        }
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let search_focused = self
            .search_field
            .is_some_and(|id| ctx.memory(|m| m.has_focus(id)));
        let blocked = self.detail_open() || search_focused || !ctx.input(|i| i.focused);
        if blocked {
            if snake {
                self.snake.pause();
            } else {
                self.games.tetris.pause();
            }
            self.snake_last_time = None;
        } else {
            if snake {
                self.snake_keys(&ctx);
            } else {
                self.tetris_keys(&ctx);
            }
            let delta = self.snake_last_time.map_or(0., |last| (now - last).max(0.));
            self.snake_last_time = Some(now);
            if snake {
                self.snake.advance(delta);
            } else {
                self.games.tetris.advance(delta);
            }
        }
        let after = if snake {
            self.snake.status
        } else {
            self.games.tetris.status
        };
        let after_score = if snake {
            self.snake.score
        } else {
            self.games.tetris.score
        };
        self.games.dirty |= status != after || after == Status::Running;
        if ended(after) {
            self.finish_game(if snake { "snake" } else { "tetris" }, false);
        }
        if self.games.dirty
            && (status != after || score != after_score || now - self.games.saved_at >= 2.)
            && self.save_games()
        {
            self.games.saved_at = now;
        }
        if after == Status::Running {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        ui.add_space(10.);
        if snake {
            self.draw_snake(ui);
        } else {
            self.draw_tetris(ui);
        }
    }
    fn snake_keys(&mut self, ctx: &egui::Context) {
        if key(ctx, egui::Key::Space) {
            self.snake.toggle();
            self.snake_last_time = None;
        }
        if key(ctx, egui::Key::Escape) {
            self.snake.pause();
            self.snake_last_time = None;
        }
        for (k, d) in [
            (egui::Key::ArrowUp, Direction::Up),
            (egui::Key::W, Direction::Up),
            (egui::Key::ArrowDown, Direction::Down),
            (egui::Key::S, Direction::Down),
            (egui::Key::ArrowLeft, Direction::Left),
            (egui::Key::A, Direction::Left),
            (egui::Key::ArrowRight, Direction::Right),
            (egui::Key::D, Direction::Right),
        ] {
            if key(ctx, k) {
                self.snake.turn(d);
            }
        }
    }
    fn tetris_keys(&mut self, ctx: &egui::Context) {
        if key(ctx, egui::Key::Enter) || key(ctx, egui::Key::P) {
            self.games.tetris.toggle();
            self.snake_last_time = None;
        }
        if key(ctx, egui::Key::Escape) {
            self.games.tetris.pause();
            self.snake_last_time = None;
        }
        for k in [egui::Key::ArrowLeft, egui::Key::A] {
            if key(ctx, k) {
                self.games.tetris.shift(-1);
            }
        }
        for k in [egui::Key::ArrowRight, egui::Key::D] {
            if key(ctx, k) {
                self.games.tetris.shift(1);
            }
        }
        if key(ctx, egui::Key::ArrowDown) || key(ctx, egui::Key::S) {
            self.games.tetris.soft_drop();
        }
        if key(ctx, egui::Key::ArrowUp) || key(ctx, egui::Key::X) {
            self.games.tetris.rotate(true);
        }
        if key(ctx, egui::Key::Z) {
            self.games.tetris.rotate(false);
        }
        if key(ctx, egui::Key::C) {
            self.games.tetris.swap_hold();
        }
        if key(ctx, egui::Key::Space) {
            self.games.tetris.hard_drop();
        }
    }
}
fn key(ctx: &egui::Context, key: egui::Key) -> bool {
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key))
}
