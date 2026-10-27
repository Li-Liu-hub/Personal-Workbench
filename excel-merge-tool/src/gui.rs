//! 图形界面（eframe / egui）
//!
//! 合并跑在后台线程，通过 channel 把进度回传主线程刷新界面。

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

use eframe::egui;
use egui::{Color32, RichText};

use crate::merge::{merge, MergeOptions, MergeSummary, Progress};

/// 每 sheet 条数的可调范围
const MIN_ROWS: u32 = 1_000;
/// Excel 单表上限（含表头，所以数据行最多 1048575）
const MAX_ROWS: u32 = 1_048_575;

/// 把条数显示成易读形式：50000 → 5万
fn human(n: u32) -> String {
    if n >= 10_000 {
        let w = n as f64 / 10_000.0;
        if (w.fract() * 10.0).abs() < 0.05 {
            format!("{}万", w as u32)
        } else {
            format!("{:.1}万", w)
        }
    } else {
        n.to_string()
    }
}

/// 后台线程 --> UI 的消息
enum UiMsg {
    Progress(Progress),
    Done(Result<MergeSummary, String>),
}

pub struct App {
    opts: MergeOptions,
    rows_text: String,
    /// 滑杆当前值（与 rows_text 双向同步）
    slider: u32,
    log: Vec<String>,
    rx: Option<Receiver<UiMsg>>,
    /// 0.0 ~ 1.0，用于进度条
    progress: f32,
    status: String,
    current_file: String,
    done_index: usize,
    total_files: usize,
    rows_written: usize,
    banner: Option<(bool, String)>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            opts: MergeOptions::default(),
            rows_text: "50000".to_string(),
            slider: 50_000,
            log: Vec::new(),
            rx: None,
            progress: 0.0,
            status: "就绪".to_string(),
            current_file: String::new(),
            done_index: 0,
            total_files: 0,
            rows_written: 0,
            banner: None,
        }
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_chinese_fonts(&cc.egui_ctx);
        let mut app = Self::default();
        app.push_log("请选择源文件夹、指定输出文件，然后点「开始合并」。");
        app
    }

    fn push_log(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 400 {
            self.log.drain(0..100);
        }
    }

    fn running(&self) -> bool {
        self.rx.is_some()
    }

    // ---------------------------------------------------------- 启动任务

    fn start(&mut self, ctx: &egui::Context) {
        if self.opts.input_dir.as_os_str().is_empty() {
            self.banner = Some((false, "请先选择源文件夹".into()));
            return;
        }
        if self.opts.output_file.as_os_str().is_empty() {
            self.banner = Some((false, "请先指定输出文件".into()));
            return;
        }
        match self.rows_text.trim().parse::<u32>() {
            Ok(n) if (1..=1_048_575).contains(&n) => self.opts.rows_per_sheet = n,
            _ => {
                self.banner = Some((
                    false,
                    "「每 sheet 条数」需要是 1 ~ 1048575 之间的整数".into(),
                ));
                return;
            }
        }

        let opts = self.opts.clone();
        let opts_bg = opts.clone();
        let (tx, rx) = channel::<UiMsg>();
        let ctx_bg = ctx.clone();

        std::thread::spawn(move || {
            let tx_prog = tx.clone();
            let ctx_prog = ctx_bg.clone();
            let result = merge(&opts_bg, move |p| {
                let _ = tx_prog.send(UiMsg::Progress(p));
                ctx_prog.request_repaint();
            });
            let _ = tx.send(UiMsg::Done(result));
            ctx_bg.request_repaint();
        });

        self.log.clear();
        self.push_log(format!("源文件夹：{}", opts.input_dir.display()));
        self.push_log(format!("输出文件：{}", opts.output_file.display()));
        self.push_log(format!("每 sheet：{} 条", opts.rows_per_sheet));
        self.push_log(format!(
            "每个 sheet 带表头：{}",
            if opts.header_on_each_sheet { "是" } else { "否" }
        ));
        self.push_log(format!(
            "移除子文件表头：{}",
            if opts.drop_file_headers { "是" } else { "否" }
        ));
        self.banner = None;
        self.progress = 0.0;
        self.status = "开始扫描…".into();
        self.rows_written = 0;
        self.done_index = 0;
        self.total_files = 0;
        self.rx = Some(rx);
    }

    // ---------------------------------------------------------- 收消息

    fn poll(&mut self, ctx: &egui::Context) {
        // 先收集消息，释放对 self.rx 的借用
        let mut msgs = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(m) = rx.try_recv() {
                msgs.push(m);
            }
        }

        let mut done: Option<Result<MergeSummary, String>> = None;
        for m in msgs {
            match m {
                UiMsg::Progress(p) => match p {
                    Progress::FoundFiles(n) => {
                        self.total_files = n;
                        self.status = format!("找到 {} 个 Excel 文件", n);
                        self.push_log(format!("扫描到 {} 个 Excel 文件", n));
                    }
                    Progress::Processing {
                        index,
                        total,
                        file,
                        rows_written,
                    } => {
                        self.done_index = index;
                        self.current_file = file.clone();
                        self.rows_written = rows_written;
                        self.progress = index as f32 / total.max(1) as f32 * 0.95;
                        self.status = format!("已处理 {}/{} 个文件", index, total);
                    }
                    Progress::NewSheet { name, index } => {
                        self.push_log(format!("新建 sheet：{}", name));
                        let _ = index;
                    }
                    Progress::Finished(s) => {
                        self.progress = 1.0;
                        self.push_log(format!(
                            "完成：{} 个文件 → {} 个 sheet / {} 行，耗时 {:.2} 秒",
                            s.files,
                            s.sheets,
                            s.rows,
                            s.elapsed_ms as f64 / 1000.0
                        ));
                    }
                },
                UiMsg::Done(r) => done = Some(r),
            }
        }

        if let Some(r) = done {
            match r {
                Ok(s) => {
                    self.progress = 1.0;
                    self.status = "合并完成".into();
                    self.banner = Some((
                        true,
                        format!(
                            "合并完成！{} 个文件，{} 个 sheet，{} 行数据，耗时 {:.2} 秒",
                            s.files,
                            s.sheets,
                            s.rows,
                            s.elapsed_ms as f64 / 1000.0
                        ),
                    ));
                }
                Err(e) => {
                    self.status = "出错".into();
                    self.push_log(format!("错误：{}", e.replace('\n', " ")));
                    self.banner = Some((false, e));
                }
            }
            self.rx = None;
            ctx.request_repaint();
        }
    }

    // ---------------------------------------------------------- 界面

    fn ui_top(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Excel 批量合并");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .color(Color32::GRAY)
                        .small(),
                );
            });
        });
        ui.label(
            RichText::new("选一个文件夹，把它里面所有 Excel 合并成一个 Excel")
                .color(Color32::from_gray(110)),
        );
    }

    fn ui_paths(&mut self, ui: &mut egui::Ui) {
        let busy = self.running();
        ui.add_enabled_ui(!busy, |ui| {
            egui::Grid::new("paths")
                .num_columns(3)
                .spacing([10.0, 10.0])
                .show(ui, |ui| {
                    ui.label("源文件夹");
                    let mut dir = self.opts.input_dir.display().to_string();
                    if ui
                        .add_sized(
                            [430.0, 26.0],
                            egui::TextEdit::singleline(&mut dir)
                                .hint_text("存放待合并 Excel 的文件夹"),
                        )
                        .changed()
                    {
                        self.opts.input_dir = PathBuf::from(dir);
                    }
                    if ui.button("浏览…").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .set_title("选择包含 Excel 的文件夹")
                            .pick_folder()
                        {
                            self.opts.input_dir = p;
                        }
                    }
                    ui.end_row();

                    ui.label("输出文件");
                    let mut out = self.opts.output_file.display().to_string();
                    if ui
                        .add_sized(
                            [430.0, 26.0],
                            egui::TextEdit::singleline(&mut out)
                                .hint_text("合并结果保存到哪里（.xlsx）"),
                        )
                        .changed()
                    {
                        self.opts.output_file = PathBuf::from(out);
                    }
                    if ui.button("另存为…").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .set_title("选择输出文件")
                            .add_filter("Excel 工作簿", &["xlsx"])
                            .set_file_name("合并结果.xlsx")
                            .save_file()
                        {
                            self.opts.output_file = p;
                        }
                    }
                    ui.end_row();
                });
        });
    }

    fn ui_options(&mut self, ui: &mut egui::Ui) {
        let busy = self.running();
        ui.add_enabled_ui(!busy, |ui| {
            // ---------- 每 sheet 容量 ----------
            ui.label(RichText::new("每个 sheet 放多少条数据").strong());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // 数值输入框
                let resp = ui.add_sized(
                    [110.0, 26.0],
                    egui::TextEdit::singleline(&mut self.rows_text),
                );
                if resp.changed() {
                    // 输入合法就立即同步到滑杆
                    if let Ok(n) = self.rows_text.trim().parse::<u32>() {
                        if (1..=1_048_575).contains(&n) {
                            self.slider = n;
                            self.opts.rows_per_sheet = n;
                        }
                    }
                }
                resp.on_hover_text("每个 sheet 最多放多少条数据（不含表头），写满自动新建下一个 sheet");

                // 滑杆（对数刻度，1 万 ~ 100 万）
                let mut v = self.slider.clamp(MIN_ROWS, MAX_ROWS);
                if ui
                    .add(
                        egui::Slider::new(&mut v, MIN_ROWS..=MAX_ROWS)
                            .logarithmic(true)
                            .show_value(false)
                            .clamping(egui::SliderClamping::Always),
                    )
                    .changed()
                {
                    self.slider = v;
                    self.rows_text = v.to_string();
                    self.opts.rows_per_sheet = v;
                }

                ui.label(
                    RichText::new(format!("≈ {}", human(v)))
                        .color(Color32::from_gray(110))
                        .small(),
                );
            });

            // 常用档位
            ui.horizontal(|ui| {
                ui.label(RichText::new("常用：").color(Color32::from_gray(120)).small());
                for preset in [5_000u32, 10_000, 50_000, 100_000, 500_000] {
                    let selected = self.rows_text.trim() == preset.to_string();
                    let btn = egui::Button::new(
                        RichText::new(human(preset)).small(),
                    )
                    .min_size(egui::vec2(56.0, 22.0));
                    let btn = if selected {
                        btn.fill(Color32::from_rgb(0xD6, 0xE8, 0xF7))
                    } else {
                        btn
                    };
                    if ui.add(btn).clicked() {
                        self.rows_text = preset.to_string();
                        self.slider = preset;
                        self.opts.rows_per_sheet = preset;
                    }
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("不分片").small())
                            .min_size(egui::vec2(60.0, 22.0)),
                    )
                    .on_hover_text("所有数据放进一个 sheet（注意别超过 104 万行）")
                    .clicked()
                {
                    self.rows_text = MAX_ROWS.to_string();
                    self.slider = MAX_ROWS;
                    self.opts.rows_per_sheet = MAX_ROWS;
                }
                ui.label(
                    RichText::new(format!("上限 {}", human(MAX_ROWS)))
                        .color(Color32::from_gray(140))
                        .small(),
                );
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label("sheet 名称前缀");
                ui.add_sized(
                    [80.0, 26.0],
                    egui::TextEdit::singleline(&mut self.opts.sheet_prefix),
                )
                .on_hover_text("生成的 sheet 形如「数据1」「数据2」");
                ui.separator();
                ui.checkbox(&mut self.opts.recursive, "包含子文件夹");
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.opts.header_on_each_sheet, "每个 sheet 都带表头")
                    .on_hover_text(
                        "勾选：每新建一个 sheet，第 1 行都重写一遍表头，方便单独查看\n\
                         不勾：表头只在整份结果的最开头出现一次，后续 sheet 第 1 行直接是数据",
                    );
                ui.separator();
                ui.checkbox(&mut self.opts.drop_file_headers, "移除子文件的表头")
                    .on_hover_text(
                        "勾选：每个子文件、每个 sheet 自己的表头行被跳过，不写进结果\n\
                         （只保留整份结果最开头的那一行基准表头）\n\
                         不勾：它们的表头行当普通数据原样写入（纯暴力合并）",
                    );
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let ready = !self.opts.input_dir.as_os_str().is_empty()
                    && !self.opts.output_file.as_os_str().is_empty();
                let btn = egui::Button::new(RichText::new("开始合并").strong().size(15.0))
                    .fill(Color32::from_rgb(0x2D, 0x7D, 0xD2))
                    .min_size(egui::vec2(120.0, 34.0));
                if ui.add_enabled(ready, btn).clicked() {
                    let ctx = ui.ctx().clone();
                    self.start(&ctx);
                }
                if !ready {
                    ui.label(
                        RichText::new("请先选好源文件夹和输出文件")
                            .color(Color32::from_rgb(0xC0, 0x39, 0x2B))
                            .small(),
                    );
                }
            });
        });
    }

    fn ui_progress(&mut self, ui: &mut egui::Ui) {
        if !self.running() && self.progress == 0.0 {
            return;
        }
        ui.add_space(8.0);
        ui.add(
            egui::ProgressBar::new(self.progress.clamp(0.0, 1.0))
                .show_percentage()
                .animate(self.running()),
        );
        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.status).strong());
            if self.running() {
                ui.label(
                    RichText::new(format!("（{}）", truncate(&self.current_file, 40)))
                        .color(Color32::GRAY)
                        .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("已写 {} 行", self.rows_written))
                            .color(Color32::GRAY)
                            .small(),
                    );
                });
            }
        });
    }

    fn ui_banner(&mut self, ui: &mut egui::Ui) {
        let Some((ok, msg)) = self.banner.clone() else {
            return;
        };
        ui.add_space(8.0);
        let color = if ok {
            Color32::from_rgb(0x1E, 0x7A, 0x3C)
        } else {
            Color32::from_rgb(0xC0, 0x39, 0x2B)
        };
        egui::Frame::none()
            .fill(if ok {
                Color32::from_rgb(0xEC, 0xF7, 0xEE)
            } else {
                Color32::from_rgb(0xFD, 0xEC, 0xEA)
            })
            .inner_margin(8.0)
            .rounding(4.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&msg).color(color).strong());
                });
                ui.horizontal(|ui| {
                    if ok {
                        if ui.small_button("打开文件").clicked() {
                            open_path(&self.opts.output_file, true);
                        }
                        if ui.small_button("打开所在文件夹").clicked() {
                            if let Some(p) = self.opts.output_file.parent() {
                                open_path(p, false);
                            }
                        }
                    }
                    if ui.small_button("关闭").clicked() {
                        self.banner = None;
                    }
                });
            });
    }

    fn ui_log(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.label(RichText::new("日志").strong());
        egui::ScrollArea::vertical()
            .max_height(170.0)
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for line in &self.log {
                    ui.label(RichText::new(line).monospace().small());
                }
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
        if self.running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(10.0);
            self.ui_top(ui);
            ui.add_space(10.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.ui_paths(ui);
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            self.ui_options(ui);
            self.ui_progress(ui);
            self.ui_banner(ui);
            ui.add_space(8.0);
            ui.separator();
            self.ui_log(ui);
        });
    }
}

