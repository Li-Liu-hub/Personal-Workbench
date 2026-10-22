use super::*;

#[test]
#[ignore = "Manual indexed reference lookup measurement"]
fn reference_search_measurement() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.desk.execute_batch("WITH RECURSIVE nums(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM nums WHERE n<10000)
        INSERT INTO command_entries(id,language,category,title,summary,syntax,example,notes,source,search_text)
        SELECT 'measure-'||n,'Rust','集合与容器','Example '||n,'','','','','','uniqueneedle'||n FROM nums;").unwrap();
    let mut times = Vec::new();
    for n in 1..=200 {
        let query = crate::reference::Filter {
            language: "Rust".into(),
            query: format!("uniqueneedle{}", n * 41),
            ..Default::default()
        };
        let start = std::time::Instant::now();
        let rows = db.search_reference(&query).unwrap();
        times.push(start.elapsed().as_secs_f64() * 1000.);
        assert!(rows.iter().any(|r| r.id == format!("measure-{}", n * 41)));
    }
    times.sort_by(f64::total_cmp);
    println!(
        "REFERENCE_10000_LOOKUP p50_ms={:.3} p95_ms={:.3}",
        times[100], times[190]
    );
}

#[test]
fn seeded_modules_reopen_without_reimport_writes_and_search_uses_fts_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let before = db.desk.total_changes();
    db.prepare_study().unwrap();
    db.prepare_reference().unwrap();
    assert_eq!(db.desk.total_changes(), before);
    let plan=db.desk.prepare("EXPLAIN QUERY PLAN SELECT rowid FROM command_entries_fts WHERE command_entries_fts MATCH 'hashmap'").unwrap()
        .query_map([],|r|r.get::<_,String>(3)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap().join(" ");
    assert!(plan.contains("VIRTUAL TABLE INDEX"));
    db.desk
        .execute(
            "INSERT INTO command_entries_fts(command_entries_fts,rank) VALUES('integrity-check',1)",
            [],
        )
        .unwrap();
}
