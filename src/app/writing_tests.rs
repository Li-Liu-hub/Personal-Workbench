use super::editor_tests::{input_frame, pointer};
use super::*;

#[test]
fn classifying_the_open_document_flushes_body_and_refreshes_its_revision() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    assert!(app.new_writing(false));
    let id = app.writing.page.as_ref().unwrap().id.clone();
    app.writing.page.as_mut().unwrap().body = "unsaved body".into();
    app.mark_writing_changed(0.);
    app.save_record_classification(TagScope::BookPages, &id, &["work".into()]);
    assert!(!app.writing.dirty);
    let page = app.writing.page.as_mut().unwrap();
    assert_eq!(page.body, "unsaved body");
    assert_eq!(page.tags, vec!["work"]);
    page.body.push_str(" continued");
    app.mark_writing_changed(1.);
    assert!(
        app.flush_writing(),
        "own tag change must not leave an obsolete editor revision"
    );
    let saved = app.db.as_ref().unwrap().writing_page(&id).unwrap().unwrap();
    assert_eq!(saved.body, "unsaved body continued");
    assert_eq!(saved.tags, vec!["work"]);
}

fn floating_frame(
    app: &mut PersonalDeskApp,
    ctx: &egui::Context,
    time: f64,
    events: Vec<egui::Event>,
    size: [f32; 2],
    close: bool,
) -> egui::FullOutput {
    let output = std::rc::Rc::new(std::cell::RefCell::new(None));
    let result = output.clone();
    ctx.set_embed_viewports(false);
    egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
        assert_eq!(
            viewport.builder.min_inner_size,
            Some(egui::vec2(340., 300.))
        );
        let mut input = egui::RawInput {
            viewport_id: viewport.ids.this,
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
            time: Some(time),
            events: events.clone(),
            ..Default::default()
        };
        input.viewports.insert(
            viewport.ids.this,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                events: if close {
                    vec![egui::ViewportEvent::Close]
                } else {
                    vec![]
                },
                ..Default::default()
            },
        );
        *result.borrow_mut() = Some(ctx.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui)));
    });
    let root_output = ctx.run_ui(
        egui::RawInput {
            time: Some(time),
            ..Default::default()
        },
        |ui| app.render_floating_writer(ui.ctx()),
    );
    let mut out = output.borrow_mut().take().unwrap();
    for (id, viewport) in root_output.viewport_output {
        if let Some(existing) = out.viewport_output.get_mut(&id) {
            existing.commands.extend(viewport.commands);
        } else {
            out.viewport_output.insert(id, viewport);
        }
    }
    out.textures_delta.clear();
    out
}

#[test]
fn floating_writer_types_autosaves_outside_documents_and_returns_to_same_page() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.new_writing(false);
    let id = app.writing.page.as_ref().unwrap().id.clone();
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    let out = input_frame(&mut app, &ctx, 0., vec![], vec![]);
    let button = text_position(&out, "悬浮窗");
    input_frame(&mut app, &ctx, 0.1, pointer(button, true), vec![]);
    input_frame(&mut app, &ctx, 0.2, pointer(button, false), vec![]);
    assert!(app.writing.floating);
    app.section = Section::Resume;
    app.open_floating_writer();
    for (n, size) in [[340., 300.], [440., 560.]].into_iter().enumerate() {
        let out = floating_frame(&mut app, &ctx, 1. + n as f64 * 0.1, vec![], size, false);
        for label in ["置顶", "保存", "回到主窗口", "更多"] {
            let position = text_position(&out, label);
            assert!(position.x > 0. && position.x < size[0] && position.y < size[1]);
        }
    }
    floating_frame(
        &mut app,
        &ctx,
        1.3,
        vec![egui::Event::Paste("简历中的项目问题\n核实后的总结".into())],
        [440., 560.],
        false,
    );
    assert!(app.writing.dirty);
    let out = floating_frame(&mut app, &ctx, 2., vec![], [440., 560.], false);
    assert!(!app.writing.dirty);
    let pinned = text_position(&out, "置顶");
    floating_frame(
        &mut app,
        &ctx,
        2.1,
        pointer(pinned, true),
        [440., 560.],
        false,
    );
    let out = floating_frame(
        &mut app,
        &ctx,
        2.2,
        pointer(pinned, false),
        [440., 560.],
        false,
    );
    let child = egui::ViewportId::from_hash_of("floating-writer");
    assert!(
        out.viewport_output[&child]
            .commands
            .iter()
            .any(|c| matches!(
                c,
                egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal)
            ))
    );
    let back = text_position(&out, "回到主窗口");
    floating_frame(
        &mut app,
        &ctx,
        2.3,
        pointer(back, true),
        [440., 560.],
        false,
    );
    floating_frame(
        &mut app,
        &ctx,
        2.4,
        pointer(back, false),
        [440., 560.],
        false,
    );
    assert!(!app.writing.floating);
    assert_eq!(app.section, Section::Documents);
    assert_eq!(app.writing.page.as_ref().unwrap().id, id);
    drop(app);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.writing_page(&id).unwrap().unwrap().body,
        "简历中的项目问题\n核实后的总结"
    );
}

