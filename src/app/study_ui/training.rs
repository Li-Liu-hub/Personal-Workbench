use super::*;
use crate::curriculum::{self, Filter};
impl PersonalDeskApp {
    pub(in crate::app) fn render_practice_selector(&mut self, ui: &mut egui::Ui, s: &mut Session) {
        let before = (s.topic.clone(), s.filter.clone());
        ui.add_enabled_ui(!self.study.busy, |ui| {
            if ui.button("编码练习").clicked() {
                s.topic = "编码题".into();
                s.filter = Filter::default();
            }
            egui::ComboBox::from_id_salt("study-topic")
                .width(128.)
                .height(340.)
                .selected_text(if s.topic.is_empty() {
                    "推荐训练"
                } else if s.topic == curriculum::MIXED {
                    "自由混合"
                } else {
                    curriculum::category(&s.topic)
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.topic, String::new(), "推荐训练");
                    for topic in study::TOPICS {
                        ui.selectable_value(&mut s.topic, topic.into(), topic);
                    }
                    ui.separator();
                    ui.selectable_value(&mut s.topic, curriculum::MIXED.into(), "自由混合");
                    if let Some(db) = self.db.as_ref()
                        && let Ok(topics) = db.study_topics()
                    {
                        let topics = topics
                            .iter()
                            .map(|topic| curriculum::category(topic))
                            .collect::<std::collections::BTreeSet<_>>();
                        for topic in topics {
                            if !study::TOPICS.contains(&topic) {
                                ui.selectable_value(&mut s.topic, topic.into(), topic);
                            }
                        }
                    }
                });
            egui::ComboBox::from_id_salt("study-area")
                .width(138.)
                .selected_text(if s.filter.area.is_empty() {
                    "全部能力"
                } else {
                    &s.filter.area
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.filter.area, String::new(), "全部能力");
                    for area in curriculum::AREAS {
                        ui.selectable_value(&mut s.filter.area, area.into(), area);
                    }
                });
            egui::ComboBox::from_id_salt("study-level")
                .width(108.)
                .selected_text(if s.filter.level == 0 {
                    "全部难度"
                } else {
                    curriculum::level_name(s.filter.level)
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.filter.level, 0, "全部难度");
                    for level in 1..=3 {
                        ui.selectable_value(
                            &mut s.filter.level,
                            level,
                            curriculum::level_name(level),
                        );
                    }
                });
        });
        if before != (s.topic.clone(), s.filter.clone()) {
            self.save_practice_selection(s);
            self.next_study("engineering", s);
        }
    }
    fn save_practice_selection(&mut self, s: &Session) {
        let result = (|| -> crate::error::Result<()> {
            let Some(db) = self.db.as_mut() else {
                return Ok(());
            };
            db.set_setting("study.practice.topic", &s.topic)?;
            db.set_setting("study.practice.filter", &serde_json::to_string(&s.filter)?)
        })();
        if let Err(error) = result {
            self.study.error = error.to_string();
        }
    }
    pub(in crate::app) fn render_training_route(&mut self, ctx: &egui::Context) {
        if !self.study.route_open {
            return;
        }
        let rows = self
            .db
            .as_ref()
            .and_then(|db| db.practice_candidates("").ok())
            .map(|items| curriculum::route(&items, &self.study.rules))
            .unwrap_or_default();
        let mut selected = None;
        let mut recommended = false;
        let result = centered_dialog(ctx, "training-route", "训练路线", "", 700., |ui| {
            ui.label("基础理解 → 应用实现 → 综合进阶");
            ui.label(RichText::new("每个方向独立记录；两道不同基础题独立达标后进入应用，两道应用题独立达标后进入进阶。掌握仍按连续作答规则计算。").small().color(muted()));
            ui.add_space(10.);
            for row in &rows {
                egui::Frame::new()
                    .fill(crate::theme::input())
                    .corner_radius(9)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&row.area).strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_enabled(
                                            !self.study.busy,
                                            egui::Button::new("练这个方向"),
                                        )
                                        .clicked()
                                    {
                                        selected = Some(row.area.clone());
                                    }
                                    ui.label(curriculum::level_name(row.stage));
                                },
                            );
                        });
                        ui.label(
                            RichText::new(format!(
                                "{} · 已练 {}/{} · 掌握 {} · 基础达标 {}/2 · 应用达标 {}/2",
                                if row.diagnosed {
                                    "已有摸底记录"
                                } else {
                                    "尚待基础摸底"
                                },
                                row.practiced,
                                row.total,
                                row.mastered,
                                row.foundation.min(2),
                                row.application.min(2)
                            ))
                            .small()
                            .color(muted()),
                        );
                    });
                ui.add_space(6.);
            }
            ui.label(RichText::new("推荐训练先摸底，再按薄弱项和新题安排；专项与自由混合可自行选择难度。AI 分数是练习反馈，路线状态不代表能力认证。").small().color(muted()));
            recommended = ui
                .add_enabled(!self.study.busy, primary_button("交给推荐训练安排"))
                .clicked();
        });
        if dialog_closed(&result) {
            self.study.route_open = false;
        }
        if selected.is_some() || recommended {
            self.study.route_open = false;
            let mut s = std::mem::take(&mut self.study.practice);
            s.knowledge = None;
            s.notebook_key = None;
            s.stats = None;
            s.topic.clear();
            s.filter = Filter {
                area: selected.unwrap_or_default(),
                level: 0,
            };
            self.save_practice_selection(&s);
            self.next_study("engineering", &mut s);
            self.study.practice = s;
        }
    }
}
