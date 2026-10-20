use super::*;
use crate::theme::{self, ImageFit, ThemeSettings};

impl PersonalDeskApp {
    pub(super) fn paint_theme_background(&mut self, ui: &mut egui::Ui) {
        self.theme.activate();
        if self.theme_applied.as_ref() != Some(&self.theme) {
            self.theme.apply(ui.ctx());
            self.theme_applied = Some(self.theme.clone());
        }
        let rect = ui.ctx().content_rect();
        ui.painter().rect_filled(rect, 0, canvas());
        if self.theme_background_key != self.theme.background {
            self.theme_background_key = self.theme.background.clone();
            self.theme_background = None;
            if let Some(value) = &self.theme.background {
                match theme::background_path(&self.data_dir, value).and_then(|path| {
                    theme::load_image_with_limit(&path, ui.input(|input| input.max_texture_side))
                }) {
                    Ok(image) => {
                        self.theme_background = Some(ui.ctx().load_texture(
                            "personal-desk-background",
                            image,
                            egui::TextureOptions::LINEAR,
                        ))
                    }
                    Err(error) => self.theme_error = error.to_string(),
                }
            }
        }
        if let Some(texture) = &self.theme_background {
            let (destination, uv) = theme::image_rects(rect, texture.size_vec2(), self.theme.fit);
            ui.painter().image(
                texture.id(),
                destination,
                uv,
                Color32::from_white_alpha((self.theme.image_opacity * 255.0).round() as u8),
            );
        }
    }

    pub(super) fn save_theme(&mut self) -> bool {
        let result = (|| {
            self.theme.validate()?;
            if let Some(value) = self.theme.background.clone()
                && std::path::Path::new(&value).is_absolute()
            {
                self.theme.background = Some(theme::import_image(
                    &self.data_dir,
                    std::path::Path::new(&value),
                )?);
                self.theme_background_key = None;
            }
            let json = serde_json::to_string(&self.theme)?;
            self.db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)?
                .set_setting("appearance.theme", &json)?;
            Ok::<_, DeskError>(())
        })();
        match result {
            Ok(()) => {
                self.theme_saved = self.theme.clone();
                self.theme_error.clear();
                self.notice = "主题已保存".into();
                true
            }
            Err(error) => {
                self.theme_error = error.to_string();
                false
            }
        }
    }

    pub(super) fn render_theme_dialog(&mut self, ctx: &egui::Context) {
        if !self.theme_editor_open {
            return;
        }
        let mut save = false;
        let mut export = false;
        let dialog = centered_dialog(
            ctx,
            "theme-editor",
            "编辑主题",
            "自由选择颜色与背景图片，修改会实时预览，点击保存后保留。",
            620.0,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    save = ui.add(primary_button("保存主题")).clicked();
                    if ui.button("恢复默认").clicked() {
                        self.theme = ThemeSettings::default();
                        self.theme_error.clear();
                    }
                    export = ui.button("导出主题").clicked();
                });
                ui.separator();
                ui.label(RichText::new("界面颜色").strong());
                egui::Grid::new("theme-colors")
                    .num_columns(4)
                    .spacing([18.0, 10.0])
                    .show(ui, |ui| {
                        for (index, (label, color)) in [
                            ("主题色", &mut self.theme.accent),
                            ("正文文字", &mut self.theme.text),
                            ("次要文字", &mut self.theme.muted),
                            ("页面背景", &mut self.theme.canvas),
                            ("侧栏背景", &mut self.theme.sidebar),
                            ("侧栏文字", &mut self.theme.sidebar_text),
                            ("卡片与面板", &mut self.theme.surface),
                            ("输入框", &mut self.theme.input),
                            ("边框", &mut self.theme.border),
                            ("按钮文字", &mut self.theme.button_text),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            ui.label(label);
                            ui.color_edit_button_srgb(color);
                            if index % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                ui.add(egui::Slider::new(&mut self.theme.zoom, 0.8..=1.5).text("界面缩放"));
                ui.add(
                    egui::Slider::new(&mut self.theme.surface_opacity, 0.15..=1.0)
                        .text("卡片与面板不透明度"),
                );
                ui.add(
                    egui::Slider::new(&mut self.theme.sidebar_opacity, 0.15..=1.0)
                        .text("侧栏不透明度"),
                );
                ui.separator();
                ui.label(RichText::new("背景图片").strong());
                ui.horizontal_wrapped(|ui| {
                    if ui.button("上传图片").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_title("选择主题背景图片")
                            .add_filter("图片", &["png", "jpg", "jpeg", "webp", "bmp"])
                            .pick_file()
                    {
                        match theme::load_image(&path) {
                            Ok(_) => {
                                self.theme.background = Some(path.to_string_lossy().into_owned());
                                self.theme_background_key = None;
                                self.theme_error.clear();
                            }
                            Err(error) => self.theme_error = error.to_string(),
                        }
                    }
                    if ui.button("移除背景图片").clicked() {
                        self.theme.background = None;
                        self.theme_background_key = None;
                        self.theme_background = None;
                    }
                });
                if let Some(path) = &self.theme.background {
                    ui.add(egui::Label::new(path).truncate())
                        .on_hover_text(path);
                } else {
                    ui.label("尚未设置图片");
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("图片显示");
                    ui.selectable_value(&mut self.theme.fit, ImageFit::Cover, "铺满裁切");
                    ui.selectable_value(&mut self.theme.fit, ImageFit::Contain, "完整显示");
                    ui.selectable_value(&mut self.theme.fit, ImageFit::Stretch, "拉伸");
                });
                ui.add(
                    egui::Slider::new(&mut self.theme.image_opacity, 0.0..=1.0)
                        .text("背景图片不透明度"),
                );
                ui.add(
                    egui::Slider::new(&mut self.theme.content_overlay, 0.0..=1.0)
                        .text("内容区底色遮罩"),
                );
                if !self.theme_error.is_empty() {
                    ui.colored_label(Color32::from_rgb(184, 65, 71), &self.theme_error);
                }
            },
        );
        if save && self.save_theme() {
            self.theme_editor_open = false;
        }
        if export && self.save_theme() {
            self.start_export(export::Scope::Theme);
        }
        if dialog_closed(&dialog) {
            self.theme = self.theme_saved.clone();
            self.theme_background_key = None;
            self.theme_background = None;
            self.theme_editor_open = false;
            self.theme_error.clear();
        }
    }
}
