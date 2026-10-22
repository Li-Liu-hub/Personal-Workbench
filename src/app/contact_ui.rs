use super::*;

impl PersonalDeskApp {
    pub(super) fn open_contact_details(&mut self, id: &str) {
        match self
            .db
            .as_ref()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.contact(id))
        {
            Ok(Some(contact)) => {
                self.contact_draft = contact;
                self.contact_error.clear();
                self.contact_chat_open = None;
                self.contact_dialog_open = true;
            }
            Ok(None) => self.notice = "联系人已不存在".into(),
            Err(error) => self.notice = format!("读取联系人失败：{error}"),
        }
    }
    fn contact_texture(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        photo: &[u8],
    ) -> Option<egui::TextureHandle> {
        if photo.is_empty() {
            return None;
        }
        if let Some(texture) = self.photo_cache.get(key) {
            return Some(texture.clone());
        }
        let image = image::load_from_memory(photo)
            .ok()?
            .thumbnail(256, 256)
            .to_rgba8();
        let texture = ctx.load_texture(
            format!("contact-{key}"),
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        self.photo_cache.insert(key.into(), texture.clone());
        Some(texture)
    }

    pub(super) fn load_contacts(&mut self) {
        self.photo_cache.clear();
        if let Some(db) = self.db.as_ref() {
            match db.contact_overviews() {
                Ok(items) => self.contact_items = items,
                Err(error) => self.notice = format!("读取联系人失败：{error}"),
            }
        }
    }

    pub(super) fn save_contact_draft(&mut self) -> bool {
        if self.contact_import_busy {
            self.contact_error = "聊天记录正在导入，请完成后保存".into();
            return false;
        }
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.save_contact_with_tags(&self.contact_draft, &self.contact_draft_tags))
        {
            Ok(()) => {
                self.load_contacts();
                self.load_tags();
                self.contact_error.clear();
                self.notice = "联系人和聊天记录已保存".into();
                true
            }
            Err(error) => {
                self.contact_error = error.to_string();
                false
            }
        }
    }

    pub(super) fn render_contacts(&mut self, ui: &mut egui::Ui) {
        let mut visible = self
            .contact_items
            .iter()
            .filter(|c| {
                c.matches(&self.search_query)
                    && self.record_matches_filter(TagScope::Contacts, &c.id)
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut selected = None;
        let mut delete = None;
        let mut add = false;
        visible.sort_by_key(|item| !self.is_favorite("contacts", &item.id));
        if self.view_mode() == ViewMode::List {
            self.render_contact_rows(ui, &visible);
            return;
        }
        collection(ui, "contacts-gallery", self.view_mode(), |ui, side| {
            for item in visible {
                let detail = [
                    item.work.as_str(),
                    item.school.as_str(),
                    item.phone.as_str(),
                ]
                .into_iter()
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
                let labels = self.record_labels(TagScope::Contacts, &item.id);
                let response = tagged_entry(
                    ui,
                    &item.id,
                    &item.name,
                    (&detail, &labels),
                    TileKind::Contact,
                    side,
                );
                self.favorite_control(ui, &response, "contacts", &item.id);
                self.record_classification_dropdown(ui, &response, TagScope::Contacts, &item.id);
                if response.rect.intersects(ui.clip_rect())
                    && !item.photo.is_empty()
                    && let Some(texture) = self.contact_texture(ui.ctx(), &item.id, &item.photo)
                {
                    let area = if self.view_mode() == ViewMode::Grid {
                        egui::Rect::from_center_size(
                            response.rect.min + egui::vec2(38.0, 72.0),
                            egui::vec2(40.0, 40.0),
                        )
                    } else {
                        egui::Rect::from_center_size(
                            response.rect.left_center() + egui::vec2(28.0, 0.0),
                            egui::vec2(30.0, 30.0),
                        )
                    };
                    ui.painter().image(
                        texture.id(),
                        area,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                if response.clicked() {
                    selected = Some(item.clone());
                }
                response.context_menu(|ui| {
                    self.record_tag_menu(ui, TagScope::Contacts, &item.id);
                    if ui.button("查看联系人").clicked() {
                        selected = Some(item.clone());
                        ui.close();
                    }
                    if ui.button("删除联系人").clicked() {
                        delete = Some(item.id.clone());
                        ui.close();
                    }
                });
            }
            add = entry(
                ui,
                "add",
                "点击添加联系人",
                "姓名、联系方式与聊天记录",
                TileKind::Add,
                side,
            )
            .clicked();
        });
        if let Some(contact) = selected {
            self.open_contact_details(&contact.id);
        }
        if let Some(id) = delete {
            match self
                .db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| db.delete_contact(&id))
            {
                Ok(()) => {
                    self.load_contacts();
                    self.notice = "联系人及其聊天记录已删除".into();
                }
                Err(error) => self.notice = format!("删除失败：{error}"),
            }
        }
        if add {
            self.add_current_item();
        }
    }

    pub(super) fn start_contact_import(&mut self, paths: Vec<PathBuf>) {
        if self.contact_import_busy {
            return;
        }
        self.contact_import_busy = true;
        self.contact_error.clear();
        self.notice = "正在读取聊天记录…".into();
        let id = self.contact_draft.id.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let mut bytes = 0;
            let result = paths
                .iter()
                .map(|path| {
                    let chat = contacts::import_chat(path).map_err(|e| {
                        format!(
                            "{}：{e}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        )
                    })?;
                    bytes += chat.original.len();
                    if bytes > contacts::MAX_CONTACT_BYTES {
                        return Err("本批原文件总量超过 32 MiB，请分批导入".to_string());
                    }
                    Ok(chat)
                })
                .collect::<std::result::Result<Vec<_>, _>>();
            let _ = sender.send(BackgroundMessage::ContactChats(id, result));
        });
    }

    pub(super) fn render_contact_dialogs(&mut self, ctx: &egui::Context) {
        if !self.contact_dialog_open {
            self.contact_scroll_id = None;
        }
        if self.contact_dialog_open {
            if self.contact_scroll_id.as_deref() != Some(&self.contact_draft.id) {
                crate::ui_components::reset_dialog_scroll(ctx, "contact-details");
                self.contact_draft_tags =
                    self.record_labels(TagScope::Contacts, &self.contact_draft.id);
                self.contact_scroll_id = Some(self.contact_draft.id.clone());
                self.photo_cache.remove("draft-photo");
            }
            let mut save = false;
            let mut import = false;
            let mut remove = None;
            let mut view = None;
            let dialog = centered_dialog(
                ctx,
                "contact-details",
                "联系人详情",
                "填写资料，导入或移除聊天记录后，点击保存联系人。",
                620.0,
                |ui| {
                    let photo = self.contact_draft.photo.clone();
                    let texture = self.contact_texture(ctx, "draft-photo", &photo);
                    ui.horizontal_wrapped(|ui| {
                        if let Some(texture) = texture {
                            ui.add(
                                egui::Image::new(&texture)
                                    .fit_to_exact_size(egui::vec2(64.0, 64.0))
                                    .corner_radius(12),
                            );
                        }
                        if ui.button("上传照片").clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .add_filter("照片", &["png", "jpg", "jpeg", "webp", "bmp"])
                                .pick_file()
                        {
                            match contacts::import_photo(&path) {
                                Ok(bytes) => {
                                    self.contact_draft.photo = bytes;
                                    self.photo_cache.remove("draft-photo");
                                }
                                Err(error) => self.contact_error = error.to_string(),
                            }
                        }
                        if !self.contact_draft.photo.is_empty() && ui.button("移除照片").clicked()
                        {
                            self.contact_draft.photo.clear();
                            self.photo_cache.remove("draft-photo");
                        }
                        save = ui
                            .add_enabled(!self.contact_import_busy, primary_button("保存联系人"))
                            .clicked();
                        ui.label(
                            RichText::new(format!("{} 份聊天记录", self.contact_draft.chats.len()))
                                .color(muted()),
                        );
                    });
                    ui.columns(2, |columns| {
                        field(&mut columns[0], "姓名", &mut self.contact_draft.name);
                        field(&mut columns[1], "性别", &mut self.contact_draft.gender);
                        field(
                            &mut columns[0],
                            "生日 YYYY-MM-DD",
                            &mut self.contact_draft.birthday,
                        );
                        field(&mut columns[1], "专业", &mut self.contact_draft.major);
                        field(&mut columns[0], "学校", &mut self.contact_draft.school);
                        field(&mut columns[1], "工作", &mut self.contact_draft.work);
                        field(&mut columns[0], "手机号", &mut self.contact_draft.phone);
                        field(
                            &mut columns[1],
                            "身份证号",
                            &mut self.contact_draft.national_id,
                        );
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("分类");
                        let options = self.names_for_scope(TagScope::Contacts);
                        crate::ui_components::classification_dropdown(
                            ui,
                            "contact-draft-tags",
                            &mut self.contact_draft_tags,
                            &options,
                            220.0,
                        );
                        if ui.button("管理分类").clicked() {
                            self.open_tag_manager(TagScope::Contacts);
                        }
                    });
                    field(ui, "地址", &mut self.contact_draft.address);
                    field(ui, "爱好", &mut self.contact_draft.hobbies);
                    ui.label("聊天笔记");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.contact_draft.chat_notes)
                            .desired_width(f32::INFINITY)
                            .desired_rows(3)
                            .hint_text("也可以直接记录交流内容或备忘"),
                    );
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "聊天记录 · {} 份",
                                self.contact_draft.chats.len()
                            ))
                            .strong(),
                        );
                        import = ui
                            .add_enabled(
                                !self.contact_import_busy,
                                egui::Button::new(if self.contact_import_busy {
                                    "导入中…"
                                } else {
                                    "+ 导入聊天记录"
                                }),
                            )
                            .clicked();
                    });
                    for chat in &self.contact_draft.chats {
                        ui.push_id(&chat.id, |ui| {
                            egui::Frame::new()
                                .fill(canvas())
                                .corner_radius(10)
                                .inner_margin(10)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let width = (ui.available_width() - 130.0).max(60.0);
                                        ui.add_sized(
                                            [width, 28.0],
                                            egui::Label::new(&chat.title).truncate(),
                                        )
                                        .on_hover_text(&chat.title);
                                        if ui.button("查看").clicked() {
                                            view = Some(chat.id.clone());
                                        }
                                        if ui.button("移除").clicked() {
                                            remove = Some(chat.id.clone());
                                        }
                                    });
                                });
                        });
                    }
                    if self.contact_draft.chats.is_empty() {
                        ui.label("尚未导入聊天记录");
                    }
                    if !self.contact_error.is_empty() {
                        ui.colored_label(Color32::from_rgb(184, 65, 71), &self.contact_error);
                    }
                    save |= ui
                        .add_enabled(!self.contact_import_busy, primary_button("保存联系人"))
                        .clicked();
                },
            );
            if let Some(id) = remove {
                self.contact_draft.chats.retain(|chat| chat.id != id);
            }
            if let Some(id) = view {
                self.contact_chat_open = Some(id);
                self.contact_sheet = 0;
            }
            if import
                && let Some(paths) = rfd::FileDialog::new()
                    .set_title("导入聊天记录")
                    .add_filter(
                        "聊天记录",
                        &["txt", "md", "markdown", "json", "xlsx", "xls"],
                    )
                    .pick_files()
            {
                self.start_contact_import(paths);
            }
            if dialog_closed(&dialog) || (save && self.save_contact_draft()) {
                self.contact_dialog_open = false;
                self.contact_chat_open = None;
                if !self.contact_import_busy {
                    self.contact_draft = Contact::new();
                    self.contact_draft_tags.clear();
                }
            }
        }
        if let Some(id) = self.contact_chat_open.as_ref() {
            if let Some(chat) = self.contact_draft.chats.iter().find(|chat| &chat.id == id) {
                let mut export = false;
                let dialog = centered_dialog(
                    ctx,
                    "contact-chat",
                    "聊天记录",
                    "聊天记录保留原文件，可查看、复制文字或导出原件。",
                    780.0,
                    |ui| {
                        ui.add(egui::Label::new(RichText::new(&chat.title).strong()).truncate())
                            .on_hover_text(&chat.title);
                        ui.horizontal_wrapped(|ui| {
                            export = ui.button("导出原文件").clicked();
                            if ui.button("复制全部文字").clicked() {
                                ctx.copy_text(chat.text.clone());
                            }
                        });
                        if chat.sheets.is_empty() {
                            egui::ScrollArea::both()
                                .id_salt("chat-text")
                                .max_height(360.0)
                                .show(ui, |ui| {
                                    let mut text = chat.text.as_str();
                                    ui.add(
                                        egui::TextEdit::multiline(&mut text)
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(15),
                                    );
                                });
                        } else {
                            ui.horizontal_wrapped(|ui| {
                                for (index, sheet) in chat.sheets.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.contact_sheet,
                                        index,
                                        &sheet.name,
                                    );
                                }
                            });
                            if let Some(sheet) = chat.sheets.get(self.contact_sheet) {
                                ui.label(format!(
                                    "{} 行 · {} 列",
                                    sheet.rows.len(),
                                    sheet.rows.first().map_or(0, Vec::len)
                                ));
                                egui::ScrollArea::horizontal()
                                    .id_salt((id, "columns"))
                                    .show(ui, |ui| {
                                        egui::ScrollArea::vertical()
                                            .id_salt((id, self.contact_sheet, "rows"))
                                            .max_height(340.0)
                                            .show_rows(ui, 30.0, sheet.rows.len(), |ui, rows| {
                                                egui::Grid::new((id, self.contact_sheet, "sheet"))
                                                    .striped(true)
                                                    .spacing([10.0, 6.0])
                                                    .show(ui, |ui| {
                                                        for row in rows {
                                                            ui.label((row + 1).to_string());
                                                            for cell in &sheet.rows[row] {
                                                                ui.add_sized(
                                                                    [160.0, 24.0],
                                                                    egui::Label::new(cell)
                                                                        .truncate(),
                                                                )
                                                                .on_hover_text(cell);
                                                            }
                                                            ui.end_row();
                                                        }
                                                    });
                                            });
                                    });
                            }
                        }
                    },
                );
                if export
                    && let Some(path) = rfd::FileDialog::new()
                        .set_title("导出聊天记录原文件")
                        .set_file_name(&chat.title)
                        .add_filter("原始格式", &[chat.format.as_str()])
                        .save_file()
                {
                    self.notice = match contacts::export_chat(chat, &path) {
                        Ok(()) => format!("聊天记录已导出：{}", path.display()),
                        Err(error) => format!("导出失败：{error}"),
                    };
                }
                if dialog_closed(&dialog) {
                    self.contact_chat_open = None;
                }
            } else {
                self.contact_chat_open = None;
            }
        }
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
}

