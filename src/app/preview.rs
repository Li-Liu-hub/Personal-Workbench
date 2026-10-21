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
        let source = self.data_dir.join("preview-documents");
        fs::create_dir_all(&source).unwrap();
        for name in ["项目资料.md", "工作清单.txt"] {
            let path = source.join(name);
            fs::write(&path, "仅用于隔离预览的文档").unwrap();
            self.add_document_indices(&[path]);
        }
        self.add_shortcut(source);
        for i in 0..45 {
            let mut account = Credential::new();
            account.title = format!("示例账号 {:02}", i + 1);
            account.username = format!("demo{:02}@example.test", i + 1);
            account.password = "Demo-Value-".repeat(1 + i % 5);
            self.db.as_mut().unwrap().save_credential(&account).unwrap();
        }
        self.load_summaries();
        self.section = Section::Credentials;
        self.view_modes.set(Section::Credentials, ViewMode::Grid);
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
                    1 => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1000., 680.,
                    ))),
                    2 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280., 800.,
                        )));
                        self.view_modes.set(Section::Credentials, ViewMode::List);
                    }
                    3 => {
                        self.section = Section::Folders;
                        self.view_modes.set(Section::Folders, ViewMode::Grid);
                    }
                    4 => self.view_modes.set(Section::Folders, ViewMode::List),
                    5 => {
                        self.section = Section::Documents;
                        self.view_modes.set(Section::Documents, ViewMode::Grid);
                    }
                    6 => self.view_modes.set(Section::Documents, ViewMode::List),
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
            "accounts-grid-wide",
            "accounts-grid-small",
            "accounts-list",
            "shortcuts-grid",
            "shortcuts-list",
            "documents-grid",
            "documents-list",
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
