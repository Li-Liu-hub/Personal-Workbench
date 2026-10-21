use super::*;
use crate::{
    local_ai::{Config, LocalAi},
    study::{self, Attempt, Card, Grade, NotebookEntry, Pending, Rules},
};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct Session {
    card: Option<Card>,
    answer: String,
    pending: Option<Pending>,
    result: Option<Attempt>,
    recent: Vec<String>,
    topic: String,
    notebook: bool,
    query: String,
    notebook_key: Option<(u64, String)>,
    entries: Vec<NotebookEntry>,
    selected: Option<String>,
    history: Vec<Attempt>,
}
pub(super) struct StudyState {
    practice: Session,
    words: Session,
    pub ai: Arc<LocalAi>,
    pub busy: bool,
    settings: bool,
    rules: Rules,
    config: Config,
    error: String,
}
impl Default for StudyState {
    fn default() -> Self {
        Self {
            practice: Default::default(),
            words: Default::default(),
            ai: Arc::new(LocalAi::default()),
            busy: false,
            settings: false,
            rules: Default::default(),
            config: Default::default(),
            error: String::new(),
        }
    }
}
pub(super) struct GradedMessage {
    kind: String,
    pending: Pending,
    result: std::result::Result<Grade, String>,
    model: String,
    elapsed: u64,
}
impl StudyState {
    fn session_mut(&mut self, kind: &str) -> &mut Session {
        if kind == "word" {
            &mut self.words
        } else {
            &mut self.practice
        }
    }
}
fn roll() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    uuid::Uuid::new_v4().hash(&mut h);
    h.finish()
}
fn pending(card: &Card, answer: &str) -> Pending {
    Pending {
        id: uuid::Uuid::new_v4().to_string(),
        card_id: card.id.clone(),
        answer: answer.into(),
        at: chrono::Local::now().to_rfc3339(),
    }
}

impl PersonalDeskApp {
    #[cfg(debug_assertions)]
    pub(super) fn prepare_study_preview(&mut self, settings: bool) {
        if settings {
            self.study.settings = true;
            return;
        }
        let card = self
            .db
            .as_ref()
            .unwrap()
            .study_cards("engineering")
            .unwrap()
            .remove(0);
        let mut session = Session {
            card: Some(card.clone()),
            pending: Some(pending(&card, "")),
            ..Default::default()
        };
        self.record_unknown(&mut session);
        session.notebook = true;
        session.selected = Some(card.id.clone());
        session.history = self.db.as_ref().unwrap().study_attempts(&card.id).unwrap();
        self.study.practice = session;
    }

