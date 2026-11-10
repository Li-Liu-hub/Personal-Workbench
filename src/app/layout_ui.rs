use super::*;

impl PersonalDeskApp {
    pub(super) fn render_workspace_header(&mut self, ui: &mut egui::Ui) {
        let title = self.page_title();
        ui.horizontal(|ui| {
            ui.set_min_height(38.0);
            let compact = self.navigation_compact && self.section == Section::Documents;
            let navigation_width = if compact { 64.0 } else { 184.0 };
            ui.add_space(12.0);
            ui.allocate_ui_with_layout(
                egui::vec2(navigation_width + 12.0, 38.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| self.render_app_brand(ui, compact),
            );
            let editing_document = matches!(self.section, Section::Documents | Section::Books)
                && self.writing.page.is_some();
            let content_actions_width = if editing_document {
                0.0
            } else if self.section == Section::Essays {
                190.0
            } else {
                300.0
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
                    }
            };
            let actions_width = content_actions_width + 126.0;
            // The title response also covers the otherwise empty space between the
            // page title and window controls, making the whole free header area draggable.
            let width = (ui.available_width() - actions_width).max(120.0);
            crate::ui_components::toolbar_title(ui, &title, width, 24.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.render_window_controls(ui);
                if self.section == Section::Essays {
                    crate::ui_components::chrome::segmented(
                        ui,
                        &mut self.essay.history,
                        &[(false, "今日主题"), (true, "记录本")],
                    );
                    if ui.animated_button("导出").clicked() {
                        self.export_dialog_open = true;
                    }
                    return;
                }
                if editing_document {
                    return;
                }
                if !self.add_label().is_empty()
                    && ui
                        .add(primary_button(&format!("+ {}", self.add_label())))
                        .clicked()
                {
                    self.add_current_item();
                }
                if self.section == Section::Documents && ui.animated_button("生成日志").clicked()
                {
                    self.new_writing(true);
                }
                if ui
                    .add_enabled(!self.export_busy, egui::Button::new("导出"))
                    .clicked()
                {
                    self.export_dialog_open = true;
                }
                if self.has_visible_edits() && ui.animated_button("保存修改").clicked() {
                    self.save_visible_rows();
                }
                if self.index_undo_available()
                    && ui.animated_button("撤回").on_hover_text("Ctrl+Z").clicked()
                {
                    self.undo_index_removal();
                }
                self.render_view_switcher(ui);
            });
        });
    }

    pub(super) fn render_header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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
                    .margin(egui::Margin {
                        left: 34,
                        right: 12,
                        top: 8,
                        bottom: 8,
                    })
                    .hint_text(hint),
            );
            self.search_field = Some(response.id);
            let center = response.rect.left_center() + egui::vec2(17.0, -1.0);
            let stroke = egui::Stroke::new(1.4, muted());
            ui.painter().circle_stroke(center, 5.0, stroke);
            ui.painter().line_segment(
                [center + egui::vec2(4.0, 4.0), center + egui::vec2(8.0, 8.0)],
                stroke,
            );
            if response.changed() {
                self.search_pending = Some(ui.input(|input| input.time) + 0.3);
                ctx.request_repaint_after(Duration::from_millis(310));
            }
            if ui.animated_button("搜索").clicked()
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

    fn render_window_controls(&self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));

        if ui
            .add_sized([32.0, 28.0], egui::Button::new("×"))
            .on_hover_text("关闭")
            .clicked()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let maximize = ui.add_sized([32.0, 28.0], egui::Button::new(""));
        let square = maximize.rect.shrink(9.0);
        let stroke = egui::Stroke::new(1.3, ui.visuals().text_color());
        if maximized {
            ui.painter().rect_stroke(
                square.translate(egui::vec2(2.0, -2.0)),
                0,
                stroke,
                egui::StrokeKind::Inside,
            );
            ui.painter()
                .rect_stroke(square, 0, stroke, egui::StrokeKind::Inside);
        } else {
            ui.painter()
                .rect_stroke(square, 0, stroke, egui::StrokeKind::Inside);
        }
        if maximize
            .on_hover_text(if maximized { "还原" } else { "最大化" })
            .clicked()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
        if ui
            .add_sized([32.0, 28.0], egui::Button::new("—"))
            .on_hover_text("最小化")
            .clicked()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
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
            if !self.notice.is_empty()
                && ui
                    .small_animated_button("×")
                    .on_hover_text("关闭提示")
                    .clicked()
            {
                self.notice.clear();
            }
        });
    }
}
