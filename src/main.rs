#![windows_subsystem = "windows"]

mod app;
mod autostart;
mod contacts;
mod crypto;
mod db;
mod error;
mod essays;
mod export;
mod files;
mod import;
mod install;
mod launcher;
mod legacy_credentials;
mod library;
mod model;
#[cfg(test)]
mod search;
mod tags;
mod taskbar;
mod theme;
mod ui_components;
mod undo;
mod updater;
mod views;
mod websites;
mod writing_images;

#[cfg(not(debug_assertions))]
mod remote_vault;

#[cfg(test)]
mod tests;

use app::PersonalDeskApp;
use eframe::egui;

fn application_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/personaldesk.png"))
        .expect("the embedded PersonalDesk icon must be a valid PNG")
}

fn main() -> eframe::Result {
    let mut arguments = std::env::args_os().skip(1);
    let action = arguments.next();
    #[cfg(debug_assertions)]
    if action.as_ref().is_some_and(|a| a == "--capture-ui") {
        let output = arguments
            .next()
            .map(std::path::PathBuf::from)
            .expect("visual output path");
        return eframe::run_native(
            "个人办公台 · 自动界面验收",
            eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([1280.0, 800.0])
                    .with_drag_and_drop(false),
                ..Default::default()
            },
            Box::new(move |cc| {
                Ok(Box::new(app::visual_preview::VisualPreview::new(
                    cc, output,
                )))
            }),
        );
    }
    if action
        .as_ref()
        .is_some_and(|a| a == "--import-writing-bundle")
    {
        let result = (|| -> error::Result<(usize, usize)> {
            let path = arguments
                .next()
                .ok_or_else(|| error::DeskError::InvalidPath("请提供文档包路径".into()))?;
            let directory = arguments
                .next()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(app::data_directory);
            let books: Vec<library::WritingImportBook> =
                serde_json::from_slice(&std::fs::read(path)?)?;
            let mut database = db::Database::open(&directory)?;
            let counts = database.import_writing_books(&books)?;
            database.sync_remote_now()?;
            Ok(counts)
        })();
        match result {
            Ok((imported, skipped)) => {
                println!("已导入 {imported} 份文档，跳过 {skipped} 份已导入文档。")
            }
            Err(error) => {
                eprintln!("导入失败：{error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if action.as_ref().is_some_and(|a| a == "--apply-update") {
        let result = arguments
            .next()
            .ok_or_else(|| "缺少更新请求".to_owned())
            .and_then(|path| updater::apply_request(std::path::Path::new(&path)));
        if let Err(error) = result {
            eprintln!("更新失败：{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if action
        .as_ref()
        .is_some_and(|argument| argument == "--import-data" || argument == "--export-all")
    {
        let directory = app::data_directory();
        let result = arguments
            .next()
            .ok_or_else(|| error::DeskError::InvalidPath("请提供数据文件路径".into()))
            .and_then(|path| {
                if action
                    .as_ref()
                    .is_some_and(|argument| argument == "--import-data")
                {
                    import::file(&directory, std::path::Path::new(&path)).map(
                        |(accounts, websites)| {
                            format!(
                                "已导入 {accounts} 条账号和 {websites} 个网站，重复记录已跳过。"
                            )
                        },
                    )
                } else {
                    export::write(&directory, std::path::Path::new(&path), export::Scope::All)
                        .map(|report| format!("已导出全部数据，{} 项说明。", report.warnings))
                }
            });
        match result {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("处理失败：{error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    #[cfg(debug_assertions)]
    if action
        .as_ref()
        .is_some_and(|a| a == "--desktop-visual-preview" || a == "--writing-preview")
    {
        let base =
            std::env::temp_dir().join(format!("personal-desk-preview-{}", std::process::id()));
        let writing_preview = action.as_ref().is_some_and(|a| a == "--writing-preview");
        return eframe::run_native(
            "个人办公台 · 界面预览",
            eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_icon(application_icon())
                    .with_drag_and_drop(false)
                    .with_inner_size([1280.0, 800.0]),
                ..Default::default()
            },
            Box::new(move |cc| {
                Ok(Box::new(if writing_preview {
                    PersonalDeskApp::writing_preview(cc, base)
                } else {
                    PersonalDeskApp::desktop_visual_preview(cc, base)
                }))
            }),
        );
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(application_icon())
            .with_drag_and_drop(false)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1000.0, 680.0])
            .with_decorations(!cfg!(target_os = "windows"))
            .with_resizable(true),
        ..Default::default()
    };

    if install::normalize_launcher() {
        return Ok(());
    }
    let Some(_instance) = install::Instance::acquire(&app::data_directory()) else {
        return Ok(());
    };
    if !cfg!(debug_assertions) || std::env::var_os("PERSONAL_DESK_PREVIEW_OUTPUT").is_none() {
        install::remember_data_directory(&app::data_directory());
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        let directory = parent.to_path_buf();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            install::archive_launchers(directory);
        });
    }

    eframe::run_native(
        "个人办公台",
        options,
        Box::new(|cc| Ok(Box::new(PersonalDeskApp::new(cc)))),
    )
}
