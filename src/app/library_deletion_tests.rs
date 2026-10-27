use super::editor_tests::{input_frame, pointer};
use super::*;

fn text_at(out: &egui::FullOutput, text: &str) -> egui::Pos2 {
    out.shapes
        .iter()
        .rev()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos + egui::vec2(4., 5.)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {text}"))
}

fn click(app: &mut PersonalDeskApp, ctx: &egui::Context, at: egui::Pos2, time: f64) {
    input_frame(app, ctx, time, pointer(at, true), vec![]);
    input_frame(app, ctx, time + 0.02, pointer(at, false), vec![]);
}

fn right_click(
    app: &mut PersonalDeskApp,
    ctx: &egui::Context,
    at: egui::Pos2,
    time: f64,
) -> egui::FullOutput {
    for (pressed, offset) in [(true, 0.), (false, 0.02)] {
        input_frame(
            app,
            ctx,
            time + offset,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            vec![],
        );
    }
    input_frame(app, ctx, time + 0.05, vec![], vec![])
}

#[test]
fn book_deletion_menu_confirms_cancels_and_persists_in_both_views() {
    for mode in [ViewMode::Grid, ViewMode::List] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        let mut book = crate::library::Book::new();
        book.title = "待删除图书".into();
        let db = app.db.as_mut().unwrap();
        db.save_book(&book).unwrap();
        let page = db.create_writing("书内页面", Some(&book.id), None).unwrap();
        let keep = db.create_writing("独立文档", None, None).unwrap();
        app.section = Section::Books;
        app.set_view_mode(mode);
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        input_frame(&mut app, &ctx, 0., vec![], vec![]);
        let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        let menu = right_click(&mut app, &ctx, text_at(&out, &book.title), 0.2);
        click(&mut app, &ctx, text_at(&menu, "删除图书"), 0.3);
        assert!(app.detail_open());
        assert!(
            app.db
                .as_ref()
                .unwrap()
                .writing_page(&page.id)
                .unwrap()
                .is_some()
        );
        let dialog = input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
        text_at(&dialog, "这本图书、所有子图书及其中的页面也会一起删除。");
        click(&mut app, &ctx, text_at(&dialog, "取消"), 0.5);
        assert!(app.library.deletion.is_none());
        assert_eq!(app.db.as_ref().unwrap().books().unwrap().len(), 1);
        app.request_library_deletion(&book.id, &book.title, false);
        let dialog = input_frame(&mut app, &ctx, 0.6, vec![], vec![]);
        click(&mut app, &ctx, text_at(&dialog, "确认删除"), 0.7);
        assert!(app.library.deletion.is_none());
        drop(app);
        let db = Database::open(dir.path()).unwrap();
        assert!(db.books().unwrap().is_empty());
        assert!(db.writing_page(&page.id).unwrap().is_none());
        assert!(db.writing_page(&keep.id).unwrap().is_some());
    }
}

#[test]
fn deleting_active_page_switches_to_neighbor_then_keeps_empty_book() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    let mut book = crate::library::Book::new();
    book.title = "保留空图书".into();
    let db = app.db.as_mut().unwrap();
    db.save_book(&book).unwrap();
    let first = db.create_writing("第一页", Some(&book.id), None).unwrap();
    let middle = db.create_writing("第二页", Some(&book.id), None).unwrap();
    let last = db.create_writing("第三页", Some(&book.id), None).unwrap();
    app.section = Section::Books;
    assert!(app.open_writing(&middle.id));
    app.writing.page.as_mut().unwrap().body = "尚未自动保存的正文".into();
    app.mark_writing_changed(0.);
    app.request_library_deletion(&middle.id, &middle.title, true);
    assert!(app.confirm_library_deletion());
    assert_eq!(app.writing.page.as_ref().unwrap().id, last.id);
    // A stale editor must not resurrect a deleted page after autosave or restart.
    assert!(
        app.db
            .as_mut()
            .unwrap()
            .save_book_page(&mut middle.clone())
            .is_err()
    );
    app.request_library_deletion(&last.id, &last.title, true);
    assert!(app.confirm_library_deletion());
    assert_eq!(app.writing.page.as_ref().unwrap().id, first.id);
    app.request_library_deletion(&first.id, &first.title, true);
    assert!(app.confirm_library_deletion());
    assert!(app.writing.page.is_none());
    assert!(!app.writing.dirty);
    assert_eq!(app.library.book.as_ref().unwrap().id, book.id);
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 1., vec![], vec![]);
    let out = input_frame(&mut app, &ctx, 1.1, vec![], vec![]);
    text_at(&out, "新建文档");
    text_at(&out, "← 全部图书");
    drop(app);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(db.books().unwrap().len(), 1);
    assert!(db.book_pages(&book.id).unwrap().is_empty());
    assert!(db.writing_pages().unwrap().is_empty());
}

