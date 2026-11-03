//! Development-only visual QA: render the real widgets with disposable records.
//! This path never opens the installed database or changes startup registration.
use super::*;

pub struct VisualPreview {
    app: PersonalDeskApp,
    _data: tempfile::TempDir,
    output: PathBuf,
    scene: usize,
    frames: usize,
    waiting: bool,
    sender: Sender<std::result::Result<(), String>>,
    receiver: Receiver<std::result::Result<(), String>>,
    page: String,
}

const SCENES: &[(&str, Section, ViewMode, [f32; 2])] = &[
    (
        "folders-grid",
        Section::Folders,
        ViewMode::Grid,
        [1280., 800.],
    ),
    (
        "folders-list",
        Section::Folders,
        ViewMode::List,
        [1000., 680.],
    ),
    (
        "accounts-grid",
        Section::Credentials,
        ViewMode::Grid,
        [1280., 800.],
    ),
    (
        "accounts-list",
        Section::Credentials,
        ViewMode::List,
        [1000., 680.],
    ),
    (
        "websites-grid",
        Section::Websites,
        ViewMode::Grid,
        [1280., 800.],
    ),
    (
        "websites-list",
        Section::Websites,
        ViewMode::List,
        [1000., 680.],
    ),
    ("tools-grid", Section::Tools, ViewMode::Grid, [1280., 800.]),
    ("tools-list", Section::Tools, ViewMode::List, [1000., 680.]),
    ("essays", Section::Essays, ViewMode::Grid, [1000., 680.]),
    ("books", Section::Books, ViewMode::Grid, [1280., 800.]),
    (
        "contact-dialog",
        Section::Contacts,
        ViewMode::List,
        [1000., 680.],
    ),
    (
        "export-dialog",
        Section::Settings,
        ViewMode::Grid,
        [1000., 680.],
    ),
    (
        "update-dialog",
        Section::Settings,
        ViewMode::Grid,
        [1000., 680.],
    ),
    (
        "contacts-focused",
        Section::Contacts,
        ViewMode::List,
        [1000., 680.],
    ),
    (
        "contacts-grid",
        Section::Contacts,
        ViewMode::Grid,
        [1280., 800.],
    ),
    (
        "contacts-list",
        Section::Contacts,
        ViewMode::List,
        [1000., 680.],
    ),
    (
        "documents",
        Section::Documents,
        ViewMode::Grid,
        [1280., 800.],
    ),
    ("writer", Section::Documents, ViewMode::Grid, [1280., 800.]),
    ("settings", Section::Settings, ViewMode::Grid, [1000., 680.]),
    (
        "theme-dialog",
        Section::Settings,
        ViewMode::Grid,
        [1280., 800.],
    ),
    (
        "inventory",
        Section::Inventory,
        ViewMode::List,
        [1000., 680.],
    ),
];

