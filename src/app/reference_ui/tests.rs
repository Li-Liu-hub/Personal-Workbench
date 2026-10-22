use super::*;
use crate::app::editor_tests::{input_frame, pointer};
fn center(out: &egui::FullOutput, label: &str) -> egui::Pos2 {
    out.shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == label => {
                Some(t.galley.rect.translate(t.pos.to_vec2()).center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {label}"))
}
#[test]
fn reference_workspace_searches_edits_saves_and_preserves_unfinished_drafts() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    app.section = Section::Reference;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    assert_eq!(app.reference.rows.len(), 128);
    let add = center(&output, "+ 添加指令");
    input_frame(&mut app, &ctx, 0.2, pointer(add, true), vec![]);
    input_frame(&mut app, &ctx, 0.3, pointer(add, false), vec![]);
    let id = egui::Id::unique(("reference-field", "标题"));
    ctx.memory_mut(|m| m.request_focus(id));
    input_frame(
        &mut app,
        &ctx,
        0.4,
        vec![egui::Event::Text("我的快速指令".into())],
        vec![],
    );
    assert_eq!(app.reference.draft.as_ref().unwrap().title, "我的快速指令");
    let draft_id = app.reference.draft.as_ref().unwrap().id.clone();
    let output = input_frame(&mut app, &ctx, 0.5, vec![], vec![]);
    let save = center(&output, "保存");
    input_frame(&mut app, &ctx, 0.6, pointer(save, true), vec![]);
    input_frame(&mut app, &ctx, 0.7, pointer(save, false), vec![]);
    assert!(app.reference.draft.is_none());
    assert_eq!(
        app.db
            .as_ref()
            .unwrap()
            .reference_entry(&draft_id)
            .unwrap()
            .unwrap()
            .title,
        "我的快速指令"
    );
    app.search_query = "我的快速".into();
    input_frame(&mut app, &ctx, 0.8, vec![], vec![]);
    assert_eq!(app.reference.rows.len(), 1);
    app.reference.draft = app.reference.selected.clone();
    app.reference.draft.as_mut().unwrap().syntax = "unfinished code".into();
    app.add_reference();
    assert_eq!(
        app.reference.draft.as_ref().unwrap().syntax,
        "unfinished code"
    );
    app.search_query = "HashMap".into();
    input_frame(&mut app, &ctx, 0.9, vec![], vec![]);
    assert_eq!(
        app.reference.draft.as_ref().unwrap().syntax,
        "unfinished code"
    );
    assert!(app.reference.rows.len() >= 2);
    app.reference.draft = None;
    app.reference.filter.language = "Rust".into();
    input_frame(&mut app, &ctx, 1., vec![], vec![]);
    assert!(!app.reference.rows.is_empty());
    assert!(app.reference.rows.iter().all(|r| r.language == "Rust"));
    assert!(app.reference.rows.iter().any(|r| r.id == "ref:rust:10"));
}

#[test]
fn reference_workspace_controls_and_preview_fit_supported_window_sizes() {
    for size in [[1000., 680.], [1280., 800.], [1920., 1080.]] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
        app.section = Section::Reference;
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let mut out = egui::FullOutput::default();
        for _ in 0..4 {
            out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                    ..Default::default()
                },
                |ui| app.render_workspace(ui),
            );
            out.textures_delta.clear();
        }
        for label in ["+ 添加指令", "全部语言", "全部分类", "编辑", "复制示例"] {
            let pos = center(&out, label);
            assert!(
                pos.x > 184. && pos.x < size[0] - 16. && pos.y > 0. && pos.y < size[1] - 24.,
                "{size:?} {label} {pos:?}"
            );
        }
        assert!(
            out.shapes
                .iter()
                .filter(
                    |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().contains(" · "))
                )
                .count()
                < 35
        );
    }
}
