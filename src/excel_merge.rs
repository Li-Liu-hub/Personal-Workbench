//! Folder-to-workbook merge service, independent of the desktop UI.
//! Reads one calamine Range at a time (a whole sheet in memory), writes rows to disk.
#[cfg(test)]
pub(crate) mod tests;
mod writer;

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use crate::error::{DeskError, Result};
use calamine::{Data, Reader, open_workbook_auto};
use writer::SheetWriter;

pub const MAX_DATA_ROWS: u32 = 1_048_575;

#[derive(Clone, Debug)]
pub struct MergeOptions {
    pub input_dir: PathBuf,
    pub output_file: PathBuf,
    pub rows_per_sheet: u32,
    pub recursive: bool,
    pub header_on_each_sheet: bool,
    pub drop_file_headers: bool,
}

impl Default for MergeOptions {
    fn default() -> Self {
        Self {
            input_dir: PathBuf::new(),
            output_file: PathBuf::new(),
            rows_per_sheet: 50_000,
            recursive: false,
            header_on_each_sheet: false,
            drop_file_headers: false,
        }
    }
}

impl MergeOptions {
    pub fn validate(&self) -> Result<()> {
        if !self.input_dir.is_dir() {
            return invalid("请选择有效的源文件夹");
        }
        if self.output_file.file_name().is_none()
            || !self
                .output_file
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("xlsx"))
        {
            return invalid("输出文件必须是 .xlsx 格式");
        }
        if self.output_file.is_dir() {
            return invalid("输出路径是文件夹，请指定 .xlsx 文件名");
        }
        if self
            .output_file
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return invalid("输出文件不能是符号链接，请选择普通文件路径");
        }
        if !(1..=MAX_DATA_ROWS).contains(&self.rows_per_sheet) {
            return invalid("每表数据条数必须在 1～1,048,575 之间");
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct MergeSummary {
    pub files: usize,
    pub source_sheets: usize,
    pub output_sheets: usize,
    pub rows: usize,
    pub elapsed_ms: u128,
    pub output_file: PathBuf,
}

#[derive(Clone, Debug)]
pub enum Progress {
    FoundFiles(usize),
    Reading {
        completed: usize,
        total: usize,
        file: String,
        sheet: String,
        rows: usize,
    },
    NewSheet(usize),
    Saving,
}

fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(DeskError::InvalidInput(message.into()))
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return invalid("合并已取消，原输出文件保留");
    }
    Ok(())
}

fn path_key(path: &Path) -> Result<PathBuf> {
    let path = path.canonicalize()?;
    #[cfg(windows)]
    let path = PathBuf::from(path.to_string_lossy().to_lowercase());
    Ok(path)
}

fn scan_files(opts: &MergeOptions, cancel: &AtomicBool) -> Result<Vec<PathBuf>> {
    let output = if opts.output_file.exists() {
        Some(path_key(&opts.output_file)?)
    } else {
        None
    };
    let depth = if opts.recursive { usize::MAX } else { 1 };
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(&opts.input_dir)
        .follow_links(false)
        .max_depth(depth)
    {
        check_cancel(cancel)?;
        let entry =
            entry.map_err(|error| DeskError::InvalidPath(format!("扫描文件夹失败：{error}")))?;
        let path = entry.path();
        if !entry.file_type().is_file()
            || path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("~$"))
        {
            continue;
        }
        let ext = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "xlsx" | "xlsm" | "xls" | "xlsb" | "ods") {
            if let Some(output) = &output
                && &path_key(path)? == output
            {
                continue;
            }
            files.push(path.to_path_buf());
        }
    }
    files.sort_by_cached_key(|path| (path.to_string_lossy().to_lowercase(), path.clone()));
    Ok(files)
}

fn width(row: &[Data]) -> usize {
    row.iter()
        .rposition(|cell| {
            !matches!(cell, Data::Empty) && !matches!(cell, Data::String(text) if text.is_empty())
        })
        .map_or(0, |index| index + 1)
}

