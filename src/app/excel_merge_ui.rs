use super::*;
use crate::excel_merge::{self, MAX_DATA_ROWS, MergeOptions, MergeSummary, Progress};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, TryRecvError},
};

pub(super) const TOOL_ID: &str = "builtin:excel-merge";

enum Message {
    Progress(Progress),
    Finished(std::result::Result<MergeSummary, String>),
}

pub(super) struct ExcelMergeState {
    pub open: bool,
    input: String,
    output: String,
    rows: String,
    recursive: bool,
    repeat_header: bool,
    drop_headers: bool,
    overwrite: bool,
    receiver: Option<Receiver<Message>>,
    cancel: Arc<AtomicBool>,
    fraction: f32,
    status: String,
    logs: std::collections::VecDeque<String>,
    error: String,
    result: Option<MergeSummary>,
}

impl Default for ExcelMergeState {
    fn default() -> Self {
        Self {
            open: false,
            input: String::new(),
            output: String::new(),
            rows: "50000".into(),
            recursive: false,
            repeat_header: false,
            drop_headers: false,
            overwrite: false,
            receiver: None,
            cancel: Default::default(),
            fraction: 0.,
            status: String::new(),
            logs: Default::default(),
            error: String::new(),
            result: None,
        }
    }
}

impl ExcelMergeState {
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }

    fn log(&mut self, message: String) {
        if self.logs.len() >= 100 {
            self.logs.pop_front();
        }
        self.logs.push_back(message);
    }

    fn options(&self) -> std::result::Result<MergeOptions, String> {
        let rows_per_sheet = self
            .rows
            .trim()
            .parse()
            .map_err(|_| "每表数据条数请输入整数".to_owned())?;
        let opts = MergeOptions {
            input_dir: PathBuf::from(self.input.trim()),
            output_file: PathBuf::from(self.output.trim()),
            rows_per_sheet,
            recursive: self.recursive,
            header_on_each_sheet: self.repeat_header,
            drop_file_headers: self.drop_headers,
        };
        opts.validate().map_err(|error| error.to_string())?;
        if opts.output_file.exists() && !self.overwrite {
            return Err("输出文件已存在，请更换文件名或勾选覆盖已有输出".into());
        }
        Ok(opts)
    }

    fn start(&mut self) {
        if self.busy() {
            return;
        }
        let opts = match self.options() {
            Ok(opts) => opts,
            Err(error) => {
                self.error = error;
                return;
            }
        };
        let (sender, receiver) = mpsc::sync_channel(128);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        self.receiver = Some(receiver);
        self.fraction = 0.;
        self.status = "正在扫描源文件夹…".into();
        self.error.clear();
        self.result = None;
        self.logs.clear();
        // Progress can be dropped when the UI is slow; final outcome is always delivered.
        std::thread::spawn(move || {
            let result = excel_merge::merge(&opts, &cancel, |event| {
                let _ = sender.try_send(Message::Progress(event));
            })
            .map_err(|error| error.to_string());
            let _ = sender.send(Message::Finished(result));
        });
    }

    pub fn poll(&mut self) -> Option<String> {
        loop {
            let message = self.receiver.as_ref()?.try_recv();
            match message {
                Ok(Message::Progress(event)) => match event {
                    Progress::FoundFiles(count) => {
                        self.log(format!("找到 {count} 个源文件，按路径名称排序"))
                    }
                    Progress::Reading {
                        completed,
                        total,
                        file,
                        sheet,
                        rows,
                    } => {
                        self.fraction = completed as f32 / total.max(1) as f32 * 0.9;
                        self.status = format!("{completed}/{total} 个文件 · 已合并 {rows} 条数据");
                        if !sheet.is_empty() {
                            self.status.push_str(&format!("\n{file} / {sheet}"));
                        } else {
                            self.log(format!("已处理 {file}，累计 {rows} 条数据"));
                        }
                    }
                    Progress::NewSheet(index) => self.log(format!("新建输出工作表：数据{index}")),
                    Progress::Saving => {
                        self.fraction = 0.95;
                        self.status = "正在保存结果…".into();
                    }
                },
                Ok(Message::Finished(result)) => {
                    self.receiver = None;
                    let message = match result {
                        Ok(summary) => {
                            self.fraction = 1.;
                            let text = format!(
                                "合并完成：{} 个文件 / {} 个来源工作表 → {} 个结果工作表，{} 条数据，耗时 {:.1} 秒",
                                summary.files,
                                summary.source_sheets,
                                summary.output_sheets,
                                summary.rows,
                                summary.elapsed_ms as f64 / 1000.
                            );
                            self.result = Some(summary);
                            text
                        }
                        Err(error) => {
                            self.error = error.clone();
                            error
                        }
                    };
                    self.status = message.clone();
                    self.log(message.clone());
                    return Some(message);
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.error = "合并任务意外结束，请重试；原输出文件保留".into();
                    return Some(self.error.clone());
                }
            }
        }
    }
}

