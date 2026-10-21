use super::*;
pub(super) struct Capture {
    output: PathBuf,
    step: usize,
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
        self.preview_capture = Some(Capture {
            output,
            step: 0,
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
                    _ => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                }
            }
            return;
        }
        c.frames += 1;
        if c.frames < 6 {
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
        ][c.step];
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
