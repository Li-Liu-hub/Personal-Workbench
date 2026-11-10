//! 应用级状态：数据目录解析、数据库初始化与访问、单实例与主窗口。

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use personal_desk::db::Database;
use tauri::{App, Manager, WebviewUrl, WebviewWindowBuilder};

/// 开发/测试数据目录覆盖变量。
///
/// 新名称是 `PERSONAL_DESK_TEST_DATA`；旧名 `PERSONAL_DESK_MIGRATION_TEST_DATA`
/// 仅作为历史兼容继续读取（旧脚本或快捷方式可能仍在使用），新代码不要写入旧名。
const TEST_DATA_ENV: &str = "PERSONAL_DESK_TEST_DATA";
const LEGACY_TEST_DATA_ENV: &str = "PERSONAL_DESK_MIGRATION_TEST_DATA";

/// WebView2 用户数据目录名。
///
/// 它位于应用数据目录之下，不再落在 exe 同目录（历史版本会生成
/// `dist\个人办公台.exe.WebView2`，污染分发目录）。
const WEBVIEW_DATA_DIRECTORY: &str = "WebView2";

static DATABASE: OnceLock<Mutex<Database>> = OnceLock::new();
static DATABASE_ERROR: OnceLock<String> = OnceLock::new();

/// 解析应用数据目录。
///
/// 测试变量优先；正式版复用核心库的默认数据目录解析（钉住的
/// `personal-desk-data-path.txt`、注册表记忆目录、本机应用数据目录）。
pub fn data_directory() -> Result<PathBuf, String> {
    for name in [TEST_DATA_ENV, LEGACY_TEST_DATA_ENV] {
        if let Some(path) = std::env::var_os(name) {
            let path = PathBuf::from(path);
            return if path.is_absolute() {
                Ok(path)
            } else {
                Err("测试数据目录必须是绝对路径".into())
            };
        }
    }
    #[cfg(not(debug_assertions))]
    {
        personal_desk::data_directory::resolve()
    }
    #[cfg(debug_assertions)]
    {
        Err("开发版仅允许使用显式指定的测试数据目录".into())
    }
}

pub fn initialize_database() {
    let result = data_directory()
        .and_then(|directory| Database::open(&directory).map_err(|error| error.to_string()));
    match result {
        Ok(database) => {
            let _ = DATABASE.set(Mutex::new(database));
        }
        Err(error) => {
            let _ = DATABASE_ERROR.set(error);
        }
    }
}

pub fn development_db() -> Result<MutexGuard<'static, Database>, String> {
    if let Some(error) = DATABASE_ERROR.get() {
        return Err(error.clone());
    }
    DATABASE
        .get()
        .ok_or_else(|| "数据库尚未初始化".to_string())?
        .lock()
        .map_err(|_| "数据库锁已损坏".to_string())
}

/// 后台按需同步私网库；数据库锁只在单次同步期间持有。
pub fn spawn_remote_sync() {
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_secs(2));
            if let Ok(mut database) = development_db() {
                let _ = database.sync_remote_if_due();
            }
        }
    });
}

/// 创建主窗口，并把 WebView2 用户数据目录显式指向应用数据目录。
pub fn build_main_window(app: &mut App) -> tauri::Result<()> {
    let webview_data_directory = webview_data_directory(app);
    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
        .title("个人办公台")
        .inner_size(1280.0, 820.0)
        .min_inner_size(760.0, 560.0)
        .resizable(true)
        .fullscreen(false)
        .decorations(false);
    if let Some(directory) = webview_data_directory {
        builder = builder.data_directory(directory);
    }
    builder.build()?;
    Ok(())
}

/// 优先使用应用数据目录；解析失败时退回 Tauri 应用数据目录，保证窗口总能创建。
fn webview_data_directory(app: &App) -> Option<PathBuf> {
    match data_directory() {
        Ok(directory) => Some(directory.join(WEBVIEW_DATA_DIRECTORY)),
        Err(error) => {
            log::warn!("无法解析应用数据目录，WebView2 数据目录退回 Tauri 应用数据目录：{error}");
            app.path()
                .app_local_data_dir()
                .ok()
                .map(|directory| directory.join(WEBVIEW_DATA_DIRECTORY))
        }
    }
}