impl PersonalDeskApp {
    pub(super) fn save_inline_contact(&mut self, id: &str) -> bool {
        let Some(draft) = self.inline_contacts.get(id).cloned() else {
            return true;
        };
        let tags = self
            .inline_contact_tags
            .get(id)
            .cloned()
            .unwrap_or_else(|| self.record_labels(TagScope::Contacts, id));
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.update_contact_fields(&draft, &tags))
        {
            Ok(()) => {
                self.inline_contacts.remove(id);
                self.inline_contact_tags.remove(id);
                self.load_contacts();
                self.load_tags();
                self.notice = "已保存".into();
                true
            }
            Err(error) => {
                self.notice = error.to_string();
                false
            }
        }
    }
    fn render_contact_rows(&mut self, ui: &mut egui::Ui, items: &[Contact]) {
        use crate::ui_components::table;
        const C: &[f32] = &[
            130., 60., 115., 140., 90., 90., 125., 180., 180., 140., 110., 140.,
        ];
        if ui
            .checkbox(&mut self.contact_extra_columns, "显示证件号、地址和爱好")
            .changed()
            && let Some(db) = self.db.as_mut()
            && let Err(error) = db.set_setting(
                "view.contacts.extra",
                if self.contact_extra_columns {
                    "true"
                } else {
                    "false"
                },
            )
        {
            self.notice = error.to_string();
        }
        let indices: Vec<usize> = if self.contact_extra_columns {
            (0..12).collect()
        } else {
            vec![0, 1, 2, 3, 4, 5, 6, 10, 11]
        };
        let widths = table::fit(
            &indices.iter().map(|i| C[*i]).collect::<Vec<_>>(),
            ui.available_width(),
        );
        let labels = [
            "姓名",
            "性别",
            "生日",
            "学校",
            "专业",
            "工作",
            "手机号",
            "身份证号",
            "地址",
            "爱好",
            "分类",
            "操作",
        ];
        let options = self.names_for_scope(TagScope::Contacts);
        ui.spacing_mut().scroll.floating = false;
        egui::ScrollArea::horizontal()
            .id_salt("contact-columns")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                table::header(
                    ui,
                    &widths,
                    &indices.iter().map(|i| labels[*i]).collect::<Vec<_>>(),
                );
                for item in items {
                    let mut draft = self.inline_contacts.get(&item.id).unwrap_or(item).clone();
                    let mut tags = self
                        .inline_contact_tags
                        .get(&item.id)
                        .cloned()
                        .unwrap_or_else(|| self.record_labels(TagScope::Contacts, &item.id));
                    let mut changed = false;
                    let (response, visible_cells) =
                        table::row(ui, ("contact-row", &item.id), &widths);
                    let mut cells = [egui::Rect::NOTHING; 12];
                    for (index, cell) in indices.iter().zip(visible_cells) {
                        cells[*index] = cell;
                    }
                    for (index, value) in [
                        (0, &mut draft.name),
                        (1, &mut draft.gender),
                        (2, &mut draft.birthday),
                        (3, &mut draft.school),
                        (4, &mut draft.major),
                        (5, &mut draft.work),
                        (6, &mut draft.phone),
                        (7, &mut draft.national_id),
                        (8, &mut draft.address),
                        (9, &mut draft.hobbies),
                    ] {
                        if !cells[index].is_positive() {
                            continue;
                        }
                        changed |= table::text(
                            ui,
                            cells[index],
                            ("contact-field", &item.id, index),
                            value,
                        );
                        if index == 2 {
                            table::date_hint(ui, cells[index], value);
                        }
                    }
                    table::cell_ui(ui, cells[10], |ui| {
                        changed |= crate::ui_components::classification_dropdown(
                            ui,
                            (&item.id, "inline-category"),
                            &mut tags,
                            &options,
                            cells[10].width(),
                        );
                    });
                    let (save, cancel, detail) = table::actions(
                        ui,
                        &item.id,
                        cells[11],
                        changed || self.inline_contacts.contains_key(&item.id),
                    );
                    self.favorite_control(ui, &response, "contacts", &item.id);
                    response.context_menu(|ui| {
                        if ui.button("删除联系人").clicked() {
                            if let Some(db) = self.db.as_mut() {
                                match db.delete_contact(&item.id) {
                                    Ok(()) => {
                                        self.inline_contacts.remove(&item.id);
                                        self.inline_contact_tags.remove(&item.id);
                                        self.load_contacts();
                                    }
                                    Err(e) => self.notice = e.to_string(),
                                }
                            }
                            ui.close();
                        }
                    });
                    if changed {
                        self.inline_contacts.insert(item.id.clone(), draft.clone());
                        self.inline_contact_tags.insert(item.id.clone(), tags);
                    }
                    if cancel {
                        self.inline_contacts.remove(&item.id);
                        self.inline_contact_tags.remove(&item.id);
                    } else if save {
                        self.save_inline_contact(&item.id);
                    }
                    if detail && self.save_inline_contact(&item.id) {
                        self.open_contact_details(&item.id);
                    }
                }
                if table::add_row(ui, &widths, "add-contact-row", "添加联系人").clicked() {
                    self.add_current_item();
                }
            });
    }
}
