//! Shared explanation panel for practice, English and linked knowledge exercises.
use super::*;
use crate::local_ai::Dialogue;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

pub fn panel_id(kind: &str, card: &str) -> egui::Id {
    egui::Id::unique(("learning-tutor", kind, card))
}

pub fn input_id(panel: egui::Id) -> egui::Id {
    panel.with("input")
}

pub enum Action {
    Ask(String),
    ToggleReference,
}

pub struct TutorPanel<'a> {
    pub id: egui::Id,
    pub input: &'a mut String,
    pub messages: &'a [Dialogue],
    pub reference: Option<&'a str>,
    pub busy: bool,
    pub generating: bool,
    pub partial: &'a str,
}

impl TutorPanel<'_> {
    pub fn show(self, ui: &mut Ui, cache: &mut CommonMarkCache) -> Option<Action> {
        let mut action = None;
        let editor_id = input_id(self.id);
        // Consume before TextEdit so sending neither inserts a newline nor grades the answer.
        let shortcut = ui.memory(|m| m.has_focus(editor_id))
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
        let height = ui.available_height();
        egui::Frame::new()
            .fill(surface())
            .stroke(egui::Stroke::new(0.7, border()))
            .corner_radius(14)
            .inner_margin(14)
            .show(ui, |ui| {
                let bottom = ui.cursor().min.y + height - 28.;
                ui.set_width((ui.available_width() - 2.).max(220.));
                ui.label(egui::RichText::new("讲解与追问").size(18.).strong());
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
                            .add_enabled(!self.busy, egui::Button::new(label).small())
                            .clicked()
                        {
                            action = Some(Action::Ask(prompt.into()));
                        }
                    }
                    if ui.small_button("参考解答").clicked() {
                        action = Some(Action::ToggleReference);
                    }
                });
                ui.add_space(8.);
                let remaining = bottom - ui.cursor().min.y;
                let editor_height = (remaining * 0.25).clamp(48., 120.);
                // Reserve the input, action row and shortcut hint before sizing history.
                let history_height = (remaining - editor_height - 88.).max(24.);
                egui::ScrollArea::vertical()
                    .id_salt(self.id.with("history"))
                    .max_height(history_height)
                    .min_scrolled_height(history_height)
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if let Some(reference) = self.reference {
                            ui.label(egui::RichText::new("参考解答").strong().color(accent()));
                            CommonMarkViewer::new().show(ui, cache, reference);
                            if ui.small_button("复制参考解答").clicked() {
                                ui.ctx().copy_text(reference.into());
                            }
                            ui.separator();
                        }
                        if self.messages.is_empty() && self.reference.is_none() {
                            ui.label(
                                egui::RichText::new(
                                    "可以让 AI 讲解整道题，也可以只问你不理解的一步。",
                                )
                                .size(13.)
                                .color(muted()),
                            );
                        }
                        for message in self.messages {
                            ui.push_id(&message.id, |ui| {
                                ui.add_space(10.);
                                ui.label(
                                    egui::RichText::new(if message.role == "user" {
                                        "我"
                                    } else {
                                        "AI"
                                    })
                                    .small()
                                    .color(accent()),
                                );
                                CommonMarkViewer::new().show(ui, cache, &message.content);
                                if message.role == "assistant"
                                    && ui.small_button("复制回答").clicked()
                                {
                                    ui.ctx().copy_text(message.content.clone());
                                }
                            });
                        }
                        if self.generating {
                            if self.partial.is_empty() {
                                ui.spinner();
                            } else {
                                CommonMarkViewer::new().show(ui, cache, self.partial);
                            }
                        }
                    });
                ui.separator();
                ui.add_enabled_ui(!self.busy, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(self.id.with("draft-scroll"))
                        .max_height(editor_height)
                        .min_scrolled_height(editor_height)
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(self.input)
                                    .id(editor_id)
                                    .desired_rows(3)
                                    .desired_width(f32::INFINITY)
                                    .hint_text("哪里没看懂？直接问…"),
                            );
                        });
                    let can_send = !self.input.trim().is_empty();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(can_send, primary_button("发送追问"))
                            .on_hover_text("Ctrl+Enter 发送；Enter 换行")
                            .clicked()
                            || (shortcut && can_send)
                        {
                            action = Some(Action::Ask(self.input.clone()));
                        }
                        if let Some(last) = self.messages.last().filter(|m| m.role == "user")
                            && ui.button("重试").clicked()
                        {
                            action = Some(Action::Ask(last.content.clone()));
                        }
                    });
                });
                ui.label(
                    egui::RichText::new("Ctrl+Enter 发送 · Enter 换行")
                        .small()
                        .color(muted()),
                );
            });
        // Disabled widgets do not block the keyboard path above.
        match action {
            Some(Action::Ask(_)) if self.busy => None,
            other => other,
        }
    }
}

#[cfg(test)]
mod tests;
