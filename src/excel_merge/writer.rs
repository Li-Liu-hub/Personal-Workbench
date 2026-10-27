use super::MergeOptions;
use crate::error::{DeskError, Result};
use calamine::Data;
use rust_xlsxwriter::{Format, Workbook, Worksheet, XlsxError};
use std::fs::File;

pub(super) struct SheetWriter {
    workbook: Workbook,
    formats: CellFormats,
    header: Vec<Data>,
    per_sheet: u32,
    repeat_header: bool,
    data_in_sheet: u32,
    next_row: u32,
    pub columns: usize,
    pub sheets: usize,
    pub rows: usize,
}

impl SheetWriter {
    pub fn new(header: &[Data], opts: &MergeOptions) -> Result<Self> {
        if header.len() > 16_384 {
            return Err(DeskError::InvalidInput(
                "表头超过 Excel 的 16,384 列上限".into(),
            ));
        }
        let mut writer = Self {
            workbook: Workbook::new(),
            formats: CellFormats::new(),
            header: header.to_vec(),
            columns: header.len(),
            per_sheet: opts.rows_per_sheet,
            repeat_header: opts.header_on_each_sheet,
            data_in_sheet: 0,
            next_row: 0,
            sheets: 0,
            rows: 0,
        };
        writer.open_sheet().map_err(write_error)?;
        Ok(writer)
    }

    fn open_sheet(&mut self) -> std::result::Result<(), XlsxError> {
        let header = self.sheets == 0 || self.repeat_header;
        self.sheets += 1;
        let sheet = self.workbook.add_worksheet_with_constant_memory();
        sheet.set_name(format!("数据{}", self.sheets))?;
        if header {
            for (column, value) in self.header.iter().enumerate() {
                write_cell(sheet, 0, column as u16, value, &self.formats, true)?;
            }
            sheet.set_freeze_panes(1, 0)?;
        }
        self.data_in_sheet = 0;
        self.next_row = u32::from(header);
        Ok(())
    }

    /// Returns whether a new output sheet was opened. Capacity counts data only.
    pub fn write_row(&mut self, cells: &[Data]) -> Result<bool> {
        let next = self.data_in_sheet == self.per_sheet;
        if next {
            self.open_sheet().map_err(write_error)?;
        }
        let sheet = self
            .workbook
            .worksheet_from_index(self.sheets - 1)
            .map_err(write_error)?;
        for (column, value) in cells.iter().enumerate() {
            write_cell(
                sheet,
                self.next_row,
                column as u16,
                value,
                &self.formats,
                false,
            )
            .map_err(write_error)?;
        }
        self.next_row += 1;
        self.data_in_sheet += 1;
        self.rows += 1;
        Ok(next)
    }

    pub fn save(&mut self, file: &mut File) -> Result<()> {
        self.workbook.save_to_writer(file).map_err(write_error)
    }
}

fn write_error(error: XlsxError) -> DeskError {
    DeskError::InvalidInput(format!("写入工作簿失败：{error}"))
}

/// Preserve text cells, especially long IDs and leading zeros; never infer formulas.
struct CellFormats {
    plain: [Format; 2],
    date: [Format; 2],
    duration: [Format; 2],
}

impl CellFormats {
    fn new() -> Self {
        let plain = [
            Format::new(),
            Format::new().set_bold().set_background_color(0xDCE6F1),
        ];
        Self {
            date: plain
                .clone()
                .map(|format| format.set_num_format("yyyy-mm-dd hh:mm:ss.000")),
            duration: plain
                .clone()
                .map(|format| format.set_num_format("[h]:mm:ss.000")),
            plain,
        }
    }
}

fn write_cell(
    sheet: &mut Worksheet,
    row: u32,
    column: u16,
    value: &Data,
    formats: &CellFormats,
    header: bool,
) -> std::result::Result<(), XlsxError> {
    let index = usize::from(header);
    let format = &formats.plain[index];
    match value {
        Data::Empty => {}
        Data::String(value) | Data::DateTimeIso(value) | Data::DurationIso(value) => {
            sheet.write_string_with_format(row, column, value, format)?;
        }
        Data::Int(value) => {
            sheet.write_number_with_format(row, column, *value as f64, format)?;
        }
        Data::Float(value) => {
            sheet.write_number_with_format(row, column, *value, format)?;
        }
        Data::Bool(value) => {
            sheet.write_boolean_with_format(row, column, *value, format)?;
        }
        Data::DateTime(value) if value.is_duration() => {
            sheet.write_number_with_format(
                row,
                column,
                value.as_f64(),
                &formats.duration[index],
            )?;
        }
        Data::DateTime(value) => {
            if let Some(datetime) = value.as_datetime() {
                sheet.write_datetime_with_format(row, column, datetime, &formats.date[index])?;
            } else {
                sheet.write_string_with_format(row, column, value.to_string(), format)?;
            }
        }
        Data::Error(value) => {
            sheet.write_string_with_format(row, column, value.to_string(), format)?;
        }
    }
    Ok(())
}
