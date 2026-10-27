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
fn pdf_images_builtin_opens_in_both_views_and_respects_search() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    app.section = Section::Tools;
    for mode in [ViewMode::Grid, ViewMode::List] {
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        app.set_view_mode(mode);
        app.search_query = "jpg".into();
        app.pdf_images.open = false;
        input_frame(&mut app, &ctx, 0., vec![], vec![]);
        let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        let pos = text_position(&output, "PDF 转图片").unwrap();
        input_frame(&mut app, &ctx, 0.2, pointer(pos, true), vec![]);
        input_frame(&mut app, &ctx, 0.3, pointer(pos, false), vec![]);
        assert!(app.pdf_images.open);
        let output = input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
        assert!(text_position(&output, "开始转换").is_some());
        for label in ["PNG", "JPG", "WebP", "BMP"] {
            assert!(text_position(&output, label).is_some());
        }
        app.pdf_images.open = false;
        app.search_query = "not-a-match".into();
        let output = input_frame(&mut app, &ctx, 0.5, vec![], vec![]);
        assert!(text_position(&output, "PDF 转图片").is_none());
    }
}

#[test]
fn background_conversion_survives_panel_close() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("文档.pdf");
    crate::pdf_images::tests::fixture(&input);
    let mut state = PdfImagesState::default();
    state.input = input.display().to_string();
    state.output = dir.path().display().to_string();
    state.open = true;
    state.start();
    assert!(state.busy());
    state.open = false;
    let until = std::time::Instant::now() + Duration::from_secs(30);
    while state.busy() && std::time::Instant::now() < until {
        state.poll();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!state.busy(), "conversion timeout");
    assert!(state.error.is_empty(), "{}", state.error);
    assert_eq!(state.result.as_ref().unwrap().pages, 3);
    assert!(state.result.as_ref().unwrap().folder.is_dir());
}