    pub(super) fn initialize_study(&mut self) {
        if let Some(db) = self.db.as_ref() {
            self.study.rules = db.study_rules();
            self.study.config = db
                .settings()
                .ok()
                .and_then(|s| {
                    s.get("study.ai.config")
                        .and_then(|v| serde_json::from_str(v).ok())
                })
                .unwrap_or_default();
        }
    }
    pub(super) fn flush_study_answer(&mut self) -> bool {
        let mut saved = true;
        for kind in ["engineering", "word"] {
            if let Some(p) = self.study.session_mut(kind).pending.clone()
                && self.study.session_mut(kind).result.is_none()
                && !p.answer.trim().is_empty()
            {
                match self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.save_study_pending(&p))
                {
                    Ok(()) => {}
                    Err(e) => {
                        self.study.error = e.to_string();
                        saved = false;
                    }
                }
            }
        }
        saved
    }
    fn next_study(&mut self, kind: &str, s: &mut Session) {
        if self.study.busy {
            return;
        }
        if let Some(p) = s.pending.as_ref()
            && s.result.is_none()
            && !s.answer.trim().is_empty()
            && let Some(db) = self.db.as_mut()
            && let Err(e) = db.save_study_pending(p)
        {
            self.study.error = e.to_string();
            return;
        }
        match self
            .db
            .as_ref()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.draw_study(kind, &s.topic, &s.recent, roll()))
        {
            Ok(card) => {
                s.card = card;
                s.answer.clear();
                s.result = None;
                s.pending = s.card.as_ref().map(|c| pending(c, ""));
                if let Some(c) = &s.card {
                    s.recent.push(c.id.clone());
                    if s.recent.len() > 5 {
                        s.recent.remove(0);
                    }
                }
                self.study.error.clear();
            }
            Err(e) => self.study.error = e.to_string(),
        }
    }
    pub(super) fn accept_study_grade(&mut self, message: GradedMessage) {
        self.study.busy = false;
        match message.result {
            Ok(grade) => match self
                .db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| {
                    db.record_study_attempt(
                        &message.pending,
                        grade,
                        &message.model,
                        message.elapsed,
                    )
                }) {
                Ok(attempt) => {
                    let s = self.study.session_mut(&message.kind);
                    if s.pending.as_ref().is_some_and(|p| p.id == attempt.id) {
                        s.result = Some(attempt);
                    }
                    self.study.error.clear();
                    self.notice = "本次练习已记入记录本".into();
                }
                Err(e) => self.study.error = format!("成绩保存失败，答案仍在待完成记录中：{e}"),
            },
            Err(e) => self.study.error = e,
        }
    }
    fn submit_study(&mut self, kind: &str, s: &mut Session) {
        if self.study.busy {
            return;
        }
        let Some(card) = s.card.clone() else { return };
        if s.answer.trim().is_empty() {
            self.study.error = "先写下你的答案".into();
            return;
        }
        let mut p = s
            .pending
            .clone()
            .unwrap_or_else(|| pending(&card, &s.answer));
        p.answer = s.answer.clone();
        if let Err(e) = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.save_study_pending(&p))
        {
            self.study.error = e.to_string();
            return;
        }
        s.pending = Some(p.clone());
        self.study.busy = true;
        self.study.error.clear();
        let ai = self.study.ai.clone();
        let config = self.study.config.clone();
        let sender = self.sender.clone();
        let kind = kind.to_string();
        let log = self.data_dir.join("study-model.log");
        thread::spawn(move || {
            let started = std::time::Instant::now();
            let result = ai.grade(&config, &log, &card, &p.answer);
            let model = config
                .model
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let _ = sender.send(BackgroundMessage::StudyGraded(GradedMessage {
                kind,
                pending: p,
                result,
                model,
                elapsed: started.elapsed().as_millis() as u64,
            }));
        });
    }
    fn record_unknown(&mut self, s: &mut Session) {
        let Some(card) = s.card.as_ref() else { return };
        let mut p = s
            .pending
            .clone()
            .unwrap_or_else(|| pending(card, &s.answer));
        p.answer = s.answer.clone();
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.record_study_attempt(&p, Grade::unknown(card), "未能作答", 0))
        {
            Ok(attempt) => {
                s.result = Some(attempt);
                self.study.error.clear();
            }
            Err(e) => self.study.error = e.to_string(),
        }
    }
    pub(super) fn render_study(&mut self, ui: &mut egui::Ui) {
        self.search_field = None;
        let kind = if self.section == Section::Words {
            "word"
        } else {
            "engineering"
        };
        let mut s = std::mem::take(self.study.session_mut(kind));
        if s.card.is_none() && !s.notebook {
            let resume = self
                .db
                .as_ref()
                .and_then(|db| db.study_pending(kind).ok())
                .and_then(|p| p.into_iter().next());
            if let Some(p) = resume {
                s.card = self
                    .db
                    .as_ref()
                    .and_then(|db| db.study_card(&p.card_id).ok().flatten());
                s.answer = p.answer.clone();
                s.pending = Some(p);
            } else {
                self.next_study(kind, &mut s);
            }
        }
        ui.horizontal(|ui| {
            crate::ui_components::toolbar_title(
                ui,
                if kind == "word" {
                    "每日单词"
                } else {
                    "刷题王"
                },
                170.,
                23.,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("练习设置").clicked() {
                    self.study.settings = true;
                }
                if ui.button("导出").clicked() {
                    self.export_dialog_open = true;
                }
                if ui.button("导入题库").clicked() {
                    self.import_study_bank();
                }
                ui.selectable_value(&mut s.notebook, true, "记录本");
                ui.selectable_value(&mut s.notebook, false, "练习");
            });
        });
        ui.add_space(10.);
        ui.horizontal(|ui| {
            if kind == "engineering" {
                let before = s.topic.clone();
                ui.add_enabled_ui(!self.study.busy, |ui| {
                    egui::ComboBox::from_id_salt("study-topic")
                        .width(140.)
                        .selected_text(if s.topic.is_empty() {
                            "混合训练"
                        } else {
                            &s.topic
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut s.topic, String::new(), "混合训练");
                            for topic in study::TOPICS {
                                ui.selectable_value(&mut s.topic, topic.to_string(), topic);
                            }
                        });
                });
                if s.topic != before {
                    self.next_study(kind, &mut s);
                }
            }
            let count = self
                .db
                .as_ref()
                .and_then(|db| db.study_count(kind).ok())
                .unwrap_or(0);
            let today = self
                .db
                .as_ref()
                .and_then(|db| db.study_today(kind).ok())
                .unwrap_or(0);
            ui.label(
                RichText::new(if kind == "word" {
                    format!(
                        "IELTS · {count} 词   今日 {today}/{}",
                        self.study.rules.daily_words
                    )
                } else {
                    format!("{count} 道工程题   今日已练 {today} 题")
                })
                .small()
                .color(muted()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(self.study.ai.status()).small().color(muted()));
            });
        });
        if !self.study.error.is_empty() {
            ui.colored_label(Color32::from_rgb(184, 65, 71), &self.study.error);
        }
        ui.add_space(12.);
        if s.notebook {
            self.render_study_notebook(ui, kind, &mut s);
        } else {
            self.render_study_question(ui, kind, &mut s);
        }
        *self.study.session_mut(kind) = s;
        self.render_study_settings(ui.ctx());
    }
    fn render_study_question(&mut self, ui: &mut egui::Ui, kind: &str, s: &mut Session) {
        let Some(card) = s.card.clone() else {
            ui.centered_and_justified(|ui| {
                ui.label(if self.study.busy {
                    "正在完成本次批改…"
                } else {
                    "当前范围已全部掌握，可在记录本中选择复习，或在设置里启用低频抽取。"
                });
            });
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt(("study-question", kind, &card.id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let width = ui.available_width().min(900.);
                let left = ((ui.available_width() - width) / 2.).max(0.);
                ui.horizontal_top(|ui| {
                    ui.add_space(left);
                    egui::Frame::new()
                        .fill(surface())
                        .corner_radius(14)
                        .inner_margin(26)
                        .show(ui, |ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.set_width((width - 52.).max(240.));
                                if kind == "word" {
                                    ui.label(RichText::new(&card.title).size(40.).strong());
                                    if !card.phonetic.is_empty() {
                                        ui.label(
                                            RichText::new(&card.phonetic).size(18.).color(muted()),
                                        );
                                    }
                                } else {
                                    ui.label(RichText::new(&card.title).size(24.).strong());
                                    ui.label(RichText::new(&card.topic).small().color(muted()));
                                }
                                ui.add_space(14.);
                                ui.label(&card.prompt);
                                ui.add_space(16.);
                                ui.add_enabled_ui(s.result.is_none() && !self.study.busy, |ui| {
                                    let response = ui.add(
                                        egui::TextEdit::multiline(&mut s.answer)
                                            .id_salt(("study-answer", kind, &card.id))
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(if kind == "word" { 3 } else { 8 })
                                            .font(
                                                if matches!(
                                                    card.topic.as_str(),
                                                    "SQL 命令" | "Linux 命令"
                                                ) {
                                                    egui::FontId::monospace(15.)
                                                } else {
                                                    egui::FontId::proportional(16.)
                                                },
                                            )
                                            .hint_text(if kind == "word" {
                                                "回忆中文含义…"
                                            } else {
                                                "写下你的思路、命令或代码…"
                                            }),
                                    );
                                    if response.changed() {
                                        let p = s.pending.get_or_insert_with(|| pending(&card, ""));
                                        p.answer = s.answer.clone();
                                        if let Some(db) = self.db.as_mut()
                                            && let Err(e) = db.save_study_pending(p)
                                        {
                                            self.study.error = e.to_string();
                                        }
                                    }
                                });
                                ui.add_space(14.);
                                ui.horizontal(|ui| {
                                    if s.result.is_none() {
                                        if ui
                                            .add_enabled(
                                                !self.study.busy && !s.answer.trim().is_empty(),
                                                primary_button("提交 · AI 批改"),
                                            )
                                            .clicked()
                                        {
                                            self.submit_study(kind, s);
                                        }
                                        if ui
                                            .add_enabled(
                                                !self.study.busy,
                                                egui::Button::new("不会，查看答案"),
                                            )
                                            .clicked()
                                        {
                                            self.record_unknown(s);
                                        }
                                    }
                                    if ui
                                        .add_enabled(
                                            !self.study.busy,
                                            egui::Button::new(if s.result.is_some() {
                                                "下一题"
                                            } else {
                                                "换一道"
                                            }),
                                        )
                                        .clicked()
                                    {
                                        self.next_study(kind, s);
                                    }
                                    if self.study.busy {
                                        ui.spinner();
                                        ui.label("正在批改…");
                                    }
                                });
                                if let Some(attempt) = s.result.as_ref() {
                                    ui.add_space(18.);
                                    ui.separator();
                                    ui.label(
                                        RichText::new(format!(
                                            "完成度 {}% · 第 {} 次",
                                            attempt.completion, attempt.number
                                        ))
                                        .size(21.)
                                        .strong()
                                        .color(accent()),
                                    );
                                    render_feedback(ui, attempt);
                                    ui.add_space(12.);
                                    ui.collapsing("参考答案与评分点", |ui| {
                                        ui.label(&card.answer);
                                        ui.hyperlink_to("参考资料", &card.source);
                                    });
                                }
                            });
                        });
                });
            });
    }
    fn render_study_notebook(&mut self, ui: &mut egui::Ui, kind: &str, s: &mut Session) {
        ui.add(
            egui::TextEdit::singleline(&mut s.query)
                .hint_text("搜索记录本")
                .desired_width(280.),
        );
        let key = (
            self.db.as_ref().map(|d| d.change_count()).unwrap_or(0),
            s.query.clone(),
        );
        if s.notebook_key.as_ref() != Some(&key) {
            s.entries = self
                .db
                .as_ref()
                .and_then(|db| db.study_notebook(kind, &s.query).ok())
                .unwrap_or_default();
            s.notebook_key = Some(key);
            if let Some(id) = s.selected.as_ref() {
                s.history = self
                    .db
                    .as_ref()
                    .and_then(|db| db.study_attempts(id).ok())
                    .unwrap_or_default();
            }
        }
        let mut select = None;
        let mut retry = None;
        let height = ui.available_height();
        let left_width = (ui.available_width() * 0.31).clamp(190., 300.);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(left_width, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("notebook-list", kind))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let pending = self
                                .db
                                .as_ref()
                                .and_then(|db| db.study_pending(kind).ok())
                                .unwrap_or_default();
                            if !pending.is_empty() {
                                ui.collapsing(format!("待完成 · {}", pending.len()), |ui| {
                                    for p in pending.iter().filter(|p| !p.answer.trim().is_empty())
                                    {
                                        let title = self
                                            .db
                                            .as_ref()
                                            .and_then(|db| db.study_card(&p.card_id).ok().flatten())
                                            .map(|c| c.title)
                                            .unwrap_or_default();
                                        if ui
                                            .add_enabled(!self.study.busy, egui::Button::new(title))
                                            .clicked()
                                        {
                                            retry = Some((p.card_id.clone(), Some(p.clone())));
                                        }
                                    }
                                });
                            }
                            for entry in &s.entries {
                                let selected = s.selected.as_ref() == Some(&entry.card.id);
                                egui::Frame::new()
                                    .fill(if selected {
                                        surface().lerp_to_gamma(accent(), 0.10)
                                    } else {
                                        surface()
                                    })
                                    .corner_radius(9)
                                    .inner_margin(10)
                                    .show(ui, |ui| {
                                        ui.set_width((left_width - 20.).max(120.));
                                        if ui
                                            .add(egui::Button::new(&entry.card.title).frame(false))
                                            .clicked()
                                        {
                                            select = Some(entry.card.id.clone());
                                        }
                                        ui.label(
                                            RichText::new(format!(
                                                "{} 次 · 最近 {}% · 连续 {}/{}{}",
                                                entry.progress.attempts,
                                                entry.progress.latest,
                                                entry
                                                    .progress
                                                    .streak
                                                    .min(self.study.rules.consecutive),
                                                self.study.rules.consecutive,
                                                if entry.progress.mastered {
                                                    " · 已掌握"
                                                } else {
                                                    ""
                                                }
                                            ))
                                            .small()
                                            .color(muted()),
                                        );
                                    });
                                ui.add_space(6.);
                            }
                            if s.entries.is_empty() {
                                ui.label("完成一次练习后，记录会出现在这里。");
                            }
                        });
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("notebook-detail", kind))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if let Some(id) = s.selected.as_ref() {
                                if let Some(entry) = s.entries.iter().find(|e| &e.card.id == id) {
                                    ui.heading(&entry.card.title);
                                    if ui
                                        .add_enabled(
                                            !self.study.busy,
                                            egui::Button::new("再练一次"),
                                        )
                                        .clicked()
                                    {
                                        retry = Some((id.clone(), None));
                                    }
                                }
                                render_trend(ui, &s.history);
                                for attempt in s.history.iter().rev() {
                                    ui.collapsing(
                                        format!(
                                            "第 {} 次 · {}% · {}",
                                            attempt.number,
                                            attempt.completion,
                                            &attempt.at[..19.min(attempt.at.len())]
                                        ),
                                        |ui| {
                                            ui.label(&attempt.answer);
                                            render_feedback(ui, attempt);
                                            ui.label(
                                                RichText::new(&attempt.model)
                                                    .small()
                                                    .color(muted()),
                                            );
                                        },
                                    );
                                }
                            } else {
                                ui.label("选择一条记录，查看每次完成度与批改详情。");
                            }
                        });
                },
            );
        });
        if let Some(id) = select {
            s.history = self
                .db
                .as_ref()
                .and_then(|db| db.study_attempts(&id).ok())
                .unwrap_or_default();
            s.selected = Some(id);
        }
        if let Some((id, p)) = retry {
            s.card = self
                .db
                .as_ref()
                .and_then(|db| db.study_card(&id).ok().flatten());
            s.answer = p.as_ref().map(|p| p.answer.clone()).unwrap_or_default();
            s.pending = p.or_else(|| s.card.as_ref().map(|c| pending(c, "")));
            s.result = None;
            s.notebook = false;
        }
    }
    fn render_study_settings(&mut self, ctx: &egui::Context) {
        if !self.study.settings {
            return;
        }
        let mut save = false;
        let dialog = centered_dialog(ctx, "study-settings", "练习设置", "", 560., |ui| {
            ui.label("连续达标后视为掌握");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.study.rules.threshold)
                        .range(1..=100)
                        .suffix("%"),
                );
                ui.label("完成度阈值");
            });
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.study.rules.consecutive).range(1..=20));
                ui.label("连续次数");
            });
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.study.rules.mastered,
                    study::MasteredMode::Rare,
                    "掌握后极低频",
                );
                ui.selectable_value(
                    &mut self.study.rules.mastered,
                    study::MasteredMode::Stop,
                    "掌握后停止抽取",
                );
            });
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.study.rules.daily_words).range(1..=200));
                ui.label("每日单词目标");
            });
            ui.separator();
            ui.label("本地 GGUF 模型");
            ui.label(self.study.config.model.display().to_string());
            if ui.button("选择模型").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("GGUF", &["gguf"])
                    .pick_file()
            {
                self.study.config.model = path;
            }
            ui.label("模型运行程序");
            ui.label(self.study.config.runtime.display().to_string());
            if ui.button("选择 llama-server.exe").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("程序", &["exe"])
                    .pick_file()
            {
                self.study.config.runtime = path;
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.study.config.idle_seconds)
                        .range(30..=600)
                        .suffix(" 秒"),
                );
                ui.label("空闲后卸载模型");
            });
            if ui
                .add_enabled(!self.study.busy, egui::Button::new("卸载模型"))
                .clicked()
            {
                self.study.ai.stop();
            }
            save = ui
                .add_enabled(!self.study.busy, primary_button("保存设置"))
                .clicked();
            ui.collapsing("开源许可", |ui| {
                ui.label(
                    RichText::new(include_str!("../../assets/THIRD_PARTY_NOTICES.txt")).small(),
                );
            });
        });
        if save {
            match self
                .db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| {
                    db.save_study_rules(&self.study.rules)?;
                    db.set_setting(
                        "study.ai.config",
                        &serde_json::to_string(&self.study.config)?,
                    )
                }) {
                Ok(()) => {
                    self.study.settings = false;
                    self.study.error.clear();
                }
                Err(e) => self.study.error = e.to_string(),
            }
        }
        if dialog_closed(&dialog) {
            self.study.settings = false;
            self.initialize_study();
        }
    }
    fn import_study_bank(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("题库 JSON", &["json"])
            .pick_file()
        {
            let result = (|| -> crate::error::Result<usize> {
                let metadata = fs::metadata(&path)?;
                if metadata.len() > 16 * 1024 * 1024 {
                    return Err(DeskError::InvalidPath("题库文件请控制在16MiB以内".into()));
                }
                let text = fs::read_to_string(path)?;
                let bundle: study::Bundle = serde_json::from_str(&text)?;
                if bundle.version != 1 {
                    return Err(DeskError::UnsupportedFormat);
                }
                self.db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)?
                    .import_study_cards(&bundle.cards)
            })();
            self.notice = match result {
                Ok(n) => format!("已导入 {n} 道新题目，已有题目和记录保留"),
                Err(e) => e.to_string(),
            };
        }
    }
}
fn render_feedback(ui: &mut egui::Ui, attempt: &Attempt) {
    ui.label(&attempt.feedback.summary);
    for check in &attempt.feedback.checks {
        let text = attempt
            .question
            .rubric
            .iter()
            .find(|r| r.id == check.id)
            .map(|r| r.text.as_str())
            .unwrap_or(&check.id);
        ui.collapsing(format!("{} 分 · {}", check.earned, text), |ui| {
            ui.label(&check.reason);
            if !check.evidence.is_empty() {
                ui.label(
                    RichText::new(format!("原文：{}", check.evidence))
                        .small()
                        .color(muted()),
                );
            }
        });
    }
    if !attempt.feedback.weak_points.is_empty() {
        ui.label(format!(
            "下次注意：{}",
            attempt.feedback.weak_points.join("；")
        ));
    }
}
fn render_trend(ui: &mut egui::Ui, history: &[Attempt]) {
    if history.is_empty() {
        return;
    }
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 100.), egui::Sense::hover());
    let graph = rect.shrink2(egui::vec2(14., 10.));
    ui.painter().line_segment(
        [graph.left_bottom(), graph.right_bottom()],
        egui::Stroke::new(1., border()),
    );
    let points = history
        .iter()
        .enumerate()
        .map(|(i, a)| {
            egui::pos2(
                graph.left() + i as f32 / (history.len() - 1).max(1) as f32 * graph.width(),
                graph.bottom() - a.completion as f32 / 100. * graph.height(),
            )
        })
        .collect::<Vec<_>>();
    for pair in points.windows(2) {
        ui.painter()
            .line_segment([pair[0], pair[1]], egui::Stroke::new(2., accent()));
    }
    for p in points {
        ui.painter().circle_filled(p, 3., accent());
    }
}

#[cfg(test)]
mod tests;