impl Drop for ExcelMergeState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl PersonalDeskApp {
    pub(super) fn render_excel_merge_dialog(&mut self, ctx: &egui::Context) {
        if !self.excel_merge.open {
            return;
        }
        let state = &mut self.excel_merge;
        let busy = state.busy();
        let dialog = centered_dialog(ctx, "excel-merge", "Excel 批量合并", "", 640., |ui| {
            ui.label("将文件夹中所有 Excel 的全部工作表，按文件名顺序合并到一个 .xlsx。");
            ui.add_enabled_ui(!busy, |ui| {
                ui.label("源文件夹");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut state.input).desired_width((ui.available_width() - 80.).max(80.)));
                    if ui.button("浏览…").clicked()
                        && let Some(path) = rfd::FileDialog::new().set_title("选择待合并的 Excel 文件夹").pick_folder()
                    {
                        state.input = path.display().to_string();
                        if state.output.is_empty() {
                            state.output = path.join("合并结果.xlsx").display().to_string();
                        }
                    }
                });
                ui.label("输出文件");
                ui.horizontal(|ui| {
                    if ui.add(egui::TextEdit::singleline(&mut state.output).desired_width((ui.available_width() - 80.).max(80.))).changed() {
                        state.overwrite = false;
                    }
                    if ui.button("另存为…").clicked()
                        && let Some(path) = rfd::FileDialog::new().set_title("保存合并结果").add_filter("Excel 工作簿", &["xlsx"]).set_file_name("合并结果.xlsx").save_file()
                    {
                        state.overwrite = path.exists();
                        state.output = path.display().to_string();
                    }
                });
                if PathBuf::from(state.output.trim()).exists() {
                    ui.checkbox(&mut state.overwrite, "覆盖已有输出（该文件自动排除，不参与合并）");
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("每个结果工作表的数据条数");
                    ui.add(egui::TextEdit::singleline(&mut state.rows).desired_width(110.));
                    ui.label("不含表头");
                    for (label, count) in [("5千", 5_000), ("5万", 50_000), ("10万", 100_000), ("最大容量", MAX_DATA_ROWS)] {
                        if ui.small_button(label).clicked() { state.rows = count.to_string(); }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut state.repeat_header, "每个结果工作表都带表头");
                    ui.checkbox(&mut state.drop_headers, "移除其余来源工作表的表头");
                    ui.checkbox(&mut state.recursive, "包含子文件夹");
                });
                ui.small("第一个非空工作表的首个非空行作为基准表头。请使用列结构一致的表格；空行、空工作表自动跳过。");
                ui.small("合并单元格值，保留文本编号、数值和日期；不保留原样式、图片或公式，公式取文件中已缓存的值。");
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!busy, primary_button("开始合并")).clicked() {
                    state.start();
                }
                if busy {
                    let cancelling = state.cancel.load(Ordering::Relaxed);
                    if ui
                        .add_enabled(!cancelling, egui::Button::new("取消合并"))
                        .clicked()
                    {
                        state.cancel.store(true, Ordering::Relaxed);
                    }
                    ui.small(if cancelling {
                        "正在取消，当前工作表读取或保存结束后停止…"
                    } else {
                        "可以关闭此面板，合并继续在后台运行。"
                    });
                }
                if let Some(result) = &state.result {
                    if ui.button("打开结果").clicked()
                        && let Err(error) = open::that_detached(&result.output_file)
                    {
                        state.error = error.to_string();
                    }
                    if ui.button("打开所在文件夹").clicked()
                        && let Some(parent) = result.output_file.parent()
                        && let Err(error) = open::that_detached(parent)
                    {
                        state.error = error.to_string();
                    }
                }
            });
            if !state.status.is_empty() {
                ui.add(egui::ProgressBar::new(state.fraction).show_percentage());
                ui.label(&state.status);
            }
            if !state.error.is_empty() {
                ui.colored_label(Color32::from_rgb(184, 65, 71), &state.error);
            }
            if !state.logs.is_empty() {
                egui::CollapsingHeader::new("处理日志（最近 100 条）").show(ui, |ui| {
                    for line in &state.logs {
                        ui.small(line);
                    }
                });
            }
        });
        if dialog_closed(&dialog) {
            self.excel_merge.open = false;
        }
    }
}

#[cfg(test)]
mod tests;
