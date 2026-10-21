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
                db.study_card(&card.id)?.ok_or(DeskError::UnsupportedFormat)
            });
        match result {
            Ok(card) => {
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
                self.study.error.clear();
            }
            Err(error) => self.study.error = error.to_string(),
        }
    }
    pub(in crate::app) fn next_english(&mut self, s: &mut Session) {
        let mut mode = if s.word_mode.is_empty() {
            crate::english::MODES[s.mixed_draws % 5]
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
            for _ in 0..4 {
                if !matches!(result, Ok(None)) {
                    break;
                }
                s.mixed_draws += 1;
                mode = crate::english::MODES[s.mixed_draws % 5].into();
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
    fn prepare_english_base(&mut self, s: &mut Session, base: Card, mode: String) {
        let id = crate::english::id(&base.id, &mode);
        let cached = self
            .db
            .as_ref()
            .and_then(|db| db.study_card(&id).ok().flatten());
        if let Some(card) = cached {
            self.install_english(s, card);
        } else if matches!(mode.as_str(), "造句" | "语法") {
            match crate::english::exercise(&base, &mode, None) {
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
        thread::spawn(move || {
            let result = ai.english_exercise(&config, &log, &base, &mode);
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
                    if ui
                        .add_enabled(!self.study.busy, egui::Button::new("启动模型"))
                        .clicked()
                    {
                        self.study.busy = true;
                        self.study.operation = "启动模型".into();
                        self.study.error.clear();
                        let ai = self.study.ai.clone();
                        let config = self.study.config.clone();
                        let log = self.data_dir.join("study-model.log");
                        let sender = self.sender.clone();
                        thread::spawn(move || {
                            let result = ai.warmup(&config, &log);
                            let _ = sender.send(BackgroundMessage::Learning(
                                LearningMessage::Started(result),
                            ));
                        });
                    }
                    if ui
                        .add_enabled(
                            self.study
                                .ai
                                .busy
                                .load(std::sync::atomic::Ordering::Acquire),
                            egui::Button::new("停止"),
                        )
                        .clicked()
                    {
                        self.study.ai.stop();
                    }
                    if !self.study.busy && ui.small_button("卸载").clicked() {
                        self.study.ai.stop();
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
        if !self.study.tutor_open {
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
                |ui| self.render_tutor(ui, kind, s, height),
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
    fn render_tutor(&mut self, ui: &mut egui::Ui, kind: &str, s: &mut Session, height: f32) {
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
        let mut ask = None;
        egui::Frame::new()
            .fill(surface())
            .stroke(egui::Stroke::new(0.7, border()))
            .corner_radius(14)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_width((ui.available_width() - 2.).max(220.));
                ui.label(RichText::new("讲解与追问").size(18.).strong());
                ui.add_space(8.);
                ui.horizontal_wrapped(|ui| {
                    for (label, prompt) in [
                        ("AI 解答", "从基础讲解这道题，给出一个完整解答和具体例子。"),
                        ("提示一步", "只给我下一步提示，不要直接给完整答案。"),
                        (
                            "解释反馈",
                            "结合我的答案和批改结果，讲清我错在哪里以及应该怎样改。",
                        ),
                    ] {
                        if ui
                            .add_enabled(!self.study.busy, egui::Button::new(label).small())
                            .clicked()
                        {
                            ask = Some(prompt.into());
                        }
                    }
                    if ui.small_button("参考解答").clicked() {
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
                });
                ui.add_space(8.);
                egui::ScrollArea::vertical()
                    .id_salt(("tutor", kind, &card.id))
                    .max_height((height - 260.).max(90.))
                    .min_scrolled_height((height - 260.).max(90.))
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if s.reference_open {
                            ui.label(RichText::new("参考解答").strong().color(accent()));
                            let reference = if let Some(lab) = &card.sql
                                && lab.runnable
                            {
                                format!("```sql\n{}\n```\n\n{}", lab.reference, lab.explanation)
                            } else {
                                card.answer.clone()
                            };
                            egui_commonmark::CommonMarkViewer::new().show(
                                ui,
                                &mut self.study.markdown,
                                &reference,
                            );
                            if ui.small_button("复制参考解答").clicked() {
                                ui.ctx().copy_text(reference);
                            }
                            ui.separator();
                        }
                        if s.chat.is_empty() && !s.reference_open {
                            ui.label(
                                RichText::new("可以让 AI 讲解整道题，也可以只问你不理解的一步。")
                                    .size(13.)
                                    .color(muted()),
                            );
                        }
                        for message in &s.chat {
                            ui.push_id(&message.id, |ui| {
                                ui.add_space(10.);
                                ui.label(
                                    RichText::new(if message.role == "user" {
                                        "我"
                                    } else {
                                        "AI"
                                    })
                                    .small()
                                    .color(accent()),
                                );
                                egui_commonmark::CommonMarkViewer::new().show(
                                    ui,
                                    &mut self.study.markdown,
                                    &message.content,
                                );
                                if message.role == "assistant"
                                    && ui.small_button("复制回答").clicked()
                                {
                                    ui.ctx().copy_text(message.content.clone());
                                }
                            });
                        }
                        if self.study.busy && self.study.operation == "讲解与追问" {
                            let live = self.study.ai.snapshot();
                            if !live.active || live.partial.is_empty() {
                                ui.spinner();
                            } else {
                                egui_commonmark::CommonMarkViewer::new().show(
                                    ui,
                                    &mut self.study.markdown,
                                    &live.partial,
                                );
                            }
                        }
                    });
                ui.separator();
                ui.add_enabled_ui(!self.study.busy, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut s.chat_input)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .hint_text("哪里没看懂？直接问…"),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !s.chat_input.trim().is_empty(),
                                primary_button("发送追问"),
                            )
                            .clicked()
                        {
                            ask = Some(s.chat_input.clone());
                        }
                        if s.chat.last().is_some_and(|m| m.role == "user")
                            && ui.button("重试").clicked()
                        {
                            ask = s.chat.last().map(|m| m.content.clone());
                        }
                    });
                });
            });
        if let Some(question) = ask {
            self.ask_tutor(kind, s, question);
        }
    }
}
