use super::*;

impl PersonalDeskApp {
    pub(super) fn page_title(&self) -> String {
        match self.section {
            Section::Reference => "指令大全".into(),
            Section::Folders => "快捷路径".into(),
            Section::Documents => "文档写作".into(),
            Section::Books => "图书".into(),
            Section::Credentials => "账号密码".into(),
            Section::Inventory => "物品清单".into(),
            Section::Contacts => "联系人".into(),
            Section::Websites => "网站收藏".into(),
            Section::Tools => "小工具".into(),
            Section::Games => "小游戏".into(),
            Section::Practice => "刷题王".into(),
            Section::Words => "每日单词".into(),
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
        if self.section == Section::Documents
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft))
            && self.flush_writing()
        {
            self.writing.page = None;
            self.library.book = None;
        }
    }

    pub(super) fn save_visible_rows(&mut self) -> bool {
        let mut saved = true;
        match self.section {
            Section::Documents => saved = self.flush_writing(),
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

    pub(super) fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 11, accent());
            for (x, y) in [(10.0, 10.0), (21.0, 10.0), (10.0, 21.0), (21.0, 21.0)] {
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(rect.min + egui::vec2(x, y), egui::vec2(7.0, 7.0)),
                    2,
                    crate::theme::button_text(),
                );
            }
            ui.label(
                RichText::new("个人办公台")
                    .size(18.0)
                    .strong()
                    .color(crate::theme::sidebar_text()),
            );
        });
        ui.add_space(16.0);
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
                        "账号密码",
                        TileKind::Credential,
                        Some(self.credentials.len()),
                    ),
                    (Section::Documents, "文档写作", TileKind::Document, None),
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
                    (Section::Practice, "刷题王", TileKind::Practice, None),
                    (Section::Words, "每日单词", TileKind::Word, None),
                    (Section::Reference, "指令大全", TileKind::Document, None),
                    (Section::Tools, "小工具", TileKind::Setting, None),
                    (Section::Games, "小游戏", TileKind::Game, Some(1)),
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
                            Section::Practice | Section::Words | Section::Reference => "学习",
                            Section::Tools | Section::Games | Section::Settings => "工具",
                            _ => "日常",
                        }
                    };
                    if group != last_group {
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
                    let response = navigation_item(ui, &label, kind, selected, count);
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
                        if !self.flush_writing() || !self.flush_study_answer() {
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
        if ui
            .button(format!("更新  v{}", crate::updater::VERSION))
            .clicked()
        {
            self.updates.open = true;
        }
        ui.spacing_mut().item_spacing.y = navigation_spacing;
    }

    pub(super) fn add_label(&self) -> &'static str {
        match self.section {
            Section::Reference => "添加指令",
            Section::Books => "新增图书",
            Section::Credentials => "添加账号",
            Section::Folders => "添加快捷路径",
            Section::Documents => "新建文档",
            Section::Inventory => "添加物品",
            Section::Websites => "添加网站",
            Section::Contacts => "添加联系人",
            Section::Tools => "添加工具",
            Section::Settings => "",
            Section::Games | Section::Practice | Section::Words => "",
        }
    }

    pub(super) fn add_current_item(&mut self) {
        match self.section {
            Section::Reference => self.add_reference(),
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
            Section::Games | Section::Practice | Section::Words => {}
            Section::Contacts => {
                self.contact_draft = Contact::new();
                self.contact_error.clear();
                self.contact_chat_open = None;
                self.contact_dialog_open = true;
            }
        }
    }
}
