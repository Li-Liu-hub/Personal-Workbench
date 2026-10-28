use super::*;

pub(super) fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("floating-writer")
}

impl PersonalDeskApp {
    pub(crate) fn open_floating_writer(&mut self) {
        if self.writing.page.is_some() {
            self.writing.floating = true;
            self.writing.floating_essay = self.section == Section::Essays;
            self.writing.focus_body = true;
        }
    }

    // A failed save keeps the editor and its recovery draft available.
    pub(crate) fn close_floating_writer(&mut self) -> bool {
        if !self.flush_writing() {
            return false;
        }
        self.writing.floating = false;
        self.writing.focus_body = true;
        true
    }

    pub(crate) fn render_floating_writer(&mut self, ctx: &egui::Context) {
        if !self.writing.floating || self.writing.page.is_none() {
            return;
        }
        let title = format!("写作悬浮窗 · {}", self.writing.page.as_ref().unwrap().title);
        let level = if self.writing.floating_pinned {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        };
        ctx.show_viewport_immediate(
            viewport_id(),
            egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size([440., 560.])
                .with_min_inner_size([340., 300.])
                .with_window_level(level),
            |ui, _class| {
                let close = ui.input(|i| i.viewport().close_requested());
                let mut restore = false;
                let mut format = None;
                let mut save = ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S));
                egui::Panel::top("floating-writer-toolbar").show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .checkbox(&mut self.writing.floating_pinned, "置顶")
                            .changed()
                        {
                            let level = if self.writing.floating_pinned {
                                egui::WindowLevel::AlwaysOnTop
                            } else {
                                egui::WindowLevel::Normal
                            };
                            ui.ctx()
                                .send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
                        }
                        save |= ui.button("保存").clicked();
                        restore = ui.button("回到主窗口").clicked();
                        ui.menu_button("更多", |ui| {
                            if ui.button("复制全文").clicked() {
                                let page = self.writing.page.as_ref().unwrap();
                                ui.ctx()
                                    .copy_text(format!("# {}\n\n{}", page.title, page.body));
                                ui.close();
                            }
                            if ui.button("导出 Markdown").clicked() {
                                self.export_writing();
                                ui.close();
                            }
                        });
                    });
                    crate::ui_components::chrome::segmented(
                        ui,
                        &mut self.writing.preview,
                        &[(false, "写作"), (true, "预览")],
                    );
                    format = writing_format_toolbar(ui);
                    ui.horizontal(|ui| {
                        let status = if !self.writing.error.is_empty() {
                            "保存失败"
                        } else if self.writing.dirty {
                            "正在保存…"
                        } else {
                            "已保存"
                        };
                        ui.label(RichText::new(status).small().color(muted()));
                        ui.label(
                            RichText::new(format!("{} 字", self.writing.chars))
                                .small()
                                .color(muted()),
                        );
                    });
                    if !self.writing.error.is_empty() {
                        ui.colored_label(Color32::from_rgb(184, 65, 71), &self.writing.error);
                    }
                });
                let mut changed = false;
                if let Some(syntax) = format {
                    self.writing.preview = false;
                    let id = egui::Id::unique((
                        "floating-body",
                        &self.writing.page.as_ref().unwrap().id,
                    ));
                    insert_writing_format(
                        ui.ctx(),
                        id,
                        &mut self.writing.page.as_mut().unwrap().body,
                        syntax,
                    );
                    changed = true;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(surface()).inner_margin(18))
                    .show(ui, |ui| {
                        let page = self.writing.page.as_mut().unwrap();
                        egui::ScrollArea::vertical()
                            .id_salt(("floating-paper", &page.id))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                changed |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut page.title)
                                            .id_salt(("floating-title", &page.id))
                                            .font(egui::FontId::proportional(22.))
                                            .frame(egui::Frame::NONE)
                                            .desired_width(f32::INFINITY)
                                            .hint_text("未命名文档"),
                                    )
                                    .changed();
                                ui.add_space(14.);
                                if self.writing.preview {
                                    CommonMarkViewer::new().show(
                                        ui,
                                        &mut self.writing.cache,
                                        &page.body,
                                    );
                                } else {
                                    let response = ui.add(
                                        egui::TextEdit::multiline(&mut page.body)
                                            .id(egui::Id::unique(("floating-body", &page.id)))
                                            .font(egui::FontId::proportional(17.))
                                            .frame(egui::Frame::NONE)
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(18)
                                            .hint_text("对照简历提问，把结论写在这里…"),
                                    );
                                    changed |= response.changed();
                                    if self.writing.focus_body {
                                        response.request_focus();
                                        self.writing.focus_body = false;
                                    }
                                }
                            });
                    });
                let now = ui.input(|i| i.time);
                if changed {
                    self.mark_writing_changed(now);
                    ctx.request_repaint();
                }
                if self.writing.dirty {
                    if now - self.writing.changed_at >= 0.5 {
                        self.flush_writing();
                    } else {
                        ui.ctx().request_repaint_after(Duration::from_millis(550));
                    }
                }
                if save {
                    self.flush_writing();
                }
                if close || restore {
                    if self.close_floating_writer() {
                        if restore {
                            self.section = if self.writing.floating_essay {
                                Section::Essays
                            } else {
                                Section::Documents
                            };
                            ctx.send_viewport_cmd_to(
                                egui::ViewportId::ROOT,
                                egui::ViewportCommand::Minimized(false),
                            );
                            ctx.send_viewport_cmd_to(
                                egui::ViewportId::ROOT,
                                egui::ViewportCommand::Focus,
                            );
                        }
                        ctx.request_repaint();
                    } else if close {
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    }
                }
            },
        );
    }
}