/// Blocking service: the caller owns scheduling and cancellation.
/// Publish a complete workbook atomically, keeping any previous output on failure.
pub fn merge(
    opts: &MergeOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<MergeSummary> {
    opts.validate()?;
    check_cancel(cancel)?;
    let started = Instant::now();
    let files = scan_files(opts, cancel)?;
    if files.is_empty() {
        return invalid("源文件夹内没有可合并的 Excel 文件（会排除输出文件和 ~$ 临时文件）");
    }
    progress(Progress::FoundFiles(files.len()));
    let mut writer: Option<SheetWriter> = None;
    let mut source_sheets = 0;
    for (completed, path) in files.iter().enumerate() {
        check_cancel(cancel)?;
        let mut book = open_workbook_auto(path).map_err(|error| {
            DeskError::InvalidInput(format!("无法打开 {}：{error}", path.display()))
        })?;
        let file = path
            .strip_prefix(&opts.input_dir)
            .unwrap_or(path)
            .display()
            .to_string();
        for sheet in book.sheet_names().to_vec() {
            check_cancel(cancel)?;
            progress(Progress::Reading {
                completed,
                total: files.len(),
                file: file.clone(),
                sheet: sheet.clone(),
                rows: writer.as_ref().map_or(0, |writer| writer.rows),
            });
            let range = book.worksheet_range(&sheet).map_err(|error| {
                DeskError::InvalidInput(format!("读取 {file} / {sheet} 失败：{error}"))
            })?;
            // Keep absolute column positions: Range may start at B/C, not A.
            let (start_row, start_col) = range.start().unwrap_or_default();
            let mut first = true;
            for (offset, cells) in range.rows().enumerate() {
                check_cancel(cancel)?;
                let row_width = width(cells);
                if row_width == 0 {
                    continue;
                }
                let row = if start_col == 0 {
                    std::borrow::Cow::Borrowed(&cells[..row_width])
                } else {
                    let mut row = vec![Data::Empty; start_col as usize];
                    row.extend_from_slice(&cells[..row_width]);
                    std::borrow::Cow::Owned(row)
                };
                let location = || {
                    format!(
                        "{file} / {sheet} / 第 {} 行",
                        start_row as usize + offset + 1
                    )
                };
                if first {
                    source_sheets += 1;
                    first = false;
                    if writer.is_none() {
                        let next = SheetWriter::new(&row, opts)?;
                        progress(Progress::NewSheet(1));
                        writer = Some(next);
                        continue;
                    }
                    let current = writer.as_ref().unwrap();
                    if row.len() != current.columns {
                        return invalid(format!(
                            "表头列数不一致：{}，基准 {} 列，当前 {} 列",
                            location(),
                            current.columns,
                            row.len()
                        ));
                    }
                    if opts.drop_file_headers {
                        continue;
                    }
                }
                let current = writer.as_mut().unwrap();
                if row.len() > current.columns {
                    return invalid(format!(
                        "数据超出表头列数：{}，基准 {} 列，当前 {} 列",
                        location(),
                        current.columns,
                        row.len()
                    ));
                }
                if current
                    .write_row(&row)
                    .map_err(|error| DeskError::InvalidInput(format!("{}：{error}", location())))?
                {
                    progress(Progress::NewSheet(current.sheets));
                }
                if current.rows.is_multiple_of(2048) {
                    progress(Progress::Reading {
                        completed,
                        total: files.len(),
                        file: file.clone(),
                        sheet: sheet.clone(),
                        rows: current.rows,
                    });
                }
            }
        }
        progress(Progress::Reading {
            completed: completed + 1,
            total: files.len(),
            file,
            sheet: String::new(),
            rows: writer.as_ref().map_or(0, |writer| writer.rows),
        });
    }
    let mut writer = writer.ok_or_else(|| {
        DeskError::InvalidInput("所有工作表均为空，没有可写出的表头或数据".into())
    })?;
    check_cancel(cancel)?;
    progress(Progress::Saving);
    let parent = opts
        .output_file
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    writer.save(temporary.as_file_mut())?;
    temporary.as_file().sync_all()?;
    check_cancel(cancel)?;
    temporary.persist(&opts.output_file).map_err(|error| {
        DeskError::InvalidPath(format!(
            "保存结果失败（原文件保留），请关闭占用文件的程序后重试：{}",
            error.error
        ))
    })?;
    Ok(MergeSummary {
        files: files.len(),
        source_sheets,
        output_sheets: writer.sheets,
        rows: writer.rows,
        elapsed_ms: started.elapsed().as_millis(),
        output_file: opts.output_file.clone(),
    })
}
