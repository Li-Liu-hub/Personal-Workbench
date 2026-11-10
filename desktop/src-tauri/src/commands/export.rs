//! 数据导出命令。

use std::path::PathBuf;

use personal_desk::export::{self, Scope};
use tauri::async_runtime::spawn_blocking;

use crate::state::data_directory;

#[derive(serde::Serialize)]
pub struct ExportResult {
    path: PathBuf,
    warnings: usize,
}

#[tauri::command]
pub async fn export_test_data(scope: String) -> Result<Option<ExportResult>, String> {
    let scope = match scope.as_str() {
        "credentials" => Scope::Credentials,
        "inventory" => Scope::Inventory,
        "folders" => Scope::Folders,
        "tools" => Scope::Tools,
        "all" => Scope::All,
        "essays" => Scope::Essays,
        "documents" => Scope::Documents,
        "contacts" => Scope::Contacts,
        "websites" => Scope::Websites,
        _ => return Err("导出范围无效".into()),
    };
    let data = data_directory()?;
    let Some(path) = rfd::AsyncFileDialog::new()
        .set_title("导出测试数据")
        .set_file_name("个人办公台数据.zip")
        .add_filter("ZIP 压缩包", &["zip"])
        .save_file()
        .await
    else {
        return Ok(None);
    };
    let mut destination = path.path().to_owned();
    if destination
        .extension()
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("zip"))
    {
        destination.set_extension("zip");
        if destination.exists() {
            return Err("补全扩展名后的文件已存在，请在保存窗口直接选择该文件".into());
        }
    }
    spawn_blocking(move || {
        export::write(&data, &destination, scope)
            .map(|report| {
                Some(ExportResult {
                    path: report.path,
                    warnings: report.warnings,
                })
            })
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}
