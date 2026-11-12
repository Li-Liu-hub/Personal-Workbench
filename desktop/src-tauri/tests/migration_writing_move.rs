use personal_desk_lib::{db::Database, library::Book};
#[test]
fn moves_reject_cycles_and_preserve_pages_with_revision_guard() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let parent = Book {
        id: "parent".into(),
        parent_id: None,
        title: "父".into(),
        tags: vec![],
    };
    let child = Book {
        id: "child".into(),
        parent_id: Some(parent.id.clone()),
        title: "子".into(),
        tags: vec![],
    };
    db.save_book(&parent).unwrap();
    db.save_book(&child).unwrap();
    let page = db.create_writing("正文", Some(&parent.id), None).unwrap();
    assert!(db.move_book(&parent.id, Some(&child.id)).is_err());
    db.group_writing(std::slice::from_ref(&page.id), Some(&child.id))
        .unwrap();
    let moved = db.writing_page(&page.id).unwrap().unwrap();
    assert_eq!(moved.book_id, child.id);
    assert_eq!(moved.revision, page.revision + 1);
    let mut stale = page.clone();
    stale.body = "旧编辑".into();
    assert!(db.finish_writing_save("test", &mut stale).is_err());
    assert_eq!(db.writing_page(&page.id).unwrap().unwrap().body, "");
}
#[test]
fn moving_to_root_keeps_content_and_invalidates_old_editor() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let book = Book {
        id: "book".into(),
        title: "图书".into(),
        parent_id: None,
        tags: vec![],
    };
    db.save_book(&book).unwrap();
    let mut page = db.create_writing("正文", Some(&book.id), None).unwrap();
    page.body = "保留内容".into();
    db.finish_writing_save("test", &mut page).unwrap();
    db.move_writing_page(&page.id, None).unwrap();
    let moved = db.writing_page(&page.id).unwrap().unwrap();
    assert_eq!(moved.book_id, "__personal_writing__");
    assert_eq!(moved.body, "保留内容");
    assert_eq!(moved.revision, page.revision + 1);
    let mut old = page;
    old.body = "旧内容".into();
    assert!(db.finish_writing_save("test", &mut old).is_err());
    assert!(db.move_writing_page(&moved.id, Some("missing")).is_err());
    assert_eq!(
        db.writing_page(&moved.id).unwrap().unwrap().body,
        "保留内容"
    );
}
