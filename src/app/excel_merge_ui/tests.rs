use super::*;
use crate::app::editor_tests::{input_frame, pointer};

fn text_position(output: &egui::FullOutput, value: &str) -> Option<egui::Pos2> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.text() == value => {
            Some(text.pos + text.galley.size() * 0.5)
        }
        _ => None,
    })
}

#[test]
fn excel_merge_builtin_opens_with_click_in_both_views_and_respects_search() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    app.section = Section::Tools;
    for mode in [ViewMode::Grid, ViewMode::List] {
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        app.set_view_mode(mode);
        app.search_query = "xlsx".into();
        app.excel_merge.open = false;
        input_frame(&mut app, &ctx, 0., vec![], vec![]);
        let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        let pos = text_position(&output, "Excel 批量合并").expect("builtin tool missing");
        input_frame(&mut app, &ctx, 0.2, pointer(pos, true), vec![]);
        input_frame(&mut app, &ctx, 0.3, pointer(pos, false), vec![]);
        assert!(app.excel_merge.open);
        let output = input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
        assert!(text_position(&output, "开始合并").is_some());
        app.excel_merge.open = false;
        app.search_query = "no-match".into();
        let output = input_frame(&mut app, &ctx, 0.5, vec![], vec![]);
        assert!(text_position(&output, "Excel 批量合并").is_none());
    }
}

#[test]
fn excel_merge_background_completion_survives_closing_panel_and_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    fs::create_dir(&input).unwrap();
    crate::excel_merge::tests::fixture(&input.join("a.xlsx"), &[&[&["ID"], &["007"]]]);
    let mut state = ExcelMergeState::default();
    state.input = input.display().to_string();
    state.output = dir.path().join("result.xlsx").display().to_string();
    state.open = true;
    state.start();
    assert!(state.busy());
    state.open = false;
    let until = std::time::Instant::now() + Duration::from_secs(10);
    while state.busy() && std::time::Instant::now() < until {
        state.poll();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!state.busy());
    assert_eq!(state.result.as_ref().unwrap().rows, 1);
    state.open = true;
    assert!(state.result.as_ref().unwrap().output_file.exists());
    state.start();
    assert!(!state.busy());
    assert!(state.error.contains("覆盖"));
}
