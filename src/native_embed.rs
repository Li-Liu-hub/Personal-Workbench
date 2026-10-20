//! Windows desktop-window hosting. This never reads or copies the hosted app's data.
use std::{path::PathBuf, ptr};
use windows_sys::Win32::{
    Foundation::*,
    System::Threading::*,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};

#[derive(Clone, Debug)]
pub struct DesktopWindow {
    pub handle: usize,
    pub process_id: u32,
    pub title: String,
    pub executable: PathBuf,
}
fn hwnd(value: usize) -> HWND {
    value as HWND
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn failure(action: &str) -> String {
    format!("{action}：{}", std::io::Error::last_os_error())
}
fn process_path(id: u32) -> Option<PathBuf> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, id);
        if process.is_null() {
            return None;
        }
        let mut buffer = vec![0u16; 32768];
        let mut size = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size);
        CloseHandle(process);
        if ok == 0 {
            None
        } else {
            Some(PathBuf::from(String::from_utf16_lossy(
                &buffer[..size as usize],
            )))
        }
    }
}
unsafe extern "system" fn enumerate(window: HWND, data: LPARAM) -> windows_sys::core::BOOL {
    unsafe {
        if IsWindowVisible(window) == 0
            || GetWindowTextLengthW(window) == 0
            || GetWindowLongPtrW(window, GWL_STYLE) as u32 & WS_CHILD != 0
        {
            return 1;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        let Some(path) = process_path(pid) else {
            return 1;
        };
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if !matches!(name.as_str(), "chatgpt.exe" | "codex.exe") {
            return 1;
        }
        // CLI codex.exe does not have an application window; titles exclude helper popups.
        let mut buffer = vec![0u16; GetWindowTextLengthW(window) as usize + 1];
        let len = GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32);
        let title = String::from_utf16_lossy(&buffer[..len.max(0) as usize]);
        if !GetWindow(window, GW_OWNER).is_null() {
            return 1;
        }
        let output = &mut *(data as *mut Vec<DesktopWindow>);
        output.push(DesktopWindow {
            handle: window as usize,
            process_id: pid,
            title,
            executable: path,
        });
        1
    }
}
pub fn desktop_windows() -> Vec<DesktopWindow> {
    let mut result = vec![];
    unsafe {
        EnumWindows(Some(enumerate), &mut result as *mut _ as LPARAM);
    }
    result
}
pub fn launch_desktop() -> Result<(), String> {
    let verb = wide("open");
    let target = wide("shell:AppsFolder\\OpenAI.Codex_2p2nqsd0c76g0!App");
    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        Err("无法启动原版应用，请先从开始菜单打开 Codex / ChatGPT。".into())
    } else {
        Ok(())
    }
}
fn set_style(window: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) -> Result<(), String> {
    unsafe {
        SetLastError(0);
        let previous = SetWindowLongPtrW(window, index, value);
        if previous == 0 && GetLastError() != 0 {
            Err(failure(&format!("调整窗口样式失败（{index}）")))
        } else {
            Ok(())
        }
    }
}
// Keep the foreign window top-level: Chromium/WinUI transparent backdrops stop
// composing correctly when WS_CHILD/SetParent is forced onto their main HWND.
fn set_owner(window: HWND, owner: HWND) -> Result<(), String> {
    unsafe {
        if GetWindow(window, GW_OWNER) == owner {
            return Ok(());
        }
        SetLastError(0);
        SetWindowLongPtrW(window, GWLP_HWNDPARENT, owner as isize);
        if GetWindow(window, GW_OWNER) != owner {
            Err(failure("绑定原应用窗口失败"))
        } else {
            Ok(())
        }
    }
}
struct Attachment {
    window: DesktopWindow,
    style: isize,
    extended: isize,
    owner: usize,
    placement: WINDOWPLACEMENT,
    visible: bool,
    restored: bool,
}
impl Attachment {
    fn alive(&self) -> bool {
        unsafe {
            let mut pid = 0;
            IsWindow(hwnd(self.window.handle)) != 0
                && GetWindowThreadProcessId(hwnd(self.window.handle), &mut pid) != 0
                && pid == self.window.process_id
        }
    }
    fn restore(&mut self) -> Result<(), String> {
        if self.restored {
            return Ok(());
        }
        if !self.alive() {
            self.restored = true;
            return Ok(());
        }
        let window = hwnd(self.window.handle);
        unsafe {
            let owner = if self.owner == 0 || IsWindow(hwnd(self.owner)) == 0 {
                ptr::null_mut()
            } else {
                hwnd(self.owner)
            };
            set_owner(window, owner)?;
            set_style(window, GWL_STYLE, self.style)?;
            set_style(window, GWL_EXSTYLE, self.extended)?;
            if SetWindowPlacement(window, &self.placement) == 0 {
                return Err(failure("还原窗口位置失败"));
            }
            if !self.visible {
                ShowWindow(window, SW_HIDE);
            }
        }
        self.restored = true;
        Ok(())
    }
}
impl Drop for Attachment {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
#[derive(Default)]
pub struct DesktopDock {
    pub basic_chat: bool,
    pub parent: usize,
    pub candidates: Vec<DesktopWindow>,
    pub selected: usize,
    pub attempted: bool,
    pub error: String,
    pub attach_request: Option<DesktopWindow>,
    pub area: Option<[i32; 4]>,
    attachment: Option<Attachment>,
    last_area: Option<[i32; 4]>,
    last_visible: bool,
}
impl DesktopDock {
    pub fn bind_parent(&mut self, parent: usize) {
        self.parent = parent;
    }
    pub fn refresh(&mut self) {
        self.candidates = desktop_windows();
        if !self.candidates.iter().any(|w| w.handle == self.selected) {
            self.selected = self.candidates.first().map(|w| w.handle).unwrap_or(0);
        }
    }
    pub fn embedded_title(&self) -> Option<&str> {
        self.attachment.as_ref().map(|a| a.window.title.as_str())
    }
    pub fn attach(&mut self, window: DesktopWindow) -> Result<(), String> {
        self.detach()?;
        if self.parent == 0 {
            return Err("办公台窗口尚未就绪，请重试。".into());
        }
        let target = hwnd(window.handle);
        let parent = hwnd(self.parent);
        unsafe {
            let mut pid = 0;
            if IsWindow(target) == 0
                || GetWindowThreadProcessId(target, &mut pid) == 0
                || pid != window.process_id
                || process_path(pid).as_ref() != Some(&window.executable)
            {
                return Err("原应用窗口已变化，请刷新后重试。".into());
            }
            if pid == std::process::id() {
                return Err("不能把办公台窗口嵌入自身。".into());
            }
            if GetWindowLongPtrW(target, GWL_STYLE) as u32 & WS_CHILD != 0 {
                return Err("请先在旧版办公台点击恢复独立窗口，再接入新版。".into());
            }
            if AreDpiAwarenessContextsEqual(
                GetWindowDpiAwarenessContext(target),
                GetWindowDpiAwarenessContext(parent),
            ) == 0
            {
                return Err("原应用与办公台的显示缩放模式不兼容，请使用独立窗口。".into());
            }
            let mut placement = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            if GetWindowPlacement(target, &mut placement) == 0 {
                return Err(failure("读取原窗口位置失败"));
            }
            let snapshot = Attachment {
                style: GetWindowLongPtrW(target, GWL_STYLE),
                extended: GetWindowLongPtrW(target, GWL_EXSTYLE),
                owner: GetWindow(target, GW_OWNER) as usize,
                visible: IsWindowVisible(target) != 0,
                placement,
                window,
                restored: false,
            };
            if IsIconic(target) != 0 || IsZoomed(target) != 0 {
                ShowWindow(target, SW_RESTORE);
            }
            set_owner(target, parent)?;
            set_style(
                target,
                GWL_EXSTYLE,
                (snapshot.extended as u32 & !(WS_EX_APPWINDOW | WS_EX_TOPMOST)) as isize,
            )?;
            self.attachment = Some(snapshot);
            self.last_area = None;
            self.last_visible = IsWindowVisible(target) != 0;
        }
        self.synchronize(true)?;
        unsafe {
            if IsWindowVisible(parent) != 0 && GetForegroundWindow() == parent {
                SetForegroundWindow(target);
            }
        }
        self.error.clear();
        Ok(())
    }
    pub fn detach(&mut self) -> Result<(), String> {
        let foreground = unsafe { GetForegroundWindow() };
        if let Some(attachment) = self.attachment.as_mut() {
            attachment.restore()?;
            if self.parent != 0
                && foreground == hwnd(self.parent)
                && attachment.extended as u32 & WS_EX_TOPMOST == 0
            {
                unsafe {
                    SetWindowPos(
                        hwnd(attachment.window.handle),
                        hwnd(self.parent),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE
                            | SWP_NOSIZE
                            | SWP_NOACTIVATE
                            | SWP_NOOWNERZORDER
                            | SWP_ASYNCWINDOWPOS,
                    );
                }
            }
        }
        self.attachment = None;
        self.last_area = None;
        self.last_visible = false;
        if self.parent != 0 && foreground == hwnd(self.parent) {
            unsafe {
                SetForegroundWindow(foreground);
            }
        }
        Ok(())
    }
    pub fn synchronize(&mut self, visible: bool) -> Result<(), String> {
        let Some(attachment) = self.attachment.as_ref() else {
            return Ok(());
        };
        if !attachment.alive() {
            self.attachment = None;
            return Err("原应用窗口已关闭，可重新启动后接入。".into());
        }
        let Some([x, y, width, height]) = self.area else {
            return Ok(());
        };
        let target = hwnd(attachment.window.handle);
        unsafe {
            let parent = hwnd(self.parent);
            let visible = visible && IsWindowVisible(parent) != 0 && IsIconic(parent) == 0;
            let mut origin = POINT { x, y };
            if windows_sys::Win32::Graphics::Gdi::ClientToScreen(parent, &mut origin) == 0 {
                return Err(failure("读取办公台位置失败"));
            }
            let desired = [origin.x, origin.y, width.max(1), height.max(1)];
            let mut current = RECT::default();
            GetWindowRect(target, &mut current);
            let actual = [
                current.left,
                current.top,
                current.right - current.left,
                current.bottom - current.top,
            ];
            if visible
                && (self.last_area != Some(desired)
                    || actual != desired
                    || IsIconic(target) != 0
                    || IsZoomed(target) != 0)
            {
                if IsIconic(target) != 0 || IsZoomed(target) != 0 {
                    ShowWindow(target, SW_RESTORE);
                }
                if SetWindowPos(
                    target,
                    ptr::null_mut(),
                    desired[0],
                    desired[1],
                    desired[2],
                    desired[3],
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS,
                ) == 0
                {
                    return Err(failure("调整原应用大小失败"));
                }
                self.last_area = Some(desired);
            }
            if self.last_visible != visible {
                ShowWindow(target, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
                self.last_visible = visible;
            }
        }
        Ok(())
    }
}
impl Drop for DesktopDock {
    fn drop(&mut self) {
        let _ = self.detach();
    }
}

#[cfg(test)]
mod tests;