#[test]
fn format_toolbar_inserts_code_blocks_in_main_and_floating_writers() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    assert!(app.new_writing(false));
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    let out = input_frame(&mut app, &ctx, 0., vec![], vec![]);
    let button = text_position(&out, "代码块");
    input_frame(&mut app, &ctx, 0.1, pointer(button, true), vec![]);
    input_frame(&mut app, &ctx, 0.2, pointer(button, false), vec![]);
    assert!(app.writing.page.as_ref().unwrap().body.contains("```"));
    assert!(app.flush_writing());
    app.open_floating_writer();
    floating_frame(&mut app, &ctx, 1., vec![], [600., 600.], false);
    let out = floating_frame(&mut app, &ctx, 1.05, vec![], [600., 600.], false);
    let button = text_position(&out, "代码块");
    floating_frame(
        &mut app,
        &ctx,
        1.1,
        pointer(button, true),
        [600., 600.],
        false,
    );
    floating_frame(
        &mut app,
        &ctx,
        1.2,
        pointer(button, false),
        [600., 600.],
        false,
    );
    assert_eq!(
        app.writing
            .page
            .as_ref()
            .unwrap()
            .body
            .matches("```")
            .count(),
        4
    );
    assert!(app.flush_writing());
}

#[test]
fn floating_close_keeps_conflicting_draft_open_and_never_overwrites_other_writer() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.new_writing(false);
    app.open_floating_writer();
    let id = app.writing.page.as_ref().unwrap().id.clone();
    let mut other = Database::open(dir.path()).unwrap();
    let mut page = other.writing_page(&id).unwrap().unwrap();
    page.body = "另一窗口的新内容".into();
    other.save_book_page(&mut page).unwrap();
    app.writing.page.as_mut().unwrap().body = "悬浮窗未保存总结".into();
    app.mark_writing_changed(0.);
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    let out = floating_frame(&mut app, &ctx, 1., vec![], [340., 300.], true);
    assert!(app.writing.floating && app.writing.dirty);
    assert!(!app.writing.error.is_empty());
    let child = egui::ViewportId::from_hash_of("floating-writer");
    assert!(
        out.viewport_output[&child]
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
    );
    assert_eq!(
        other.writing_page(&id).unwrap().unwrap().body,
        "另一窗口的新内容"
    );
    drop(app);
    drop(other);
    let mut restored = PersonalDeskApp::open_data_directory(dir.path().into());
    assert!(
        restored
            .db
            .as_ref()
            .unwrap()
            .writing_pages()
            .unwrap()
            .iter()
            .any(|p| p.body == "悬浮窗未保存总结")
    );
    restored.open_writing(&id);
    restored.open_floating_writer();
    floating_frame(&mut restored, &ctx, 2., vec![], [440., 560.], true);
    assert!(!restored.writing.floating);
}

