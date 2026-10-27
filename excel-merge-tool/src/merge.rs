//! Excel 批量合并 —— 核心逻辑（真流式读 + 真流式写）
//!
//! 设计要点：
//! - **流式读**：用 calamine 的迭代式接口逐行取数据，不把整表读进内存
//! - **流式写**：用 rust_xlsxwriter 的 `constant_memory` 模式，行写完即落盘
//! - **逐文件流水线**：读一个文件 → 立即写进当前 sheet → 释放该文件的内存
//!   所以峰值内存只与「单个最大的文件」有关，与文件总数无关
//!
//! 规则（刻意保持简单，不做纠错）：
//! - **遍历每个文件里的全部 sheet**（按文件顺序，文件内按 sheet 顺序），
//!   所有 sheet 的数据打平汇入同一组输出 sheet
//! - 只有「第一个文件的第一个 sheet」的第一行被当作基准表头
//! - 表头在结果最开头写**唯一一次**；后续新开的 sheet 默认**不写表头**
//!   （可通过 `MergeOptions.header_on_each_sheet` 打开，让每个 sheet 都带一行表头）
//! - 各 sheet 自己的表头行默认当普通数据写入；
//!   `MergeOptions.drop_file_headers = true` 时跳过（不进入结果）
//! - 各 sheet 列数不一致 → 直接报错并指明是哪个文件、哪个 sheet

use std::path::{Path, PathBuf};

use calamine::{open_workbook_auto, Data, Reader};
use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Workbook, Worksheet};

/// Excel 单 sheet 行数上限（含表头）
const EXCEL_MAX_ROWS: u32 = 1_048_576;

/// 合并完成后回报的结果
#[derive(Debug, Clone, Default)]
pub struct MergeSummary {
    pub files: usize,
    pub rows: usize,
    pub sheets: usize,
    pub elapsed_ms: u128,
}

/// 进度事件（GUI 通过它刷新界面）
#[derive(Debug, Clone)]
pub enum Progress {
    /// 扫描完成，共找到多少个 Excel
    FoundFiles(usize),
    /// 正在处理第 index / total 个文件
    Processing {
        index: usize,
        total: usize,
        file: String,
        rows_written: usize,
    },
    /// 已开始一个新的 sheet
    NewSheet { name: String, index: usize },
    /// 任务完成
    Finished(MergeSummary),
}

/// 合并参数
#[derive(Debug, Clone)]
pub struct MergeOptions {
    pub input_dir: PathBuf,
    pub output_file: PathBuf,
    /// 每个 sheet 最多放多少条数据（不含表头）
    pub rows_per_sheet: u32,
    /// 是否递归扫描子文件夹
    pub recursive: bool,
    pub sheet_prefix: String,
    /// 是否**每个 sheet 都写一行表头**
    ///
    /// - `false`（默认）：表头只在整份结果的最开头写一次，后续 sheet 第一行直接是数据
    /// - `true`：每个 sheet 的第 0 行都重写一遍表头，方便单独查看/筛选每个 sheet
    pub header_on_each_sheet: bool,
    /// 是否**移除每个子文件自己的表头行**
    ///
    /// - `false`（默认）：各文件的第一行原样当普通数据写入（纯暴力合并）
    /// - `true`：每个文件（除第一个）的第一行被视为它自己的表头，**跳过不写入**
    pub drop_file_headers: bool,
}

impl Default for MergeOptions {
    fn default() -> Self {
        Self {
            input_dir: PathBuf::new(),
            output_file: PathBuf::new(),
            rows_per_sheet: 50_000,
            recursive: false,
            sheet_prefix: "数据".to_string(),
            header_on_each_sheet: false,
            drop_file_headers: false,
        }
    }
}

// ------------------------------------------------------------ 文件扫描

fn is_excel(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("xlsx") | Some("xlsm") | Some("xls") | Some("xlsb") | Some("ods")
    )
}

/// Excel 打开时会生成 `~$xxx.xlsx` 临时文件，需要排除
fn is_temp(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('~'))
        .unwrap_or(false)
}

/// 扫描文件夹，返回全部 Excel 文件（按路径名排序，保证顺序稳定可复现）
pub fn scan_files(dir: &Path, recursive: bool) -> Result<Vec<PathBuf>, String> {
    if !dir.exists() {
        return Err(format!("文件夹不存在：{}", dir.display()));
    }
    if !dir.is_dir() {
        return Err(format!("这不是一个文件夹：{}", dir.display()));
    }

    let mut out = Vec::new();
    walk(dir, recursive, &mut out)?;
    out.retain(|p| is_excel(p) && !is_temp(p));
    out.sort_by(|a, b| {
        a.to_string_lossy()
            .to_lowercase()
            .cmp(&b.to_string_lossy().to_lowercase())
    });
    Ok(out)
}

