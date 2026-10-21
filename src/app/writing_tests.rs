use super::editor_tests::{input_frame, pointer};
use super::*;

fn text_position(out: &egui::FullOutput, text: &str) -> egui::Pos2 {
    out.shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos + egui::vec2(4., 5.)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {text}"))
}

#[test]
fn writer_accepts_text_autosaves_previews_and_keeps_return_toolbar_fixed() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.new_writing(false);
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    let text = "# 开始记录\n\n轻量而简约。\n".repeat(50);
    input_frame(
        &mut app,
        &ctx,
        0.2,
        vec![egui::Event::Paste(text.clone())],
        vec![],
    );
    assert_eq!(app.writing.page.as_ref().unwrap().body, text);
    assert!(app.writing.dirty);
    let id = app.writing.page.as_ref().unwrap().id.clone();
    let output = input_frame(&mut app, &ctx, 1., vec![], vec![]);
    let title = text_position(&output, "未命名文档");
    let body = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos),
            _ => None,
        })
        .unwrap();
    assert!(
        (title.x - body.x).abs() < 12. && body.y > title.y + 25.,
        "title and body must share a vertical paper layout"
    );
    assert!(!app.writing.dirty);
    assert_eq!(
        app.db
            .as_ref()
            .unwrap()
            .writing_page(&id)
            .unwrap()
            .unwrap()
            .body,
        text
    );
    let preview = text_position(&output, "预览");
    input_frame(&mut app, &ctx, 1.1, pointer(preview, true), vec![]);
    input_frame(&mut app, &ctx, 1.2, pointer(preview, false), vec![]);
    assert!(app.writing.preview);
    input_frame(
        &mut app,
        &ctx,
        1.3,
        vec![
            egui::Event::PointerMoved(egui::pos2(700., 500.)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0., -2000.),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        vec![],
    );
    let output = input_frame(&mut app, &ctx, 1.4, vec![], vec![]);
    let back = text_position(&output, "← 文档");
    assert!(back.y < 80.);
    for old in ["文档索引", "大模型聊天", "更新内容索引", "添加文档索引"] {
        assert!(
            !output
                .shapes
                .iter()
                .any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==old))
        );
    }
    input_frame(&mut app, &ctx, 1.5, pointer(back, true), vec![]);
    input_frame(&mut app, &ctx, 1.6, pointer(back, false), vec![]);
    assert!(app.writing.page.is_none());
}

#[test]
fn text_undo_restores_the_previous_body_and_autosaves_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.new_writing(false);
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    input_frame(
        &mut app,
        &ctx,
        0.2,
        vec![egui::Event::Text("原正文".into())],
        vec![],
    );
    input_frame(&mut app, &ctx, 2., vec![], vec![]);
    input_frame(
        &mut app,
        &ctx,
        2.1,
        vec![egui::Event::Paste("第二段".into())],
        vec![],
    );
    assert_eq!(app.writing.page.as_ref().unwrap().body, "原正文第二段");
    input_frame(&mut app, &ctx, 3., vec![], vec![]);
    input_frame(
        &mut app,
        &ctx,
        3.1,
        vec![egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            },
        }],
        vec![],
    );
    assert_eq!(app.writing.page.as_ref().unwrap().body, "原正文");
    assert!(app.flush_writing());
    assert_eq!(
        app.db.as_ref().unwrap().writing_pages().unwrap()[0].body,
        "原正文"
    );
}

#[test]
fn duplicating_before_autosave_keeps_the_original_and_copy_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.new_writing(false);
    let id = app.writing.page.as_ref().unwrap().id.clone();
    app.writing.page.as_mut().unwrap().title = "刚写好的文档".into();
    app.writing.page.as_mut().unwrap().body = "复制前尚未自动保存的正文".into();
    app.mark_writing_changed(0.);
    app.save_writing_copy();
    assert_ne!(app.writing.page.as_ref().unwrap().id, id);
    assert!(!app.writing.dirty);
    drop(app);
    let app = PersonalDeskApp::open_data_directory(dir.path().into());
    let pages = app.db.as_ref().unwrap().writing_pages().unwrap();
    assert_eq!(pages.len(), 2);
    assert!(pages.iter().all(|p| p.body == "复制前尚未自动保存的正文"));
    assert!(
        pages
            .iter()
            .any(|p| p.id == id && p.title == "刚写好的文档")
    );
    assert!(
        pages
            .iter()
            .any(|p| p.id != id && p.title == "刚写好的文档（副本）")
    );
}

