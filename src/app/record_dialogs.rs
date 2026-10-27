use super::*;

impl PersonalDeskApp {
    pub(super) fn render_detail_windows(&mut self, ctx: &egui::Context) {
        if self.website_dialog_open {
            let mut save = false;
            let mut browse = false;
            let dialog = centered_dialog(
                ctx,
                "website-details",
                if self.website_editing {
                    "网站详情"
                } else {
                    "添加网站"
                },
                "保存常用网址，需要时一键打开。",
                520.0,
                |ui| {
                    ui.label("名称");
                    ui.text_edit_singleline(&mut self.website_draft.title);
                    ui.label("网址");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.website_draft.url)
                            .hint_text("https://example.com"),
                    );
                    ui.label("分类");
                    let options = self.names_for_scope(TagScope::Websites);
                    crate::ui_components::classification_dropdown(
                        ui,
                        "website-dialog-category",
                        &mut self.website_draft_tags,
                        &options,
                        ui.available_width(),
                    );
                    if ui.button("管理分类").clicked() {
                        self.open_tag_manager(TagScope::Websites);
                    }
                    ui.label("备注");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.website_draft.notes)
                            .desired_width(f32::INFINITY)
                            .desired_rows(3),
                    );
                    if !self.website_error.is_empty() {
                        ui.colored_label(Color32::from_rgb(184, 65, 71), &self.website_error);
                    }
                    ui.horizontal_wrapped(|ui| {
                        save = ui.add(primary_button("保存网站")).clicked();
                        browse = ui.button("打开网站").clicked();
                        if ui.button("复制网址").clicked() {
                            ctx.copy_text(self.website_draft.url.clone());
                        }
                    });
                },
            );
            if dialog_closed(&dialog) || (save && self.save_website_draft()) {
                self.website_dialog_open = false;
            }
            if browse {
                match websites::prepare(self.website_draft.clone()) {
                    Ok(_) => {
                        self.website_open_request = Some(self.website_draft.clone());
                        self.website_dialog_open = false;
                    }
                    Err(error) => self.website_error = error.to_string(),
                }
            }
        }
        if self.export_dialog_open {
            let scope = export::Scope::for_section(self.section);
            let mut selection = None;
            let dialog = centered_dialog(
                ctx,
                "export-details",
                "导出数据",
                "导出为 ZIP 压缩包，可在其他位置保存和查看。",
                480.0,
                |ui| {
                    if scope != export::Scope::All
                        && ui
                            .add_sized(
                                [ui.available_width(), 40.0],
                                egui::Button::new(format!("导出{}", scope.label())),
                            )
                            .clicked()
                    {
                        selection = Some(scope);
                    }
                    if ui
                        .add_sized([ui.available_width(), 40.0], primary_button("导出全部数据"))
                        .clicked()
                    {
                        selection = Some(export::Scope::All);
                    }
                },
            );
            if dialog_closed(&dialog) {
                self.export_dialog_open = false;
            }
            if let Some(scope) = selection
                && self.start_export(scope)
            {
                self.export_dialog_open = false;
            }
        }
        if self.credential_dialog_open {
            let mut open = true;
            let mut save = false;
            let dialog = centered_dialog(
                ctx,
                "credential-details",
                if self.selected_credential.is_some() {
                    "账号详情"
                } else {
                    "添加账号"
                },
                "直接查看、编辑和复制账号信息。",
                520.0,
                |ui| {
                    ui.style_mut()
                        .text_styles
                        .insert(egui::TextStyle::Body, egui::FontId::proportional(16.0));
                    ui.style_mut()
                        .text_styles
                        .insert(egui::TextStyle::Button, egui::FontId::proportional(16.0));
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label("名称");
                    ui.text_edit_singleline(&mut self.credential_draft.title);
                    ui.horizontal(|ui| {
                        ui.label("用户名");
                        if ui.small_button("复制账号").clicked() {
                            ctx.copy_text(self.credential_draft.username.clone());
                            self.notice = "账号已复制到剪贴板".into();
                        }
                    });
                    ui.text_edit_singleline(&mut self.credential_draft.username);
                    ui.label("网址或应用");
                    ui.text_edit_singleline(&mut self.credential_draft.website);
                    ui.label("密码");
                    ui.text_edit_singleline(&mut self.credential_draft.password);
                    if ui.button("复制密码").clicked() {
                        ctx.copy_text(self.credential_draft.password.clone());
                        self.notice = "密码已复制到剪贴板".into();
                    }
                    if tag_choices(ui, &mut self.credential_draft.tags, &self.account_tags) {
                        self.open_tag_manager(TagScope::Credentials);
                    }
                    ui.label("备注");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.credential_draft.notes)
                            .desired_width(f32::INFINITY)
                            .desired_rows(2),
                    );
                    if ui
                        .add_sized([ui.available_width(), 38.0], primary_button("保存账号"))
                        .clicked()
                    {
                        save = true;
                    }
                },
            );
            if dialog_closed(&dialog) {
                open = false;
            }
            if save && self.save_credential_draft() {
                open = false;
            }
            self.credential_dialog_open = open;
            if !open {
                self.credential_draft = Credential::new();
                self.selected_credential = None;
            }
        }
        if self.inventory_dialog_open {
            let mut open = true;
            let mut save = false;
            let dialog = centered_dialog(
                ctx,
                "inventory-details",
                if self.selected_inventory.is_some() {
                    "物品详情"
                } else {
                    "添加物品"
                },
                "记下拥有的物品，也记住它们放在哪里。",
                440.0,
                |ui| {
                    if ui.add(primary_button("保存物品")).clicked() {
                        save = true;
                    }
                    ui.label("物品名称");
                    ui.text_edit_singleline(&mut self.inventory_draft.name);
                    ui.columns(2, |columns| {
                        columns[0].label("分类");
                        columns[0].label(if self.inventory_draft.tags.is_empty() {
                            "未分类".into()
                        } else {
                            self.inventory_draft.tags.join(" · ")
                        });
                        columns[1].label("数量");
                        let quantity_width = columns[1].available_width();
                        columns[1].add_sized(
                            [quantity_width, 36.0],
                            egui::DragValue::new(&mut self.inventory_draft.quantity)
                                .range(0..=100_000),
                        );
                    });
                    if tag_choices(ui, &mut self.inventory_draft.tags, &self.inventory_tags) {
                        self.open_tag_manager(TagScope::Inventory);
                    }
                    ui.label("存放位置");
                    ui.text_edit_singleline(&mut self.inventory_draft.location);
                    ui.columns(2, |columns| {
                        columns[0].label("品牌");
                        columns[0].text_edit_singleline(&mut self.inventory_draft.brand);
                        columns[1].label("型号");
                        columns[1].text_edit_singleline(&mut self.inventory_draft.model);
                    });
                    ui.columns(2, |columns| {
                        columns[0].label("购买日期");
                        columns[0].add(
                            egui::TextEdit::singleline(&mut self.inventory_draft.purchase_date)
                                .hint_text("YYYY-MM-DD"),
                        );
                        let mut filled = self.inventory_draft.purchase_price_cents.is_some();
                        if columns[1].checkbox(&mut filled, "购买价格").changed() {
                            self.inventory_draft.purchase_price_cents = filled.then_some(0);
                        }
                        if let Some(cents) = self.inventory_draft.purchase_price_cents.as_mut() {
                            let mut price = *cents as f64 / 100.0;
                            if columns[1]
                                .add(
                                    egui::DragValue::new(&mut price)
                                        .prefix("¥ ")
                                        .fixed_decimals(2)
                                        .range(0.0..=1_000_000_000_000.0)
                                        .speed(1.0),
                                )
                                .changed()
                            {
                                *cents = (price * 100.0).round() as i64;
                            }
                        }
                    });
                    ui.label("丢弃日期");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.inventory_draft.disposal_date)
                            .hint_text("YYYY-MM-DD"),
                    );
                    ui.label("备注");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.inventory_draft.notes)
                            .desired_width(f32::INFINITY)
                            .desired_rows(3),
                    );
                },
            );
            if dialog_closed(&dialog) {
                open = false;
            }
            if save && self.save_inventory_item() {
                open = false;
            }
            self.inventory_dialog_open = open;
        }
        if self.shortcut_dialog_open {
            let mut open = true;
            let mut path = None;
            let dialog = centered_dialog(
                ctx,
                "shortcut-details",
                "添加快捷路径",
                "把常用文件夹和文件放在伸手可及的地方。",
                440.0,
                |ui| {
                    ui.horizontal(|ui| {
                        if ui.add(primary_button("选择文件夹")).clicked() {
                            path = files::choose_folder();
                        }
                        if ui.button("选择文件").clicked() {
                            path = files::choose_file();
                        }
                    });
                },
            );
            if dialog_closed(&dialog) {
                open = false;
            }
            if let Some(path) = path {
                self.add_shortcut(path);
                open = false;
            }
            self.shortcut_dialog_open = open;
        }
        if let Some(title) = self.settings_detail.clone() {
            let mut open = true;
            let dialog = centered_dialog(
                ctx,
                "settings-details",
                &title,
                "按自己的习惯使用个人办公台。",
                if title == "版本历史" {
                    620.0
                } else {
                    460.0
                },
                |ui| {
                    if title == "版本历史" {
                        self.render_version_history(ui);
                    } else if title == "本地数据" {
                        ui.label(format!("数据目录：{}", self.data_dir.display()));
                        if ui.button("打开数据目录").clicked()
                            && let Err(error) = files::open_in_system(&self.data_dir)
                        {
                            self.notice = error.to_string();
                        }
                    }
                },
            );
            if dialog_closed(&dialog) {
                open = false;
            }
            if !open {
                self.settings_detail = None;
            }
        }
        self.render_tag_dialogs(ctx);
        self.render_contact_dialogs(ctx);
        self.render_tool_dialog(ctx);
        self.render_excel_merge_dialog(ctx);
        self.render_pdf_images_dialog(ctx);
        self.render_campus_dialog(ctx);
        self.render_jobhunt_dialogs(ctx);
        self.render_hiring_dialog(ctx);
        self.render_theme_dialog(ctx);
        self.render_update_dialog(ctx);
        self.render_book_dialog(ctx);
        self.render_library_deletion_dialog(ctx);
    }
}