fn walk(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("读取文件夹失败 {}：{}", dir.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                // 子目录读失败不致命，跳过即可
                let _ = walk(&path, recursive, out);
            }
        } else {
            out.push(path);
        }
    }
    Ok(())
}

// ------------------------------------------------------------ 单元格转换

/// calamine 的 `Data` → `String`；返回 `None` 表示空单元格
fn to_text(cell: &Data) -> Option<String> {
    match cell {
        Data::Empty => None,
        Data::String(s) => {
            if s.is_empty() {
                None
            } else {
                Some(s.clone())
            }
        }
        Data::Float(f) => {
            // 整数不带小数点，避免 1234 变成 1234.0
            if f.fract() == 0.0 && f.abs() < 1e15 {
                Some(format!("{}", *f as i64))
            } else {
                Some(format!("{}", f))
            }
        }
        Data::Int(i) => Some(format!("{}", i)),
        Data::Bool(b) => Some(if *b { "TRUE" } else { "FALSE" }.to_string()),
        Data::DateTime(dt) => Some(format!("{}", dt.as_f64())),
        Data::DateTimeIso(s) => Some(s.clone()),
        Data::DurationIso(s) => Some(s.clone()),
        Data::Error(e) => Some(format!("#ERR:{:?}", e)),
    }
}

/// 判断字符串看起来是否是一个数字（用于决定写出类型）
///
/// 前导 0 的编码（007、00123）保持文本，避免丢前导零
fn as_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if t.len() > 1 && t.starts_with('0') && !t.starts_with("0.") {
        return None;
    }
    t.parse::<f64>().ok()
}

/// 一行实际有内容的列数（去掉尾部空列）
fn row_width(values: &[Option<String>]) -> usize {
    values
        .iter()
        .rposition(|v| v.is_some())
        .map(|i| i + 1)
        .unwrap_or(0)
}

// ------------------------------------------------------------ 写出器

/// 流式写出器：负责管理「当前 sheet 写满就换下一个」的逻辑
///
/// 表头取自第一个文件的第一行，保存在 `header` 里：
/// - `header_on_each_sheet = false`（默认）：只在第一个 sheet 的第 0 行写一次
/// - `header_on_each_sheet = true`：每新建一个 sheet 都在第 0 行重写一遍表头
struct SheetWriter {
    workbook: Workbook,
    header_fmt: Format,
    text_fmt: Format,
    num_fmt: Format,
    prefix: String,
    per_sheet: u32,
    /// 当前 sheet 的下标（0 起）
    index: u32,
    /// 当前 sheet 已写的行数
    rows_in_sheet: u32,
    /// 累计数据行数
    total_rows: usize,
    col_count: usize,
    /// 基准表头（来自第一个文件的第一行）
    header: Vec<String>,
    /// 是否每个 sheet 都写表头
    header_on_each_sheet: bool,
    /// 是否已经写过至少一次表头（第一个 sheet 一定写）
    wrote_first_header: bool,
}

impl SheetWriter {
    #[allow(clippy::too_many_arguments)]
    fn new(
        prefix: String,
        per_sheet: u32,
        header: Vec<String>,
        header_on_each_sheet: bool,
        header_fmt: Format,
        text_fmt: Format,
        num_fmt: Format,
    ) -> Self {
        Self {
            workbook: Workbook::new(),
            header_fmt,
            text_fmt,
            num_fmt,
            prefix,
            per_sheet,
            index: 0,
            rows_in_sheet: 0,
            total_rows: 0,
            col_count: header.len(),
            header,
            header_on_each_sheet,
            wrote_first_header: false,
        }
    }

    /// 新建一个 sheet。
    ///
    /// 是否写表头由 `header_on_each_sheet` 决定：
    /// - 第一个 sheet **一定写**表头（它是整份结果的表头）
    /// - 之后新开的 sheet，勾选了「每个 sheet 带表头」才写
    fn open_sheet(&mut self) -> Result<String, String> {
        let name = format!("{}{}", self.prefix, self.index + 1);
        let want_header = !self.wrote_first_header || self.header_on_each_sheet;

        // `ws` 借用了 self.workbook，所以表头所需的数据先取出，避免同时不可变借用 self
        let header = std::mem::take(&mut self.header);
        let header_fmt = self.header_fmt.clone();

        let ws = self.workbook.add_worksheet_with_constant_memory();
        let res = (|| -> Result<(), String> {
            ws.set_name(&name)
                .map_err(|e| format!("设置 sheet 名失败：{}", e))?;

            if want_header {
                for (c, h) in header.iter().enumerate() {
                    ws.write_string_with_format(0, c as u16, h, &header_fmt)
                        .map_err(|e| format!("写表头失败（{}）：{}", name, e))?;
                    // 列宽：按表头字符数给个合理值
                    let w = (h.chars().count() as f64 * 1.8 + 4.0).clamp(8.0, 36.0);
                    let _ = ws.set_column_width(c as u16, w);
                }
                // 冻结表头，方便滚动查看
                let _ = ws.set_freeze_panes(1, 0);
            }
            Ok(())
        })();

        // 无论成败都把表头放回去
        self.header = header;
        res?;

        if want_header {
            // 表头占了第 0 行，数据从第 1 行开始
            self.rows_in_sheet = 1;
            self.wrote_first_header = true;
        } else {
            // 后续 sheet 没有表头，数据从第 0 行开始
            self.rows_in_sheet = 0;
        }
        Ok(name)
    }

