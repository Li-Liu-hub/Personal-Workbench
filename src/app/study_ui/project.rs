use super::*;
impl PersonalDeskApp {
    pub(in crate::app) fn render_project_exercise(
        &mut self,
        ui: &mut egui::Ui,
        s: &mut Session,
        card: &Card,
    ) {
        if s.project.as_ref().is_none_or(|p| p.card_id != card.id) {
            s.project = self
                .db
                .as_ref()
                .and_then(|db| db.study_project(&card.id).ok().flatten())
                .or_else(|| {
                    Some(crate::challenges::Saved {
                        card_id: card.id.clone(),
                        source: crate::challenges::original(),
                        report: None,
                    })
                });
            s.answer = crate::challenges::answer(&s.project.as_ref().unwrap().source);
            if let Some(p) = s.pending.as_mut() {
                p.answer = s.answer.clone();
            }
        }
        ui.add_enabled_ui(!self.study.busy && s.result.is_none(), |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("导出项目 ZIP").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_file_name("checkout-repair.zip")
                        .add_filter("项目 ZIP", &["zip"])
                        .save_file()
                {
                    match crate::challenges::export(&s.project.as_ref().unwrap().source, &path) {
                        Ok(()) => self.notice = "项目已导出，修复后导入 ZIP 即可验证".into(),
                        Err(e) => self.study.error = e,
                    }
                }
                if ui.button("导入修复后的 ZIP").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("项目 ZIP", &["zip"])
                        .pick_file()
                {
                    match crate::challenges::import(&path) {
                        Ok(source) => {
                            let saved = crate::challenges::Saved {
                                card_id: card.id.clone(),
                                source,
                                report: None,
                            };
                            let result = self
                                .db
                                .as_mut()
                                .ok_or(DeskError::UnsupportedFormat)
                                .and_then(|db| db.save_study_project(&saved));
                            match result {
                                Ok(()) => {
                                    s.answer = crate::challenges::answer(&saved.source);
                                    s.project = Some(saved);
                                    if let Some(p) = s.pending.as_mut() {
                                        p.answer = s.answer.clone();
                                        if let Some(db) = self.db.as_mut()
                                            && let Err(e) = db.save_study_pending(p)
                                        {
                                            self.study.error = e.to_string();
                                        }
                                    }
                                }
                                Err(e) => self.study.error = e.to_string(),
                            }
                        }
                        Err(e) => self.study.error = e,
                    }
                }
                if ui.button("运行项目测试").clicked() {
                    self.study.busy = true;
                    self.study.operation = "运行项目测试".into();
                    self.study.error.clear();
                    let mut saved = s.project.clone().unwrap();
                    let sender = self.sender.clone();
                    thread::spawn(move || {
                        saved.report = Some(crate::challenges::run(&saved.source));
                        let _ = sender.send(BackgroundMessage::Learning(
                            LearningMessage::ProjectTested(saved),
                        ));
                    });
                }
            });
        });
        if let Some(saved) = &s.project {
            let original = crate::challenges::original();
            let changed = saved
                .source
                .files
                .iter()
                .filter(|(name, text)| original.files.get(*name) != Some(*text))
                .count();
            ui.label(
                RichText::new(format!(
                    "Java 17 · 5 份主源码 · 已修改 {changed} 份 · 9 个测试场景"
                ))
                .size(12.)
                .color(muted()),
            );
            if let Some(report) = &saved.report {
                ui.label(
                    RichText::new(format!(
                        "测试通过 {}/{} · {} ms",
                        report.passed, report.total, report.elapsed_ms
                    ))
                    .color(
                        if report.passed == report.total && report.compiled {
                            Color32::from_rgb(47, 139, 108)
                        } else {
                            accent()
                        },
                    ),
                );
                ui.collapsing("查看测试输出", |ui| {
                    ui.monospace(&report.output);
                });
            }
        }
        ui.add_space(12.);
    }
}
