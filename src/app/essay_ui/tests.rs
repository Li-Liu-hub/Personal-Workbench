use super::*;
use crate::app::editor_tests::{input_frame, pointer};

fn center(out: &egui::FullOutput, text: &str) -> egui::Pos2 {
    out.shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == text => {
                Some(t.galley.rect.translate(t.pos.to_vec2()).center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {text}"))
}

#[test]
fn daily_essay_can_start_type_autosave_return_and_reopen_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.section = Section::Essays;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    let start = center(&out, "开始写作");
    input_frame(&mut app, &ctx, 0.2, pointer(start, true), vec![]);
    input_frame(&mut app, &ctx, 0.3, pointer(start, false), vec![]);
    input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
    input_frame(
        &mut app,
        &ctx,
        0.5,
        vec![egui::Event::Text("我愿意记录今天的一点想法。".into())],
        vec![],
    );
    assert_eq!(
        app.writing.page.as_ref().unwrap().body,
        "我愿意记录今天的一点想法。"
    );
    let id = app.writing.page.as_ref().unwrap().id.clone();
    let out = input_frame(&mut app, &ctx, 1.2, vec![], vec![]);
    assert!(!app.writing.dirty);
    let back = center(&out, "← 随笔");
    input_frame(&mut app, &ctx, 1.3, pointer(back, true), vec![]);
    input_frame(&mut app, &ctx, 1.4, pointer(back, false), vec![]);
    assert!(app.writing.page.is_none());
    let out = input_frame(&mut app, &ctx, 1.5, vec![], vec![]);
    let history = center(&out, "记录本");
    input_frame(&mut app, &ctx, 1.6, pointer(history, true), vec![]);
    input_frame(&mut app, &ctx, 1.7, pointer(history, false), vec![]);
    let out = input_frame(&mut app, &ctx, 1.8, vec![], vec![]);
    assert_eq!(app.essay.catalog.len(), 1);
    let row = &app.essay.catalog[0];
    let pos = center(&out, &format!("{}\n{}", row.title, row.day));
    input_frame(&mut app, &ctx, 1.9, pointer(pos, true), vec![]);
    input_frame(&mut app, &ctx, 2., pointer(pos, false), vec![]);
    assert_eq!(app.writing.page.as_ref().unwrap().id, id);
    drop(app);
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.section = Section::Essays;
    app.start_daily_essay();
    assert_eq!(
        app.writing.page.as_ref().unwrap().body,
        "我愿意记录今天的一点想法。"
    );
}

#[test]
fn essay_topic_card_and_editor_controls_fit_small_and_standard_windows() {
    for size in [[1000., 680.], [1280., 800.]] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        app.section = Section::Essays;
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let frame = |app: &mut PersonalDeskApp| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                    ..Default::default()
                },
                |ui| app.render_workspace(ui),
            );
            out.textures_delta.clear();
            out
        };
        frame(&mut app);
        let out = frame(&mut app);
        let title = app.essay.today.as_ref().unwrap().topic.title.clone();
        let title_pos = center(&out, &title);
        let button = center(&out, "开始写作");
        assert!(button.y > title_pos.y + 45. && button.x > 200. && button.x < size[0] - 20.);
        app.start_daily_essay();
        frame(&mut app);
        let out = frame(&mut app);
        for label in ["← 随笔", "写作", "预览", "更多"] {
            let pos = center(&out, label);
            assert!(
                pos.x > 184. && pos.x < size[0] - 12. && pos.y < 70.,
                "{size:?} {label} {pos:?}"
            );
        }
    }
}