    /// 当前 sheet 是否已写满（需要换新 sheet）
    ///
    /// `rows_in_sheet` 里已包含第一个 sheet 的表头那一行，
    /// 所以这里判断的是「已占用行数」是否达到 per_sheet。
    fn is_full(&self) -> bool {
        self.rows_in_sheet >= self.per_sheet
    }

    /// 推进到下一个 sheet
    fn advance_index(&mut self) {
        self.index += 1;
    }

    /// 把一行数据写进**当前** sheet（调用方需保证 sheet 已存在且未满）
    fn write_row(&mut self, values: &[Option<String>]) -> Result<(), String> {
        let ws: &mut Worksheet = self
            .workbook
            .worksheet_from_index(self.index as usize)
            .map_err(|e| format!("内部错误：找不到当前 sheet —— {}", e))?;

        // 行号 = 当前已占用行数（第一个 sheet 时已把表头算进 rows_in_sheet）
        let r = self.rows_in_sheet;
        for c in 0..self.col_count {
            let Some(v) = values.get(c).and_then(|x| x.as_ref()) else {
                continue;
            };
            if let Some(n) = as_number(v) {
                ws.write_number_with_format(r, c as u16, n, &self.num_fmt)
                    .map_err(|e| format!("写数据失败：{}", e))?;
            } else {
                ws.write_string_with_format(r, c as u16, v, &self.text_fmt)
                    .map_err(|e| format!("写数据失败：{}", e))?;
            }
        }

        self.rows_in_sheet += 1;
        self.total_rows += 1;
        Ok(())
    }

    fn save(&mut self, path: &Path) -> Result<(), String> {
        self.workbook
            .save(path)
            .map_err(|e| format!("保存文件失败 {}：{}", path.display(), e))
    }

    fn sheet_count(&mut self) -> usize {
        self.workbook.worksheets().len()
    }
}

// ------------------------------------------------------------ 合并主流程