impl VisualPreview {
    pub fn new(cc: &eframe::CreationContext<'_>, output: PathBuf) -> Self {
        configure_theme(&cc.egui_ctx);
        let data = tempfile::tempdir().expect("temporary visual test directory");
        let mut app = PersonalDeskApp::open_data_directory(data.path().join("data"));
        for (index, name) in ["林晓", "陈晨", "许安", "周宁", "沈悦", "陆遥"]
            .iter()
            .enumerate()
        {
            let mut contact = Contact::new();
            contact.name = (*name).into();
            contact.work = ["产品设计", "软件开发", "内容创作"][index % 3].into();
            contact.school = "示例学校".into();
            app.db
                .as_mut()
                .unwrap()
                .save_contact_with_tags(&contact, &["工作伙伴".into()])
                .unwrap();
        }
        let mut page_id = String::new();
        let mut book = crate::library::Book::new();
        book.title = "工作与生活".into();
        app.db.as_mut().unwrap().save_book(&book).unwrap();
        for title in [
            "本周工作计划",
            "项目复盘与思考",
            "阅读笔记",
            "会议纪要",
            "灵感收集",
            "旅行清单",
        ] {
            let mut page = app
                .db
                .as_mut()
                .unwrap()
                .create_writing(title, None, None)
                .unwrap();
            page.body = "## 今天的重点\n\n把重要的事写下来，留出专注的时间。\n\n- 整理本周工作进展\n- 记录新的想法与待办\n- 回顾完成的事情\n\n## 随手记\n\n简洁的界面，让内容成为主角。".into();
            app.db.as_mut().unwrap().save_book_page(&mut page).unwrap();
            page_id = page.id;
        }
        app.db
            .as_mut()
            .unwrap()
            .group_writing(std::slice::from_ref(&page_id), Some(&book.id))
            .unwrap();
        for name in ["项目文件", "阅读资料", "个人文档"] {
            let folder = data.path().join(name);
            std::fs::create_dir_all(&folder).unwrap();
            app.db
                .as_mut()
                .unwrap()
                .save_shortcut(name, &folder, true)
                .unwrap();
        }
        for (name, brand, location) in [
            ("无线键盘", "示例品牌", "书桌"),
            ("随身笔记本", "纸品", "背包"),
            ("桌面台灯", "照明", "书房"),
        ] {
            let mut item = InventoryItem::new();
            item.name = name.into();
            item.brand = brand.into();
            item.location = location.into();
            app.db.as_mut().unwrap().save_inventory_item(&item).unwrap();
        }
        app.load_contacts();
        app.load_inventory();
        for title in ["学习资料", "项目协作", "个人服务"] {
            let mut account = Credential::new();
            account.title = title.into();
            account.username = "demo@example.test".into();
            account.password = "Example-Only-123".into();
            app.db.as_mut().unwrap().save_credential(&account).unwrap();
            let mut website = Website::new();
            website.title = title.into();
            website.url = "https://example.test".into();
            app.db
                .as_mut()
                .unwrap()
                .save_website_classified(&website, &[])
                .unwrap();
        }
        let sample_program = data.path().join("Example.exe");
        std::fs::write(&sample_program, b"visual fixture, never executed").unwrap();
        let mut program = crate::launcher::AppTool::new(sample_program);
        program.name = "示例工具".into();
        app.db.as_mut().unwrap().save_app_tool(&program).unwrap();
        app.load_summaries();
        app.load_websites();
        app.load_tags();
        std::fs::create_dir_all(&output).expect("visual output directory");
        let (sender, receiver) = mpsc::channel();
        Self {
            app,
            _data: data,
            output,
            scene: 0,
            frames: 0,
            waiting: false,
            sender,
            receiver,
            page: page_id,
        }
    }
}

impl eframe::App for VisualPreview {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Ok(result) = self.receiver.try_recv() {
            result.expect("save visual capture");
            self.scene += 1;
            self.frames = 0;
            self.waiting = false;
        }
        let Some(&(name, section, mode, size)) = SCENES.get(self.scene) else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        if self.frames == 0 {
            self.app.section = section;
            self.app.view_modes.set(section, mode);
            self.app.theme_editor_open = name == "theme-dialog";
            self.app.export_dialog_open = name == "export-dialog";
            self.app.updates.open = name == "update-dialog";
            self.app.contact_dialog_open = name == "contact-dialog";
            if self.app.contact_dialog_open {
                self.app.contact_draft = self.app.contact_items[0].clone();
            }
            self.app.writing.page = if name == "writer" {
                self.app
                    .db
                    .as_ref()
                    .unwrap()
                    .writing_page(&self.page)
                    .unwrap()
            } else {
                None
            };
            if name == "writer" {
                self.app.open_writing(&self.page);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size.into()));
        }
        if name == "contacts-focused" && self.frames == 4 {
            ctx.memory_mut(|memory| {
                memory.request_focus(egui::Id::unique((
                    "contact-field",
                    &self.app.contact_items[0].id,
                    1usize,
                )))
            });
        }
        self.app.render_workspace(ui);
        self.app.render_tool_dialog(&ctx);
        self.frames += 1;
        if self.frames >= 24 && !self.waiting {
            self.waiting = true;
            let path = self.output.join(format!("{name}.png"));
            let sender = self.sender.clone();
            ctx.request_screenshot(move |image| {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                let result = image::save_buffer(
                    &path,
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                )
                .map_err(|e| e.to_string());
                let _ = sender.send(result);
            });
        }
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}
