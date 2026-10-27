use super::*;
use crate::local_ai::Dialogue;

pub(crate) enum LearningMessage {
    Started(std::result::Result<(), String>),
    Generated(Box<std::result::Result<Card, String>>),
    Replied {
        kind: String,
        card_id: String,
        result: std::result::Result<String, String>,
    },
    ProjectTested(crate::challenges::Saved),
}
impl PersonalDeskApp {
    fn install_english(&mut self, s: &mut Session, card: Card) {
        let result = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| {
                db.import_study_cards(std::slice::from_ref(&card))?;
                if let Some(sequence) = card.tags.iter().find_map(|t| {
                    t.strip_prefix("练习序号:")
                        .and_then(|v| v.parse::<u64>().ok())
                }) {
                    db.set_setting(
                        &format!("study.word.sequence.{}", crate::english::slug(&card.topic)),
                        &sequence.saturating_add(1).to_string(),
                    )?;
                }
                db.study_card(&card.id)?.ok_or(DeskError::UnsupportedFormat)
            });
        match result {
            Ok(card) => {
                self.study.error.clear();
                s.answer.clear();
                s.result = None;
                s.sql_preview = None;
                s.sql_run = None;
                s.pending = Some(pending(&card, ""));
                s.chat_for = None;
                s.reference_open = false;
                s.recent
                    .push(card.origin.clone().unwrap_or_else(|| card.id.clone()));
                if s.recent.len() > 5 {
                    s.recent.remove(0);
                }
                s.card = Some(card);
                s.generating = None;
                s.mixed_draws += 1;
                if let Some(db) = self.db.as_mut()
                    && let Err(error) =
                        db.set_setting("study.word.mixed_draws", &s.mixed_draws.to_string())
                {
                    self.study.error = error.to_string();
                }
                s.stats = None;
            }
            Err(error) => self.study.error = error.to_string(),
        }
    }
    pub(in crate::app) fn next_english(&mut self, s: &mut Session) {
        let mut mode = if s.word_mode.is_empty() {
            crate::english::MODES[s.mixed_draws % crate::english::MODES.len()]
        } else {
            &s.word_mode
        }
        .to_string();
        let mut result = self
            .db
            .as_ref()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.draw_word(&mode, &s.recent, roll()));
        if s.word_mode.is_empty() {
            for _ in 1..crate::english::MODES.len() {
                if !matches!(result, Ok(None)) {
                    break;
                }
                s.mixed_draws += 1;
                mode = crate::english::MODES[s.mixed_draws % crate::english::MODES.len()].into();
                result = self
                    .db
                    .as_ref()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.draw_word(&mode, &s.recent, roll()));
            }
        }
        match result {
            Ok(Some(base)) => self.prepare_english_base(s, base, mode),
            Ok(None) => {
                s.card = None;
                self.study.error = "当前方式的词条已掌握，可切换另一种练习方式".into();
            }
            Err(error) => self.study.error = error.to_string(),
        }
    }
    pub(super) fn prepare_english_base(&mut self, s: &mut Session, base: Card, mode: String) {
        if mode == "听写" || mode == "释义" {
            match crate::english::exercise(&base, &mode, None) {
                Ok(card) => self.install_english(s, card),
                Err(error) => self.study.error = error,
            }
            return;
        }
        if mode == "语法" {
            if base.id.starts_with("english:grammar:v3:") {
                self.install_english(s, base);
            } else {
                match self
                    .db
                    .as_ref()
                    .map(|db| db.draw_word("语法", &s.recent, roll()))
                {
                    Some(Ok(Some(card))) => self.install_english(s, card),
                    Some(Ok(None)) => {
                        s.card = None;
                        self.study.error = "基础语法已掌握，可切换其他方式".into();
                    }
                    Some(Err(error)) => self.study.error = error.to_string(),
                    None => self.study.error = "数据库未打开".into(),
                }
            }
            return;
        }
        s.english_sequence = self
            .db
            .as_ref()
            .and_then(|db| {
                db.setting(&format!(
                    "study.word.sequence.{}",
                    crate::english::slug(&mode)
                ))
                .ok()
                .flatten()
            })
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let plan = crate::english::plan(&mode, s.english_sequence);
        let id = crate::english::variant_id(&base.id, &mode, s.english_sequence);
        let cached = self
            .db
            .as_ref()
            .and_then(|db| db.study_card(&id).ok().flatten());
        if let Some(card) = cached {
            self.install_english(s, card);
        } else if mode == "造句" {
            match plan.exercise(&base, &mode, None) {
                Ok(card) => self.install_english(s, card),
                Err(error) => self.study.error = error,
            }
        } else {
            s.generating = Some((base, mode));
            s.card = None;
            s.answer.clear();
            s.pending = None;
            self.generate_english(s);
        }
    }
    pub(in crate::app) fn switch_english_mode(&mut self, s: &mut Session) {
        if self.study.busy {
            return;
        }
        if s.word_mode.is_empty() {
            self.next_study("word", s);
            return;
        }
        if s.result.is_none()
            && let Some(p) = &s.pending
            && !p.answer.trim().is_empty()
            && let Some(db) = self.db.as_mut()
            && let Err(error) = db.save_study_pending(p)
        {
            self.study.error = error.to_string();
            return;
        }
        let base = s
            .card
            .as_ref()
            .and_then(|card| {
                self.db.as_ref().and_then(|db| {
                    db.study_card(card.origin.as_deref().unwrap_or(&card.id))
                        .ok()
                        .flatten()
                })
            })
            .or_else(|| s.generating.as_ref().map(|g| g.0.clone()));
        if let Some(base) = base {
            self.prepare_english_base(s, base, s.word_mode.clone());
        } else {
            self.next_english(s);
        }
    }
    fn generate_english(&mut self, s: &Session) {
        let Some((base, mode)) = s.generating.clone() else {
            return;
        };
        if self.study.busy {
            return;
        }
        self.study.busy = true;
        self.study.operation = "生成英语练习".into();
        self.study.error.clear();
        let ai = self.study.ai.clone();
        let config = self.study.config.clone();
        let log = self.data_dir.join("study-model.log");
        let sender = self.sender.clone();
        let plan = crate::english::plan(&mode, s.english_sequence);
        thread::spawn(move || {
            let result = ai.english_variant(&config, &log, &base, &mode, &plan);
            let _ = sender.send(BackgroundMessage::Learning(LearningMessage::Generated(
                Box::new(result),
            )));
        });
    }
    pub(in crate::app) fn accept_learning(&mut self, message: LearningMessage) {
        self.study.busy = false;
        self.study.operation.clear();
        match message {
            LearningMessage::ProjectTested(saved) => {
                match self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.save_study_project(&saved))
                {
                    Ok(()) => {
                        let s = &mut self.study.practice;
                        if s.card.as_ref().is_some_and(|c| c.id == saved.card_id) {
                            s.project = Some(saved);
                        }
                    }
                    Err(error) => self.study.error = error.to_string(),
                }
            }
            LearningMessage::Started(result) => {
                if let Err(error) = result {
                    self.study.error = error;
                }
            }
            LearningMessage::Generated(result) => match *result {
                Ok(card) => {
                    let mut s = std::mem::take(&mut self.study.words);
                    self.install_english(&mut s, card);
                    self.study.words = s;
                }
                Err(error) => self.study.error = error,
            },
            LearningMessage::Replied {
                kind,
                card_id,
                result,
            } => match result {
                Ok(text) => {
                    let helper = self.study.session_mut(&kind);
                    let pending_id = if helper.result.is_none() {
                        helper.pending.as_ref().map(|p| p.id.clone())
                    } else {
                        None
                    };
                    if let Some(id) = pending_id
                        && let Some(db) = self.db.as_mut()
                        && let Err(error) = db.mark_study_help(&id)
                    {
                        self.study.error = error.to_string();
                    }
                    let message = Dialogue::new("assistant", text);
                    let saved = self
                        .db
                        .as_mut()
                        .ok_or(DeskError::UnsupportedFormat)
                        .and_then(|db| db.append_study_dialogue(&card_id, &message));
                    match saved {
                        Ok(()) => {
                            let s = self.study.session_mut(&kind);
                            if s.card.as_ref().is_some_and(|c| c.id == card_id) {
                                s.chat.push(message);
                            }
                        }
                        Err(error) => {
                            self.study.error = format!("回复未保存：{error}");
                            let s = self.study.session_mut(&kind);
                            s.chat.push(message);
                        }
                    }
                }
                Err(error) => self.study.error = error,
            },
        }
    }
    pub(in crate::app) fn render_model_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.);
        egui::Frame::new()
            .fill(crate::theme::input())
            .corner_radius(9)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.add_enabled_ui(!self.study.busy && !self.audio_active(), |ui| {
                        egui::ComboBox::from_id_salt("active-model")
                            .width(150.)
                            .selected_text(if self.study.speech_selected {
                                "Kokoro · 朗读"
                            } else {
                                "Spark2.6 Q4"
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.study.speech_selected,
                                    false,
                                    "Spark2.6 Q4",
                                );
                                ui.selectable_value(
                                    &mut self.study.speech_selected,
                                    true,
                                    "Kokoro · 朗读",
                                );
                            });
                    });
                    if self.study.speech_selected {
                        self.render_kokoro_controls(ui);
                        return;
                    }
                    let loaded = self.study.ai.is_loaded();
                    let generating = self
                        .study
                        .ai
                        .busy
                        .load(std::sync::atomic::Ordering::Acquire);
                    let starting = self.study.operation == "启动模型";
                    if ui
                        .add_enabled(!self.study.busy && !loaded, egui::Button::new("启动模型"))
                        .clicked()
                    {
                        self.start_text_model();
                    }
                    if ui
                        .add_enabled(
                            loaded || generating,
                            egui::Button::new(if starting {
                                "取消启动"
                            } else if generating {
                                "停止生成"
                            } else {
                                "停止模型"
                            }),
                        )
                        .clicked()
                    {
                        if generating && !starting {
                            self.study.ai.cancel_generation();
                        } else {
                            self.study.ai.stop();
                        }
                    }
                    ui.label(RichText::new(self.study.ai.status()).size(12.));
                    let state = self.study.ai.snapshot();
                    if state.seconds > 0. {
                        ui.label(
                            RichText::new(format!(
                                "{} {:.1}s · {} token · {} token/s",
                                if state.active { "本次" } else { "上次" },
                                state.seconds,
                                state
                                    .tokens
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "—".into()),
                                state
                                    .speed
                                    .map(|v| format!("{v:.1}"))
                                    .unwrap_or_else(|| "—".into())
                            ))
                            .size(12.)
                            .color(muted()),
                        );
                        if let Some((done, total)) = state.prompt
                            && total > 0
                            && self.study.busy
                        {
                            ui.label(
                                RichText::new(format!("读取上下文 {done}/{total}"))
                                    .small()
                                    .color(muted()),
                            );
                        }
                    }
                });
            });
    }
    pub(in crate::app) fn render_learning_workspace(
        &mut self,
        ui: &mut egui::Ui,
        kind: &str,
        s: &mut Session,
    ) {
        if let Some((base, _)) = &s.generating
            && s.card.is_none()
        {
            ui.label(RichText::new(format!("正在为 {} 准备配套练习", base.title)).size(22.));
            if !self.study.busy {
                ui.horizontal(|ui| {
                    if ui.button("重试生成").clicked() {
                        self.generate_english(s);
                    }
                    if ui.button("先练释义").clicked() {
                        let base = s.generating.as_ref().unwrap().0.clone();
                        s.word_mode = "释义".into();
                        self.install_english(s, base);
                    }
                });
            }
            return;
        }
        if !self.study.tutor_open
            || s.card.as_ref().is_some_and(|c| c.topic == "听写") && s.result.is_none()
        {
            self.render_study_question(ui, kind, s);
            return;
        }
        let height = ui.available_height();
        let right = (ui.available_width() * 0.36).clamp(260., 390.);
        let left = (ui.available_width() - right - 14.).max(300.);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(left, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.render_study_question(ui, kind, s),
            );
            ui.add_space(6.);
            ui.allocate_ui_with_layout(
                egui::vec2(right, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.render_tutor(ui, kind, s),
            );
        });
    }
    pub(in crate::app) fn ask_tutor(&mut self, kind: &str, s: &mut Session, question: String) {
        if self.study.busy || question.trim().is_empty() {
            return;
        }
        let Some(card) = s.card.clone() else {
            return;
        };
        let question = question.chars().take(2000).collect::<String>();
        if !s
            .chat
            .last()
            .is_some_and(|m| m.role == "user" && m.content == question)
        {
            let message = Dialogue::new("user", question);
            let result = self
                .db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| db.append_study_dialogue(&card.id, &message));
            if let Err(error) = result {
                self.study.error = error.to_string();
                return;
            }
            s.chat.push(message);
        }
        s.chat_input.clear();
        self.study.error.clear();
        self.study.busy = true;
        self.study.operation = "讲解与追问".into();
        let ai = self.study.ai.clone();
        let config = self.study.config.clone();
        let log = self.data_dir.join("study-model.log");
        let sender = self.sender.clone();
        let answer = s
            .project
            .as_ref()
            .filter(|p| p.card_id == card.id)
            .and_then(|p| {
                p.source
                    .files
                    .get("src/main/java/training/OrderService.java")
            })
            .cloned()
            .unwrap_or_else(|| s.answer.clone());
        let feedback = s
            .result
            .as_ref()
            .map(|a| a.feedback.summary.clone())
            .unwrap_or_default();
        let history = s.chat.clone();
        let kind = kind.to_string();
        thread::spawn(move || {
            let result = ai.discuss(&config, &log, &card, &answer, &feedback, &history);
            let _ = sender.send(BackgroundMessage::Learning(LearningMessage::Replied {
                kind,
                card_id: card.id,
                result,
            }));
        });
    }
    fn render_tutor(&mut self, ui: &mut egui::Ui, kind: &str, s: &mut Session) {
        let Some(card) = s.card.clone() else {
            return;
        };
        if s.chat_for.as_deref() != Some(&card.id) {
            s.chat = self
                .db
                .as_ref()
                .and_then(|db| db.study_dialogue(&card.id).ok())
                .unwrap_or_default();
            s.chat_for = Some(card.id.clone());
        }
        use crate::ui_components::tutor::{self, Action, TutorPanel};
        let reference = s.reference_open.then(|| {
            if let Some(lab) = &card.sql
                && lab.runnable
            {
                format!("```sql\n{}\n```\n\n{}", lab.reference, lab.explanation)
            } else {
                card.answer.clone()
            }
        });
        let live = self.study.ai.snapshot();
        let action = TutorPanel {
            id: tutor::panel_id(kind, &card.id),
            input: &mut s.chat_input,
            messages: &s.chat,
            reference: reference.as_deref(),
            busy: self.study.busy,
            generating: self.study.busy && self.study.operation == "讲解与追问",
            partial: if live.active { &live.partial } else { "" },
        }
        .show(ui, &mut self.study.markdown);
        match action {
            Some(Action::Ask(question)) => self.ask_tutor(kind, s, question),
            Some(Action::ToggleReference) => {
                s.reference_open = !s.reference_open;
                if s.reference_open
                    && s.result.is_none()
                    && let Some(p) = &s.pending
                    && let Some(db) = self.db.as_mut()
                    && let Err(error) = db.mark_study_help(&p.id)
                {
                    self.study.error = error.to_string();
                }
            }
            None => {}
        }
    }
}
