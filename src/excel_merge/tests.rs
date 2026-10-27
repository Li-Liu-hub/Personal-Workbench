use super::*;
use rust_xlsxwriter::{Format, Workbook};

pub(crate) fn fixture(path: &Path, sheets: &[&[&[&str]]]) {
    let mut workbook = Workbook::new();
    for rows in sheets {
        let sheet = workbook.add_worksheet();
        for (r, row) in rows.iter().enumerate() {
            for (c, value) in row.iter().enumerate() {
                sheet.write_string(r as u32, c as u16, *value).unwrap();
            }
        }
    }
    workbook.save(path).unwrap();
}

fn options(dir: &Path) -> MergeOptions {
    MergeOptions {
        input_dir: dir.to_path_buf(),
        output_file: dir.join("result.xlsx"),
        rows_per_sheet: 2,
        ..Default::default()
    }
}

fn read(path: &Path) -> Vec<Vec<Vec<Data>>> {
    let mut workbook = open_workbook_auto(path).unwrap();
    workbook
        .sheet_names()
        .to_vec()
        .iter()
        .map(|name| {
            workbook
                .worksheet_range(name)
                .unwrap()
                .rows()
                .map(|row| row.to_vec())
                .collect()
        })
        .collect()
}

#[test]
fn excel_merge_all_header_combinations_keep_order_and_exact_data_capacity() {
    for repeat in [false, true] {
        for drop_headers in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            fixture(
                &dir.path().join("a.xlsx"),
                &[&[&["编号"], &["a"], &["b"]], &[&["编号"], &["c"]]],
            );
            fixture(&dir.path().join("b.xlsx"), &[&[&["编号"], &["d"]]]);
            let mut opts = options(dir.path());
            opts.header_on_each_sheet = repeat;
            opts.drop_file_headers = drop_headers;
            let report = merge(&opts, &AtomicBool::new(false), |_| {}).unwrap();
            assert_eq!((report.files, report.source_sheets), (2, 3));
            let expected = if drop_headers {
                vec!["a", "b", "c", "d"]
            } else {
                vec!["a", "b", "编号", "c", "编号", "d"]
            };
            assert_eq!(report.rows, expected.len());
            assert_eq!(report.output_sheets, expected.len().div_ceil(2));
            let mut actual = Vec::new();
            for (index, mut sheet) in read(&opts.output_file).into_iter().enumerate() {
                if index == 0 || repeat {
                    assert_eq!(sheet.remove(0)[0], Data::String("编号".into()));
                }
                assert!(sheet.len() <= 2);
                actual.extend(sheet.into_iter().map(|row| row[0].to_string()));
            }
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn excel_merge_capacity_one_writes_no_header_only_leading_sheet() {
    let dir = tempfile::tempdir().unwrap();
    fixture(&dir.path().join("a.xlsx"), &[&[&["ID"], &["a"], &["b"]]]);
    let mut opts = options(dir.path());
    opts.rows_per_sheet = 1;
    opts.header_on_each_sheet = true;
    let report = merge(&opts, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!((report.rows, report.output_sheets), (2, 2));
    assert!(read(&opts.output_file).iter().all(|sheet| sheet.len() == 2));
}

#[test]
fn excel_merge_skips_empty_sheets_and_rows_and_allows_blank_trailing_cells() {
    let dir = tempfile::tempdir().unwrap();
    fixture(
        &dir.path().join("a.xlsx"),
        &[&[], &[&["ID", "备注"], &["a"], &[], &["b", "备注"]]],
    );
    let report = merge(&options(dir.path()), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(
        (report.rows, report.source_sheets, report.output_sheets),
        (2, 1, 1)
    );
}

#[test]
fn excel_merge_late_overflow_and_bad_header_preserve_previous_output() {
    for rows in [
        vec![vec!["ID"], vec!["a"], vec!["b", "unexpected"]],
        vec![vec!["ID"], vec!["a"]],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let refs: Vec<&[&str]> = rows.iter().map(Vec::as_slice).collect();
        fixture(&dir.path().join("a.xlsx"), &[&refs]);
        if rows.len() == 2 {
            fixture(&dir.path().join("b.xlsx"), &[&[&["ID", "unexpected"]]]);
        }
        let mut opts = options(dir.path());
        opts.drop_file_headers = true;
        std::fs::write(&opts.output_file, b"previous result").unwrap();
        let error = merge(&opts, &AtomicBool::new(false), |_| {})
            .unwrap_err()
            .to_string();
        assert!(error.contains("列数"), "{error}");
        assert!(error.contains(if rows.len() == 2 {
            "b.xlsx"
        } else {
            "第 3 行"
        }));
        assert_eq!(
            std::fs::read(&opts.output_file).unwrap(),
            b"previous result"
        );
    }
}

#[test]
fn excel_merge_recursion_temp_files_and_existing_output_are_handled() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    fixture(&dir.path().join("a.xlsx"), &[&[&["ID"], &["a"]]]);
    fixture(&nested.join("b.XLSX"), &[&[&["ID"], &["b"]]]);
    std::fs::write(dir.path().join("~$locked.xlsx"), b"not an Excel file").unwrap();
    let mut opts = options(dir.path());
    opts.drop_file_headers = true;
    let first = merge(&opts, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!((first.files, first.rows), (1, 1));
    opts.recursive = true;
    let second = merge(&opts, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!((second.files, second.rows), (2, 2));
}

#[test]
fn excel_merge_native_values_text_identifiers_dates_and_formula_cache_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    for (column, title) in ["编码", "长编号", "金额", "启用", "日期", "公式", "文本"]
        .iter()
        .enumerate()
    {
        sheet.write_string(0, column as u16, *title).unwrap();
    }
    sheet.write_string(1, 0, "007").unwrap();
    sheet.write_string(1, 1, "123456789012345678").unwrap();
    sheet.write_number(1, 2, 12.5).unwrap();
    sheet.write_boolean(1, 3, true).unwrap();
    let datetime = chrono::NaiveDate::from_ymd_opt(2026, 10, 24)
        .unwrap()
        .and_hms_opt(8, 30, 0)
        .unwrap();
    sheet
        .write_datetime_with_format(
            1,
            4,
            datetime,
            &Format::new().set_num_format("yyyy-mm-dd hh:mm:ss"),
        )
        .unwrap();
    sheet
        .write_formula(1, 5, rust_xlsxwriter::Formula::new("=1+2").set_result("3"))
        .unwrap();
    sheet.write_string(1, 6, "=SUM(A1:A9)").unwrap();
    book.save(dir.path().join("a.xlsx")).unwrap();
    let opts = options(dir.path());
    merge(&opts, &AtomicBool::new(false), |_| {}).unwrap();
    let result = read(&opts.output_file);
    let row = &result[0][1];
    assert_eq!(row[0], Data::String("007".into()));
    assert_eq!(row[1], Data::String("123456789012345678".into()));
    assert_eq!(row[2], Data::Float(12.5));
    assert_eq!(row[3], Data::Bool(true));
    match row[4] {
        Data::DateTime(value) => assert_eq!(value.as_datetime(), Some(datetime)),
        _ => panic!("date lost: {:?}", row[4]),
    }
    assert_eq!(row[5], Data::Float(3.));
    assert_eq!(row[6], Data::String("=SUM(A1:A9)".into()));
    let mut output = open_workbook_auto(&opts.output_file).unwrap();
    assert!(
        output
            .worksheet_formula("数据1")
            .unwrap()
            .used_cells()
            .next()
            .is_none()
    );
}

#[test]
fn excel_merge_cancellation_during_read_and_save_keeps_previous_output() {
    for cancel_at_save in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        fixture(&dir.path().join("a.xlsx"), &[&[&["ID"], &["a"]]]);
        let opts = options(dir.path());
        std::fs::write(&opts.output_file, b"previous").unwrap();
        let cancel = AtomicBool::new(false);
        let result = merge(&opts, &cancel, |event| {
            if matches!(event, Progress::Saving) == cancel_at_save {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        assert!(result.unwrap_err().to_string().contains("取消"));
        assert_eq!(std::fs::read(&opts.output_file).unwrap(), b"previous");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}

#[test]
fn excel_merge_invalid_options_empty_and_corrupt_sources_fail_without_output() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = options(dir.path());
    for count in [0, MAX_DATA_ROWS + 1] {
        opts.rows_per_sheet = count;
        assert!(opts.validate().is_err());
    }
    opts.rows_per_sheet = 1;
    opts.output_file = dir.path().join("bad.xls");
    assert!(opts.validate().is_err());
    opts.output_file = dir.path().join("result.xlsx");
    assert!(merge(&opts, &AtomicBool::new(false), |_| {}).is_err());
    fixture(&dir.path().join("empty.xlsx"), &[&[]]);
    assert!(merge(&opts, &AtomicBool::new(false), |_| {}).is_err());
    std::fs::write(dir.path().join("bad.xlsx"), b"corrupted").unwrap();
    assert!(merge(&opts, &AtomicBool::new(false), |_| {}).is_err());
    assert!(!opts.output_file.exists());
}

#[test]
fn excel_merge_keeps_absolute_column_positions_when_source_starts_at_b3() {
    let dir = tempfile::tempdir().unwrap();
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.write_string(2, 1, "编号").unwrap();
    sheet.write_string(2, 2, "备注").unwrap();
    sheet.write_string(3, 1, "007").unwrap();
    sheet.write_string(3, 2, "内容").unwrap();
    book.save(dir.path().join("a.xlsx")).unwrap();
    let opts = options(dir.path());
    merge(&opts, &AtomicBool::new(false), |_| {}).unwrap();
    let mut output = open_workbook_auto(&opts.output_file).unwrap();
    let range = output.worksheet_range("数据1").unwrap();
    assert_eq!(range.get_value((0, 1)), Some(&Data::String("编号".into())));
    assert_eq!(range.get_value((1, 1)), Some(&Data::String("007".into())));
    assert_eq!(range.get_value((1, 2)), Some(&Data::String("内容".into())));
}

#[test]
fn excel_merge_publish_failure_cleans_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    fixture(&dir.path().join("a.xlsx"), &[&[&["ID"], &["a"]]]);
    let opts = options(dir.path());
    let result = merge(&opts, &AtomicBool::new(false), |event| {
        if matches!(event, Progress::Saving) {
            std::fs::create_dir(&opts.output_file).unwrap();
        }
    });
    assert!(result.unwrap_err().to_string().contains("保存结果失败"));
    assert!(opts.output_file.is_dir());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}
