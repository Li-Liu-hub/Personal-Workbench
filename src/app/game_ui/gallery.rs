use super::*;
impl PersonalDeskApp {
    pub(super) fn render_game_gallery(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if !self.games.selected.is_empty() && ui.button("‹ 全部游戏").clicked() {
                self.games.selected.clear();
            }
            chrome::segmented(
                ui,
                &mut self.games.history,
                &[(false, "关卡"), (true, "记录")],
            );
        });
        ui.add_space(12.);
        if self.games.history {
            let key = (
                self.db.as_ref().map(|db| db.change_count()).unwrap_or(0),
                self.games.history_limit,
            );
            if self.games.history_key != Some(key) {
                match self
                    .db
                    .as_ref()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.game_records(self.games.history_limit))
                {
                    Ok(rows) => {
                        self.games.history_rows = rows;
                        self.games.history_key = Some(key);
                    }
                    Err(e) => self.games.error = e.to_string(),
                }
            }
            if self.games.history_rows.len() == self.games.history_limit as usize
                && ui.button("加载更早记录").clicked()
            {
                self.games.history_limit += 100;
            }
            let records = self
                .games
                .history_rows
                .iter()
                .filter(|r| self.games.selected.is_empty() || r.game == self.games.selected)
                .collect::<Vec<_>>();
            egui::ScrollArea::vertical()
                .id_salt("game-history")
                .show_rows(ui, 58., records.len(), |ui, range| {
                    for index in range {
                        let r = records[index];
                        chrome::paper().show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(if r.game == "snake" {
                                        "贪吃蛇"
                                    } else {
                                        "俄罗斯方块"
                                    })
                                    .strong(),
                                );
                                ui.label(mode(r.stage));
                                ui.label(&r.outcome);
                                ui.label(format!("{} 分", r.score));
                                if r.game == "tetris" {
                                    ui.label(format!("{} 行", r.lines));
                                }
                                ui.label(RichText::new(&r.at).small().color(muted()));
                            });
                        });
                        ui.add_space(6.);
                    }
                    if records.is_empty() {
                        ui.label("还没有完成的游戏；进行中的棋盘会自动存档。");
                    }
                });
            return;
        }
        if self.games.selected.is_empty() {
            let query = self.search_query.to_lowercase();
            let mut chosen = None;
            collection(ui, "games-gallery", self.view_mode(), |ui, layout| {
                for (id, title, description) in [
                    ("snake", "贪吃蛇", "12 关 · 障碍与加速 · 无尽模式"),
                    ("tetris", "俄罗斯方块", "12 关 · 连续消行 · 暂存与落点预览"),
                ] {
                    if matches_query(&query, &[id, title, description])
                        && entry(ui, id, title, description, TileKind::Game, layout).clicked()
                    {
                        chosen = Some(id.to_owned());
                    }
                }
            });
            if let Some(game) = chosen {
                self.games.selected = game;
            }
            return;
        }
        let game = self.games.selected.clone();
        let snake = game == "snake";
        let library = self.games.library.as_ref().unwrap();
        let unlocked = if snake {
            library.snake_unlocked
        } else {
            library.tetris_unlocked
        };
        let resumable = if snake {
            library
                .snake
                .as_ref()
                .is_some_and(|s| !ended(s.game.status))
        } else {
            library
                .tetris
                .as_ref()
                .is_some_and(|s| !ended(s.game.status))
        };
        let mut start = None;
        let mut resume = false;
        ui.horizontal_wrapped(|ui| {
            ui.heading(if snake {
                "贪吃蛇"
            } else {
                "俄罗斯方块"
            });
            if resumable && ui.add(primary_button("继续存档")).clicked() {
                resume = true;
            }
            if ui.button("无尽模式").clicked() {
                start = Some(0);
            }
            ui.label(RichText::new(format!("已解锁 {unlocked} / 12 关")).color(muted()));
        });
        ui.add_space(12.);
        let columns = (ui.available_width() / 180.).floor().clamp(2., 6.) as usize;
        let width = (ui.available_width() - (columns - 1) as f32 * 10.) / columns as f32;
        egui::ScrollArea::vertical()
            .id_salt("game-levels-scroll")
            .show(ui, |ui| {
                egui::Grid::new(("game-levels", snake))
                    .num_columns(columns)
                    .spacing([10., 10.])
                    .show(ui, |ui| {
                        for stage in 1..=12u8 {
                            let best = library.stage_best.get(&format!("{game}:{stage}")).copied();
                            let name = if snake {
                                format!(
                                    "第 {stage} 关\n{} 个食物{}",
                                    4 + stage,
                                    if stage >= 9 {
                                        " · 迷宫"
                                    } else if stage >= 3 {
                                        " · 障碍"
                                    } else {
                                        " · 边界"
                                    }
                                )
                            } else {
                                format!(
                                    "第 {stage} 关\n{} 行{}",
                                    4 + stage * 2,
                                    if stage >= 4 {
                                        " · 底部障碍"
                                    } else {
                                        " · 消行"
                                    }
                                )
                            };
                            let text = format!(
                                "{name}\n{}",
                                best.map(|v| format!("★ {v} 分")).unwrap_or_else(|| {
                                    if stage > unlocked {
                                        "未解锁".into()
                                    } else {
                                        "开始挑战".into()
                                    }
                                })
                            );
                            if ui
                                .add_enabled(
                                    stage <= unlocked,
                                    egui::Button::new(text).min_size(egui::vec2(width, 102.)),
                                )
                                .clicked()
                            {
                                start = Some(stage);
                            }
                            if usize::from(stage) % columns == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });
        if resume {
            self.snake_open = snake;
            self.games.tetris_open = !snake;
            self.snake_last_time = None;
        }
        if let Some(stage) = start {
            self.start_game(&game, stage);
        }
    }
}
