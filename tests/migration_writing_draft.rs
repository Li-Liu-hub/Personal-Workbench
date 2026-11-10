use personal_desk::db::Database;
#[test]
fn saved_draft_recovers_as_separate_document_without_overwriting_original() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut page = db.create_writing("原文", None, None).unwrap();
    page.body = "已保存正文".into();
    db.finish_writing_save("tauri-migration", &mut page)
        .unwrap();
    let mut draft = page.clone();
    draft.body = "草稿正文".into();
    db.save_book_draft("tauri-migration", &draft).unwrap();
    db.recover_writing_drafts().unwrap();
    db.recover_writing_drafts().unwrap();
    let pages = db.writing_pages().unwrap();
    assert_eq!(
        db.writing_page(&page.id).unwrap().unwrap().body,
        "已保存正文"
    );
    assert_eq!(pages.iter().filter(|p| p.body == "草稿正文").count(), 1);
}