#[test]
fn grouped_book_reopens_and_accepts_edits_from_both_catalog_routes() {
    for section in [Section::Documents, Section::Books] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        let db = app.db.as_mut().unwrap();
        let a = db.create_writing("第一篇", None, None).unwrap();
        let b = db.create_writing("第二篇", None, None).unwrap();
        let book = db
            .group_writing(&[a.id.clone(), b.id.clone()], None)
            .unwrap();
        app.section = section;
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let out = input_frame(&mut app, &ctx, 0., vec![], vec![]);
        let position = text_position(&out, &book.title);
        input_frame(&mut app, &ctx, 0.1, pointer(position, true), vec![]);
        input_frame(&mut app, &ctx, 0.2, pointer(position, false), vec![]);
        assert!(app.writing.page.is_some());
        let id = app.writing.page.as_ref().unwrap().id.clone();
        input_frame(&mut app, &ctx, 0.3, vec![], vec![]);
        input_frame(
            &mut app,
            &ctx,
            0.4,
            vec![egui::Event::Paste("合成图书后继续修改".into())],
            vec![],
        );
        assert_eq!(
            app.writing.page.as_ref().unwrap().body,
            "合成图书后继续修改"
        );
        assert!(app.save_visible_rows());
        drop(app);
        let db = Database::open(dir.path()).unwrap();
        assert_eq!(
            db.writing_page(&id).unwrap().unwrap().body,
            "合成图书后继续修改"
        );
    }
}

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

fn drag_writing(app: &mut PersonalDeskApp, ctx: &egui::Context, from: egui::Pos2, to: egui::Pos2) {
    input_frame(app, ctx, 1., pointer(from, true), vec![]);
    input_frame(
        app,
        ctx,
        1.1,
        vec![egui::Event::PointerMoved(from + egui::vec2(18., 0.))],
        vec![],
    );
    input_frame(app, ctx, 1.2, vec![egui::Event::PointerMoved(to)], vec![]);
    input_frame(app, ctx, 1.3, pointer(to, false), vec![]);
    input_frame(app, ctx, 1.4, vec![], vec![]);
}

#[test]
fn dragging_diary_into_existing_book_keeps_title_content_and_daily_link() {
    for mode in [crate::views::ViewMode::Grid, crate::views::ViewMode::List] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        let mut book = crate::library::Book::new();
        book.title = "我的日记本".into();
        app.db.as_mut().unwrap().save_book(&book).unwrap();
        app.db
            .as_mut()
            .unwrap()
            .create_writing("已存在的篇章", Some(&book.id), None)
            .unwrap();
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        app.new_writing_for_day(true, day);
        let id = app.writing.page.as_ref().unwrap().id.clone();
        app.writing.page.as_mut().unwrap().body = "完整的日记正文，不能被归组覆盖。".into();
        app.mark_writing_changed(0.);
        assert!(app.flush_writing());
        app.writing.page = None;
        app.library.book = None;
        app.set_view_mode(mode);
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        input_frame(&mut app, &ctx, 0., vec![], vec![]);
        let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        drag_writing(
            &mut app,
            &ctx,
            text_position(&out, "2026-10-22"),
            text_position(&out, "我的日记本"),
        );
        let db = app.db.as_ref().unwrap();
        let saved = db.writing_page(&id).unwrap().unwrap();
        assert_eq!(
            saved.book_id, book.id,
            "diary should move into the existing book"
        );
        assert_eq!(saved.title, "2026-10-22");
        assert_eq!(saved.body, "完整的日记正文，不能被归组覆盖。");
        assert_eq!(db.books().unwrap().len(), 1);
        assert_eq!(db.books().unwrap()[0].title, "我的日记本");
        assert_eq!(app.writing.page.as_ref().unwrap().id, id);
        assert_eq!(app.library.book.as_ref().unwrap().id, book.id);
        drop(app);
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        assert!(app.new_writing_for_day(true, day));
        assert_eq!(app.writing.page.as_ref().unwrap().id, id);
        assert_eq!(app.library.book.as_ref().unwrap().id, book.id);
        assert_eq!(
            app.db.as_ref().unwrap().book_pages(&book.id).unwrap().len(),
            2
        );
    }
}

