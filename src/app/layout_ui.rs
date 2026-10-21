use super::*;

impl PersonalDeskApp {
    pub(super) fn render_header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let title = self.page_title();
        ui.horizontal(|ui| {
            ui.set_min_height(38.0);
            let actions_width = 300.0
                + if self.section == Section::Documents {
                    90.
                } else {
                    0.
                }
                + if self.has_visible_edits() { 88.0 } else { 0.0 }
                + if self.index_undo_available() {
                    60.0
                } else {
                    0.0
                };
            let width = (ui.available_width() - actions_width).clamp(120.0, 320.0);
            crate::ui_components::toolbar_title(ui, &title, width, 24.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !self.add_label().is_empty()
                    && ui
                        .add(primary_button(&format!("+ {}", self.add_label())))
                        .clicked()
                {
                    self.add_current_item();
                }
                if self.section == Section::Documents && ui.button("生成日志").clicked() {
                    self.new_writing(true);
                }
                if ui
                    .add_enabled(!self.export_busy, egui::Button::new("导出"))
                    .clicked()
                {
                    self.export_dialog_open = true;
                }
                if self.has_visible_edits() && ui.button("保存修改").clicked() {
                    self.save_visible_rows();
                }
                if self.index_undo_available()
                    && ui.button("撤回").on_hover_text("Ctrl+Z").clicked()
                {
                    self.undo_index_removal();
                }
                if !matches!(self.section, Section::Games) {
                    self.render_view_switcher(ui);
                }
            });
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.set_min_height(32.0);
            let has_filter = TagScope::for_section(self.section).is_some();
            let controls = if has_filter { 310.0 } else { 130.0 };
            let width = (ui.available_width() - controls).max(160.0);
            let hint = match self.section {
                Section::Folders => "搜索名称、路径、标签  Ctrl+K",
                Section::Credentials => "搜索名称、账号、密码、标签  Ctrl+K",
                Section::Documents => "搜索标题、正文、标签  Ctrl+K",
                _ => "搜索当前页面  Ctrl+K",
            };
            let response = ui.add_sized(
                [width, 32.0],
                egui::TextEdit::singleline(&mut self.search_query)
                    .id_salt("global-search")
                    .margin(egui::Margin::symmetric(10, 7))
                    .hint_text(hint),
            );
            self.search_field = Some(response.id);
            if response.changed() {
                self.search_pending = Some(ui.input(|input| input.time) + 0.3);
                ctx.request_repaint_after(Duration::from_millis(310));
            }
            if ui.button("搜索").clicked()
                || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                self.search_pending = None;
                self.run_search();
            }
            if ui
                .add_enabled(!self.search_query.is_empty(), egui::Button::new("清空"))
                .clicked()
            {
                self.search_query.clear();

                self.search_pending = None;
            }
            if has_filter {
                ui.separator();
                self.render_module_filters(ui);
            }
        });
        if self
            .search_pending
            .is_some_and(|deadline| ctx.input(|i| i.time) >= deadline)
        {
            self.search_pending = None;
            if matches!(self.section, Section::Folders | Section::Documents) {
                self.run_search();
            }
        }
    }

    fn has_visible_edits(&self) -> bool {
        match self.section {
            Section::Contacts => !self.inline_contacts.is_empty(),
            Section::Credentials => !self.inline_credentials.is_empty(),
            Section::Inventory => !self.inline_inventory.is_empty(),
            _ => false,
        }
    }

    pub(super) fn render_status(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 36.0).max(0.0);
            ui.add_sized(
                [width, 18.0],
                egui::Label::new(RichText::new(&self.notice).small().color(muted())).truncate(),
            )
            .on_hover_text(&self.notice);
            if !self.notice.is_empty() && ui.small_button("×").on_hover_text("关闭提示").clicked()
            {
                self.notice.clear();
            }
        });
    }
}
