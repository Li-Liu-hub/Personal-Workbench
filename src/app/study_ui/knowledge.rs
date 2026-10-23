use super::*;

impl PersonalDeskApp {
    pub(in crate::app) fn practice_knowledge(&mut self, entry: &crate::reference::Entry) {
        if self.study.busy || self.study.running_sql {
            self.notice = "当前练习处理完成后即可切换知识点".into();
            return;
        }
        if !self.flush_writing() || !self.flush_study_answer() {
            return;
        }
        let mut session = Session {
            knowledge: Some((entry.id.clone(), entry.title.clone())),
            topic: crate::curriculum::MIXED.into(),
            ..Default::default()
        };
        self.next_study("engineering", &mut session);
        self.study.practice = session;
        self.section = Section::Practice;
        self.search_query.clear();
    }

    pub(super) fn render_knowledge_scope(&mut self, ui: &mut egui::Ui, s: &mut Session) {
        if let Some((_, title)) = &s.knowledge {
            ui.label(RichText::new(format!("专项 · {title}")).strong());
            if ui.button("返回知识点").clicked() {
                s.knowledge_back = true;
            }
            if ui
                .add_enabled(
                    !self.study.busy && !self.study.running_sql,
                    egui::Button::new("结束专项"),
                )
                .clicked()
            {
                s.knowledge = None;
                s.stats = None;
                s.notebook_key = None;
                s.topic.clear();
                s.recent.clear();
                self.next_study("engineering", s);
            }
        }
    }

    pub(super) fn open_knowledge_reference(&mut self, id: &str) {
        if !self.flush_study_answer() {
            return;
        }
        if self.reference.draft.is_some() {
            self.notice = "请先保存百科条目草稿".into();
            return;
        }
        let mut state = std::mem::take(&mut self.reference);
        state.filter = Default::default();
        state.loaded = None;
        self.select_reference(&mut state, id);
        self.reference = state;
        self.section = Section::Reference;
        self.search_query.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::editor_tests::{input_frame, pointer};
    fn location(out: &egui::FullOutput, text: &str) -> egui::Pos2 {
        out.shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == text => {
                    Some(t.galley.rect.translate(t.pos.to_vec2()).center())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing {text}"))
    }
    #[test]
    fn practice_and_back_buttons_keep_exact_scope_and_both_pending_answers() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        let old = app
            .db
            .as_ref()
            .unwrap()
            .study_card("eng:2:01")
            .unwrap()
            .unwrap();
        app.study.practice = Session {
            pending: Some(pending(&old, "此前答案")),
            answer: "此前答案".into(),
            card: Some(old.clone()),
            ..Default::default()
        };
        let mut state = std::mem::take(&mut app.reference);
        app.select_reference(&mut state, "ref:mysql:last-value");
        app.reference = state;
        app.section = Section::Reference;
        app.search_query = "LAST_VALUE".into();
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        input_frame(&mut app, &ctx, 0., vec![], vec![]);
        let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        let p = location(&out, "练习");
        input_frame(&mut app, &ctx, 0.2, pointer(p, true), vec![]);
        input_frame(&mut app, &ctx, 0.3, pointer(p, false), vec![]);
        assert_eq!(app.section, Section::Practice);
        assert_eq!(
            app.study.practice.knowledge.as_ref().unwrap().0,
            "ref:mysql:last-value"
        );
        assert!(
            app.study
                .practice
                .card
                .as_ref()
                .unwrap()
                .knowledge_ids
                .contains(&"ref:mysql:last-value".into())
        );
        app.study.practice.answer = "专项答案".into();
        app.study.practice.pending.as_mut().unwrap().answer = "专项答案".into();
        let out = input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
        let p = location(&out, "返回知识点");
        input_frame(&mut app, &ctx, 0.5, pointer(p, true), vec![]);
        input_frame(&mut app, &ctx, 0.6, pointer(p, false), vec![]);
        assert_eq!(app.section, Section::Reference);
        assert_eq!(
            app.reference.selected.as_ref().unwrap().id,
            "ref:mysql:last-value"
        );
        let pending = app
            .db
            .as_ref()
            .unwrap()
            .study_pending("engineering")
            .unwrap();
        assert!(
            pending
                .iter()
                .any(|p| p.card_id == old.id && p.answer == "此前答案")
        );
        assert!(pending.iter().any(|p| p.answer == "专项答案"));
    }
    #[test]
    fn empty_custom_point_does_not_resume_an_unrelated_draft_and_busy_work_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        let card = app
            .db
            .as_ref()
            .unwrap()
            .study_card("eng:2:01")
            .unwrap()
            .unwrap();
        app.db
            .as_mut()
            .unwrap()
            .save_study_pending(&pending(&card, "unrelated"))
            .unwrap();
        let entry = crate::reference::Entry::new();
        app.practice_knowledge(&entry);
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        input_frame(&mut app, &ctx, 0., vec![], vec![]);
        assert!(app.study.practice.card.is_none());
        assert!(app.study.practice.draw_attempted);
        app.study.busy = true;
        let old = app.study.practice.knowledge.clone();
        app.practice_knowledge(&crate::reference::Entry::new());
        assert_eq!(app.study.practice.knowledge, old);
    }

    #[test]
    fn scoped_sql_keeps_a_usable_answer_area_above_actions_in_small_windows() {
        for size in [[1000., 680.], [1280., 800.]] {
            let dir = tempfile::tempdir().unwrap();
            let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
            let entry = app
                .db
                .as_ref()
                .unwrap()
                .reference_entry("ref:mysql:last-value")
                .unwrap()
                .unwrap();
            app.practice_knowledge(&entry);
            app.study.tutor_open = false;
            let card = app
                .db
                .as_ref()
                .unwrap()
                .study_card("knowledge:ref:mysql:last-value")
                .unwrap()
                .unwrap();
            app.study.practice.card = Some(card.clone());
            app.study.practice.pending = Some(pending(&card, ""));
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            let mut out = egui::FullOutput::default();
            for _ in 0..3 {
                out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                        ..Default::default()
                    },
                    |ui| app.render_workspace(ui),
                );
                out.textures_delta.clear();
            }
            let editor = location(&out, "你的 SQL");
            let actions = location(&out, "运行 SQL");
            assert!(
                editor.y + 65. < actions.y,
                "{size:?}: editor={editor:?}, actions={actions:?}"
            );
            assert!(actions.y < size[1] - 12.);
        }
    }
}
