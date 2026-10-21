use super::*;
impl PersonalDeskApp {
    pub(super) fn render_browser(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("←").on_hover_text("后退").clicked() {
                self.browser.back();
            }
            if ui.button("→").on_hover_text("前进").clicked() {
                self.browser.forward();
            }
            if ui.button("刷新").clicked() {
                self.browser.reload();
            }
            let input = ui.add_sized(
                [(ui.available_width() - 200.0).max(100.0), 32.0],
                egui::TextEdit::singleline(&mut self.browser.input).hint_text("输入网址"),
            );
            if ui.button("前往").clicked()
                || (input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                self.browser.navigate();
            }
            ui.menu_button("+", |ui| {
                if ui.button("DeepSeek").clicked() {
                    self.browser.open("https://chat.deepseek.com/");
                    ui.close();
                }
                if ui.button("ChatGPT").clicked() {
                    self.browser.open("https://chatgpt.com/");
                    ui.close();
                }
            });
            ui.menu_button("更多", |ui| {
                if ui.button("外部打开").clicked() {
                    if let Err(error) = open::that_detached(&self.browser.input) {
                        self.browser.error = error.to_string();
                    }
                    ui.close();
                }
            });
            if self.browser.loading() {
                ui.spinner();
            }
        });
        let mut close = None;
        let mut select = None;
        egui::ScrollArea::horizontal()
            .id_salt("browser-tabs")
            .max_height(34.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, tab) in self.browser.tabs.iter().enumerate() {
                        ui.push_id(&tab.id, |ui| {
                            if ui
                                .selectable_label(index == self.browser.selected, &tab.title)
                                .clicked()
                            {
                                select = Some(index);
                            }
                            if ui.small_button("×").clicked() {
                                close = Some(index);
                            }
                        });
                    }
                });
            });
        if let Some(index) = select {
            self.browser.selected = index;
            self.browser.input = self.browser.tabs[index].address.clone();
        }
        if let Some(index) = close {
            self.browser.close(index);
        }
        if !self.browser.error.is_empty() {
            ui.colored_label(Color32::from_rgb(185, 65, 71), &self.browser.error);
        }
        let page_error = self.browser.page_error();
        if !page_error.is_empty() {
            ui.colored_label(Color32::from_rgb(185, 65, 71), page_error);
        }
        if self.browser.tabs.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.heading("新标签页");
                ui.add_space(20.0);
                if ui
                    .add_sized([220.0, 48.0], primary_button("DeepSeek"))
                    .clicked()
                {
                    self.browser.open("https://chat.deepseek.com/");
                }
                if ui
                    .add_sized([220.0, 48.0], egui::Button::new("ChatGPT"))
                    .clicked()
                {
                    self.browser.open("https://chatgpt.com/");
                }
            });
        }
        ui.ctx().request_repaint_after(Duration::from_millis(100));
        ui.add_space(6.0);
        let (rect, _) = ui.allocate_exact_size(
            ui.available_size().max(egui::vec2(100.0, 80.0)),
            egui::Sense::hover(),
        );
        self.browser_area = Some(rect);
    }
    pub(super) fn sync_browser(&mut self, ctx: &egui::Context) {
        self.browser.sync(
            self.native_host,
            self.browser_area,
            ctx.pixels_per_point(),
            self.section == Section::Browser && !self.detail_open(),
        );
    }
}
