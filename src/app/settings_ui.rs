use super::*;

mod history;
#[cfg(test)]
mod tests;

impl PersonalDeskApp {
    pub(super) fn render_settings(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style())
            .fill(surface())
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.set_min_width((ui.available_width() - 1.0).max(0.0));
                ui.strong("启动设置");
                ui.add_space(8.0);
                let registered = crate::autostart::registered_command();
                let mut enabled = registered.as_ref().is_ok_and(|value| value.is_some());
                ui.horizontal_wrapped(|ui| {
                    if ui.checkbox(&mut enabled, "开机自启动").changed() {
                        match std::env::current_exe()
                            .map_err(|e| e.to_string())
                            .and_then(|path| crate::autostart::set(enabled, &path))
                        {
                            Ok(()) => {
                                self.notice = if enabled {
                                    "已开启开机自启动".into()
                                } else {
                                    "已关闭开机自启动".into()
                                }
                            }
                            Err(error) => {
                                enabled = registered.as_ref().is_ok_and(|value| value.is_some());
                                self.notice = error;
                            }
                        }
                    }
                    ui.label(
                        RichText::new(if enabled { "已开启" } else { "已关闭" }).color(muted()),
                    );
                    if let Ok(Some(value)) = &registered
                        && let Ok(path) = std::env::current_exe()
                        && *value != crate::autostart::command(&path)
                        && ui.button("更新启动路径").clicked()
                    {
                        self.notice = match crate::autostart::set(true, &path) {
                            Ok(()) => "启动路径已更新".into(),
                            Err(error) => error,
                        };
                    }
                });
                if let Err(error) = registered {
                    ui.colored_label(Color32::from_rgb(185, 65, 71), error);
                }
            });
        ui.add_space(16.0);
        collection(ui, "settings-gallery", self.view_mode(), |ui, side| {
            if entry(
                ui,
                "version-history",
                "版本历史",
                "查看每个版本的新增功能与修复",
                TileKind::Setting,
                side,
            )
            .clicked()
            {
                crate::ui_components::reset_dialog_scroll(ui.ctx(), "settings-details");
                self.settings_detail = Some("版本历史".into());
            }
            if entry(
                ui,
                "edit-theme",
                "编辑主题",
                "颜色、背景图片与透明度",
                TileKind::Setting,
                side,
            )
            .clicked()
            {
                self.theme_editor_open = true;
                self.theme_error.clear();
            }
            if entry(
                ui,
                "export-all",
                "导出全部数据",
                "账号、联系人、聊天与文档",
                TileKind::Setting,
                side,
            )
            .clicked()
            {
                self.export_dialog_open = true;
            }
            if entry(
                ui,
                "data",
                "本地数据",
                "查看数据保存位置",
                TileKind::Setting,
                side,
            )
            .clicked()
            {
                self.settings_detail = Some("本地数据".into());
            }
        });
    }

    pub(super) fn render_version_history(&self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(format!("当前版本  v{}", crate::updater::VERSION))
                .strong()
                .color(accent()),
        );
        ui.label(
            RichText::new("从新到旧排列，点击版本展开更新内容。")
                .small()
                .color(muted()),
        );
        ui.add_space(4.0);
        for release in history::releases() {
            egui::CollapsingHeader::new(format!("v{} · {}", release.version, release.title))
                .id_salt(("release-history", &release.version))
                .default_open(release.version == crate::updater::VERSION)
                .show(ui, |ui| {
                    for change in &release.changes {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new("•").color(accent()));
                            ui.add(egui::Label::new(change).wrap());
                        });
                    }
                });
            ui.separator();
        }
    }
}
