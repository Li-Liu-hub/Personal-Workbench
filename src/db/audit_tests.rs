//! Failure injection and lifecycle regressions use temporary databases only.
use super::*;
use crate::library::{Book, Page};

#[test]
fn page_classification_changes_invalidate_other_editors_and_replays_are_stable() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut page = db.create_writing("original", None, None).unwrap();
    for action in ["set", "rename", "delete"] {
        match action {
            "set" => db
                .set_record_tags(TagScope::BookPages, &page.id, &["tag".into()])
                .unwrap(),
            "rename" => db
                .rename_tag(TagScope::BookPages, "tag", "renamed")
                .unwrap(),
            _ => db.delete_tag(TagScope::BookPages, "renamed").unwrap(),
        }
        assert!(
            matches!(
                db.save_book_page(&mut page),
                Err(DeskError::ConcurrentModification)
            ),
            "{action}"
        );
        page = db.writing_page(&page.id).unwrap().unwrap();
        let revision = page.revision;
        db.set_record_tags(TagScope::BookPages, &page.id, &page.tags)
            .unwrap();
        assert_eq!(
            db.writing_page(&page.id).unwrap().unwrap().revision,
            revision
        );
    }
}

#[test]
fn writing_save_and_draft_cleanup_roll_back_together() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut page = db.create_writing("draft", None, None).unwrap();
    let revision = page.revision;
    page.body = "unsaved work".into();
    db.save_book_draft("writer", &page).unwrap();
    db.desk.execute_batch("CREATE TRIGGER fail_draft_cleanup BEFORE DELETE ON book_page_drafts BEGIN SELECT RAISE(ABORT,'injected cleanup failure'); END;").unwrap();
    assert!(db.finish_writing_save("writer", &mut page).is_err());
    assert_eq!(page.revision, revision);
    assert_eq!(page.body, "unsaved work");
    assert_eq!(db.writing_page(&page.id).unwrap().unwrap().body, "");
    assert_eq!(db.book_drafts(WRITING_ROOT, "other").unwrap().len(), 1);
    db.desk
        .execute_batch("DROP TRIGGER fail_draft_cleanup")
        .unwrap();
    db.finish_writing_save("writer", &mut page).unwrap();
    assert_eq!(page.revision, revision + 1);
    assert!(db.book_drafts(WRITING_ROOT, "other").unwrap().is_empty());
}

#[test]
fn deleted_books_hide_pages_and_reject_new_or_stale_writes() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let book = Book::new();
    db.save_book(&book).unwrap();
    let mut page = db.create_writing("original", Some(&book.id), None).unwrap();
    db.set_library_deleted(&book.id, false, true).unwrap();
    assert!(db.writing_page(&page.id).unwrap().is_none());
    assert!(db.book_pages(&book.id).unwrap().is_empty());
    assert!(db.create_writing("hidden", Some(&book.id), None).is_err());
    assert!(db.save_book_page(&mut Page::new(book.id.clone())).is_err());
    page.body = "stale editor".into();
    assert!(db.save_book_page(&mut page).is_err());
    db.set_library_deleted(&book.id, false, false).unwrap();
    assert!(matches!(
        db.save_book_page(&mut page),
        Err(DeskError::ConcurrentModification)
    ));
    assert_eq!(db.writing_page(&page.id).unwrap().unwrap().body, "");
}

#[test]
fn deleting_and_restoring_a_page_invalidates_old_editors_only_once_per_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut page = db.create_writing("original", None, None).unwrap();
    db.set_library_deleted(&page.id, true, false).unwrap();
    assert_eq!(
        db.writing_page(&page.id).unwrap().unwrap().revision,
        page.revision
    );
    db.set_library_deleted(&page.id, true, true).unwrap();
    db.set_library_deleted(&page.id, true, false).unwrap();
    assert!(matches!(
        db.save_book_page(&mut page),
        Err(DeskError::ConcurrentModification)
    ));
}

#[test]
fn moved_page_draft_tracks_its_new_book() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut page = db.create_writing("draft", None, None).unwrap();
    db.save_book_draft("writer", &page).unwrap();
    let book = Book::new();
    db.save_book(&book).unwrap();
    db.group_writing(std::slice::from_ref(&page.id), Some(&book.id))
        .unwrap();
    page = db.writing_page(&page.id).unwrap().unwrap();
    page.body = "moved draft".into();
    db.save_book_draft("writer", &page).unwrap();
    assert!(db.book_drafts(WRITING_ROOT, "other").unwrap().is_empty());
    let drafts = db.book_drafts(&book.id, "other").unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].1.body, "moved draft");
}

#[test]
fn invalid_persisted_study_rules_fall_back_without_changing_the_setting() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    for value in [
        "not json",
        r#"{"threshold":0,"consecutive":0,"mastered":"Rare","daily_words":0}"#,
    ] {
        db.set_setting("study.rules", value).unwrap();
        assert_eq!(db.study_rules().threshold, 85);
        assert_eq!(db.study_rules().consecutive, 5);
        assert_eq!(db.setting("study.rules").unwrap().as_deref(), Some(value));
    }
}

#[test]
fn website_storage_validates_and_normalizes_without_relying_on_a_form() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut item = Website::new();
    item.url = " example.test/path ".into();
    db.save_website_classified(&item, &["work".into()]).unwrap();
    let saved = db.websites().unwrap().remove(0);
    assert_eq!(saved.url, "https://example.test/path");
    assert_eq!(saved.title, "example.test");
    item.url = "file:///C:/secret".into();
    assert!(
        db.save_website_classified(&item, &["must roll back".into()])
            .is_err()
    );
    assert_eq!(db.websites().unwrap()[0].url, saved.url);
    assert!(
        !db.tags(TagScope::Websites)
            .unwrap()
            .contains(&"must roll back".into())
    );
}

#[test]
fn writing_creation_rejects_blank_titles_without_creating_hidden_records() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    assert!(db.create_writing(" \n ", None, None).is_err());
    assert!(db.writing_pages().unwrap().is_empty());
}

#[test]
fn favorite_scopes_and_settings_remain_independent_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    for scope in ["tools", "websites"] {
        db.set_favorite(scope, "same-id", true).unwrap();
        db.set_favorite(scope, "same-id", true).unwrap();
    }
    db.set_setting("custom.future.option", "preserve me")
        .unwrap();
    db.set_favorite("tools", "same-id", false).unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.favorites().unwrap(),
        vec![("websites".into(), "same-id".into())]
    );
    assert_eq!(
        db.setting("custom.future.option").unwrap().as_deref(),
        Some("preserve me")
    );
}
