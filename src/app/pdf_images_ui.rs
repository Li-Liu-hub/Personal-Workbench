use super::*;
use crate::pdf_images::{self, Format, Options, Summary};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, TryRecvError},
};

pub(super) const TOOL_ID: &str = "builtin:pdf-images";
#[cfg(test)]
mod tests;
enum Message {
    Progress(usize, usize, u32),
    Finished(std::result::Result<Summary, String>),
}

pub(super) struct PdfImagesState {
    pub open: bool,
    input: String,
    output: String,
    format: Format,
    dpi: u32,
    quality: u8,
    pages: String,
    receiver: Option<Receiver<Message>>,
    cancel: Arc<AtomicBool>,
    fraction: f32,
    status: String,
    error: String,
    result: Option<Summary>,
}

impl Default for PdfImagesState {
    fn default() -> Self {
        Self {
            open: false,
            input: String::new(),
            output: String::new(),
            format: Format::Png,
            dpi: 150,
            quality: 92,
            pages: String::new(),
            receiver: None,
            cancel: Default::default(),
            fraction: 0.,
            status: String::new(),
            error: String::new(),
            result: None,
        }
    }
}

impl PdfImagesState {
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }
    fn start(&mut self) {
        if self.busy() {
            return;
        }
        let opts = Options {
            input: PathBuf::from(self.input.trim()),
            output: PathBuf::from(self.output.trim()),
            format: self.format,
            dpi: self.dpi,
            quality: self.quality,
            pages: self.pages.clone(),
        };
        if let Err(error) = opts.validate() {
            self.error = error;
            return;
        }
        let (sender, receiver) = mpsc::sync_channel(32);
        self.receiver = Some(receiver);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        self.fraction = 0.;
        self.status = "正在打开 PDF…".into();
        self.error.clear();
        self.result = None;
        std::thread::spawn(move || {
            let result = pdf_images::convert(&opts, &cancel, |done, total, page| {
                let _ = sender.try_send(Message::Progress(done, total, page));
            });
            let _ = sender.send(Message::Finished(result));
        });
    }
    pub fn poll(&mut self) -> Option<String> {
        loop {
            match self.receiver.as_ref()?.try_recv() {
                Ok(Message::Progress(done, total, page)) => {
                    self.fraction = done as f32 / total.max(1) as f32;
                    self.status = format!("已转换 {done}/{total} 页 · 原 PDF 第 {page} 页");
                }
                Ok(Message::Finished(result)) => {
                    self.receiver = None;
                    self.status = match result {
                        Ok(summary) => {
                            self.fraction = 1.;
                            let text = format!("转换完成，共导出 {} 张图片", summary.pages);
                            self.result = Some(summary);
                            text
                        }
                        Err(error) => {
                            self.error = error.clone();
                            error
                        }
                    };
                    return Some(self.status.clone());
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.error = "转换任务意外结束，请重试".into();
                    return Some(self.error.clone());
                }
            }
        }
    }
}
impl Drop for PdfImagesState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl PersonalDeskApp {
    pub(super) fn render_pdf_images_entry(
        &mut self,
        ui: &mut egui::Ui,
        layout: crate::ui_components::EntryLayout,
    ) {
        let id = TOOL_ID;
        if !self.record_matches_filter(TagScope::Tools, id)
            || !matches_query(
                &self.search_query.to_lowercase(),
                &[
                    "PDF 转图片",
                    "pdf png jpg jpeg webp bmp 图片 转换 导出 内置工具",
                ],
            )
        {
            return;
        }
        let response = entry(
            ui,
            id,
            "PDF 转图片",
            if self.pdf_images.busy() {
                "正在转换 · 点击查看进度"
            } else {
                "内置工具 · PNG / JPG / WebP / BMP"
            },
            TileKind::Document,
            layout,
        );
        self.favorite_control(ui, &response, "tools", id);
        if response.clicked() {
            self.pdf_images.open = true;
        }
        response.context_menu(|ui| {
            if ui.button("打开 PDF 转图片").clicked() {
                self.pdf_images.open = true;
                ui.close();
            }
        });
    }
    pub(super) fn render_pdf_images_dialog(&mut self, ctx: &egui::Context) {
        if !self.pdf_images.open {
            return;
        }
        let state = &mut self.pdf_images;
        let busy = state.busy();
        let dialog = centered_dialog(ctx, "pdf-images", "PDF 转图片", "", 620., |ui| {
            ui.label("将 PDF 每页导出为一张图片，文件保留原页码。全程在本机处理。");
            ui.add_enabled_ui(!busy, |ui| {
                ui.label("PDF 文件");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut state.input).desired_width((ui.available_width() - 80.).max(80.)));
                    if ui.button("选择…").clicked() && let Some(path) = rfd::FileDialog::new().add_filter("PDF 文档", &["pdf"]).pick_file() {
                        state.input = path.display().to_string();
                        if state.output.is_empty() && let Some(parent) = path.parent() { state.output = parent.display().to_string(); }
                    }
                });
                ui.label("输出文件夹");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut state.output).desired_width((ui.available_width() - 80.).max(80.)));
                    if ui.button("浏览…").clicked() && let Some(path) = rfd::FileDialog::new().pick_folder() { state.output = path.display().to_string(); }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("图片格式");
                    for format in Format::ALL { ui.selectable_value(&mut state.format, format, format.label()); }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("清晰度");
                    ui.add(egui::DragValue::new(&mut state.dpi).range(72..=600).suffix(" DPI"));
                    for (label, dpi) in [("普通 96", 96), ("清晰 150", 150), ("高清 300", 300)] {
                        if ui.small_button(label).clicked() { state.dpi = dpi; }
                    }
                });
                if state.format == Format::Jpg { ui.add(egui::Slider::new(&mut state.quality, 1..=100).text("JPG 质量")); }
                ui.horizontal(|ui| {
                    ui.label("页码范围");
                    ui.add(egui::TextEdit::singleline(&mut state.pages).hint_text("留空转换全部；示例：1,3-5").desired_width(320.));
                });
                ui.small("每次新建独立结果文件夹，不覆盖已有图片。PNG / WebP 为无损格式；JPG 体积较小，BMP 较大。");
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!busy, primary_button("开始转换")).clicked() {
                    state.start();
                }
                if busy {
                    let cancelling = state.cancel.load(Ordering::Relaxed);
                    if ui
                        .add_enabled(!cancelling, egui::Button::new("取消转换"))
                        .clicked()
                    {
                        state.cancel.store(true, Ordering::Relaxed);
                    }
                    ui.small(if cancelling {
                        "当前页面处理结束后取消…"
                    } else {
                        "可以关闭面板，任务继续在后台运行。"
                    });
                }
                if let Some(result) = &state.result
                    && ui.button("打开结果文件夹").clicked()
                    && let Err(error) = open::that_detached(&result.folder)
                {
                    state.error = error.to_string();
                }
            });
            if !state.status.is_empty() {
                ui.add(egui::ProgressBar::new(state.fraction).show_percentage());
                ui.label(&state.status);
            }
            if let Some(result) = &state.result {
                ui.small(result.folder.display().to_string());
            }
            if !state.error.is_empty() {
                ui.colored_label(Color32::from_rgb(184, 65, 71), &state.error);
            }
        });
        if dialog_closed(&dialog) {
            self.pdf_images.open = false;
        }
    }
}