#[test]
fn dropping_between_pages_in_a_book_creates_an_editable_child_book() {
    for mode in [crate::views::ViewMode::Grid, crate::views::ViewMode::List] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        let mut book = crate::library::Book::new();
        book.title = "日常记录".into();
        let db = app.db.as_mut().unwrap();
        db.save_book(&book).unwrap();
        let first = db.create_writing("第一篇", Some(&book.id), None).unwrap();
        let diary = db
            .create_writing("2026-10-22", Some(&book.id), Some("2026-10-22"))
            .unwrap();
        app.section = Section::Documents;
        app.library.book = Some(book.clone());
        app.set_view_mode(mode);
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        input_frame(&mut app, &ctx, 0., vec![], vec![]);
        let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        drag_writing(
            &mut app,
            &ctx,
            text_position(&out, "2026-10-22"),
            text_position(&out, "第一篇"),
        );
        let child = app.library.book.clone().unwrap();
        assert_eq!(child.parent_id.as_deref(), Some(book.id.as_str()));
        assert_ne!(child.id, book.id);
        let db = app.db.as_ref().unwrap();
        assert_eq!(db.books().unwrap().len(), 2);
        assert_eq!(db.book_pages(&child.id).unwrap().len(), 2);
        assert!(db.book_pages(&book.id).unwrap().is_empty());
        assert_eq!(app.writing.page.as_ref().unwrap().id, diary.id);
        app.writing.page.as_mut().unwrap().body = "合并后继续编辑".into();
        app.mark_writing_changed(2.);
        assert!(app.flush_writing());
        app.open_parent_book();
        assert!(app.writing.page.is_none());
        assert_eq!(app.child_books(Some(&book.id))[0].id, child.id);
        assert!(app.enter_book(child.clone()));
        assert!(app.open_writing(&first.id));
        drop(app);
        let db = Database::open(dir.path()).unwrap();
        assert_eq!(
            db.writing_page(&diary.id).unwrap().unwrap().body,
            "合并后继续编辑"
        );
        assert_eq!(
            db.books()
                .unwrap()
                .iter()
                .find(|b| b.id == child.id)
                .unwrap()
                .parent_id
                .as_deref(),
            Some(book.id.as_str())
        );
    }
}

#[test]
fn grouping_root_diary_opens_its_own_tab_and_continues_saving_after_restart() {
    for mode in [crate::views::ViewMode::Grid, crate::views::ViewMode::List] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        let db = app.db.as_mut().unwrap();
        let other = db.create_writing("想一起保存的文档", None, None).unwrap();
        let mut diary = db
            .create_writing("2026-10-22", None, Some("2026-10-22"))
            .unwrap();
        diary.body = "不会消失的日记正文".into();
        diary.tags = vec!["日记".into()];
        db.save_book_page(&mut diary).unwrap();
        app.section = Section::Documents;
        app.set_view_mode(mode);
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        input_frame(&mut app, &ctx, 0., vec![], vec![]);
        let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        drag_writing(
            &mut app,
            &ctx,
            text_position(&out, "2026-10-22"),
            text_position(&out, "想一起保存的文档"),
        );
        assert_eq!(app.writing.page.as_ref().unwrap().id, diary.id);
        let book = app.library.book.as_ref().unwrap().clone();
        assert_eq!(book.title, "新建图书");
        let out = input_frame(&mut app, &ctx, 1.5, vec![], vec![]);
        for title in [&book.title, &diary.title, &other.title] {
            text_position(&out, title);
        }
        app.writing
            .page
            .as_mut()
            .unwrap()
            .body
            .push_str("\n归组后继续写");
        app.mark_writing_changed(2.);
        assert!(
            app.flush_writing(),
            "grouping must refresh the editor revision"
        );
        drop(app);
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        app.new_writing_for_day(true, chrono::NaiveDate::from_ymd_opt(2026, 10, 22).unwrap());
        let saved = app.writing.page.as_ref().unwrap();
        assert_eq!(saved.id, diary.id);
        assert_eq!(saved.title, diary.title);
        assert_eq!(saved.tags, diary.tags);
        assert_eq!(saved.body, "不会消失的日记正文\n归组后继续写");
        assert_eq!(saved.book_id, book.id);
        assert_eq!(app.db.as_ref().unwrap().writing_pages().unwrap().len(), 2);
        assert_eq!(app.db.as_ref().unwrap().books().unwrap().len(), 1);
    }
}

