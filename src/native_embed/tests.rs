use super::*;
use std::{
    fs,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
fn create_test_window() -> HWND {
    unsafe {
        CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide("PersonalDesk synthetic fixture").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            50,
            60,
            640,
            480,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        )
    }
}
struct Fixture {
    process: Child,
    directory: tempfile::TempDir,
    window: DesktopWindow,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let executable = std::env::current_exe().unwrap();
        let mut command = Command::new(&executable);
        command
            .args([
                "--exact",
                "native_embed::tests::native_fixture_child",
                "--ignored",
            ])
            .env("PERSONAL_DESK_NATIVE_FIXTURE", directory.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let process = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let marker = directory.path().join("window.txt");
        while !marker.exists() {
            assert!(
                Instant::now() < deadline,
                "Fixture window startup timed out"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let handle = fs::read_to_string(marker).unwrap().parse().unwrap();
        let window = DesktopWindow {
            handle,
            process_id: process.id(),
            title: "synthetic".into(),
            executable,
        };
        Self {
            process,
            directory,
            window,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::write(self.directory.path().join("exit"), b"stop");
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.process.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
#[test]
#[ignore = "Internal hidden subprocess fixture, started by native hosting tests"]
fn native_fixture_child() {
    let Some(directory) = std::env::var_os("PERSONAL_DESK_NATIVE_FIXTURE") else {
        return;
    };
    let directory = PathBuf::from(directory);
    let window = create_test_window();
    assert!(!window.is_null());
    fs::write(directory.join("window.txt"), (window as usize).to_string()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    unsafe {
        while Instant::now() < deadline && !directory.join("exit").exists() && IsWindow(window) != 0
        {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if IsWindow(window) != 0 {
            DestroyWindow(window);
        }
    }
}
#[test]
fn original_compositor_and_owner_survive_docking_resizing_hiding_and_restore() {
    let mut fixture = Fixture::new();
    let window = hwnd(fixture.window.handle);
    let parent = create_test_window();
    unsafe {
        SetWindowPos(
            parent,
            ptr::null_mut(),
            -30000,
            -30000,
            640,
            480,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        ShowWindow(parent, SW_SHOWNOACTIVATE);
    }
    assert!(!parent.is_null());
    let original_style = unsafe { GetWindowLongPtrW(window, GWL_STYLE) };
    let mut original_rect = RECT::default();
    unsafe {
        GetWindowRect(window, &mut original_rect);
    }
    let mut dock = DesktopDock::default();
    dock.bind_parent(parent as usize);
    dock.area = Some([20, 30, 500, 350]);
    dock.attach(fixture.window.clone()).unwrap();
    dock.synchronize(true).unwrap();
    assert_eq!(unsafe { GetWindow(window, GW_OWNER) }, parent);
    assert_eq!(
        unsafe { GetWindowLongPtrW(window, GWL_STYLE) } as u32 & WS_CHILD,
        0
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let mut rect = RECT::default();
        unsafe {
            GetWindowRect(window, &mut rect);
        }
        if rect.right - rect.left == 500 && rect.bottom - rect.top == 350 {
            break;
        }
        assert!(Instant::now() < deadline, "Child size did not follow host");
        std::thread::sleep(Duration::from_millis(10));
    }
    unsafe {
        SetWindowPos(
            parent,
            ptr::null_mut(),
            -29000,
            -28500,
            640,
            480,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    dock.synchronize(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let mut expected = POINT { x: 20, y: 30 };
        let mut actual = RECT::default();
        unsafe {
            windows_sys::Win32::Graphics::Gdi::ClientToScreen(parent, &mut expected);
            GetWindowRect(window, &mut actual);
        }
        if actual.left == expected.x && actual.top == expected.y {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Docked window did not follow the host move"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    dock.synchronize(false).unwrap();
    assert_eq!(
        unsafe { GetWindowLongPtrW(window, GWL_STYLE) } as u32 & WS_VISIBLE,
        0
    );
    dock.synchronize(true).unwrap();
    dock.detach().unwrap();
    assert!(unsafe { GetParent(window) }.is_null());
    assert_eq!(
        unsafe { GetWindowLongPtrW(window, GWL_STYLE) },
        original_style
    );
    assert!(fixture.process.try_wait().unwrap().is_none());
    let mut restored = RECT::default();
    unsafe {
        GetWindowRect(window, &mut restored);
    }
    assert_eq!(
        (restored.left, restored.top, restored.right, restored.bottom),
        (
            original_rect.left,
            original_rect.top,
            original_rect.right,
            original_rect.bottom
        )
    );
    drop(dock);
    assert_ne!(unsafe { IsWindow(window) }, 0);
    unsafe {
        DestroyWindow(parent);
    }
}
#[test]
fn dropping_the_host_restores_a_foreign_window_and_invalid_candidates_do_not_change_it() {
    let fixture = Fixture::new();
    let parent = create_test_window();
    unsafe {
        SetWindowPos(
            parent,
            ptr::null_mut(),
            -30000,
            -30000,
            640,
            480,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        ShowWindow(parent, SW_SHOWNOACTIVATE);
    }
    let window = hwnd(fixture.window.handle);
    let original = unsafe { GetWindowLongPtrW(window, GWL_STYLE) };
    {
        let mut dock = DesktopDock::default();
        dock.bind_parent(parent as usize);
        let mut wrong = fixture.window.clone();
        wrong.process_id += 1;
        assert!(dock.attach(wrong).is_err());
        assert_eq!(unsafe { GetWindowLongPtrW(window, GWL_STYLE) }, original);
        dock.attach(fixture.window.clone()).unwrap();
    }
    assert!(unsafe { GetParent(window) }.is_null());
    assert_eq!(unsafe { GetWindowLongPtrW(window, GWL_STYLE) }, original);
    assert_ne!(unsafe { IsWindow(window) }, 0);
    unsafe {
        DestroyWindow(parent);
    }
}
#[test]
#[ignore = "Read-only discovery of the installed desktop application"]
fn live_desktop_discovery() {
    let windows = desktop_windows();
    assert!(
        !windows.is_empty(),
        "Open the installed Codex / ChatGPT desktop application first"
    );
    for window in windows {
        assert!(window.executable.is_file());
        assert!(window.handle != 0);
    }
}
#[test]
#[ignore = "Briefly hosts and immediately restores the existing desktop window"]
fn live_desktop_round_trip() {
    let candidates = desktop_windows();
    assert_eq!(
        candidates.len(),
        1,
        "This test only runs with exactly one main desktop window"
    );
    let target = candidates[0].clone();
    let window = hwnd(target.handle);
    unsafe {
        let context = GetWindowDpiAwarenessContext(window);
        let previous = SetThreadDpiAwarenessContext(context);
        let parent = create_test_window();
        SetWindowPos(
            parent,
            ptr::null_mut(),
            -30000,
            -30000,
            640,
            480,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        ShowWindow(parent, SW_SHOWNOACTIVATE);
        SetThreadDpiAwarenessContext(previous);
        assert!(!parent.is_null());
        let original_style = GetWindowLongPtrW(window, GWL_STYLE);
        let mut original = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        assert_ne!(GetWindowPlacement(window, &mut original), 0);
        let foreground = GetForegroundWindow();
        {
            let mut dock = DesktopDock::default();
            dock.bind_parent(parent as usize);
            dock.area = Some([0, 0, 1000, 680]);
            dock.attach(target).unwrap();
            dock.synchronize(false).unwrap();
            std::thread::sleep(Duration::from_millis(250));
            assert_eq!(GetWindow(window, GW_OWNER), parent);
            assert_ne!(IsWindow(window), 0);
            dock.detach().unwrap();
            assert!(GetParent(window).is_null());
            assert_eq!(GetWindowLongPtrW(window, GWL_STYLE), original_style);
        }
        let mut restored = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        assert_ne!(GetWindowPlacement(window, &mut restored), 0);
        assert_eq!(restored.showCmd, original.showCmd);
        for (after, before) in [
            (
                restored.rcNormalPosition.left,
                original.rcNormalPosition.left,
            ),
            (restored.rcNormalPosition.top, original.rcNormalPosition.top),
            (
                restored.rcNormalPosition.right,
                original.rcNormalPosition.right,
            ),
            (
                restored.rcNormalPosition.bottom,
                original.rcNormalPosition.bottom,
            ),
        ] {
            assert!(
                (after - before).abs() <= 2,
                "DPI-rounded restored position changed: {after} vs {before}"
            );
        }
        DestroyWindow(parent);
        if foreground == window {
            SetForegroundWindow(window);
        }
    }
}