#[test]
fn managed_files_and_recovery_import_once_without_changing_sources_and_export_body() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("documents")).unwrap();
    fs::create_dir_all(root.join("recovery")).unwrap();
    let file = root.join("documents/old.md");
    fs::write(&file, "旧文档正文").unwrap();
    let recovery = uuid::Uuid::new_v4().to_string();
    let draft = root.join(format!("recovery/{recovery}.recovery.txt"));
    fs::write(&draft, "未保存的旧草稿").unwrap();
    fs::write(
        root.join(format!("recovery/{recovery}.json")),
        r#"{"original_path":"old.md"}"#,
    )
    .unwrap();
    let mut app = PersonalDeskApp::open_data_directory(root.into());
    assert_eq!(app.db.as_ref().unwrap().writing_pages().unwrap().len(), 2);
    app.new_writing(false);
    app.writing.page.as_mut().unwrap().body = "新写的文档".into();
    app.mark_writing_changed(0.);
    assert!(app.flush_writing());
    let zip_path = root.join("all.zip");
    export::write(root, &zip_path, export::Scope::All).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(zip_path).unwrap()).unwrap();
    let pages: serde_json::Value =
        serde_json::from_reader(zip.by_name("写作文档.json").unwrap()).unwrap();
    assert_eq!(pages.as_array().unwrap().len(), 3);
    assert!(
        pages
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["body"] == "新写的文档")
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), "旧文档正文");
    assert_eq!(fs::read_to_string(draft).unwrap(), "未保存的旧草稿");
    drop(app);
    let app = PersonalDeskApp::open_data_directory(root.into());
    assert_eq!(app.db.as_ref().unwrap().writing_pages().unwrap().len(), 3);
}

#[test]
fn daily_log_uses_local_date_reopens_existing_body_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
    assert!(app.new_writing_for_day(true, day));
    let id = app.writing.page.as_ref().unwrap().id.clone();
    assert_eq!(app.writing.page.as_ref().unwrap().title, "2026-10-21");
    app.writing.page.as_mut().unwrap().body = "今天完成了什么\n- 一件事".into();
    app.mark_writing_changed(0.);
    assert!(app.flush_writing());
    assert!(app.new_writing_for_day(true, day));
    assert_eq!(app.writing.page.as_ref().unwrap().id, id);
    assert!(app.writing.page.as_ref().unwrap().body.contains("一件事"));
    drop(app);
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.new_writing_for_day(true, day);
    assert_eq!(app.writing.page.as_ref().unwrap().id, id);
    assert_eq!(app.db.as_ref().unwrap().writing_pages().unwrap().len(), 1);
}

#[test]
fn interrupted_and_conflicting_drafts_recover_as_copies_without_overwriting_saved_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.new_writing(false);
    let id = app.writing.page.as_ref().unwrap().id.clone();
    app.writing.page.as_mut().unwrap().body = "保留的草稿".into();
    app.mark_writing_changed(0.);
    let mut other = Database::open(dir.path()).unwrap();
    let mut page = other.writing_page(&id).unwrap().unwrap();
    page.body = "另一个更新".into();
    other.save_book_page(&mut page).unwrap();
    assert!(!app.flush_writing());
    assert!(app.writing.dirty);
    assert_eq!(other.writing_page(&id).unwrap().unwrap().body, "另一个更新");
    drop(app);
    drop(other);
    let app = PersonalDeskApp::open_data_directory(dir.path().into());
    let notes = app.db.as_ref().unwrap().writing_pages().unwrap();
    assert_eq!(notes.len(), 2);
    assert!(notes.iter().any(|p| p.body == "保留的草稿"));
    assert!(notes.iter().any(|p| p.body == "另一个更新"));
    drop(app);
    let app = PersonalDeskApp::open_data_directory(dir.path().into());
    assert_eq!(app.db.as_ref().unwrap().writing_pages().unwrap().len(), 2);
}
