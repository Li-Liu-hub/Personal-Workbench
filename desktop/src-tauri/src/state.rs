//! 应用级状态：数据目录解析、数据库初始化与访问、单实例与主窗口。

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
#[cfg(not(debug_assertions))]
use std::time::{Duration, Instant};

use crate::db::Database;
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

#[cfg(not(debug_assertions))]
struct VaultConnection {
    directory: PathBuf,
    database: Option<Database>,
    last_error: Option<(Instant, String)>,
}

#[cfg(not(debug_assertions))]
static VAULT: OnceLock<Mutex<VaultConnection>> = OnceLock::new();

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
        crate::data_directory::resolve()
    }
    #[cfg(debug_assertions)]
    {
        Err("开发版仅允许使用显式指定的测试数据目录".into())
    }
}

pub(crate) fn initialize_database(directory: Result<&Path, &String>) {
    let result = directory.map_err(Clone::clone).and_then(|path| {
        let database = Database::open_local(path).map_err(|error| error.to_string())?;
        #[cfg(not(debug_assertions))]
        let _ = VAULT.set(Mutex::new(VaultConnection {
            directory: path.to_owned(),
            database: None,
            last_error: None,
        }));
        Ok(database)
    });
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

#[cfg(not(debug_assertions))]
impl VaultConnection {
    fn open(&mut self) -> Result<&mut Database, String> {
        if self.database.is_none() {
            if let Some((at, error)) = &self.last_error
                && at.elapsed() < Duration::from_secs(2)
            {
                return Err(error.clone());
            }
            match Database::open_vault(&self.directory) {
                Ok(database) => {
                    self.database = Some(database);
                    self.last_error = None;
                }
                Err(error) => {
                    let error = error.to_string();
                    self.last_error = Some((Instant::now(), error.clone()));
                    return Err(error);
                }
            }
        }
        self.database
            .as_mut()
            .ok_or_else(|| "账号库尚未初始化".into())
    }
}

/// 账号命令可以等待连接或同步，本地页面的连接及窗口线程独立运行。
pub(crate) async fn with_vault<T: Send + 'static>(
    action: impl FnOnce(&mut Database) -> crate::error::Result<T> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(debug_assertions)]
        let result = action(&mut *development_db()?);
        #[cfg(not(debug_assertions))]
        let result = {
            let vault = VAULT.get().ok_or_else(|| {
                DATABASE_ERROR
                    .get()
                    .cloned()
                    .unwrap_or_else(|| "账号库尚未初始化".into())
            })?;
            action(vault.lock().map_err(|_| "账号库锁已损坏")?.open()?)
        };
        result.map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 提前在后台连接账号库，之后按需同步已连接的账号资料。
pub(crate) fn spawn_remote_sync() {
    #[cfg(not(debug_assertions))]
    if let Some(vault) = VAULT.get() {
        std::thread::spawn(move || {
            if let Ok(mut vault) = vault.lock() {
                let _ = vault.open();
            }
            loop {
                std::thread::sleep(Duration::from_secs(2));
                if let Ok(mut vault) = vault.lock()
                    && let Some(database) = vault.database.as_mut()
                {
                    let _ = database.sync_remote_if_due();
                }
            }
        });
    }
}

/// 创建主窗口，并把 WebView2 用户数据目录显式指向应用数据目录。
pub(crate) fn build_main_window(app: &mut App, data: Result<&Path, &String>) -> tauri::Result<()> {
    let webview_data_directory = webview_data_directory(app, data);
    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
        .title("个人办公台")
        .inner_size(1280.0, 820.0)
        .min_inner_size(760.0, 560.0)
        .resizable(true)
        .fullscreen(false)
        .decorations(false)
        .disable_drag_drop_handler();
    if let Some(directory) = webview_data_directory {
        builder = builder.data_directory(directory);
    }
    builder.build()?;
    Ok(())
}

/// 优先使用应用数据目录；解析失败时退回 Tauri 应用数据目录，保证窗口总能创建。
fn webview_data_directory(app: &App, data: Result<&Path, &String>) -> Option<PathBuf> {
    match data {
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