#[test]
fn failed_writing_group_rolls_back_and_repeated_drop_does_not_change_saved_pages() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut book = crate::library::Book::new();
    book.title = "保留名称".into();
    db.save_book(&book).unwrap();
    let page = db.create_writing("日记", None, Some("2026-10-22")).unwrap();
    assert!(
        db.group_writing(&[page.id.clone(), "missing-page".into()], Some(&book.id))
            .is_err()
    );
    assert_eq!(
        db.writing_page(&page.id).unwrap().unwrap().book_id,
        page.book_id
    );
    assert!(db.group_writing(&[], None).is_err());
    assert!(
        db.group_writing(&[page.id.clone(), page.id.clone()], None)
            .is_err()
    );
    db.group_writing(std::slice::from_ref(&page.id), Some(&book.id))
        .unwrap();
    let moved = db.writing_page(&page.id).unwrap().unwrap();
    db.group_writing(std::slice::from_ref(&page.id), Some(&book.id))
        .unwrap();
    assert_eq!(
        db.writing_page(&page.id).unwrap().unwrap().revision,
        moved.revision
    );
    assert_eq!(db.books().unwrap()[0].title, book.title);
}

#[test]
fn nesting_books_survives_restart_rejects_cycles_and_hides_deleted_subtrees() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut parent = crate::library::Book::new();
    parent.title = "父图书".into();
    db.save_book(&parent).unwrap();
    let mut child = crate::library::Book::new();
    child.title = "子图书".into();
    db.save_book(&child).unwrap();
    db.move_book(&child.id, Some(&parent.id)).unwrap();
    let mut grandchild = crate::library::Book::new();
    grandchild.title = "孙图书".into();
    grandchild.parent_id = Some(child.id.clone());
    db.save_book(&grandchild).unwrap();
    let mut page = db
        .create_writing("深层页面", Some(&grandchild.id), None)
        .unwrap();
    page.body = "正文和标签完整保留".into();
    page.tags = vec!["测试".into()];
    db.save_book_page(&mut page).unwrap();
    let mut stale = page.clone();
    assert!(db.move_book(&parent.id, Some(&grandchild.id)).is_err());
    assert!(db.move_book(&child.id, Some(&child.id)).is_err());
    assert!(db.move_book(&child.id, Some("missing")).is_err());
    db.save_book_draft("other", &page).unwrap();
    db.set_library_deleted(&parent.id, false, true).unwrap();
    assert!(db.books().unwrap().is_empty());
    assert!(db.writing_pages().unwrap().is_empty());
    assert!(db.writing_summaries("").unwrap().is_empty());
    assert!(db.book_pages(&grandchild.id).unwrap().is_empty());
    assert!(db.writing_page(&page.id).unwrap().is_none());
    assert!(
        db.create_writing("不能写入已删除的子图书", Some(&grandchild.id), None)
            .is_err()
    );
    assert!(db.save_book_page(&mut stale).is_err());
    db.recover_writing_drafts().unwrap();
    assert!(db.writing_pages().unwrap().is_empty());
    db.set_library_deleted(&parent.id, false, false).unwrap();
    assert_eq!(db.books().unwrap().len(), 3);
    assert!(db.save_book_page(&mut stale).is_err());
    assert_eq!(db.writing_page(&page.id).unwrap().unwrap().body, page.body);
    drop(db);
    let mut db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.books()
            .unwrap()
            .iter()
            .find(|b| b.id == child.id)
            .unwrap()
            .parent_id
            .as_deref(),
        Some(parent.id.as_str())
    );
    let backup = dir.path().join("nested-export.zip");
    crate::export::write(dir.path(), &backup, crate::export::Scope::All).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(backup).unwrap()).unwrap();
    let books: serde_json::Value =
        serde_json::from_reader(zip.by_name("图书.json").unwrap()).unwrap();
    assert!(
        books
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["id"] == grandchild.id && b["parent_id"] == child.id)
    );
    db.move_book(&child.id, None).unwrap();
    assert!(
        db.books()
            .unwrap()
            .iter()
            .find(|b| b.id == child.id)
            .unwrap()
            .parent_id
            .is_none()
    );
}