/// 流式合并：读一个文件 → 立刻写出 → 释放
///
/// `on_progress` 在关键节点被调用，用于刷新 GUI。
/// 本函数会阻塞到全部完成，调用方应放在后台线程执行。
pub fn merge<F>(opts: &MergeOptions, mut on_progress: F) -> Result<MergeSummary, String>
where
    F: FnMut(Progress),
{
    let started = std::time::Instant::now();

    // ---- 1. 扫描文件 ----
    let files = scan_files(&opts.input_dir, opts.recursive)?;
    if files.is_empty() {
        return Err(format!(
            "这个文件夹里没有找到 Excel 文件：{}",
            opts.input_dir.display()
        ));
    }
    on_progress(Progress::FoundFiles(files.len()));

    // ---- 2. 准备输出目录 ----
    if let Some(parent) = opts.output_file.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建输出目录失败 {}：{}", parent.display(), e))?;
        }
    }

    let header_fmt = Format::new()
        .set_bold()
        .set_background_color(0xDCE6F1)
        .set_border(FormatBorder::Thin)
        .set_align(FormatAlign::Center);
    let text_fmt = Format::new().set_border(FormatBorder::Thin);
    let num_fmt = Format::new().set_border(FormatBorder::Thin);

    // 每 sheet 容量：扣掉表头 1 行，且不超 Excel 上限
    let per_sheet = opts.rows_per_sheet.clamp(1, EXCEL_MAX_ROWS - 1);

    // 基准表头（来自第一个文件的第一行），由写出器持有
    let mut writer: Option<SheetWriter> = None;

    // ---- 3. 逐文件、逐 sheet：读一个、写一个 ----
    for (i, path) in files.iter().enumerate() {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<未知文件>")
            .to_string();

        // 先把 sheet 名单取出来（book 后面要被 worksheet_range 借用）
        let sheet_names: Vec<String> = {
            let book = open_workbook_auto(path)
                .map_err(|e| format!("打开文件失败：{} —— {}", file_name, e))?;
            let names = book.sheet_names().to_vec();
            names
        };
        if sheet_names.is_empty() {
            return Err(format!("文件里没有任何 sheet：{}", file_name));
        }

        // ---- 遍历本文件的每一个 sheet ----
        for sheet_name in &sheet_names {
            let mut book = open_workbook_auto(path)
                .map_err(|e| format!("打开文件失败：{} —— {}", file_name, e))?;

            // 先把 Range 绑到局部变量，避免临时值在 rows() 借用期间被释放
            let range = book
                .worksheet_range(sheet_name)
                .map_err(|e| format!("读取 sheet 失败：{} / {} —— {}", file_name, sheet_name, e))?;
            let mut rows = range.rows();

            // 基准表头：取「第一个文件的第一个 sheet」的第一行
            if writer.is_none() {
                let first = match rows.next() {
                    Some(r) => r,
                    None => {
                        // 首个 sheet 是空的，继续往后找
                        continue;
                    }
                };
                let mut header: Vec<String> =
                    first.iter().map(|c| to_text(c).unwrap_or_default()).collect();
                // 去掉尾部全空的列
                while header.last().map(|s| s.trim().is_empty()).unwrap_or(false) {
                    header.pop();
                }
                if header.is_empty() {
                    return Err(format!(
                        "第一个 sheet 没有可用的表头：{} / {}",
                        file_name, sheet_name
                    ));
                }
                let mut w = SheetWriter::new(
                    opts.sheet_prefix.clone(),
                    per_sheet,
                    header,
                    opts.header_on_each_sheet,
                    header_fmt.clone(),
                    text_fmt.clone(),
                    num_fmt.clone(),
                );
                // 第一个 sheet 一定写表头
                let name = w.open_sheet()?;
                on_progress(Progress::NewSheet {
                    name,
                    index: 1,
                });
                writer = Some(w);
            } else {
                // 非首个 sheet：第一行是它自己的表头，按开关决定是否跳过
                if let Some(first) = rows.next() {
                    if !opts.drop_file_headers {
                        // 不剔除 → 把这一行当普通数据写进去
                        let values: Vec<Option<String>> = first.iter().map(to_text).collect();
                        if !values.iter().all(|v| v.is_none()) {
                            let w = writer.as_mut().unwrap();
                            let col_count = w.col_count;
                            let width = row_width(&values);
                            if width != col_count {
                                return Err(format!(
                                    "列数不一致，已停止合并。\n\n\
                                     基准表头：{} 列\n\
                                     问题文件：{} / {}\n\
                                     问题位置：第 1 行（该 sheet 的表头行）\n\
                                     该行实际：{} 列\n\n\
                                     请确认这批 Excel 的格式是否一致。",
                                    col_count, file_name, sheet_name, width
                                ));
                            }
                            if w.is_full() {
                                w.advance_index();
                                let name = w.open_sheet()?;
                                on_progress(Progress::NewSheet {
                                    name,
                                    index: w.index as usize + 1,
                                });
                            }
                            w.write_row(&values)?;
                        }
                    }
                }
            }

            let w = writer.as_mut().unwrap();
            let col_count = w.col_count;
            let mut line_no: u64 = 1; // 该 sheet 的表头行已消费
            let mut checked = false;

            for row in rows {
                line_no += 1;
                let values: Vec<Option<String>> = row.iter().map(to_text).collect();
                // 整行空 → 跳过
                if values.iter().all(|v| v.is_none()) {
                    continue;
                }

                // 列数校验：只看本 sheet 的第一个有效数据行
                if !checked {
                    let width = row_width(&values);
                    if width != col_count {
                        return Err(format!(
                            "列数不一致，已停止合并。\n\n\
                             基准表头：{} 列\n\
                             问题文件：{} / {}\n\
                             问题位置：数据第 {} 行\n\
                             该行实际：{} 列\n\n\
                             请确认这批 Excel 的格式是否一致。",
                            col_count,
                            file_name,
                            sheet_name,
                            line_no - 1,
                            width
                        ));
                    }
                    checked = true;
                }

                // 先看当前 sheet 是否已写满：满了就换新 sheet
                // （新 sheet 是否带表头由 header_on_each_sheet 决定）
                if w.is_full() {
                    w.advance_index();
                    let name = w.open_sheet()?;
                    on_progress(Progress::NewSheet {
                        name,
                        index: w.index as usize + 1,
                    });
                }

                w.write_row(&values)?;
            }

            // 本 sheet 读完，立即释放它占的内存
            drop(book);
        }

        on_progress(Progress::Processing {
            index: i + 1,
            total: files.len(),
            file: file_name,
            rows_written: writer.as_ref().map(|w| w.total_rows).unwrap_or(0),
        });
    }

    let mut w = writer.ok_or_else(|| "没有可写出的数据".to_string())?;
    w.save(&opts.output_file)?;

    let summary = MergeSummary {
        files: files.len(),
        rows: w.total_rows,
        sheets: w.sheet_count(),
        elapsed_ms: started.elapsed().as_millis(),
    };
    on_progress(Progress::Finished(summary.clone()));
    Ok(summary)
}
