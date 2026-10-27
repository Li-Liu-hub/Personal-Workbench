use super::*;
use crate::resume::{self, Bullet, Entry, Resume, ResumeSection, Store, TypstRenderer};
use std::time::Instant;
#[cfg(test)]
mod tests;

type RenderMessage = (
    std::result::Result<resume::Rendered, String>,
    Option<PathBuf>,
    u32,
);
#[derive(Default)]
pub(super) struct ResumeState {
    loaded: bool,
    pub(super) document: Option<Resume>,
    directory: PathBuf,
    revision: Option<Vec<u8>>,
    pub(super) dirty: bool,
    changed: Option<Instant>,
    preview_due: bool,
    receiver: Option<Receiver<RenderMessage>>,
    textures: Vec<egui::TextureHandle>,
    preview_revision: Option<Vec<u8>>,
    preview_stale: bool,
    pub(super) error: String,
    save_error: String,
    save_conflict: bool,
    status: String,
    query: String,
    page: usize,
    zoom: f32,
    auto_preview: bool,
    engine: PathBuf,
    preview_ppi: u32,
    rendered_ppi: u32,
}
impl ResumeState {
    #[cfg(debug_assertions)]
    pub(super) fn preview_ready(&self) -> bool {
        !self.textures.is_empty()
    }
    pub(super) fn busy(&self) -> bool {
        self.receiver.is_some()
    }
}

