use super::*;
use crate::app::editor_tests::{input_frame, pointer};

fn position(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() * 0.5),
        _ => None,
    })
}
#[test]
fn campus_tool_opens_from_both_gallery_modes_and_respects_search() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.section = Section::Tools;
    for mode in [ViewMode::Grid, ViewMode::List] {
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        app.set_view_mode(mode);
        app.search_query = "校招".into();
        app.campus.open = false;
        input_frame(&mut app, &ctx, 0., vec![], vec![]);
        let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        let pos = position(&output, "校招搜索").unwrap();
        input_frame(&mut app, &ctx, 0.2, pointer(pos, true), vec![]);
        input_frame(&mut app, &ctx, 0.3, pointer(pos, false), vec![]);
        assert!(app.campus.open);
        let output = input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
        assert!(position(&output, "开始采集 / 续采").is_some());
        app.campus.open = false;
        app.search_query = "no-match".into();
        let output = input_frame(&mut app, &ctx, 0.5, vec![], vec![]);
        assert!(position(&output, "校招搜索").is_none());
    }
}
#[test]
fn closed_panel_saves_background_results_and_reopens_without_losing_flags() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    let (sender, receiver) = mpsc::channel();
    app.campus.receiver = Some(receiver);
    app.campus.open = false;
    let mut job = Job {
        title: "样例公司2027届Java招聘".into(),
        source_url: "https://example.com/jobs/7".into(),
        ..Default::default()
    };
    job.identify();
    sender
        .send(Message::Progress(Progress::Jobs(vec![job.clone()])))
        .unwrap();
    sender
        .send(Message::Finished(Ok(campus::Outcome {
            jobs: vec![job],
            ..Default::default()
        })))
        .unwrap();
    app.poll_campus();
    assert!(!app.campus.busy());
    assert_eq!(app.campus.jobs.len(), 1);
    assert_eq!(app.campus.new.len(), 1);
    let mut restored = PersonalDeskApp::open_data_directory(dir.path().into());
    restored.load_campus();
    assert_eq!(restored.campus.jobs.len(), 1);
}
#[test]
fn filters_keep_ignored_records_recoverable_and_separate_new_from_refreshed() {
    let mut state = CampusState::default();
    let mut job = Job {
        title: "Java开发".into(),
        id: "7".into(),
        ..Default::default()
    };
    assert!(state.visible(&job));
    job.ignored = true;
    assert!(!state.visible(&job));
    state.view = 3;
    assert!(state.visible(&job));
    state.only_new = true;
    assert!(!state.visible(&job));
    state.new.insert("7".into());
    assert!(state.visible(&job));
    state.filter = "Python".into();
    assert!(!state.visible(&job));
}
