use super::*;
impl PersonalDeskApp {
    pub(super) fn render_codex(&mut self, ui: &mut egui::Ui) {
        if self.desktop.basic_chat {
            self.render_basic_codex(ui);
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("Codex / ChatGPT 原版界面")
                    .strong()
                    .color(ink()),
            );
            if self.desktop.embedded_title().is_some() {
                if ui.button("恢复独立窗口").clicked() {
                    if let Err(e) = self.desktop.detach() {
                        self.desktop.error = e;
                    }
                    self.desktop.attempted = true;
                }
            } else {
                if ui.add(primary_button("嵌入现有窗口")).clicked() {
                    self.desktop.refresh();
                    self.desktop.attach_request = self
                        .desktop
                        .candidates
                        .iter()
                        .find(|w| w.handle == self.desktop.selected)
                        .cloned();
                    self.desktop.attempted = true;
                    if self.desktop.attach_request.is_none() {
                        self.desktop.error =
                            "未找到原版应用窗口，请先打开 Codex / ChatGPT，再点击刷新。".into();
                    }
                }
                if ui.button("启动原版应用").clicked() {
                    if let Err(e) = crate::native_embed::launch_desktop() {
                        self.desktop.error = e;
                    }
                    self.desktop.candidates.clear();
                    self.desktop.attempted = false;
                }
            }
            if ui.button("刷新窗口").clicked() {
                self.desktop.refresh();
                self.desktop.attempted = false;
                self.desktop.error.clear();
            }
            if ui.button("基础聊天").clicked() {
                self.desktop.basic_chat = true;
            }
        });
        if !self.desktop.error.is_empty() {
            ui.colored_label(Color32::from_rgb(184, 65, 71), &self.desktop.error);
        }
        if self.desktop.embedded_title().is_none() {
            if self.desktop.candidates.is_empty() {
                self.desktop.refresh();
            }
            if !self.desktop.candidates.is_empty() {
                egui::ComboBox::from_id_salt("native-codex-window")
                    .selected_text(
                        self.desktop
                            .candidates
                            .iter()
                            .find(|w| w.handle == self.desktop.selected)
                            .map(|w| w.title.as_str())
                            .unwrap_or("选择窗口"),
                    )
                    .show_ui(ui, |ui| {
                        for candidate in &self.desktop.candidates {
                            ui.selectable_value(
                                &mut self.desktop.selected,
                                candidate.handle,
                                format!("{} · {}", candidate.title, candidate.process_id),
                            );
                        }
                    });
            }
            if !self.desktop.attempted
                && self.desktop.parent != 0
                && self.desktop.candidates.len() == 1
            {
                self.desktop.attach_request = self.desktop.candidates.first().cloned();
                self.desktop.attempted = true;
            }
        }
        ui.add_space(8.0);
        let (area, _) = ui.allocate_exact_size(
            ui.available_size().max(egui::vec2(100.0, 80.0)),
            egui::Sense::hover(),
        );
        let scale = ui.ctx().pixels_per_point();
        self.desktop.area = Some([
            (area.left() * scale).round() as i32,
            (area.top() * scale).round() as i32,
            (area.width() * scale).round() as i32,
            (area.height() * scale).round() as i32,
        ]);
        if self.desktop.embedded_title().is_none() {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(area.shrink(22.0))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.add_space(28.0);
            child.heading("Codex");
        }
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
    pub(super) fn sync_native_codex(&mut self) {
        if self.section != Section::Codex || self.desktop.basic_chat {
            if let Err(error) = self.desktop.detach() {
                self.desktop.error = error;
            }
            self.desktop.attempted = false;
            return;
        }
        if let Some(window) = self.desktop.attach_request.take()
            && let Err(error) = self.desktop.attach(window)
        {
            self.desktop.error = error;
        }
        if let Err(error) = self.desktop.synchronize(!self.detail_open()) {
            self.desktop.error = error;
        }
    }
}