impl PersonalDeskApp {
    pub(super) fn ensure_resume_loaded(&mut self) {
        self.load_resume();
    }
    fn load_resume(&mut self) {
        if self.resume.loaded {
            return;
        }
        let configured = self
            .db
            .as_ref()
            .and_then(|db| db.setting("resume.directory").ok().flatten());
        self.resume.directory = resume::directory(&self.data_dir, configured.as_deref());
        self.resume.engine = self
            .db
            .as_ref()
            .and_then(|db| db.setting("resume.engine").ok().flatten())
            .map(PathBuf::from)
            .unwrap_or_else(|| TypstRenderer::discover().executable);
        self.resume.auto_preview = true;
        self.resume.zoom = 1.;
        self.resume.loaded = true;
        self.resume.save_error.clear();
        self.resume.save_conflict = false;
        match (Store {
            directory: self.resume.directory.clone(),
        })
        .load()
        {
            Ok(loaded) => {
                self.resume.document = Some(loaded.document);
                self.resume.revision = loaded.revision;
                self.resume.preview_due = true;
                self.resume.preview_stale = true;
                self.resume.changed = Some(Instant::now());
                self.resume.error.clear();
            }
            Err(e) => {
                self.resume.error = format!("读取简历失败：{e}。请检查 resume.json，原文件未覆盖。")
            }
        }
    }
    pub(super) fn flush_resume(&mut self) -> bool {
        if !self.resume.dirty {
            return true;
        }
        let Some(document) = self.resume.document.as_ref() else {
            return false;
        };
        match (Store {
            directory: self.resume.directory.clone(),
        })
        .save(document, self.resume.revision.as_deref())
        {
            Ok(revision) => {
                self.resume.revision = Some(revision);
                self.resume.dirty = false;
                self.resume.save_error.clear();
                self.resume.save_conflict = false;
                self.resume.status = "已保存到 docs/简历".into();
                true
            }
            Err(e) => {
                self.resume.save_conflict =
                    matches!(e, crate::error::DeskError::ConcurrentModification);
                self.resume.save_error = format!("保存失败，编辑内容已保留：{e}");
                false
            }
        }
    }
    fn save_resume_over_external(&mut self) {
        let Some(document) = self.resume.document.as_ref() else {
            return;
        };
        match (Store {
            directory: self.resume.directory.clone(),
        })
        .save_preserving_external(document)
        {
            Ok((revision, backup)) => {
                self.resume.revision = Some(revision);
                self.resume.dirty = false;
                self.resume.save_error.clear();
                self.resume.save_conflict = false;
                self.resume.preview_due = true;
                self.resume.status =
                    format!("已保存当前编辑；外部版本已备份：{}", backup.display());
            }
            Err(e) => self.resume.save_error = format!("保存失败，当前编辑已保留：{e}"),
        }
    }
    fn start_resume_render(&mut self, export: Option<PathBuf>) {
        if self.resume.busy() {
            return;
        }
        // A failed save must not prevent previewing the in-memory draft.
        if self.resume.save_error.is_empty() {
            self.flush_resume();
        }
        let Some(document) = self.resume.document.clone() else {
            return;
        };
        if let Some(path) = export.as_ref() {
            let protected = path.parent().and_then(|p| p.canonicalize().ok())
                == self.resume.directory.canonicalize().ok()
                && path.file_name().is_some_and(|n| n != "简历_编辑版.pdf");
            if protected {
                self.resume.error = "请另存为新的文件名，简历目录内的原始资料会保留".into();
                return;
            }
        }
        let store = Store {
            directory: self.resume.directory.clone(),
        };
        let renderer = TypstRenderer {
            executable: self.resume.engine.clone(),
        };
        let (sender, receiver) = mpsc::channel();
        self.resume.receiver = Some(receiver);
        self.resume.preview_due = false;
        self.resume.error.clear();
        self.resume.status = "正在排版…".into();
        let ppi = self.resume.preview_ppi.max(300);
        thread::spawn(move || {
            let result = renderer.render_at_ppi(&document, &store, ppi);
            let _ = sender.send((result, export, ppi));
        });
    }
    pub(super) fn poll_resume(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = self.resume.receiver.as_ref() {
            match receiver.try_recv() {
                Ok((result, export, ppi)) => {
                    self.resume.receiver = None;
                    match result {
                        Ok(rendered) => {
                            let current = self
                                .resume
                                .document
                                .as_ref()
                                .and_then(|d| d.revision().ok());
                            let disk_revision = (Store {
                                directory: self.resume.directory.clone(),
                            })
                            .load()
                            .ok()
                            .and_then(|loaded| loaded.revision);
                            let publish = !self.resume.dirty
                                && current.as_ref() == Some(&rendered.revision)
                                && disk_revision == self.resume.revision;
                            let write = (|| -> crate::error::Result<()> {
                                if publish {
                                    resume::atomic_write(
                                        &self.resume.directory.join("简历_编辑版.pdf"),
                                        &rendered.pdf,
                                    )?;
                                    resume::atomic_write(
                                        &self.resume.directory.join("resume.typ"),
                                        resume::TEMPLATE.as_bytes(),
                                    )?;
                                }
                                if let Some(path) = export.as_ref() {
                                    resume::atomic_write(path, &rendered.pdf)?;
                                }
                                Ok(())
                            })();
                            let wrote = write.is_ok();
                            if let Err(e) = write {
                                self.resume.error = format!("写出 PDF 失败：{e}");
                            }
                            self.resume.textures.clear();
                            for (i, bytes) in rendered.pages.iter().enumerate() {
                                match image::load_from_memory(bytes) {
                                    Ok(image) => {
                                        let limit = ctx.input(|i| i.max_texture_side) as u32;
                                        let image = if image.width().max(image.height()) > limit {
                                            image.resize(
                                                limit,
                                                limit,
                                                image::imageops::FilterType::Lanczos3,
                                            )
                                        } else {
                                            image
                                        };
                                        let image = image.to_rgba8();
                                        self.resume.textures.push(ctx.load_texture(
                                            format!("resume-page-{i}"),
                                            egui::ColorImage::from_rgba_unmultiplied(
                                                [image.width() as usize, image.height() as usize],
                                                image.as_raw(),
                                            ),
                                            egui::TextureOptions::LINEAR.with_mipmap_mode(Some(
                                                egui::TextureFilter::Linear,
                                            )),
                                        ));
                                    }
                                    Err(e) => self.resume.error = format!("预览图片读取失败：{e}"),
                                }
                            }
                            self.resume.page = self
                                .resume
                                .page
                                .min(self.resume.textures.len().saturating_sub(1));
                            self.resume.preview_revision = Some(rendered.revision);
                            self.resume.rendered_ppi = ppi;
                            self.resume.preview_stale = self
                                .resume
                                .document
                                .as_ref()
                                .and_then(|d| d.revision().ok())
                                != self.resume.preview_revision;
                            self.resume.status = if !wrote {
                                "排版完成，文件写出失败，请检查保存位置后重试".into()
                            } else if let Some(path) = export {
                                format!("PDF 已导出：{}", path.display())
                            } else if !publish {
                                format!(
                                    "草稿预览 · {} 页；保存状态见上方",
                                    self.resume.textures.len()
                                )
                            } else {
                                format!("已排版 · {} 页", self.resume.textures.len())
                            };
                        }
                        Err(e) => {
                            self.resume.error = e;
                            self.resume.status = "排版失败，可修改后重试".into();
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.resume.receiver = None;
                    self.resume.error = "排版任务中断，请重试".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let ready = self
            .resume
            .changed
            .is_some_and(|t| t.elapsed() >= Duration::from_millis(1000));
        if ready && self.resume.dirty && self.resume.save_error.is_empty() {
            self.flush_resume();
        }
        if ready
            && self.section == Section::Resume
            && self.resume.auto_preview
            && self.resume.preview_due
            && !self.resume.busy()
        {
            self.start_resume_render(None);
        }
        if self.resume.busy()
            || (self.resume.dirty && self.resume.save_error.is_empty())
            || (self.resume.preview_due && self.resume.auto_preview)
        {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
    pub(super) fn render_resume(&mut self, ui: &mut egui::Ui) {
        self.load_resume();
        let ctx = ui.ctx().clone();
        let save_key = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S));
        if save_key {
            self.flush_resume();
        }
        let mut render = false;
        let mut export = false;
        ui.horizontal_wrapped(|ui| {
            ui.heading("简历");
            ui.label(
                RichText::new(if self.resume.dirty {
                    "待保存"
                } else {
                    "本地保存"
                })
                .small()
                .color(muted()),
            );
            if ui
                .button("保存")
                .on_hover_text("Ctrl+S；停止输入 1 秒后自动保存")
                .clicked()
            {
                self.flush_resume();
            }
            render = ui
                .add_enabled(!self.resume.busy(), egui::Button::new("刷新预览"))
                .clicked();
            export = ui
                .add_enabled(
                    !self.resume.busy() && self.resume.document.is_some(),
                    primary_button("导出 PDF"),
                )
                .clicked();
            if ui.button("备份资料").clicked() && self.flush_resume() {
                self.start_export(export::Scope::Resume);
            }
            if ui.button("打开文件夹").clicked() {
                let _ = fs::create_dir_all(&self.resume.directory);
                self.location_request = Some(self.resume.directory.clone());
            }
            ui.menu_button("更多", |ui| {
                ui.checkbox(&mut self.resume.auto_preview, "自动刷新预览");
                if ui
                    .add_enabled(
                        !self.resume.busy(),
                        egui::Button::new("选择简历资料文件夹…"),
                    )
                    .clicked()
                    && self.flush_resume()
                    && let Some(folder) = rfd::FileDialog::new()
                        .set_title("选择保存 resume.json 的文件夹")
                        .pick_folder()
                {
                    match self
                        .db
                        .as_mut()
                        .unwrap()
                        .set_setting("resume.directory", &folder.to_string_lossy())
                    {
                        Ok(()) => {
                            self.resume = ResumeState::default();
                            self.load_resume();
                        }
                        Err(e) => self.resume.error = e.to_string(),
                    }
                }
                if ui.button("选择 Typst 引擎…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Typst", &["exe"])
                        .pick_file()
                {
                    match self
                        .db
                        .as_mut()
                        .unwrap()
                        .set_setting("resume.engine", &path.to_string_lossy())
                    {
                        Ok(()) => {
                            self.resume.engine = path;
                            self.resume.error.clear();
                            render = true;
                        }
                        Err(e) => self.resume.error = e.to_string(),
                    }
                }
                if ui
                    .add_enabled(!self.resume.busy(), egui::Button::new("重新读取已保存内容"))
                    .on_hover_text("会放弃当前未保存的编辑；用于处理外部文件修改冲突")
                    .clicked()
                {
                    self.resume.document = None;
                    self.resume.dirty = false;
                    self.resume.loaded = false;
                    self.load_resume();
                }
                ui.label(self.resume.directory.display().to_string());
            });
        });
        ui.add_space(6.);
        if !self.resume.error.is_empty() {
            ui.colored_label(Color32::from_rgb(180, 55, 55), &self.resume.error);
        }
        if !self.resume.save_error.is_empty() {
            ui.colored_label(Color32::from_rgb(180, 55, 55), &self.resume.save_error);
            ui.label("可以继续编辑和预览，当前修改尚未保存。");
            if self.resume.save_conflict
                && ui
                    .button("备份外部版本并保存当前编辑")
                    .on_hover_text("将磁盘上的版本另存为独立备份，再保存当前左侧的编辑")
                    .clicked()
            {
                self.save_resume_over_external();
            }
        }
        let stale = self.resume.preview_stale;
        ui.horizontal(|ui| {
            if self.resume.busy() {
                ui.spinner();
            }
            ui.label(RichText::new(&self.resume.status).small().color(muted()));
            if stale && !self.resume.textures.is_empty() {
                ui.label(
                    RichText::new("内容已变化，预览等待更新")
                        .small()
                        .color(accent()),
                );
            }
        });
        ui.separator();
        let mut changed = false;
        let mut photo = false;
        let available = ui.available_size();
        let editor_width = (available.x * 0.51).clamp(320., 610.);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(editor_width, available.y),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.resume.query)
                            .hint_text("查找经历、技能、内容")
                            .desired_width(f32::INFINITY),
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("resume-editor")
                        .max_height(ui.available_height())
                        .show(ui, |ui| {
                            if let Some(document) = self.resume.document.as_mut() {
                                let edits = edit_document(ui, document, &self.resume.query);
                                changed |= edits.0;
                                photo = edits.1;
                            } else {
                                ui.label("请修复资料文件后，在“更多”中重新读取。");
                            }
                        });
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), available.y),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong("排版预览");
                        if ui.small_button("−").on_hover_text("缩小预览").clicked() {
                            self.resume.zoom = (self.resume.zoom - 0.25).max(1.);
                        }
                        ui.label(format!("适宽 × {:.2}", self.resume.zoom));
                        if ui.small_button("+").on_hover_text("放大预览").clicked() {
                            self.resume.zoom = (self.resume.zoom + 0.25).min(3.);
                        }
                        if ui.small_button("适宽").clicked() {
                            self.resume.zoom = 1.;
                        }
                        if self.resume.textures.len() > 1 {
                            if ui.small_button("上一页").clicked() {
                                self.resume.page = self.resume.page.saturating_sub(1);
                            }
                            ui.label(format!(
                                "{}/{}",
                                self.resume.page + 1,
                                self.resume.textures.len()
                            ));
                            if ui.small_button("下一页").clicked() {
                                self.resume.page =
                                    (self.resume.page + 1).min(self.resume.textures.len() - 1);
                            }
                        }
                    });
                    if self.resume.textures.len() > 1 {
                        ui.label(
                            RichText::new("当前超过一页，可调整字号、页边距或精简内容")
                                .small()
                                .color(accent()),
                        );
                    }
                    let preview_width = ui.available_width().max(100.) * self.resume.zoom;
                    let required_ppi =
                        ((preview_width * ctx.pixels_per_point() / (210. / 25.4) / 150.).ceil()
                            as u32
                            * 150)
                            .clamp(300, 600);
                    if required_ppi > self.resume.preview_ppi {
                        self.resume.preview_ppi = required_ppi;
                        if required_ppi > self.resume.rendered_ppi {
                            self.resume.preview_due = true;
                            self.resume.changed = Some(Instant::now());
                        }
                    }
                    egui::ScrollArea::both()
                        .id_salt("resume-preview")
                        .show(ui, |ui| {
                            if let Some(texture) = self.resume.textures.get(self.resume.page) {
                                let width = preview_width;
                                ui.add(egui::Image::from_texture(texture).fit_to_exact_size(
                                    egui::vec2(
                                        width,
                                        width * texture.size_vec2().y / texture.size_vec2().x,
                                    ),
                                ));
                            } else {
                                ui.label("修改左侧内容后，这里会显示 Typst 实际生成的页面。");
                            }
                        });
                },
            );
        });
        if photo
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("照片", &["png", "jpg", "jpeg", "webp"])
                .pick_file()
        {
            match (Store {
                directory: self.resume.directory.clone(),
            })
            .import_photo(&path)
            {
                Ok(name) => {
                    if let Some(d) = self.resume.document.as_mut() {
                        d.photo = name;
                        changed = true;
                    }
                }
                Err(e) => self.resume.error = e.to_string(),
            }
        }
        if changed {
            self.resume.preview_stale = true;
            self.resume.dirty = true;
            self.resume.changed = Some(Instant::now());
            self.resume.preview_due = true;
            ctx.request_repaint_after(Duration::from_millis(1100));
        }
        if render {
            self.start_resume_render(None);
        }
        if export
            && let Some(mut path) = rfd::FileDialog::new()
                .set_title("导出简历 PDF")
                .set_file_name("简历_编辑版.pdf")
                .add_filter("PDF", &["pdf"])
                .save_file()
        {
            path.set_extension("pdf");
            self.start_resume_render(Some(path));
        }
    }
}

