use super::*;

pub(crate) struct CodeMessage {
    pub card_id: String,
    pub run: crate::coding::Run,
}
impl PersonalDeskApp {
    pub(super) fn run_code(&mut self, s: &Session, card: &Card) {
        let Some(lab) = card.coding.clone() else {
            return;
        };
        if self.study.busy {
            return;
        }
        self.study.busy = true;
        self.study.code_running = true;
        self.study.operation = "运行代码".into();
        self.study.error.clear();
        self.study.code_cancel = Arc::default();
        let cancel = self.study.code_cancel.clone();
        let sender = self.sender.clone();
        let source = s.answer.clone();
        let card_id = card.id.clone();
        let root = crate::speech::runtime_root(&self.data_dir);
        thread::spawn(move || {
            let report = crate::coding::run(&lab, &source, &root, &cancel);
            let _ = sender.send(BackgroundMessage::CodeExecuted(CodeMessage {
                card_id,
                run: crate::coding::Run { source, report },
            }));
        });
    }
    pub(in crate::app) fn accept_code(&mut self, message: CodeMessage) {
        self.study.busy = false;
        self.study.code_running = false;
        self.study.operation.clear();
        if self
            .study
            .practice
            .card
            .as_ref()
            .is_some_and(|c| c.id == message.card_id)
        {
            self.study.practice.code_run = Some(message.run);
        }
    }
    pub(super) fn submit_local(&mut self, s: &mut Session, card: &Card, pending: &Pending) {
        let grade = if card.topic == "听写" {
            crate::english::dictation_grade(card, &s.answer)
        } else {
            let Some(run) = s.code_run.as_ref().filter(|r| r.source == s.answer) else {
                self.study.error = "请先运行当前源码的样例，再提交判题".into();
                return;
            };
            run.report.grade(card, &s.answer)
        };
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| {
                db.record_study_attempt(
                    pending,
                    grade,
                    if card.topic == "听写" {
                        "本地拼写核对"
                    } else {
                        "本机编译与样例判题"
                    },
                    0,
                )
            }) {
            Ok(attempt) => {
                s.result = Some(attempt);
                s.stats = None;
                self.study.error.clear();
                self.notice = "本次练习已记入记录本".into();
            }
            Err(error) => self.study.error = error.to_string(),
        }
    }
    pub(super) fn render_code_materials(
        &mut self,
        ui: &mut egui::Ui,
        s: &mut Session,
        card: &Card,
    ) {
        let Some(lab) = &card.coding else {
            return;
        };
        if s.code_initialized.as_deref() != Some(&card.id) {
            if s.answer.is_empty() && s.result.is_none() {
                s.answer = lab.starter.clone();
                if let Some(p) = s.pending.as_mut() {
                    p.answer = s.answer.clone();
                }
            }
            s.code_initialized = Some(card.id.clone());
            s.code_run = None;
        }
        ui.label(
            RichText::new(format!(
                "{} · {} 组样例 · 每组限时 3 秒",
                lab.language,
                lab.cases.len()
            ))
            .small()
            .color(accent()),
        );
        if lab.language == "C"
            && !crate::coding::compiler(&crate::speech::runtime_root(&self.data_dir), "C").is_file()
            && ui.button("安装轻量 C 编译器（约 0.5 MB）").clicked()
        {
            self.install_learning_runtime("Compiler");
        }
        ui.collapsing("输入与预期输出", |ui| {
            for (i, case) in lab.cases.iter().enumerate() {
                ui.label(format!("样例 {}", i + 1));
                ui.monospace(format!("输入：\n{}\n输出：\n{}", case.input, case.expected));
            }
        });
        if let Some(run) = &s.code_run {
            if run.source == s.answer {
                render_report(ui, &run.report);
            } else {
                ui.label("源码已修改，请重新运行样例");
            }
        }
        ui.add_space(8.);
    }
    pub(super) fn render_card_knowledge(
        &mut self,
        ui: &mut egui::Ui,
        s: &mut Session,
        card: &Card,
    ) {
        if card.knowledge_ids.is_empty() {
            return;
        }
        if s.knowledge_links_for != card.id {
            s.knowledge_links_for = card.id.clone();
            s.knowledge_links = card
                .knowledge_ids
                .iter()
                .filter_map(|id| {
                    self.db
                        .as_ref()
                        .and_then(|db| db.reference_entry(id).ok().flatten())
                        .map(|e| (e.id, e.title))
                })
                .collect();
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("关联知识").small().color(muted()));
            for (id, title) in &s.knowledge_links {
                if ui.small_button(title).clicked() {
                    s.reference_request = Some(id.clone());
                }
            }
        });
        ui.add_space(8.);
    }
}
pub(super) fn render_report(ui: &mut egui::Ui, report: &crate::coding::Report) {
    ui.label(format!(
        "编译{} · 样例通过 {}/{} · {} ms",
        if report.compiled { "成功" } else { "失败" },
        report.passed(),
        report.total,
        report.elapsed_ms
    ));
    if !report.output.is_empty() {
        ui.collapsing("编译 / 运行信息", |ui| {
            ui.monospace(&report.output);
        });
    }
    for (i, case) in report.cases.iter().enumerate() {
        ui.collapsing(
            format!(
                "样例 {} · {}",
                i + 1,
                if case.passed { "通过" } else { "未通过" }
            ),
            |ui| {
                ui.monospace(format!(
                    "输入：\n{}\n预期：\n{}\n实际：\n{}\n{}",
                    case.input, case.expected, case.actual, case.error
                ));
            },
        );
    }
}
