use super::*;
use crate::app::editor_tests::pointer;

#[test]
fn resume_defaults_to_the_selected_data_directory_even_without_a_database() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.db = None;
    app.ensure_resume_loaded();
    assert_eq!(app.resume.directory, dir.path().join("docs/简历"));
    assert!(app.resume.document.as_ref().unwrap().name.is_empty());
    assert!(!app.resume.dirty);
}

fn frame(
    app: &mut PersonalDeskApp,
    ctx: &egui::Context,
    size: [f32; 2],
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
            events,
            ..Default::default()
        },
        |ui| app.render_workspace(ui),
    );
    output.textures_delta.clear();
    output
}
fn position(output: &egui::FullOutput, text: &str) -> egui::Pos2 {
    output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos + egui::vec2(8., 8.)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {text}"))
}
fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        },
    }
}

#[test]
fn resume_form_edits_with_keyboard_saves_reopens_and_keeps_conflicting_draft() {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().join("docs/简历");
    let store = Store {
        directory: directory.clone(),
    };
    store
        .save(
            &Resume {
                name: "原姓名".into(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    app.db
        .as_mut()
        .unwrap()
        .set_setting("resume.directory", &directory.to_string_lossy())
        .unwrap();
    app.section = Section::Resume;
    app.load_resume();
    app.resume.auto_preview = false;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    let mut output = frame(&mut app, &ctx, [1280., 800.], vec![]);
    for _ in 0..3 {
        output = frame(&mut app, &ctx, [1280., 800.], vec![]);
    }
    let at = position(&output, "原姓名");
    frame(&mut app, &ctx, [1280., 800.], pointer(at, true));
    frame(&mut app, &ctx, [1280., 800.], pointer(at, false));
    frame(
        &mut app,
        &ctx,
        [1280., 800.],
        vec![key(egui::Key::A), egui::Event::Text("更新姓名".into())],
    );
    assert_eq!(app.resume.document.as_ref().unwrap().name, "更新姓名");
    assert!(app.resume.dirty);
    frame(&mut app, &ctx, [1280., 800.], vec![key(egui::Key::S)]);
    assert!(!app.resume.dirty);
    assert_eq!(store.load().unwrap().document.name, "更新姓名");
    let mut reopened = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    reopened.load_resume();
    assert_eq!(reopened.resume.document.as_ref().unwrap().name, "更新姓名");
    let mut external = store.load().unwrap();
    external.document.name = "外部修改".into();
    store
        .save(&external.document, external.revision.as_deref())
        .unwrap();
    reopened.resume.document.as_mut().unwrap().name = "保留草稿".into();
    reopened.resume.dirty = true;
    assert!(!reopened.flush_resume());
    assert!(reopened.resume.save_conflict);
    assert_eq!(reopened.resume.document.as_ref().unwrap().name, "保留草稿");
    assert_eq!(store.load().unwrap().document.name, "外部修改");
    reopened.save_resume_over_external();
    assert!(!reopened.resume.dirty);
    assert!(reopened.resume.save_error.is_empty());
    assert_eq!(store.load().unwrap().document.name, "保留草稿");
    let backup = fs::read_dir(&directory)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("resume.external-")
        })
        .unwrap();
    let external: Resume = serde_json::from_slice(&fs::read(backup.path()).unwrap()).unwrap();
    assert_eq!(external.name, "外部修改");
}

#[test]
#[ignore = "requires local Typst and Windows fonts"]
fn conflicting_style_edits_still_preview_and_export_without_overwriting_disk() {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().join("resume");
    let store = Store {
        directory: directory.clone(),
    };
    store
        .save(
            &Resume {
                name: "原内容".into(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    app.db
        .as_mut()
        .unwrap()
        .set_setting("resume.directory", &directory.to_string_lossy())
        .unwrap();
    app.section = Section::Resume;
    app.load_resume();
    app.resume.auto_preview = true;
    let mut external = store.load().unwrap();
    external.document.name = "外部内容".into();
    store
        .save(&external.document, external.revision.as_deref())
        .unwrap();
    let external_bytes = fs::read(directory.join("resume.json")).unwrap();
    fs::write(directory.join("简历_编辑版.pdf"), b"keep external PDF").unwrap();
    let draft = app.resume.document.as_mut().unwrap();
    draft.style.font_pt = 12.;
    draft.style.margin_mm = 12.;
    draft.name = "当前草稿".into();
    let draft_revision = draft.revision().unwrap();
    app.resume.dirty = true;
    app.resume.changed = Some(Instant::now() - Duration::from_secs(2));
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            max_texture_side: Some(8192),
            ..Default::default()
        },
        |_| {},
    );
    output.textures_delta.clear();
    // This must start automatically even though autosave hits a conflict first.
    app.poll_resume(&ctx);
    assert!(app.resume.save_conflict);
    assert!(app.resume.busy());
    let wait = |app: &mut PersonalDeskApp| {
        let deadline = Instant::now() + Duration::from_secs(60);
        while app.resume.busy() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
            app.poll_resume(&ctx);
        }
        assert!(!app.resume.busy());
        assert!(app.resume.error.is_empty(), "{}", app.resume.error);
    };
    wait(&mut app);
    assert_eq!(app.resume.preview_revision.as_ref(), Some(&draft_revision));
    assert!(app.resume.textures[0].size()[0] >= 2480);
    assert!(app.resume.dirty);
    assert!(!app.resume.save_error.is_empty());
    assert_eq!(
        fs::read(directory.join("resume.json")).unwrap(),
        external_bytes
    );
    assert_eq!(
        fs::read(directory.join("简历_编辑版.pdf")).unwrap(),
        b"keep external PDF"
    );
    let export = dir.path().join("draft.pdf");
    app.start_resume_render(Some(export.clone()));
    wait(&mut app);
    assert!(fs::read(export).unwrap().starts_with(b"%PDF-"));
    assert_eq!(
        fs::read(directory.join("resume.json")).unwrap(),
        external_bytes
    );
    assert_eq!(
        fs::read(directory.join("简历_编辑版.pdf")).unwrap(),
        b"keep external PDF"
    );
    app.resume.preview_ppi = 450;
    app.start_resume_render(None);
    wait(&mut app);
    assert!(app.resume.textures[0].size()[0] >= 3720);
    let mut output = ctx.run_ui(
        egui::RawInput {
            max_texture_side: Some(2048),
            ..Default::default()
        },
        |_| {},
    );
    output.textures_delta.clear();
    app.start_resume_render(None);
    wait(&mut app);
    assert!(app.resume.textures[0].size().into_iter().all(|n| n <= 2048));
}

#[test]
fn resume_toolbar_editor_and_preview_fit_supported_sizes() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    app.db
        .as_mut()
        .unwrap()
        .set_setting(
            "resume.directory",
            &dir.path().join("docs/简历").to_string_lossy(),
        )
        .unwrap();
    app.section = Section::Resume;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    for size in [[1000., 680.], [1280., 800.]] {
        let mut output = frame(&mut app, &ctx, size, vec![]);
        for _ in 0..3 {
            output = frame(&mut app, &ctx, size, vec![]);
        }
        for text in ["保存", "刷新预览", "导出 PDF", "更多", "排版预览", "姓名"] {
            let p = position(&output, text);
            assert!(
                p.x >= 184. && p.x < size[0] && p.y > 0. && p.y < size[1],
                "{text} {p:?} {size:?}"
            );
        }
        assert_eq!(
            app.resume.document.as_ref().unwrap().style.line_pt,
            14.72,
            "rendering the form must not round the original line height"
        );
        assert!(
            !app.resume.dirty,
            "viewing the original style must not silently change it"
        );
    }
}
