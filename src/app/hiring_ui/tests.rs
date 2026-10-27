use super::*;
use crate::app::editor_tests::{input_frame, pointer};
fn position(output: &egui::FullOutput, value: &str) -> Option<egui::Pos2> {
    output.shapes.iter().find_map(|s| match &s.shape {
        egui::Shape::Text(t) if t.galley.text() == value => Some(t.pos + t.galley.size() * 0.5),
        _ => None,
    })
}
#[test]
fn all_three_builtin_tools_open_from_grid_and_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    app.section = Section::Tools;
    for mode in [ViewMode::Grid, ViewMode::List] {
        for (index, (_, label, _)) in TOOLS.iter().enumerate() {
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            app.set_view_mode(mode);
            app.hiring.open = false;
            // 搜索词用关键词而非卡片标题，避免 position 命中搜索框文本而不是工具入口。
            app.search_query = ["岗位搜索", "自动填表", "手机短信"][index].into();
            input_frame(&mut app, &ctx, 0., vec![], vec![]);
            let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
            let p = position(&out, label).unwrap_or_else(|| panic!("missing tool {label}"));
            input_frame(&mut app, &ctx, 0.2, pointer(p, true), vec![]);
            input_frame(&mut app, &ctx, 0.3, pointer(p, false), vec![]);
            assert!(
                app.hiring.open,
                "tool {label}, view {mode:?} failed to open"
            );
            assert_eq!(app.hiring.tab, index);
            let out = input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
            assert!(position(&out, "求职工具").is_some());
        }
    }
}
#[test]
fn unconfirmed_single_send_remains_single_send_across_frames() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    app.hiring.open = true;
    app.hiring.confirmation = Some(Action::Send(Job {
        title: "测试岗位".into(),
        company: "测试公司".into(),
        ..Default::default()
    }));
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    assert!(matches!(app.hiring.confirmation, Some(Action::Send(_))));
    assert!(!app.hiring.busy());
}
#[test]
fn otp_refresh_detects_service_owned_by_another_process() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let mut app = PersonalDeskApp::open_data_directory(data.clone());
    let _receiver = hiring::otp::Receiver::start(&data.join("hiring"), "127.0.0.1", 0).unwrap();
    app.hiring.open = true;
    app.hiring.loaded = true;
    app.hiring.tab = 2;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    let p = position(&out, "刷新服务状态").unwrap();
    input_frame(&mut app, &ctx, 0.2, pointer(p, true), vec![]);
    input_frame(&mut app, &ctx, 0.3, pointer(p, false), vec![]);
    assert!(app.hiring.otp_checked);
    assert!(app.hiring.otp_external.is_some());
    assert_eq!(app.hiring.status, "接码服务正在运行（可能由 MCP 启动）");
}
#[test]
fn otp_auto_start_binds_a_private_or_loopback_address() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    app.hiring.open = true;
    app.hiring.tab = 2;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    app.hiring.settings.otp_host = "auto".into();
    app.hiring.settings.otp_port = 0;
    let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    let p = position(&out, "启动接码服务").unwrap();
    input_frame(&mut app, &ctx, 0.2, pointer(p, true), vec![]);
    input_frame(&mut app, &ctx, 0.3, pointer(p, false), vec![]);
    let service = app
        .hiring
        .receiver_service
        .as_ref()
        .expect("auto should start the receiver");
    let ip = service
        .address
        .parse::<std::net::SocketAddr>()
        .unwrap()
        .ip();
    assert!(
        matches!(ip, std::net::IpAddr::V4(v4) if v4.is_loopback() || v4.is_private()),
        "unexpected bind {ip}"
    );
}
#[test]
fn otp_error_is_visible_without_scrolling() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    app.hiring.open = true;
    app.hiring.tab = 2;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    app.hiring.error = "测试错误：无法启动接码服务".into();
    let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    let p = position(&out, "测试错误：无法启动接码服务").expect("otp start error must be rendered");
    assert!(
        p.y < 600.0,
        "error must sit above the scroll body, got {p:?}"
    );
}
#[test]
fn profile_draft_is_flushed_on_exit_path() {
    let d = tempfile::tempdir().unwrap();
    let mut state = HiringState {
        profile: "## 基本信息\n姓名：测试人".into(),
        profile_dirty: true,
        ..Default::default()
    };
    assert!(state.flush(d.path()));
    assert_eq!(
        fs::read_to_string(d.path().join("profile.md")).unwrap(),
        state.profile
    );
}

#[test]
fn account_selection_preserves_unsaved_settings_and_invalidates_send_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let stored = Settings {
        greeting: "已保存招呼".into(),
        ..Default::default()
    };
    stored.save(dir.path()).unwrap();
    let account = Store::open(dir.path())
        .unwrap()
        .create_account("账号2")
        .unwrap();
    let mut state = HiringState {
        settings: stored.clone(),
        confirmed: true,
        ..Default::default()
    };
    state.settings.greeting = "尚未保存的招呼".into();
    state
        .select_account(dir.path(), account.id.clone())
        .unwrap();
    assert_eq!(state.settings.greeting, "尚未保存的招呼");
    assert_eq!(Settings::load(dir.path()).unwrap().greeting, "已保存招呼");
    assert_eq!(Settings::load(dir.path()).unwrap().account_id, account.id);
    assert!(!state.confirmed);
    assert!(state.confirmation.is_none());
}
