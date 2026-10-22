use super::*;
use crate::essays::{Assignment, Summary};

#[derive(Default)]
pub(super) struct EssayState {
    today: Option<Assignment>,
    pub(super) active: Option<Assignment>,
    history: bool,
    query: String,
    catalog_key: Option<(u64, String)>,
    catalog: Vec<Summary>,
    error: String,
}

impl PersonalDeskApp {
    fn refresh_essay_day(&mut self) {
        let day = chrono::Local::now().date_naive();
        if self
            .essay
            .today
            .as_ref()
            .is_some_and(|a| a.day == day.to_string())
        {
            return;
        }
        match self.db.as_ref().unwrap().ensure_essay_day(day) {
            Ok(a) => {
                self.essay.today = Some(a);
                self.essay.error.clear();
            }
            Err(e) => self.essay.error = e.to_string(),
        }
    }

    pub(super) fn start_daily_essay(&mut self) {
        if !self.flush_writing() {
            return;
        }
        self.refresh_essay_day();
        let Some(day) = self.essay.today.as_ref().map(|a| a.day.clone()) else {
            return;
        };
        match self.db.as_mut().unwrap().start_essay(&day) {
            Ok(page) => {
                let assignment = self.db.as_ref().unwrap().essay_assignment(&day);
                match assignment {
                    Ok(Some(a)) => {
                        self.essay.today = Some(a.clone());
                        self.essay.active = Some(a);
                        self.set_writing_page(page);
                        self.essay.error.clear();
                    }
                    Ok(None) => self.essay.error = "随笔主题已不存在".into(),
                    Err(e) => self.essay.error = e.to_string(),
                }
            }
            Err(e) => self.essay.error = e.to_string(),
        }
    }

    pub(super) fn render_essays(&mut self, ui: &mut egui::Ui) {
        self.refresh_essay_day();
        let editing = self
            .essay
            .active
            .as_ref()
            .and_then(|a| a.page_id.as_deref())
            .zip(self.writing.page.as_ref())
            .is_some_and(|(id, p)| id == p.id);
        if editing {
            self.render_writing_editor(ui);
            self.handle_workspace_shortcuts(ui.ctx());
            return;
        }
        self.essay.active = None;
        self.search_field = None;
        ui.horizontal(|ui| {
            crate::ui_components::toolbar_title(ui, "每日随笔", 180., 24.);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("导出").clicked() {
                    self.export_dialog_open = true;
                }
                crate::ui_components::chrome::segmented(
                    ui,
                    &mut self.essay.history,
                    &[(false, "今日主题"), (true, "记录本")],
                );
            });
        });
        ui.add_space(18.);
        if !self.essay.error.is_empty() {
            ui.colored_label(Color32::from_rgb(184, 65, 71), &self.essay.error);
        }
        if self.essay.history {
            self.render_essay_history(ui);
        } else {
            egui::ScrollArea::vertical()
                .id_salt("essay-today")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let width = ui.available_width().min(820.);
                    ui.horizontal_top(|ui| {
                        ui.add_space(((ui.available_width() - width) / 2.).max(0.));
                        egui::Frame::new()
                            .fill(surface())
                            .corner_radius(16)
                            .inner_margin(egui::Margin::same(30))
                            .show(ui, |ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                    ui.set_width((width - 60.).max(240.));
                                    let Some(a) = self.essay.today.clone() else {
                                        return;
                                    };
                                    ui.label(
                                        RichText::new(format!(
                                            "{}  /  {}",
                                            a.day, a.topic.category
                                        ))
                                        .small()
                                        .color(muted()),
                                    );
                                    ui.add_space(22.);
                                    ui.label(RichText::new(&a.topic.title).size(30.).color(ink()));
                                    ui.add_space(16.);
                                    ui.label(
                                        RichText::new(&a.topic.prompt).size(17.).color(muted()),
                                    );
                                    ui.add_space(32.);
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add(primary_button(if a.page_id.is_some() {
                                                "继续写作"
                                            } else {
                                                "开始写作"
                                            }))
                                            .clicked()
                                        {
                                            self.start_daily_essay();
                                        }
                                        if a.page_id.is_none() && ui.button("换个主题").clicked()
                                        {
                                            match self
                                                .db
                                                .as_ref()
                                                .unwrap()
                                                .reroll_essay(&a.day, &a.topic.id)
                                            {
                                                Ok(a) => {
                                                    self.essay.today = Some(a);
                                                    self.essay.error.clear();
                                                }
                                                Err(e) => self.essay.error = e.to_string(),
                                            }
                                        }
                                    });
                                });
                            });
                    });
                });
        }
        self.handle_workspace_shortcuts(ui.ctx());
    }

    fn render_essay_history(&mut self, ui: &mut egui::Ui) {
        let response = ui.add_sized(
            [ui.available_width(), 32.],
            egui::TextEdit::singleline(&mut self.essay.query)
                .hint_text("搜索日期、主题、标题或正文  Ctrl+K"),
        );
        self.search_field = Some(response.id);
        let key = (
            self.db.as_ref().unwrap().change_count(),
            self.essay.query.clone(),
        );
        if self.essay.catalog_key.as_ref() != Some(&key) {
            match self.db.as_ref().unwrap().essay_summaries(&self.essay.query) {
                Ok(rows) => {
                    self.essay.catalog = rows;
                    self.essay.catalog_key = Some(key);
                }
                Err(e) => self.essay.error = e.to_string(),
            }
        }
        ui.add_space(12.);
        if self.essay.catalog.is_empty() {
            ui.label(RichText::new("暂无随笔").color(muted()));
        }
        let mut selected = None;
        egui::ScrollArea::vertical()
            .id_salt("essay-history")
            .auto_shrink([false, false])
            .show_rows(ui, 68., self.essay.catalog.len(), |ui, range| {
                for index in range {
                    let row = &self.essay.catalog[index];
                    let label = format!("{}\n{}", row.title, row.day);
                    if ui
                        .add_sized(
                            [ui.available_width(), 64.],
                            egui::Button::new(label).wrap_mode(egui::TextWrapMode::Truncate),
                        )
                        .on_hover_text(&row.topic)
                        .clicked()
                    {
                        selected = Some((row.day.clone(), row.page_id.clone()));
                    }
                }
            });
        if let Some((day, id)) = selected {
            match self.db.as_ref().unwrap().essay_assignment(&day) {
                Ok(Some(a)) if self.open_writing(&id) => self.essay.active = Some(a),
                Ok(_) => {}
                Err(e) => self.essay.error = e.to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests;
