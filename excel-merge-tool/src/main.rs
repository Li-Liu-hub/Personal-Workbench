//! Excel 批量合并工具
//!
//! 选择一个文件夹，把它里面所有的 Excel 流式合并成一个 Excel。
//! 双击运行即为图形界面。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gui;
mod merge;

use std::path::PathBuf;

fn main() {
    // 隐藏的自测入口：
    //   excel-merge.exe --selftest <输入目录> <输出文件> [每sheet条数] [每sheet带表头 0/1] [移除子文件表头 0/1]
    // 仅用于开发期验证，正常运行不带参数即进图形界面。
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 4 && args[1] == "--selftest" {
        let rows: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(50_000);
        let header_each = args.get(5).map(|s| s == "1").unwrap_or(false);
        let drop_headers = args.get(6).map(|s| s == "1").unwrap_or(false);
        let opts = merge::MergeOptions {
            input_dir: PathBuf::from(&args[2]),
            output_file: PathBuf::from(&args[3]),
            rows_per_sheet: rows,
            header_on_each_sheet: header_each,
            drop_file_headers: drop_headers,
            ..Default::default()
        };
        match merge::merge(&opts, |p| match p {
            merge::Progress::FoundFiles(n) => println!("找到 {} 个文件", n),
            merge::Progress::NewSheet { name, .. } => println!("新建 sheet: {}", name),
            merge::Progress::Processing {
                index,
                total,
                file,
                rows_written,
            } => println!("[{}/{}] {} (累计 {} 行)", index, total, file, rows_written),
            merge::Progress::Finished(s) => println!(
                "完成：{} 文件 / {} sheet / {} 行 / {:.2}s",
                s.files,
                s.sheets,
                s.rows,
                s.elapsed_ms as f64 / 1000.0
            ),
        }) {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("失败：{}", e);
                std::process::exit(1);
            }
        }
    }

    if let Err(e) = gui::run_gui() {
        eprintln!("启动界面失败：{}", e);
        std::process::exit(1);
    }
}
