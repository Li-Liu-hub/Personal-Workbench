//! Isolated, debug-only render captures. No desktop input or personal data.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
pub(super) struct Capture {
    output: PathBuf,
    completed: Arc<AtomicUsize>,
    step: usize,
    frames: usize,
    waiting: bool,
    browser_url: String,
}
impl PersonalDeskApp {
    pub(super) fn prepare_preview_capture(&mut self) {
        let Some(output) = std::env::var_os("PERSONAL_DESK_PREVIEW_OUTPUT").map(PathBuf::from)
        else {
            return;
        };
        fs::create_dir_all(&output).expect("preview output");
        let browser_url = start_browser_fixture();
        let mut contact = Contact::new();
        contact.name = "示例联系人".into();
        contact.school = "示例大学".into();
        contact.work = "软件开发".into();
        contact.birthday = "2000-01-01".into();
        contact.major = "计算机科学".into();
        let photo = output.join("synthetic-photo.png");
        image::DynamicImage::new_rgb8(64, 64).save(&photo).unwrap();
        contact.photo = contacts::import_photo(&photo).unwrap();
        let mut website = Website::new();
        website.title = "开发文档".into();
        website.url = "https://example.test/docs".into();
        let mut item = InventoryItem::new();
        item.name = "笔记本电脑".into();
        item.quantity = 1;
        item.location = "书桌".into();
        item.brand = "Apple".into();
        item.model = "MacBook Pro".into();
        item.tags = vec!["数码".into()];
        item.purchase_date = "2026-10-20".into();
        item.purchase_price_cents = Some(599900);
        let mut account = Credential::new();
        account.title = "开发账号".into();
        account.username = "demo@example.test".into();
        account.password = "Synthetic-Demo-123".into();
        account.tags = vec!["工作".into()];
        let db = self.db.as_mut().unwrap();
        db.save_credential(&account).unwrap();
        db.save_contact(&contact).unwrap();
        db.save_website(&website).unwrap();
        db.save_inventory_item(&item).unwrap();
        db.set_record_tags(TagScope::Contacts, &contact.id, &["朋友".into()])
            .unwrap();
        db.set_record_tags(TagScope::Websites, &website.id, &["工作".into()])
            .unwrap();
        if let Some(folder) = self.current_directory.clone() {
            for name in ["项目笔记.md", "需求清单.md"] {
                db.set_document_title(&folder.join(name), name.trim_end_matches(".md"))
                    .unwrap();
            }
        }
        self.load_contacts();
        self.load_websites();
        self.load_inventory();
        self.load_summaries();
        self.load_tags();
        self.preview_capture = Some(Capture {
            output,
            completed: Arc::new(AtomicUsize::new(0)),
            step: 0,
            frames: 0,
            waiting: false,
            browser_url,
        });
    }
    pub(super) fn capture_preview(&mut self, ctx: &egui::Context) {
        let Some(capture) = self.preview_capture.as_mut() else {
            return;
        };
        ctx.request_repaint_after(Duration::from_millis(16));
        if capture.waiting {
            if capture.completed.load(Ordering::Acquire) <= capture.step {
                return;
            }
            capture.step += 1;
            capture.frames = 0;
            capture.waiting = false;
            match capture.step {
                1 => {
                    self.view_modes.set(Section::Folders, ViewMode::Grid);
                }
                2 => self.section = Section::Documents,
                3 => self.section = Section::Contacts,
                4 => self.section = Section::Websites,
                5 => self.section = Section::Inventory,
                6 => self.section = Section::Settings,
                7 => {
                    self.section = Section::Credentials;
                    self.view_modes.set(Section::Credentials, ViewMode::List);
                }
                8 => self.view_modes.set(Section::Credentials, ViewMode::Grid),
                9 => {
                    self.view_modes.set(Section::Credentials, ViewMode::List);
                    let mut draft = self.credentials[0].clone();
                    draft.username = "updated@example.test".into();
                    self.inline_credentials.insert(draft.id.clone(), draft);
                }
                10 => self.updates.open = true,
                11 => {
                    self.updates.open = false;
                    self.section = Section::Contacts;
                    self.contact_draft = self.contact_items[0].clone();
                    self.contact_dialog_open = true;
                }
                12 => {
                    self.contact_dialog_open = false;
                    self.section = Section::Inventory;
                    self.inventory_draft = self.inventory_items[0].clone();
                    self.inventory_dialog_open = true;
                }
                13 => {
                    self.inventory_dialog_open = false;
                    self.section = Section::Documents;
                    let path = self.db.as_ref().unwrap().documents().unwrap()[0].clone();
                    self.open_editor(path);
                }
                14 => {
                    self.editor_visible = false;
                    self.section = Section::Books;
                    let mut book = crate::library::Book::new();
                    book.title = "我的读书笔记".into();
                    self.db.as_mut().unwrap().save_book(&book).unwrap();
                    let mut page = crate::library::Page::new(book.id.clone());
                    page.title = "第一章 · 想法".into();
                    page.body="记录书里的要点、自己的理解和下一步行动。\n\n每一页独立保存，可以增加多个标签，在标签页间切换。".into();
                    page.tags = vec!["阅读".into()];
                    self.db.as_mut().unwrap().save_book_page(&mut page).unwrap();
                    self.library.book = Some(book);
                    self.library.page = Some(page);
                }
                15 => {
                    self.section = Section::Terminal;
                    match crate::terminal::Terminal::start_command(
                        &self.data_dir,
                        "Write-Output 'PERSONAL_DESK_PTY_OK'; Get-Location",
                    ) {
                        Ok(terminal) => self.terminal = Some(terminal),
                        Err(error) => self.terminal_error = error,
                    }
                }
                16 => {
                    self.section = Section::Browser;
                    self.browser.open(&format!("{}?write", capture.browser_url));
                }
                17 => {
                    self.browser =
                        crate::browser::Browser::new(self.data_dir.join("browser-profile"));
                    self.browser.open(&format!("{}?read", capture.browser_url));
                }
                18 => {
                    self.section = Section::Inventory;
                    self.view_modes.set(Section::Inventory, ViewMode::List);
                    let mut draft = self.inventory_items[0].clone();
                    draft.disposal_date = "2026-10-20".into();
                    self.inline_inventory.insert(draft.id.clone(), draft);
                }
                19 => {
                    self.section = Section::Folders;
                    self.terminal_inline = true;
                }
                20 => {
                    self.section = Section::Contacts;
                    self.view_modes.set(Section::Contacts, ViewMode::List);
                }
                21 => {
                    self.section = Section::Documents;
                    self.editor_visible = false;
                    self.library.book = None;
                    self.library.page = None;
                    let records = self.db.as_ref().unwrap().document_records().unwrap();
                    let paths = records
                        .iter()
                        .take(2)
                        .map(|d| d.path.clone())
                        .collect::<Vec<_>>();
                    if paths.len() == 2 {
                        let mut book = self
                            .db
                            .as_mut()
                            .unwrap()
                            .group_documents(&paths, None)
                            .unwrap();
                        book.title = "项目手记".into();
                        self.db.as_mut().unwrap().save_book(&book).unwrap();
                    }
                }
                22 => {
                    let book = self
                        .db
                        .as_ref()
                        .unwrap()
                        .books()
                        .unwrap()
                        .into_iter()
                        .find(|b| b.title == "项目手记");
                    if let Some(book) = book {
                        self.open_document_book(book);
                    }
                }
                _ => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
            }
            self.notice.clear();
            return;
        }
        capture.frames += 1;
        if capture.step == 15
            && !self.terminal.as_ref().is_some_and(|terminal| {
                terminal
                    .parser
                    .screen()
                    .contents()
                    .contains("PERSONAL_DESK_PTY_OK")
            })
            && capture.frames < 600
        {
            return;
        }
        if matches!(capture.step, 16 | 17) {
            let file = capture.output.join(if capture.step == 16 {
                "browser-write-probe.json"
            } else {
                "browser-read-probe.json"
            });
            if !file.exists() && capture.frames < 600 {
                if capture.frames % 30 == 0 {
                    self.browser.debug_probe(file);
                }
                return;
            }
        }
        if capture.frames < 6 {
            return;
        }
        if capture.step == 15
            && let Some(terminal) = &self.terminal
        {
            let _ = fs::write(
                capture.output.join("powershell-probe.txt"),
                terminal.parser.screen().contents(),
            );
        }
        let name = [
            "files-list",
            "files-grid",
            "documents",
            "contacts",
            "websites",
            "inventory",
            "settings",
            "accounts-list",
            "accounts-grid",
            "accounts-editing",
            "updates",
            "contact-details",
            "inventory-details",
            "document-editor",
            "library-editor",
            "powershell",
            "browser",
            "browser-restored",
            "inventory-list-editing",
            "files-with-powershell",
            "contacts-list",
            "documents-with-books",
            "book-document-tabs",
        ][capture.step];
        let destination = capture.output.join(format!("{name}.png"));
        let completed = capture.completed.clone();
        let step = capture.step;
        capture.waiting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
            egui::ScreenshotCallback::new(move |image| {
                let bytes = image
                    .pixels
                    .iter()
                    .flat_map(|color| color.to_array())
                    .collect::<Vec<_>>();
                image::save_buffer_with_format(
                    destination,
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                    image::ImageFormat::Png,
                )
                .expect("preview screenshot");
                completed.store(step + 1, Ordering::Release);
            }),
        ));
    }
}

pub(super) fn start_browser_fixture() -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                break;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut request = [0; 8192];
            let count = stream.read(&mut request).unwrap_or(0);
            let set = String::from_utf8_lossy(&request[..count]).contains("?write");
            let script = if set {
                "localStorage.setItem('desk-fixture','persisted');document.cookie='desk-fixture=persisted; Max-Age=600; path=/';"
            } else {
                ""
            };
            let body = format!(
                "<!doctype html><html><head><title>PersonalDesk Browser Test</title></head><body style='font-family:Segoe UI;padding:32px;background:#f7f8fc'><h1>PersonalDesk Browser Test</h1><p>网页引擎与持久化验证</p><textarea style='width:80%;height:160px;padding:16px' placeholder='合成网页，不连接真实账号'></textarea><script>{script}</script></body></html>"
            );
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    url
}
