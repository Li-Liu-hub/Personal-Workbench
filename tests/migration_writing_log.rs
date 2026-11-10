use personal_desk::db::Database;
#[test]
fn daily_log_reuses_existing_page_without_overwriting_content() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let day = "2026-11-05";
    let mut first = db.create_writing(day, None, Some(day)).unwrap();
    first.body = "原有日志".into();
    db.finish_writing_save("test", &mut first).unwrap();
    let second = db.create_writing(day, None, Some(day)).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.body, "原有日志");
}
