use super::*;
use serde_json::json;
impl PersonalDeskApp {
    fn save_codex_settings(&mut self) {
        if let Some(db) = self.db.as_mut() {
            for (key, value) in [
                (
                    "codex.executable",
                    self.codex.executable.to_string_lossy().into_owned(),
                ),
                (
                    "codex.project",
                    self.codex.project.to_string_lossy().into_owned(),
                ),
                ("codex.model", self.codex.model.clone()),
            ] {
                if let Err(error) = db.set_setting(key, &value) {
                    self.codex.error = error.to_string();
                }
            }
        }
    }
    pub(super) fn render_basic_codex(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.codex.busy,
                    egui::Button::new(if self.codex.ready {
                        "重新连接"
                    } else {
                        "连接本机 Codex"
                    }),
                )
                .clicked()
            {
                self.save_codex_settings();
                self.codex.connect();
            }
            if ui
                .add_enabled(
                    self.codex.ready && !self.codex.busy,
                    egui::Button::new("登录 Codex"),
                )
                .clicked()
            {
                self.codex.login();
            }
            if ui
                .add_enabled(!self.codex.busy, egui::Button::new("选择项目"))
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_title("选择 Codex 项目文件夹")
                    .pick_folder()
            {
                self.codex.project = path;
                self.codex.new_chat();
                self.save_codex_settings();
            }
            if ui
                .add_enabled(!self.codex.busy, egui::Button::new("新对话"))
                .clicked()
            {
                self.codex.new_chat();
            }
            if ui.button("返回原版应用界面").clicked() {
                self.desktop.basic_chat = false;
                self.desktop.attempted = false;
            }
        });
        ui.label(RichText::new(&self.codex.status).color(muted()));
        ui.add(
            egui::Label::new(if self.codex.project.as_os_str().is_empty() {
                "尚未选择项目".into()
            } else {
                self.codex.project.to_string_lossy().into_owned()
            })
            .truncate(),
        )
        .on_hover_text(self.codex.project.to_string_lossy());
        if ui.button("连接设置").clicked() {
            self.codex.settings_open = true;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("模型");
            let old = self.codex.model.clone();
            egui::ComboBox::from_id_salt("codex-model")
                .selected_text(if self.codex.model.is_empty() {
                    "本机默认"
                } else {
                    &self.codex.model
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.codex.model, String::new(), "本机默认");
                    for model in &self.codex.models {
                        ui.selectable_value(&mut self.codex.model, model.id.clone(), &model.name);
                    }
                });
            if old != self.codex.model {
                self.save_codex_settings();
            }
            egui::ComboBox::from_id_salt("codex-history")
                .selected_text("历史对话")
                .show_ui(ui, |ui| {
                    let mut selected = None;
                    for chat in self.codex.history.iter().filter(|chat| {
                        matches_query(
                            &self.search_query.to_lowercase(),
                            &[
                                &chat.title,
                                &chat
                                    .messages
                                    .iter()
                                    .map(|m| m.text.as_str())
                                    .collect::<Vec<_>>()
                                    .join(" "),
                            ],
                        )
                    }) {
                        if ui
                            .add_enabled(!self.codex.busy, egui::Button::new(&chat.title))
                            .clicked()
                        {
                            selected = Some(chat.clone());
                            ui.close();
                        }
                    }
                    if let Some(chat) = selected {
                        self.codex.select_chat(chat);
                        self.save_codex_settings();
                    }
                });
        });
        if let Some(url) = self.codex.login_url.clone() {
            ui.hyperlink_to("打开浏览器完成 Codex 登录", url);
        }
        if !self.codex.error.is_empty() {
            ui.colored_label(Color32::from_rgb(184, 65, 71), &self.codex.error);
        }
        ui.add_space(10.0);
        egui::Frame::new()
            .fill(surface())
            .stroke(egui::Stroke::new(1.0, border()))
            .corner_radius(14)
            .inner_margin(16)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("codex-messages")
                    .max_height((ui.available_height() - 145.0).max(70.0))
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        if self.codex.current.messages.is_empty() {
                            ui.add_space(20.0);
                            ui.heading("今天想完成什么？");
                            ui.label("选择项目并连接后，直接输入你的需求。");
                            ui.add_space(20.0);
                        }
                        for (index, message) in self
                            .codex
                            .current
                            .messages
                            .iter()
                            .enumerate()
                            .filter(|(_, message)| {
                                matches_query(&self.search_query.to_lowercase(), &[&message.text])
                            })
                        {
                            if message.role == "activity" {
                                ui.collapsing(format!("执行记录 {}", index + 1), |ui| {
                                    ui.add(egui::Label::new(&message.text).wrap().selectable(true));
                                });
                                continue;
                            }
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(if message.role == "user" {
                                        "你"
                                    } else {
                                        "Codex"
                                    })
                                    .strong()
                                    .color(accent()),
                                );
                                if ui.small_button("复制").clicked() {
                                    ui.ctx().copy_text(message.text.clone());
                                }
                            });
                            ui.add(egui::Label::new(&message.text).wrap().selectable(true));
                            ui.add_space(14.0);
                        }
                        if self.codex.busy {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("正在处理…");
                            });
                        }
                    });
            });
        ui.add_space(10.0);
        let input = ui.add(
            egui::TextEdit::multiline(&mut self.codex.prompt)
                .desired_width(f32::INFINITY)
                .desired_rows(3)
                .hint_text("输入需求，Ctrl+Enter 发送"),
        );
        let shortcut =
            input.has_focus() && ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.codex.ready && !self.codex.busy && !self.codex.prompt.trim().is_empty(),
                    primary_button("发送消息"),
                )
                .clicked()
                || shortcut
            {
                self.save_codex_settings();
                self.codex.submit();
            }
            if ui
                .add_enabled(
                    self.codex.busy && self.codex.turn_id.is_some(),
                    egui::Button::new("停止任务"),
                )
                .clicked()
            {
                self.codex.stop();
            }
        });
        if self.codex.busy || !self.codex.ready {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    }
    pub(super) fn render_codex_requests(&mut self, ctx: &egui::Context) {
        if self.codex.settings_open {
            let mut save = false;
            let mut executable = self.codex.executable.to_string_lossy().into_owned();
            let dialog = centered_dialog(
                ctx,
                "codex-settings",
                "Codex 连接设置",
                "选择支持 app-server 的本机 Codex 程序。",
                560.0,
                |ui| {
                    ui.label("本机 Codex 程序");
                    ui.add_enabled(
                        !self.codex.busy,
                        egui::TextEdit::singleline(&mut executable).desired_width(f32::INFINITY),
                    );
                    if ui
                        .add_enabled(!self.codex.busy, egui::Button::new("选择 Codex 程序"))
                        .clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Codex 程序", &["exe"])
                            .pick_file()
                    {
                        executable = path.to_string_lossy().into_owned();
                    }
                    ui.label("连接使用本机已有登录和模型配置。");
                    save = ui
                        .add_enabled(!self.codex.busy, primary_button("保存连接设置"))
                        .clicked();
                },
            );
            self.codex.executable = PathBuf::from(executable);
            if save {
                self.save_codex_settings();
                self.codex.settings_open = false;
            }
            if dialog_closed(&dialog) {
                self.codex.settings_open = false;
            }
            return;
        }
        let Some(mut request) = self.codex.requests.front().cloned() else {
            return;
        };
        let mut result = None;
        let mut reject = false;
        let question = request.method == "item/tool/requestUserInput";
        let dialog = centered_dialog(
            ctx,
            "codex-request",
            if question {
                "Codex 需要你的回答"
            } else {
                "Codex 操作确认"
            },
            "这是本机 Codex 发出的请求。",
            620.0,
            |ui| {
                if question {
                    for item in request.params["questions"].as_array().into_iter().flatten() {
                        let id = item["id"].as_str().unwrap_or_default().to_owned();
                        ui.label(item["question"].as_str().unwrap_or_default());
                        let answer = request.answers.entry(id).or_default();
                        for option in item["options"].as_array().into_iter().flatten() {
                            let label = option["label"].as_str().unwrap_or_default();
                            ui.radio_value(answer, label.to_owned(), label)
                                .on_hover_text(option["description"].as_str().unwrap_or_default());
                        }
                        ui.text_edit_singleline(answer);
                    }
                    if ui.add(primary_button("提交回答")).clicked() {
                        let answers = request
                            .answers
                            .iter()
                            .map(|(id, answer)| (id.clone(), json!({"answers":[answer]})))
                            .collect::<serde_json::Map<_, _>>();
                        result = Some(json!({"answers":answers}));
                    }
                } else {
                    let details = serde_json::to_string_pretty(&request.params).unwrap_or_default();
                    ui.add(egui::Label::new(details).wrap().selectable(true));
                    ui.horizontal(|ui| {
                        if ui.add(primary_button("允许本次操作")).clicked() {
                            result = Some(json!({"decision":"accept"}));
                        }
                        if ui.button("拒绝").clicked() {
                            result = Some(json!({"decision":"decline"}));
                        }
                    });
                }
                reject = ui.button("取消任务").clicked();
            },
        );
        if let Some(current) = self.codex.requests.front_mut() {
            current.answers = request.answers;
        }
        if let Some(result) = result {
            self.codex.respond(result);
        } else if reject || dialog_closed(&dialog) {
            self.codex.reject_request();
            self.codex.stop();
        }
    }
}
