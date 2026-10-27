use super::*;
pub(super) struct Capture {
    output: PathBuf,
    pub(super) step: usize,
    frames: usize,
    waiting: bool,
    completed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl PersonalDeskApp {
    pub(super) fn prepare_preview_capture(&mut self) {
        let Some(output) = std::env::var_os("PERSONAL_DESK_PREVIEW_OUTPUT").map(PathBuf::from)
        else {
            return;
        };
        fs::create_dir_all(&output).unwrap();
        for (title, user) in [
            ("工作邮箱", "hello@example.test"),
            ("设计工具", "studio@example.test"),
            ("代码仓库", "developer@example.test"),
        ] {
            let mut item = Credential::new();
            item.title = title.into();
            item.username = user.into();
            item.password = "Sample-Only-2026".into();
            item.tags = vec!["工作".into()];
            self.db.as_mut().unwrap().save_credential(&item).unwrap();
        }
        let mut contact = Contact::new();
        contact.name = "示例联系人".into();
        contact.school = "示例大学".into();
        contact.work = "产品设计".into();
        contact.gender = "男".into();
        contact.major = "计算机科学".into();
        contact.birthday = "2005-05-20".into();
        contact.phone = "13800000000".into();
        self.db
            .as_mut()
            .unwrap()
            .save_contact_with_tags(&contact, &[])
            .unwrap();
        let mut item = InventoryItem::new();
        item.name = "笔记本电脑".into();
        item.brand = "示例品牌".into();
        item.model = "Pro 14".into();
        item.location = "书房".into();
        item.purchase_date = "2025-09-01".into();
        item.purchase_price_cents = Some(699900);
        item.tags = vec!["数码".into()];
        self.db
            .as_mut()
            .unwrap()
            .save_inventory_item(&item)
            .unwrap();
        self.load_summaries();
        self.load_contacts();
        self.load_inventory();
        self.load_tags();
        self.new_writing(false);
        self.writing.page.as_mut().unwrap().title = "让写作回到简单".into();
        self.writing.page.as_mut().unwrap().body="## 今天，写点什么\n\n把想法、工作和生活记录在这里。\n\n- 专注一件重要的事\n- 留下值得记住的瞬间\n\n> 少一点打扰，多一点专注。".into();
        self.mark_writing_changed(0.);
        self.flush_writing();
        self.section = Section::Documents;
        self.notice.clear();
        let pdf_only = std::env::var_os("PERSONAL_DESK_PREVIEW_PDF_ONLY").is_some();
        if pdf_only {
            self.section = Section::Tools;
            self.pdf_images.open = true;
        }
        let hiring_only = std::env::var_os("PERSONAL_DESK_PREVIEW_HIRING_ONLY").is_some();
        if hiring_only {
            self.section = Section::Tools;
            self.hiring.preview_tab(0);
        }
        self.preview_capture = Some(Capture {
            output,
            step: if hiring_only {
                59
            } else if pdf_only {
                57
            } else {
                0
            },
            frames: 0,
            waiting: false,
            completed: Default::default(),
        });
    }
    pub(super) fn capture_preview(&mut self, ctx: &egui::Context) {
        let Some(c) = self.preview_capture.as_mut() else {
            return;
        };
        ctx.request_repaint_after(Duration::from_millis(40));
        if c.waiting {
            if c.completed
                .swap(false, std::sync::atomic::Ordering::Acquire)
            {
                c.step += 1;
                c.frames = 0;
                c.waiting = false;
                match c.step {
                    1 => self.writing.preview = true,
                    2 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1000., 680.,
                        )));
                        self.writing.preview = false;
                    }
                    3 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                        self.writing.page = None;
                        self.view_modes.set(Section::Documents, ViewMode::List);
                    }
                    4 => self.section = Section::Practice,
                    5 => self.prepare_study_preview(false),
                    6 => self.section = Section::Words,
                    7 => self.prepare_study_preview(true),
                    8 => {
                        self.close_study_preview();
                        self.section = Section::Credentials;
                    }
                    9 => {
                        self.section = Section::Contacts;
                        self.view_modes.set(Section::Contacts, ViewMode::List);
                    }
                    10 => {
                        self.section = Section::Inventory;
                        self.view_modes.set(Section::Inventory, ViewMode::List);
                    }
                    11 => {
                        self.section = Section::Practice;
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1000., 680.,
                        )));
                        self.reset_study_preview();
                    }
                    12 => {
                        self.section = Section::Words;
                    }
                    13 => {
                        self.section = Section::Practice;
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                        self.prepare_sql_preview();
                    }
                    14 => {
                        self.section = Section::Words;
                        self.prepare_learning_preview("造句");
                    }
                    15 => {
                        self.section = Section::Words;
                        self.prepare_learning_preview("中译英");
                    }
                    16 => {
                        self.section = Section::Practice;
                        self.prepare_learning_preview("project");
                    }
                    17 => {
                        self.section = Section::Contacts;
                        self.view_modes.set(Section::Contacts, ViewMode::Grid);
                    }
                    18 => {
                        self.section = Section::Documents;
                        self.library.book = None;
                        let mut book = crate::library::Book::new();
                        book.title = "日记本".into();
                        let db = self.db.as_mut().unwrap();
                        db.save_book(&book).unwrap();
                        db.create_writing("昨天的记录", Some(&book.id), None)
                            .unwrap();
                        self.new_writing_for_day(
                            true,
                            chrono::NaiveDate::from_ymd_opt(2026, 10, 22).unwrap(),
                        );
                        self.writing.page.as_mut().unwrap().body =
                            "## 今天的记录\n\n日记放进图书后，标题和正文都保留在自己的标签页里。"
                                .into();
                        self.mark_writing_changed(0.);
                        let id = self.writing.page.as_ref().unwrap().id.clone();
                        self.group_written_documents(&[id], Some(&book.id));
                        self.notice.clear();
                    }
                    19 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1000., 680.,
                    ))),
                    20 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                        self.section = Section::Practice;
                        self.prepare_route_preview();
                        self.study.route_open = false;
                    }
                    21 => self.study.route_open = true,
                    22 => {
                        self.study.route_open = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1000., 680.,
                        )));
                    }
                    23 => {
                        self.section = Section::Credentials;
                        self.view_modes.set(Section::Credentials, ViewMode::List);
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                    }
                    24 => {
                        self.section = Section::Contacts;
                        self.view_modes.set(Section::Contacts, ViewMode::List);
                        let id =
                            egui::Id::unique(("contact-field", &self.contact_items[0].id, 1usize));
                        ctx.memory_mut(|m| m.request_focus(id));
                    }
                    25 => {
                        self.section = Section::Reference;
                        self.preview_reference(false);
                    }
                    26 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1000., 680.,
                    ))),
                    27 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                        self.preview_reference(true);
                    }
                    28 => {
                        self.section = Section::Essays;
                        self.essay.active = None;
                    }
                    29 => {
                        self.start_daily_essay();
                    }
                    30 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1000., 680.,
                    ))),
                    31 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                        self.section = Section::Reference;
                        self.reference.draft = None;
                        self.reference.filter = Default::default();
                        self.reference.loaded = None;
                        self.search_query = "原型对象".into();
                        let mut state = std::mem::take(&mut self.reference);
                        self.select_reference(&mut state, "ref:js:prototype-object");
                        self.reference = state;
                    }
                    32 => {
                        self.search_query = "LAST_VALUE".into();
                        let mut state = std::mem::take(&mut self.reference);
                        self.select_reference(&mut state, "ref:mysql:last-value");
                        self.reference = state;
                    }
                    33 => {
                        let entry = self
                            .db
                            .as_ref()
                            .unwrap()
                            .reference_entry("ref:mysql:last-value")
                            .unwrap()
                            .unwrap();
                        self.practice_knowledge(&entry);
                        self.study.tutor_open = false;
                    }
                    34 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1000., 680.,
                    ))),
                    35 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                        self.preview_game("", 0, false);
                    }
                    36 => self.preview_game("snake", 1, false),
                    37 => self.preview_game("snake", 6, true),
                    38 => self.preview_game("tetris", 6, true),
                    39 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1000., 680.,
                    ))),
                    40 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                        self.section = Section::Words;
                        self.prepare_learning_preview("语法");
                    }
                    41 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1000., 680.,
                    ))),
                    42 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                        self.section = Section::Practice;
                        self.prepare_learning_preview("coding");
                    }
                    43 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1000., 680.,
                    ))),
                    44 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                        self.section = Section::Words;
                        self.prepare_learning_preview("dictation");
                    }
                    45 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1000., 680.,
                    ))),
                    46 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                        self.section = Section::Reference;
                        self.reference.filter = Default::default();
                        self.reference.loaded = None;
                        self.search_query.clear();
                        let mut state = std::mem::take(&mut self.reference);
                        self.select_reference(&mut state, "ref:java:01");
                        self.reference = state;
                    }
                    48 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1000., 680.,
                    ))),
                    49 => {
                        self.excel_merge.open = true;
                    }
                    50 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1280., 800.,
                    ))),
                    51 => {
                        self.excel_merge.open = false;
                        self.section = Section::Resume;
                    }
                    52 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1000., 680.,
                    ))),
                    53 => {
                        self.section = Section::Settings;
                        self.view_modes.set(Section::Settings, ViewMode::Grid);
                    }
                    54 => {
                        self.view_modes.set(Section::Settings, ViewMode::List);
                    }
                    55 => {
                        self.settings_detail = Some("版本历史".into());
                    }
                    56 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1280., 800.,
                    ))),
                    57 => {
                        self.settings_detail = None;
                        self.section = Section::Tools;
                        self.pdf_images.open = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1000., 680.,
                        )));
                    }
                    58 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1280., 800.,
                    ))),
                    59 => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    60 => self.hiring.preview_tab(1),
                    61 => self.hiring.preview_tab(2),
                    62 => {
                        self.hiring.preview_tab(0);
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1000., 680.,
                        )));
                    }
                    63 => self.hiring.preview_tab(1),
                    64 => self.hiring.preview_tab(2),
                    65 => {
                        self.hiring.open = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                    }
                    66 => self.hiring.preview_tab(3),
                    67 => self.hiring.preview_tab(4),
                    68 => self.hiring.preview_tab(5),
                    69 => self.hiring.preview_tab(7),
                    70 => {
                        self.hiring.preview_tab(3);
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1000., 680.,
                        )));
                    }
                    71 => self.hiring.preview_tab(4),
                    72 => self.hiring.preview_tab(5),
                    73 => self.hiring.preview_tab(7),
                    74 => {
                        self.hiring.preview_tab(6);
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                    }
                    75 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1000., 680.,
                    ))),
                    _ => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                }
            }
            return;
        }
        if c.step == 57 && c.frames == 0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1000., 680.)));
        }
        c.frames += 1;
        if matches!(c.step, 51 | 52) && !self.resume.preview_ready() && self.resume.error.is_empty()
        {
            return;
        }
        if c.frames < 6 {
            return;
        }
        let campus_preview = std::env::var_os("PERSONAL_DESK_PREVIEW_CAMPUS").is_some()
            || std::env::var_os("PERSONAL_DESK_PREVIEW_JOBHUNT").is_some();
        if campus_preview && c.step >= 59 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let name = [
            "writing-editor",
            "writing-preview",
            "writing-small",
            "writing-library",
            "engineering-practice",
            "engineering-notebook",
            "daily-words",
            "practice-settings",
            "accounts-grid",
            "contacts-list",
            "inventory-list",
            "practice-small",
            "words-small",
            "sql-practice",
            "word-sentence",
            "word-translation",
            "project-repair",
            "contacts-grid",
            "diary-in-book",
            "diary-in-book-small",
            "training-recommendation",
            "training-route",
            "training-recommendation-small",
            "accounts-list",
            "contacts-focused",
            "reference",
            "reference-small",
            "reference-edit",
            "daily-essay",
            "daily-essay-editor",
            "daily-essay-small",
            "reference-prototypes",
            "reference-windows",
            "knowledge-practice",
            "knowledge-practice-small",
            "games-gallery",
            "snake-levels",
            "snake-game",
            "tetris-game",
            "tetris-game-small",
            "grammar-foundations",
            "grammar-foundations-small",
            "coding-editor",
            "coding-editor-small",
            "english-dictation",
            "english-dictation-small",
            "reference-coding",
            "tools-gallery",
            "tools-gallery-small",
            "excel-merge-small",
            "excel-merge",
            "resume-editor",
            "resume-editor-small",
            "settings-grid-small",
            "settings-list-small",
            "version-history-small",
            "version-history",
            "pdf-images-small",
            "pdf-images",
            "hiring-boss",
            "hiring-official",
            "hiring-otp",
            "hiring-boss-small",
            "hiring-official-small",
            "hiring-otp-small",
            "hiring-tools-gallery",
            "hiring-51job",
            "hiring-zhaopin",
            "hiring-shixiseng",
            "hiring-unified-queue",
            "hiring-51job-small",
            "hiring-zhaopin-small",
            "hiring-shixiseng-small",
            "hiring-unified-queue-small",
            "hiring-unified-run",
            "hiring-unified-run-small",
        ][c.step];
        let name = if campus_preview {
            if c.step == 57 {
                "campus-search-small"
            } else {
                "campus-search"
            }
        } else {
            name
        };
        let name = if std::env::var_os("PERSONAL_DESK_PREVIEW_JOBHUNT").is_some() {
            if c.step == 57 {
                "jobhunt-small"
            } else {
                "jobhunt"
            }
        } else {
            name
        };
        let path = c.output.join(format!("{name}.png"));
        let completed = c.completed.clone();
        c.waiting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
            egui::ScreenshotCallback::new(move |img| {
                let bytes = img
                    .pixels
                    .iter()
                    .flat_map(|c| c.to_array())
                    .collect::<Vec<_>>();
                image::save_buffer_with_format(
                    path,
                    &bytes,
                    img.width() as u32,
                    img.height() as u32,
                    image::ColorType::Rgba8,
                    image::ImageFormat::Png,
                )
                .unwrap();
                completed.store(true, std::sync::atomic::Ordering::Release);
            }),
        ));
    }
}
