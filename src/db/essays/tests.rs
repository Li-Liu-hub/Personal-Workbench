use super::*;

fn day(n: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 10, n).unwrap()
}

#[test]
fn daily_topic_is_stable_rerollable_before_writing_and_restart_safe() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let a = db.ensure_essay_day(day(22)).unwrap();
    let before = db.change_count();
    assert_eq!(a.topic, db.ensure_essay_day(day(22)).unwrap().topic);
    assert_eq!(before, db.change_count());
    let b = db.reroll_essay(&a.day, &a.topic.id).unwrap();
    assert_ne!(a.topic.id, b.topic.id);
    assert!(db.reroll_essay(&a.day, &a.topic.id).is_err());
    drop(db);
    let mut db = Database::open(dir.path()).unwrap();
    assert_eq!(b.topic, db.ensure_essay_day(day(22)).unwrap().topic);
    let page = db.start_essay(&a.day).unwrap();
    assert!(db.reroll_essay(&b.day, &b.topic.id).is_err());
    assert_eq!(page.id, db.start_essay(&b.day).unwrap().id);
    assert_eq!(db.writing_pages().unwrap().len(), 1);
    assert!(page.title.contains(&b.topic.title));
}

#[test]
fn essays_keep_content_grouping_revisions_and_search_with_no_recent_repeats() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut seen = std::collections::HashSet::new();
    let mut ids = vec![];
    for n in 1..=25 {
        let a = db.ensure_essay_day(day(n)).unwrap();
        assert!(seen.insert(a.topic.id));
        if n <= 2 {
            let mut p = db.start_essay(&a.day).unwrap();
            p.body = format!("unique-essay-{n}");
            db.finish_writing_save("writer", &mut p).unwrap();
            ids.push(p.id);
        }
    }
    let book = db.group_writing(&ids, None).unwrap();
    let mut p = db.start_essay("2026-10-01").unwrap();
    assert_eq!(p.book_id, book.id);
    assert_eq!(p.body, "unique-essay-1");
    p.body += " continued";
    db.finish_writing_save("writer", &mut p).unwrap();
    assert_eq!(db.essay_summaries("continued").unwrap().len(), 1);
    assert_eq!(db.essay_summaries("2026-10-02").unwrap().len(), 1);
    assert!(db.essay_summaries("no-match").unwrap().is_empty());
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(db.writing_page(&ids[0]).unwrap().unwrap().body, p.body);
    assert_eq!(db.essay_records().unwrap().len(), 25);
    assert_eq!(db.essay_summaries("").unwrap().len(), 2);
}

#[test]
fn essay_creation_rolls_back_on_failure_and_deleted_page_is_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    db.ensure_essay_day(day(22)).unwrap();
    db.desk.execute_batch("CREATE TRIGGER fail_essay BEFORE UPDATE ON daily_essays BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    assert!(db.start_essay("2026-10-22").is_err());
    assert!(db.writing_pages().unwrap().is_empty());
    assert!(
        db.essay_assignment("2026-10-22")
            .unwrap()
            .unwrap()
            .page_id
            .is_none()
    );
    db.desk.execute_batch("DROP TRIGGER fail_essay").unwrap();
    let p = db.start_essay("2026-10-22").unwrap();
    db.desk
        .execute("UPDATE book_pages SET deleted=1 WHERE id=?1", [&p.id])
        .unwrap();
    assert!(db.start_essay("2026-10-22").is_err());
    assert!(db.essay_summaries("").unwrap().is_empty());
    assert!(db.essay_records().unwrap()[0].page.is_none());
}

#[test]
fn essay_module_and_full_export_include_topic_snapshot_and_body() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let mut db = Database::open(&data).unwrap();
    let a = db.ensure_essay_day(day(22)).unwrap();
    let mut p = db.start_essay(&a.day).unwrap();
    p.body = "An essay worth keeping".into();
    db.finish_writing_save("writer", &mut p).unwrap();
    for scope in [crate::export::Scope::Essays, crate::export::Scope::All] {
        let output = dir.path().join(format!("{}.zip", scope.label()));
        crate::export::write(&data, &output, scope).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(output).unwrap()).unwrap();
        let records: serde_json::Value =
            serde_json::from_reader(zip.by_name("每日随笔.json").unwrap()).unwrap();
        assert_eq!(records[0]["assignment"]["topic"]["title"], a.topic.title);
        assert_eq!(records[0]["page"]["body"], p.body);
        use std::io::Read;
        let mut text = String::new();
        zip.by_name("essays/2026-10-22.md")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert!(text.contains(&p.body) && text.contains(&a.topic.prompt));
        assert_eq!(
            zip.by_name("每日随笔.json").unwrap().name(),
            "每日随笔.json"
        );
    }
}
