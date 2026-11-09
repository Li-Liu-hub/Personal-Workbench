use super::*;

impl PersonalDeskApp {
    pub(super) fn page_title(&self) -> String {
        match self.section {
            Section::Essays => "每日随笔".into(),
            Section::Folders => "快捷路径".into(),
            Section::Documents => "文档写作".into(),
            Section::Books => "图书".into(),
            Section::Credentials => "账号密码管理".into(),
            Section::Inventory => "物品清单".into(),
            Section::Contacts => "联系人".into(),
            Section::Websites => "网站收藏".into(),
            Section::Tools => "小工具".into(),
            Section::Settings => "设置".into(),
        }
    }

    pub(super) fn handle_workspace_shortcuts(&mut self, ctx: &egui::Context) {
        if self.detail_open() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::K))
            && let Some(id) = self.search_field
        {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
            self.add_current_item();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.save_visible_rows();
        }
        if !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Z))
        {
            self.undo_index_removal();
        }
        if matches!(
            self.section,
            Section::Documents | Section::Books | Section::Essays
        ) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft))
            && self.flush_writing()
        {
            self.essay.active = None;
            self.writing.page = None;
            self.library.book = None;
        }
    }

    pub(super) fn save_visible_rows(&mut self) -> bool {
        let mut saved = true;
        match self.section {
            Section::Documents | Section::Books | Section::Essays => saved = self.flush_writing(),
            Section::Contacts => {
                for id in self.inline_contacts.keys().cloned().collect::<Vec<_>>() {
                    saved &= self.save_inline_contact(&id);
                }
            }
            Section::Inventory => {
                for id in self.inline_inventory.keys().cloned().collect::<Vec<_>>() {
                    saved &= self.save_inline_inventory(&id);
                }
            }
            Section::Credentials => {
                for id in self.inline_credentials.keys().cloned().collect::<Vec<_>>() {
                    saved &= self.save_inline_credential(&id);
                }
            }
            _ => {}
        }
        saved
    }

    pub(super) fn render_app_brand(&mut self, ui: &mut egui::Ui, compact: bool) {
        ui.horizontal(|ui| {
            let can_toggle = self.section == Section::Documents;
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::click_and_drag());
            crate::ui_components::window_drag_target(ui, &response);
            ui.painter().rect_filled(rect, 11, accent());
            for (x, y) in [(10.0, 10.0), (21.0, 10.0), (10.0, 21.0), (21.0, 21.0)] {
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(rect.min + egui::vec2(x, y), egui::vec2(7.0, 7.0)),
                    2,
                    crate::theme::button_text(),
                );
            }
            if can_toggle {
                let response = response.on_hover_text(if compact {
                    "展开导航"
                } else {
                    "收起导航"
                });
                if response.clicked() {
                    self.navigation_compact = !compact;
                }
            }
            if !compact {
                let response = ui.add_sized(
                    [ui.available_width(), 38.0],
                    egui::Label::new(RichText::new("个人办公台").size(17.0).strong().color(ink()))
                        .truncate()
                        .sense(egui::Sense::click_and_drag()),
                );
                crate::ui_components::window_drag_target(ui, &response);
            }
        });
    }

    pub(super) fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        let compact = self.navigation_compact && self.section == Section::Documents;
        ui.add_space(14.0);
        ui.add_space(4.0);
        let navigation_spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 4.0;
        egui::ScrollArea::vertical()
            .id_salt("navigation-scroll")
            .max_height((ui.available_height() - 56.0).max(160.0))
            .show(ui, |ui| {
                let mut navigation = vec![
                    (Section::Folders, "快捷路径", TileKind::Folder, None),
                    (
                        Section::Credentials,
                        "账号密码管理",
                        TileKind::Credential,
                        Some(self.credentials.len()),
                    ),
                    (Section::Documents, "文档写作", TileKind::Document, None),
                    (Section::Essays, "每日随笔", TileKind::Document, None),
                    (
                        Section::Contacts,
                        "联系人",
                        TileKind::Contact,
                        Some(self.contact_items.len()),
                    ),
                    (
                        Section::Inventory,
                        "物品清单",
                        TileKind::Item,
                        Some(self.inventory_items.len()),
                    ),
                    (
                        Section::Websites,
                        "网站收藏",
                        TileKind::Website,
                        Some(self.website_items.len()),
                    ),
                    (Section::Tools, "小工具", TileKind::Tool, None),
                    (Section::Settings, "设置", TileKind::Setting, None),
                ];
                navigation.sort_by_key(|(section, _, _, _)| {
                    !self.is_favorite("modules", views::key(*section))
                });
                let mut last_group = "";
                for (section, title, kind, count) in navigation {
                    let group = if self.is_favorite("modules", views::key(section)) {
                        "置顶"
                    } else {
                        match section {
                            Section::Tools | Section::Settings => "工具",
                            _ => "日常",
                        }
                    };
                    if !compact && group != last_group {
                        ui.add_space(if last_group.is_empty() { 0. } else { 10. });
                        ui.horizontal(|ui| {
                            ui.add_space(12.);
                            ui.label(RichText::new(group).size(11.).color(muted()));
                        });
                        ui.add_space(3.);
                        last_group = group;
                    }

                    let selected = section == self.section;
                    let label = if self.is_favorite("modules", views::key(section)) {
                        format!("★ {title}")
                    } else {
                        title.to_string()
                    };
                    let response = if compact {
                        let response =
                            crate::ui_components::navigation_icon_item(ui, kind, selected);
                        let hint = count
                            .filter(|&count| count > 0)
                            .map_or(label.clone(), |count| format!("{label} · {count}"));
                        response.on_hover_text(hint)
                    } else {
                        navigation_item(ui, &label, kind, selected, count)
                    };
                    response.context_menu(|ui| {
                        if ui
                            .button(if self.is_favorite("modules", views::key(section)) {
                                "取消收藏"
                            } else {
                                "收藏并置顶"
                            })
                            .clicked()
                        {
                            self.toggle_favorite("modules", views::key(section));
                            ui.close();
                        }
                    });
                    if response.clicked() {
                        if !self.flush_writing() {
                            continue;
                        }
                        self.section = section;
                        self.notice.clear();
                        self.search_query.clear();

                        if section == Section::Credentials {
                            self.load_summaries();
                        }
                        if section == Section::Inventory {
                            self.load_inventory();
                        }
                        if section == Section::Websites {
                            self.load_websites();
                        }
                        if section == Section::Contacts {
                            self.load_contacts();
                        }
                    }
                }
            });
        ui.add_space(12.0);
        let update_clicked = if compact {
            crate::ui_components::navigation_icon_item(ui, TileKind::Update, false)
                .on_hover_text(format!("更新  v{}", crate::updater::VERSION))
                .clicked()
        } else {
            ui.animated_button(format!("更新  v{}", crate::updater::VERSION))
                .clicked()
        };
        if update_clicked {
            self.updates.open = true;
        }
        ui.spacing_mut().item_spacing.y = navigation_spacing;
    }

    pub(super) fn add_label(&self) -> &'static str {
        match self.section {
            Section::Essays => "今日随笔",
            Section::Books => "新增图书",
            Section::Credentials => "添加账号",
            Section::Folders => "添加快捷路径",
            Section::Documents => "新建文档",
            Section::Inventory => "添加物品",
            Section::Websites => "添加网站",
            Section::Contacts => "添加联系人",
            Section::Tools => "添加程序",
            Section::Settings => "",
        }
    }

    pub(super) fn add_current_item(&mut self) {
        match self.section {
            Section::Essays => self.start_daily_essay(),
            Section::Books => self.library.dialog = Some(crate::library::Book::new()),
            Section::Credentials => {
                self.selected_credential = None;
                self.credential_draft = Credential::new();
                self.credential_dialog_open = true;
            }
            Section::Folders => self.shortcut_dialog_open = true,
            Section::Documents => {
                self.new_writing(false);
            }
            Section::Inventory => {
                self.selected_inventory = None;
                self.inventory_draft = InventoryItem::new();
                self.inventory_dialog_open = true;
            }
            Section::Websites => {
                self.website_draft = Website::new();
                self.website_draft_tags.clear();
                self.website_editing = false;
                self.website_error.clear();
                self.website_dialog_open = true;
            }
            Section::Tools => self.add_tool(),
            Section::Settings => {}
            Section::Contacts => {
                self.contact_draft = Contact::new();
                self.contact_error.clear();
                self.contact_chat_open = None;
                self.contact_dialog_open = true;
            }
        }
    }
}
