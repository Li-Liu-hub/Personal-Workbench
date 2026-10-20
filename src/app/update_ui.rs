use super::*;

pub(super) struct UpdateState {
    pub open: bool,
    source: PathBuf,
    release: Option<crate::updater::Release>,
    status: String,
    receiver: Option<Receiver<std::result::Result<Option<crate::updater::Release>, String>>>,
}
impl UpdateState {
    pub fn new(source: Option<String>) -> Self {
        let status = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("personal-desk-update.log")))
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default();
        Self {
            open: status.starts_with("更新失败："),
            source: source
                .map(PathBuf::from)
                .unwrap_or_else(crate::updater::default_source),
            release: None,
            status,
            receiver: None,
        }
    }
    #[cfg(test)]
    pub(super) fn set_test_release(&mut self, release: crate::updater::Release) {
        self.release = Some(release);
    }
    fn check(&mut self, ctx: &egui::Context) {
        let source = self.source.clone();
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.release = None;
        self.status = "正在检查更新…".into();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = crate::updater::check(&source, crate::updater::VERSION);
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
}
impl PersonalDeskApp {
    pub(super) fn render_update_dialog(&mut self, ctx: &egui::Context) {
        if let Some(result) = self
            .updates
            .receiver
            .as_ref()
            .and_then(|r| r.try_recv().ok())
        {
            self.updates.receiver = None;
            match result {
                Ok(Some(release)) => {
                    self.updates.status = format!("发现新版本 {}", release.manifest.version);
                    self.updates.release = Some(release);
                }
                Ok(None) => self.updates.status = "当前已是最新版本".into(),
                Err(error) => self.updates.status = error,
            }
        }
        if !self.updates.open {
            return;
        }
        let mut install = false;
        let dialog = centered_dialog(ctx, "app-update", "更新", "", 520.0, |ui| {
            ui.label(format!("当前版本  {}", crate::updater::VERSION));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.updates.receiver.is_none(),
                        egui::Button::new("检查更新"),
                    )
                    .clicked()
                {
                    self.updates.check(ctx);
                }
                if ui.button("选择更新清单").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("更新清单", &["json"])
                        .pick_file()
                {
                    self.updates.source = path;
                    if let Some(db) = self.db.as_mut() {
                        let _ = db
                            .set_setting("updates.source", &self.updates.source.to_string_lossy());
                    }
                    self.updates.check(ctx);
                }
            });
            ui.add(
                egui::Label::new(
                    RichText::new(self.updates.source.display().to_string())
                        .small()
                        .color(muted()),
                )
                .wrap(),
            );
            if !self.updates.status.is_empty() {
                ui.label(&self.updates.status);
            }
            if let Some(release) = self.updates.release.as_ref() {
                if !release.manifest.notes.is_empty() {
                    ui.label(&release.manifest.notes);
                }
                install = ui.add(primary_button("安装更新并重启")).clicked();
            }
        });
        if dialog_closed(&dialog) {
            self.updates.open = false;
        }
        if install {
            if !self.inline_credentials.is_empty() {
                self.updates.status = "请先保存或取消账号修改".into();
            } else if !self.inline_inventory.is_empty() {
                self.updates.status = "请先保存或取消物品修改".into();
            } else if self.terminal_starting
                || self.file_operations > 0
                || self.drop_busy
                || self.export_busy
                || self.contact_import_busy
                || self.codex.busy
            {
                self.updates.status = "当前操作完成后即可更新".into();
            } else if self.editor_dirty && !self.save_editor() {
                self.updates.status = "文档尚未保存，未开始更新".into();
            } else if !self.save_library_page() {
                self.updates.status = "图书页面尚未保存，未开始更新".into();
            } else if self
                .terminal
                .as_mut()
                .is_some_and(|terminal| terminal.active())
            {
                self.updates.status = "请先关闭 PowerShell 终端后更新".into();
            } else if !self.browser.tabs.is_empty() {
                self.updates.status = "请先关闭网页标签页后更新，避免丢失未发送的内容".into();
            } else if let Some(release) = self.updates.release.as_ref() {
                match crate::updater::launch(release, &self.data_dir) {
                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(error) => self.updates.status = error,
                }
            }
        }
    }
}