#[test]
fn toolbar_and_inactive_tab_both_delete_pages_without_losing_current_edits() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    let mut book = crate::library::Book::new();
    book.title = "编辑中的图书".into();
    let db = app.db.as_mut().unwrap();
    db.save_book(&book).unwrap();
    let active = db.create_writing("正在写作", Some(&book.id), None).unwrap();
    let other = db
        .create_writing("待删除标签", Some(&book.id), None)
        .unwrap();
    app.section = Section::Books;
    assert!(app.open_writing(&active.id));
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    let menu = right_click(&mut app, &ctx, text_at(&out, &other.title), 0.2);
    click(&mut app, &ctx, text_at(&menu, "删除页面"), 0.3);
    assert_eq!(app.library.deletion.as_ref().unwrap().id, other.id);
    app.writing.page.as_mut().unwrap().body = "继续保留的正文".into();
    app.mark_writing_changed(0.32);
    let dialog = input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
    click(&mut app, &ctx, text_at(&dialog, "确认删除"), 0.5);
    assert_eq!(app.writing.page.as_ref().unwrap().id, active.id);
    assert_eq!(
        app.db
            .as_ref()
            .unwrap()
            .writing_page(&active.id)
            .unwrap()
            .unwrap()
            .body,
        "继续保留的正文"
    );
    assert!(
        app.db
            .as_ref()
            .unwrap()
            .writing_page(&other.id)
            .unwrap()
            .is_none()
    );
    let out = input_frame(&mut app, &ctx, 0.6, vec![], vec![]);
    click(&mut app, &ctx, text_at(&out, "删除页面"), 0.7);
    assert_eq!(app.library.deletion.as_ref().unwrap().id, active.id);
    let dialog = input_frame(&mut app, &ctx, 0.8, vec![], vec![]);
    click(&mut app, &ctx, text_at(&dialog, "确认删除"), 0.9);
    assert!(app.writing.page.is_none());
    assert!(app.library.book.is_some());
}

#[test]
fn deleting_open_book_closes_editor_and_floating_writer() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    let book = crate::library::Book::new();
    let db = app.db.as_mut().unwrap();
    db.save_book(&book).unwrap();
    let page = db.create_writing("正文", Some(&book.id), None).unwrap();
    app.section = Section::Books;
    app.open_writing(&page.id);
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    click(&mut app, &ctx, text_at(&out, "更多"), 0.2);
    let menu = input_frame(&mut app, &ctx, 0.3, vec![], vec![]);
    click(&mut app, &ctx, text_at(&menu, "删除图书"), 0.4);
    assert_eq!(app.library.deletion.as_ref().unwrap().id, book.id);
    app.writing.floating = true;
    app.writing.page.as_mut().unwrap().body = "删除前的最新正文".into();
    app.mark_writing_changed(0.5);
    assert!(app.confirm_library_deletion());
    assert!(app.library.book.is_none());
    assert!(app.writing.page.is_none());
    assert!(!app.writing.floating);
    assert!(app.flush_writing());
    assert!(
        app.db
            .as_ref()
            .unwrap()
            .writing_page(&page.id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn failed_save_blocks_deletion_and_keeps_editor_and_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    assert!(app.new_writing(false));
    let page = app.writing.page.as_ref().unwrap().clone();
    let mut newer = page.clone();
    newer.body = "另一窗口已经保存".into();
    app.db.as_mut().unwrap().save_book_page(&mut newer).unwrap();
    app.writing.page.as_mut().unwrap().body = "本窗口未保存".into();
    app.mark_writing_changed(0.);
    app.request_library_deletion(&page.id, &page.title, true);
    assert!(!app.confirm_library_deletion());
    assert!(app.library.deletion.is_some());
    assert!(!app.library.error.is_empty());
    assert!(app.writing.dirty);
    assert_eq!(app.writing.page.as_ref().unwrap().body, "本窗口未保存");
    assert_eq!(
        app.db
            .as_ref()
            .unwrap()
            .writing_page(&page.id)
            .unwrap()
            .unwrap()
            .body,
        "另一窗口已经保存"
    );
}

#[test]
fn deletion_and_writing_controls_fit_the_smallest_window() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    let mut book = crate::library::Book::new();
    book.title = "名称很长也能正常操作的图书".repeat(4);
    let db = app.db.as_mut().unwrap();
    db.save_book(&book).unwrap();
    let page = db.create_writing("正文", Some(&book.id), None).unwrap();
    app.section = Section::Books;
    app.open_writing(&page.id);
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    for time in [0., 0.1] {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 680.),
                )),
                time: Some(time),
                ..Default::default()
            },
            |ui| app.render_workspace(ui),
        );
        out.textures_delta.clear();
        if time > 0. {
            for label in [
                "← 图书",
                "删除页面",
                "新建",
                "生成日志",
                "悬浮窗",
                "写作",
                "预览",
                "更多",
            ] {
                let shape = out
                    .shapes
                    .iter()
                    .find(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == label))
                    .unwrap();
                if let egui::Shape::Text(text) = &shape.shape {
                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                    assert!(
                        shape.clip_rect.contains_rect(rect),
                        "clipped control: {label}"
                    );
                    assert!(
                        rect.min.x >= 0. && rect.max.x <= 1000. && rect.max.y <= 680.,
                        "offscreen control: {label}"
                    );
                }
            }
        }
    }
}

#[test]
fn deleted_pages_and_books_stay_deleted_when_old_drafts_are_recovered() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let book = crate::library::Book::new();
    db.save_book(&book).unwrap();
    let mut page = db.create_writing("已删除的独立页", None, None).unwrap();
    let mut chapter = db
        .create_writing("已删除图书中的页面", Some(&book.id), None)
        .unwrap();
    let mut keep = db.create_writing("保留的草稿", None, None).unwrap();
    for page in [&mut page, &mut chapter, &mut keep] {
        page.body = "中断窗口留下的未保存内容".into();
        db.save_book_draft("interrupted-writer", page).unwrap();
    }
    db.set_library_deleted(&page.id, true, true).unwrap();
    db.set_library_deleted(&book.id, false, true).unwrap();
    drop(db);
    for _ in 0..2 {
        let mut db = Database::open(dir.path()).unwrap();
        db.recover_writing_drafts().unwrap();
        let pages = db.writing_pages().unwrap();
        assert_eq!(pages.len(), 2);
        assert!(pages.iter().all(|p| p.title.starts_with("保留的草稿")));
        assert!(pages.iter().any(|p| p.body == "中断窗口留下的未保存内容"));
    }
}
