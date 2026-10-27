use super::*;
use crate::launcher::AppTool;

impl PersonalDeskApp {
    pub(super) fn render_tools(&mut self, ui: &mut egui::Ui) {
        let mut records = self
            .db
            .as_ref()
            .and_then(|db| db.app_tools().ok())
            .unwrap_or_default();
        records.sort_by_key(|item| !self.is_favorite("tools", &item.id));
        let mut edit = None;
        let mut remove = None;
        let mut launch = None;
        let mut add = false;
        collection(ui, "tools-gallery", self.view_mode(), |ui, layout| {
            if self.is_favorite("tools", campus_ui::TOOL_ID) {
                self.render_campus_entry(ui, layout);
            }
            self.render_hiring_entries(ui, layout, true);
            if self.is_favorite("tools", pdf_images_ui::TOOL_ID) {
                self.render_pdf_images_entry(ui, layout);
            }
            let mut builtin_rendered = self.is_favorite("tools", excel_merge_ui::TOOL_ID);
            if builtin_rendered {
                self.render_excel_merge_entry(ui, layout);
            }
            let visible = records
                .iter()
                .filter(|item| {
                    self.record_matches_filter(TagScope::Tools, &item.id)
                        && matches_query(
                            &self.search_query.to_lowercase(),
                            &[&item.name, &item.path.to_string_lossy(), &item.notes],
                        )
                })
                .cloned()
                .collect::<Vec<_>>();
            for item in &visible {
                if !builtin_rendered && !self.is_favorite("tools", &item.id) {
                    self.render_excel_merge_entry(ui, layout);
                    builtin_rendered = true;
                }
                let labels = self.record_labels(TagScope::Tools, &item.id);
                let response = tagged_entry(
                    ui,
                    &item.id,
                    &item.name,
                    (&item.path.to_string_lossy(), &labels),
                    TileKind::Setting,
                    layout,
                );
                self.favorite_control(ui, &response, "tools", &item.id);
                if response.double_clicked() {
                    launch = Some(item.path.clone());
                }
                response.context_menu(|ui| {
                    self.record_tag_menu(ui, TagScope::Tools, &item.id);
                    if ui.button("启动程序").clicked() {
                        launch = Some(item.path.clone());
                        ui.close();
                    }
                    if ui.button("编辑工具").clicked() {
                        edit = Some(item.clone());
                        ui.close();
                    }
                    if ui.button("移除工具").clicked() {
                        remove = Some(item.id.clone());
                        ui.close();
                    }
                });
            }
            if !builtin_rendered {
                self.render_excel_merge_entry(ui, layout);
            }
            if !self.is_favorite("tools", pdf_images_ui::TOOL_ID) {
                self.render_pdf_images_entry(ui, layout);
            }
            self.render_hiring_entries(ui, layout, false);
            if !self.is_favorite("tools", campus_ui::TOOL_ID) {
                self.render_campus_entry(ui, layout);
            }
            add = entry(
                ui,
                "add",
                "点击添加小工具",
                "选择 .exe 程序路径",
                TileKind::Add,
                layout,
            )
            .clicked();
        });
        if add {
            self.add_tool();
        }
        if let Some(item) = edit {
            self.tool_draft = Some(item);
            self.tool_error.clear();
        }
        if let Some(id) = remove
            && let Some(db) = self.db.as_mut()
        {
            self.notice = match db.delete_app_tool(&id) {
                Ok(()) => "快捷入口已移除，原程序保留".into(),
                Err(e) => e.to_string(),
            };
        }
        if let Some(path) = launch {
            self.tool_launch_request = Some(path);
        }
    }
    fn render_excel_merge_entry(
        &mut self,
        ui: &mut egui::Ui,
        layout: crate::ui_components::EntryLayout,
    ) {
        let id = excel_merge_ui::TOOL_ID;
        if self.record_matches_filter(TagScope::Tools, id)
            && matches_query(
                &self.search_query.to_lowercase(),
                &[
                    "Excel 批量合并",
                    "excel xlsx xlsm xls xlsb ods 表格 文件夹 分表 内置工具",
                ],
            )
        {
            let response = entry(
                ui,
                id,
                "Excel 批量合并",
                if self.excel_merge.busy() {
                    "正在合并 · 点击查看进度"
                } else {
                    "内置工具 · 合并文件夹中全部工作表"
                },
                TileKind::Document,
                layout,
            );
            self.favorite_control(ui, &response, "tools", id);
            if response.clicked() {
                self.excel_merge.open = true;
            }
            response.context_menu(|ui| {
                if ui.button("打开 Excel 合并").clicked() {
                    self.excel_merge.open = true;
                    ui.close();
                }
            });
        }
    }
    pub(super) fn add_tool(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("选择小工具程序")
            .add_filter("Windows 程序", &["exe"])
            .pick_file()
        {
            self.tool_draft = Some(AppTool::new(path));
            self.tool_error.clear();
        }
    }
    pub(super) fn render_tool_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.tool_draft.take() else {
            return;
        };
        let mut path_text = draft.path.to_string_lossy().into_owned();
        let mut save = false;
        let dialog = centered_dialog(
            ctx,
            "tool-details",
            "小工具",
            "双击入口即可启动这个程序，移除入口不会删除程序。",
            520.0,
            |ui| {
                ui.label("名称");
                ui.text_edit_singleline(&mut draft.name);
                ui.label("程序路径");
                ui.text_edit_singleline(&mut path_text);
                if ui.button("重新选择程序").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Windows 程序", &["exe"])
                        .pick_file()
                {
                    path_text = path.to_string_lossy().into_owned();
                }
                ui.label("备注");
                ui.add(
                    egui::TextEdit::multiline(&mut draft.notes)
                        .desired_width(f32::INFINITY)
                        .desired_rows(2),
                );
                if !self.tool_error.is_empty() {
                    ui.colored_label(Color32::from_rgb(184, 65, 71), &self.tool_error);
                }
                save = ui.add(primary_button("保存工具")).clicked();
            },
        );
        draft.path = PathBuf::from(path_text.trim());
        let mut close = dialog_closed(&dialog);
        if save {
            match self
                .db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| db.save_app_tool(&draft))
            {
                Ok(()) => {
                    close = true;
                    self.notice = "小工具已保存".into();
                }
                Err(error) => self.tool_error = error.to_string(),
            }
        }
        if !close {
            self.tool_draft = Some(draft);
        }
    }
}
