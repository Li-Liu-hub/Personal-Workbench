// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! 个人办公台可执行文件入口。
//!
//! 无参数启动图形界面；带参数时先处理无窗口命令行模式（更新、导入）。
//! 命令行模式不创建窗口、不抢占单实例锁，供发布脚本与 CI 在无 GUI 环境执行。

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use personal_desk_lib::{db::Database, import, library::WritingImportBook, updater};

fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let Some(action) = arguments.next() else {
        personal_desk_lib::run();
        return;
    };
    let result = match action.to_str() {
        Some("--apply-update") => {
            apply_update(arguments.next()).map_err(|error| format!("更新失败：{error}"))
        }
        Some("--import-data") => {
            import_data(arguments.next()).map_err(|error| format!("处理失败：{error}"))
        }

        Some("--import-writing-bundle") => {
            import_writing_bundle(arguments.next(), arguments.next())
                .map_err(|error| format!("导入失败：{error}"))
        }
        _ => {
            personal_desk_lib::run();
            return;
        }
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn apply_update(request: Option<OsString>) -> Result<(), String> {
    let request = request.ok_or_else(|| "缺少更新请求".to_owned())?;
    updater::apply_request(Path::new(&request))
}

/// 命令行模式与图形界面使用同一套数据目录解析（含测试目录覆盖变量）。
fn data_directory() -> Result<PathBuf, String> {
    personal_desk_lib::state::data_directory()
}

fn import_data(path: Option<OsString>) -> Result<(), String> {
    let path = path.ok_or_else(|| "请提供数据文件路径".to_owned())?;
    let directory = data_directory()?;
    let (accounts, websites) =
        import::file(&directory, Path::new(&path)).map_err(|error| error.to_string())?;
    println!("已导入 {accounts} 条账号和 {websites} 个网站，重复记录已跳过。");
    Ok(())
}

fn import_writing_bundle(
    path: Option<OsString>,
    directory: Option<OsString>,
) -> Result<(), String> {
    let path = path.ok_or_else(|| "请提供文档包路径".to_owned())?;
    let directory = match directory {
        Some(directory) => PathBuf::from(directory),
        None => data_directory()?,
    };
    let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
    let books: Vec<WritingImportBook> =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let mut database = Database::open(&directory).map_err(|error| error.to_string())?;
    let (imported, skipped) = database
        .import_writing_books(&books)
        .map_err(|error| error.to_string())?;
    database
        .sync_remote_now()
        .map_err(|error| error.to_string())?;
    println!("已导入 {imported} 份文档，跳过 {skipped} 份已导入文档。");
    Ok(())
}