fn truncate(s: &str, n: usize) -> String {
    let cs: Vec<char> = s.chars().collect();
    if cs.len() <= n {
        s.to_string()
    } else {
        format!("{}…", cs[..n].iter().collect::<String>())
    }
}

/// 在资源管理器里打开文件或文件夹
fn open_path(path: &std::path::Path, reveal: bool) {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("explorer");
        if reveal {
            cmd.arg(format!("/select,{}", path.display()));
        } else {
            cmd.arg(path);
        }
        let _ = cmd.spawn();
    }
    #[cfg(not(windows))]
    {
        let target = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent().unwrap_or(path).to_path_buf()
        };
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}

/// 加载系统中文字体（egui 自带字体没有中文，不加载会显示成方块）
fn install_chinese_fonts(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyhl.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
        r"C:\Windows\Fonts\Deng.ttf",
    ];
    for p in candidates {
        if let Ok(bytes) = std::fs::read(p) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "cjk".to_owned(),
                egui::FontData::from_owned(bytes),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "cjk".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("cjk".to_owned());
            ctx.set_fonts(fonts);
            return;
        }
    }
}

/// 窗口图标（编译期把 png 嵌进 exe，运行时无需外部文件）
fn load_window_icon() -> Option<egui::IconData> {
    const PNG: &[u8] = include_bytes!("../assets/icon.png");
    let img = image::load_from_memory(PNG).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    })
}

pub fn run_gui() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([760.0, 600.0])
        .with_min_inner_size([660.0, 500.0])
        .with_title("Excel 批量合并");

    // 任务栏 / 标题栏图标
    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Excel 批量合并",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
