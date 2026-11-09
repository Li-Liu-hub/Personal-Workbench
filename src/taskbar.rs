use std::sync::mpsc::Sender;

use eframe::egui;

#[cfg(windows)]
use std::{cell::RefCell, path::Path};

#[cfg(windows)]
use windows::{
    UI::Shell::TaskbarManager,
    Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile},
            WinRT::{RO_INIT_MULTITHREADED, RO_INIT_SINGLETHREADED, RoInitialize},
        },
        UI::Shell::{IShellLinkW, ShellLink},
    },
    core::{IUnknown, Interface, PCWSTR},
};

#[cfg(windows)]
use windows_future::AsyncOperationCompletedHandler;

#[cfg(windows)]
thread_local! {
    static WINRT_APARTMENT: RefCell<Option<WinRtApartment>> = const { RefCell::new(None) };
}

#[cfg(windows)]
struct WinRtApartment;

#[cfg(windows)]
impl Drop for WinRtApartment {
    fn drop(&mut self) {
        unsafe { windows::Win32::System::WinRT::RoUninitialize() };
    }
}

#[cfg(windows)]
fn initialize_winrt() -> Result<(), String> {
    WINRT_APARTMENT.with(|apartment| {
        if apartment.borrow().is_some() {
            return Ok(());
        }
        let result = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        if let Err(error) = result {
            if error.code() == RPC_E_CHANGED_MODE {
                unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }
                    .map_err(|error| format!("无法初始化 Windows 固定功能：{error}"))?;
            } else {
                return Err(format!("无法初始化 Windows 固定功能：{error}"));
            }
        }
        *apartment.borrow_mut() = Some(WinRtApartment);
        Ok(())
    })
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(windows)]
fn ensure_start_menu_shortcut(executable: &Path) -> Result<(), String> {
    let app_data =
        std::env::var_os("APPDATA").ok_or_else(|| "无法找到当前用户的开始菜单目录".to_owned())?;
    let programs = Path::new(&app_data)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs");
    let group = programs.join("个人办公台");
    std::fs::create_dir_all(&group).map_err(|error| format!("无法创建开始菜单入口：{error}"))?;
    let shortcut = group.join("个人办公台.lnk");
    let target = wide(&executable.to_string_lossy());
    let shortcut_path = wide(&shortcut.to_string_lossy());
    let working_directory = wide(
        &executable
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_string_lossy(),
    );

    let shell_link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }
            .map_err(|error| format!("无法创建开始菜单入口：{error}"))?;
    unsafe {
        shell_link
            .SetPath(PCWSTR(target.as_ptr()))
            .map_err(|error| format!("无法设置开始菜单入口：{error}"))?;
        shell_link
            .SetWorkingDirectory(PCWSTR(working_directory.as_ptr()))
            .map_err(|error| format!("无法设置开始菜单入口：{error}"))?;
        shell_link
            .SetDescription(PCWSTR(wide("个人办公台").as_ptr()))
            .map_err(|error| format!("无法设置开始菜单入口：{error}"))?;
        shell_link
            .SetIconLocation(PCWSTR(target.as_ptr()), 0)
            .map_err(|error| format!("无法设置开始菜单图标：{error}"))?;
    }
    let persist_file: IPersistFile = shell_link
        .cast()
        .map_err(|error| format!("无法保存开始菜单入口：{error}"))?;
    unsafe { persist_file.Save(PCWSTR(shortcut_path.as_ptr()), true) }
        .map_err(|error| format!("无法保存开始菜单入口：{error}"))
}

#[cfg(windows)]
pub fn request_pin(
    context: egui::Context,
    sender: Sender<crate::app::BackgroundMessage>,
) -> Result<(), String> {
    initialize_winrt()?;
    let manager = TaskbarManager::GetDefault()
        .map_err(|error| format!("无法打开 Windows 任务栏固定功能：{error}"))?;
    if !manager
        .IsSupported()
        .map_err(|error| format!("无法检查 Windows 任务栏状态：{error}"))?
    {
        return Err(
            "当前 Windows 版本不支持应用内固定；可右键任务栏中的个人办公台图标进行固定。".into(),
        );
    }
    if !manager
        .IsPinningAllowed()
        .map_err(|error| format!("无法检查任务栏固定权限：{error}"))?
    {
        return Err("Windows 当前不允许固定应用，请检查系统设置或设备管理策略。".into());
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    ensure_start_menu_shortcut(&executable)?;

    let operation = manager
        .RequestPinCurrentAppAsync()
        .map_err(|error| format!("无法请求固定到任务栏：{error}"))?;
    let completed = AsyncOperationCompletedHandler::<bool>::new(move |operation, _status| {
        let result = operation
            .ok()
            .and_then(|operation| operation.GetResults())
            .map_err(|error| format!("Windows 未能完成任务栏固定：{error}"));
        let _ = sender.send(crate::app::BackgroundMessage::TaskbarPin(result));
        context.request_repaint();
        Ok(())
    });
    operation
        .SetCompleted(&completed)
        .map_err(|error| format!("无法等待 Windows 固定确认：{error}"))
}

#[cfg(not(windows))]
pub fn request_pin(
    _context: egui::Context,
    _sender: Sender<crate::app::BackgroundMessage>,
) -> Result<(), String> {
    Err("任务栏固定功能仅支持 Windows。".into())
}