fn field(ui: &mut egui::Ui, title: &str, value: &mut String, multiline: bool) -> bool {
    ui.label(RichText::new(title).small().color(muted()));
    if multiline {
        ui.add(
            egui::TextEdit::multiline(value)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        )
        .changed()
    } else {
        ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY))
            .changed()
    }
}
fn highlights_field(ui: &mut egui::Ui, values: &mut Vec<String>) -> bool {
    let mut text = values.join("\n");
    if field(ui, "重点加粗（每行填写一段正文中的词句）", &mut text, true) {
        *values = text.split('\n').take(100).map(str::to_owned).collect();
        true
    } else {
        false
    }
}
#[derive(Clone, Copy)]
enum Action {
    Up(usize),
    Down(usize),
    Remove(usize),
}
fn controls(ui: &mut egui::Ui, index: usize, len: usize) -> Option<Action> {
    if ui
        .add_enabled(index > 0, egui::Button::new("↑").small())
        .clicked()
    {
        return Some(Action::Up(index));
    }
    if ui
        .add_enabled(index + 1 < len, egui::Button::new("↓").small())
        .clicked()
    {
        return Some(Action::Down(index));
    }
    if ui.small_button("删除").clicked() {
        return Some(Action::Remove(index));
    }
    None
}
fn apply<T>(items: &mut Vec<T>, action: Option<Action>) -> bool {
    match action {
        Some(Action::Up(i)) => items.swap(i, i - 1),
        Some(Action::Down(i)) => items.swap(i, i + 1),
        Some(Action::Remove(i)) => {
            items.remove(i);
        }
        None => return false,
    }
    true
}
fn edit_document(ui: &mut egui::Ui, d: &mut Resume, query: &str) -> (bool, bool) {
    let mut changed = false;
    let mut photo = false;
    if query.is_empty() {
        egui::CollapsingHeader::new("个人信息与版式")
            .default_open(true)
            .show(ui, |ui| {
                changed |= field(ui, "姓名", &mut d.name, false);
                changed |= field(ui, "届别 / 求职方向", &mut d.tagline, false);
                changed |= field(ui, "电话 / 邮箱", &mut d.contact, false);
                changed |= field(ui, "求职地点", &mut d.location, false);
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(&mut d.style.show_photo, "显示照片").changed();
                    photo = ui.button("更换照片").clicked();
                });
                if ui
                    .button("恢复原版版式")
                    .on_hover_text("恢复原版字号、行高、间距和页边距，保留简历文字")
                    .clicked()
                {
                    d.style = resume::Style::default();
                    changed = true;
                }
                changed |= ui
                    .add(
                        egui::Slider::new(&mut d.style.font_pt, 7.0..=14.0)
                            .text("字号")
                            .step_by(0.1),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut d.style.margin_mm, 6.0..=25.0)
                            .text("左右边距 mm")
                            .step_by(0.5),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut d.style.line_pt, 8.0..=28.0)
                            .text("正文行高")
                            .fixed_decimals(2),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut d.style.project_gap_pt, 0.0..=20.0)
                            .text("项目要点间距")
                            .step_by(0.5),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut d.style.skills_gap_pt, 0.0..=20.0)
                            .text("技能条目间距")
                            .step_by(0.5),
                    )
                    .changed();
                egui::CollapsingHeader::new("上下页边距").show(ui, |ui| {
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut d.style.top_mm, 3.0..=25.0)
                                .text("上边距 mm")
                                .step_by(0.5),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut d.style.bottom_mm, 2.0..=25.0)
                                .text("下边距 mm")
                                .step_by(0.5),
                        )
                        .changed();
                });
            });
    }
    let mut section_action = None;
    let section_len = d.sections.len();
    for (i, section) in d.sections.iter_mut().enumerate() {
        let section_match =
            query.is_empty() || section.title.to_lowercase().contains(&query.to_lowercase());
        if !section_match
            && !section.entries.iter().any(|e| {
                matches_query(&query.to_lowercase(), &[&e.heading, &e.meta, &e.summary])
                    || e.bullets
                        .iter()
                        .any(|b| matches_query(&query.to_lowercase(), &[&b.title, &b.body]))
            })
        {
            continue;
        }
        ui.push_id(&section.id, |ui| {
            ui.horizontal(|ui| {
                ui.strong(&section.title);
                section_action = controls(ui, i, section_len).or(section_action);
            });
            changed |= field(ui, "栏目名称", &mut section.title, false);
            let mut action = None;
            let len = section.entries.len();
            for (j, entry) in section.entries.iter_mut().enumerate() {
                if !section_match
                    && !matches_query(
                        &query.to_lowercase(),
                        &[&entry.heading, &entry.meta, &entry.summary],
                    )
                    && !entry
                        .bullets
                        .iter()
                        .any(|b| matches_query(&query.to_lowercase(), &[&b.title, &b.body]))
                {
                    continue;
                }
                ui.push_id(&entry.id, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(if entry.heading.is_empty() {
                            "新条目"
                        } else {
                            &entry.heading
                        });
                        action = controls(ui, j, len).or(action);
                    });
                    egui::CollapsingHeader::new("编辑条目")
                        .id_salt("entry")
                        .default_open(true)
                        .show(ui, |ui| {
                            changed |=
                                field(ui, "学校 / 公司 / 项目名称", &mut entry.heading, false);
                            changed |= field(ui, "日期 / 职位 / 技术栈", &mut entry.meta, false);
                            let mut project = entry.kind == "project";
                            if ui.checkbox(&mut project, "项目标题样式").changed() {
                                entry.kind = if project { "project" } else { "entry" }.into();
                                changed = true;
                            }
                            changed |= field(ui, "简介 / 成果 / 职责", &mut entry.summary, true);
                            if !entry.summary.is_empty() {
                                changed |= highlights_field(ui, &mut entry.highlights);
                            }
                            let mut bullet_action = None;
                            let bullet_len = entry.bullets.len();
                            for (k, bullet) in entry.bullets.iter_mut().enumerate() {
                                ui.push_id(k, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(format!("要点 {}", k + 1));
                                        bullet_action =
                                            controls(ui, k, bullet_len).or(bullet_action);
                                    });
                                    changed |= field(ui, "要点标题", &mut bullet.title, false);
                                    changed |= field(ui, "内容", &mut bullet.body, true);
                                    changed |= highlights_field(ui, &mut bullet.highlights);
                                });
                            }
                            changed |= apply(&mut entry.bullets, bullet_action);
                            if ui.small_button("+ 添加要点").clicked() {
                                entry.bullets.push(Bullet::default());
                                changed = true;
                            }
                        });
                });
                ui.add_space(8.);
            }
            changed |= apply(&mut section.entries, action);
            if ui.button("+ 添加经历 / 条目").clicked() {
                section.entries.push(Entry {
                    id: uuid::Uuid::new_v4().to_string(),
                    kind: "entry".into(),
                    ..Default::default()
                });
                changed = true;
            }
            ui.separator();
        });
    }
    changed |= apply(&mut d.sections, section_action);
    if ui.button("+ 添加栏目").clicked() {
        d.sections.push(ResumeSection {
            id: uuid::Uuid::new_v4().to_string(),
            title: "新栏目".into(),
            entries: vec![],
        });
        changed = true;
    }
    (changed, photo)
}