#[test]
fn dragging_pages_in_editor_directory_creates_deeper_book_and_preserves_dirty_body() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    let mut book = crate::library::Book::new();
    book.title = "编辑器中的图书".into();
    let db = app.db.as_mut().unwrap();
    db.save_book(&book).unwrap();
    let first = db
        .create_writing("编辑器页面甲", Some(&book.id), None)
        .unwrap();
    let second = db
        .create_writing("编辑器页面乙", Some(&book.id), None)
        .unwrap();
    app.section = Section::Books;
    app.open_writing(&first.id);
    app.writing.page.as_mut().unwrap().body = "拖动前尚未保存的文字".into();
    app.mark_writing_changed(0.);
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    let from = text_position(&out, "编辑器页面乙");
    let to = text_position(&out, "编辑器页面甲");
    drag_writing(&mut app, &ctx, from, to);
    let child = app.library.book.clone().unwrap();
    assert_eq!(child.parent_id.as_deref(), Some(book.id.as_str()));
    assert_eq!(app.writing.page.as_ref().unwrap().id, second.id);
    assert_eq!(
        app.db
            .as_ref()
            .unwrap()
            .writing_page(&first.id)
            .unwrap()
            .unwrap()
            .body,
        "拖动前尚未保存的文字"
    );
}

#[test]
fn dragging_book_onto_book_nests_in_grid_and_list() {
    for mode in [crate::views::ViewMode::Grid, crate::views::ViewMode::List] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        let mut first = crate::library::Book::new();
        first.title = "目标图书".into();
        let mut second = crate::library::Book::new();
        second.title = "被拖动的图书".into();
        let db = app.db.as_mut().unwrap();
        db.save_book(&first).unwrap();
        db.save_book(&second).unwrap();
        let page = db
            .create_writing("保留的文件", Some(&second.id), None)
            .unwrap();
        app.section = Section::Books;
        app.set_view_mode(mode);
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        input_frame(&mut app, &ctx, 0., vec![], vec![]);
        let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        drag_writing(
            &mut app,
            &ctx,
            text_position(&out, &second.title),
            text_position(&out, &first.title),
        );
        assert_eq!(app.child_books(Some(&first.id))[0].id, second.id);
        assert_eq!(app.child_books(None).len(), 1);
        assert!(app.enter_book(first.clone()));
        assert!(app.writing.page.is_none());
        let out = input_frame(&mut app, &ctx, 2., vec![], vec![]);
        let at = text_position(&out, &format!("▸ 图书 · {}", second.title));
        input_frame(&mut app, &ctx, 2.1, pointer(at, true), vec![]);
        input_frame(&mut app, &ctx, 2.12, pointer(at, false), vec![]);
        let out = input_frame(&mut app, &ctx, 2.2, vec![], vec![]);
        assert!(
            app.writing.page.is_none(),
            "expansion must not enter the book"
        );
        let at = text_position(&out, "保留的文件");
        input_frame(&mut app, &ctx, 2.3, pointer(at, true), vec![]);
        input_frame(&mut app, &ctx, 2.32, pointer(at, false), vec![]);
        assert_eq!(app.writing.page.as_ref().unwrap().id, page.id);
        assert_eq!(app.library.book.as_ref().unwrap().id, first.id);
    }
}
