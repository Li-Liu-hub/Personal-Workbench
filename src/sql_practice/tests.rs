use super::*;
use crate::study::Card;

fn cards() -> Vec<Card> {
    serde_json::from_str(include_str!("../../assets/sql_practice.json")).unwrap()
}
fn lab(id: &str) -> Lab {
    cards()
        .into_iter()
        .find(|c| c.id == id)
        .unwrap()
        .sql
        .unwrap()
}
#[test]
fn every_fixture_builds_and_every_reference_passes() {
    let cards = cards();
    assert_eq!(cards.len(), 12);
    for card in cards {
        card.validate().unwrap();
        let lab = card.sql.unwrap();
        let preview = preview(&lab).unwrap_or_else(|e| panic!("{}: {e}", card.id));
        assert!(!preview.tables.is_empty());
        if lab.runnable {
            let report = run(&lab, &lab.reference);
            assert_eq!(report.matches, Some(true), "{}: {report:?}", card.id);
            assert!(report.error.is_none() && report.rolled_back);
        }
    }
}
#[test]
fn successful_and_partially_failed_mutations_always_start_again_from_seed() {
    for id in [
        "eng:3:05",
        "sql:sqlite:insert",
        "sql:sqlite:delete",
        "sql:sqlite:upsert",
        "sql:sqlite:transfer",
    ] {
        let lab = lab(id);
        let before = serde_json::to_value(preview(&lab).unwrap().tables).unwrap();
        for _ in 0..2 {
            let report = run(&lab, &lab.reference);
            assert_eq!(report.matches, Some(true));
            assert!(report.changed_rows > 0 && report.rolled_back);
        }
        let failed = run(
            &lab,
            &format!("{}\n SELECT * FROM missing_table;", lab.reference),
        );
        assert!(failed.error.is_some() && failed.rolled_back);
        assert_eq!(failed.matches, None);
        assert_eq!(
            before,
            serde_json::to_value(preview(&lab).unwrap().tables).unwrap()
        );
        assert_eq!(run(&lab, &lab.reference).matches, Some(true));
    }
}
#[test]
fn null_ties_and_overbroad_deletes_are_checked_by_data() {
    let joined = lab("eng:3:02");
    let wrong = run(
        &joined,
        "SELECT u.id,COUNT(*) FROM users u LEFT JOIN orders o ON o.user_id=u.id GROUP BY u.id ORDER BY u.id;",
    );
    assert_eq!(wrong.matches, Some(false));
    let nulls = lab("eng:3:04");
    assert_eq!(
        run(
            &nulls,
            "SELECT id,name FROM users WHERE id NOT IN (SELECT user_id FROM blocked) ORDER BY id"
        )
        .matches,
        Some(false)
    );
    let ranked = lab("eng:3:01");
    assert_eq!(
        run(
            &ranked,
            &ranked.reference.replace("DENSE_RANK()", "ROW_NUMBER()")
        )
        .matches,
        Some(false)
    );
    let deleting = lab("sql:sqlite:delete");
    assert_eq!(run(&deleting, "DELETE FROM orders;").matches, Some(false));
}
#[test]
fn sqlite_parser_handles_semicolons_comments_and_returning() {
    let mut lab = lab("sql:sqlite:insert");
    lab.reference = "SELECT '; inside literal' AS value;".into();
    lab.verify = None;
    assert_eq!(
        run(
            &lab,
            "-- comment ;\n SELECT '; inside literal' AS different_alias; -- trailing ;"
        )
        .matches,
        Some(true)
    );
    let lab = lab_from_insert();
    let report = run(
        &lab,
        &format!("{} RETURNING id;", lab.reference.trim_end_matches(';')),
    );
    assert_eq!(report.matches, Some(true));
    assert_eq!(report.results[0].rows[0][0], Value::from(107));
}
fn lab_from_insert() -> Lab {
    lab("sql:sqlite:insert")
}
#[test]
fn practice_sql_cannot_attach_or_write_a_real_database_or_escape_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sentinel.db");
    let original = Connection::open(&path).unwrap();
    original
        .execute_batch("CREATE TABLE important(value TEXT); INSERT INTO important VALUES('keep');")
        .unwrap();
    drop(original);
    let bytes = std::fs::read(&path).unwrap();
    let lab = lab("eng:3:05");
    for sql in [
        format!(
            "ATTACH DATABASE '{}' AS real; DELETE FROM real.important;",
            path.to_string_lossy().replace('\\', "/")
        ),
        "UPDATE accounts SET balance=0; COMMIT;".into(),
        "UPDATE accounts SET balance=0; RELEASE personaldesk_attempt;".into(),
        format!(
            "VACUUM INTO '{}';",
            dir.path()
                .join("leak.db")
                .to_string_lossy()
                .replace('\\', "/")
        ),
        "PRAGMA writable_schema=ON;".into(),
        "SELECT load_extension('anything');".into(),
    ] {
        let report = run(&lab, &sql);
        assert!(
            report.error.is_some() && report.rolled_back,
            "{sql}: {report:?}"
        );
    }
    assert_eq!(bytes, std::fs::read(&path).unwrap());
    assert!(!dir.path().join("leak.db").exists());
    let mut imported = lab.clone();
    imported.setup = "ATTACH DATABASE ':memory:' AS external;".into();
    assert!(preview(&imported).is_err());
    assert_eq!(run(&lab, &lab.reference).matches, Some(true));
}
#[test]
fn constraint_rollback_and_excessive_work_are_recoverable() {
    let lab = lab("eng:3:05");
    for sql in [
        "UPDATE accounts SET balance=0 WHERE id=1; INSERT OR ROLLBACK INTO accounts VALUES(1,'duplicate',10);",
        "WITH RECURSIVE infinite(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM infinite) SELECT SUM(x) FROM infinite;",
        "SELECT randomblob(1000000);",
        "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<201) SELECT x FROM n;",
    ] {
        let started = Instant::now();
        let report = run(&lab, sql);
        assert!(report.error.is_some() && report.rolled_back, "{report:?}");
        assert!(started.elapsed() < Duration::from_secs(4));
        assert_eq!(run(&lab, &lab.reference).matches, Some(true));
    }
}
#[test]
fn comparison_preserves_duplicate_rows_null_and_large_integers() {
    let table = |values: Vec<Value>| Table {
        columns: vec!["v".into()],
        rows: values.into_iter().map(|v| vec![v]).collect(),
        ..Default::default()
    };
    assert!(equal(
        &table(vec![Value::from(1)]),
        &table(vec![Value::from(1.0)]),
        true
    ));
    assert!(!equal(
        &table(vec![Value::from(9_007_199_254_740_992i64)]),
        &table(vec![Value::from(9_007_199_254_740_993i64)]),
        true
    ));
    assert!(!equal(
        &table(vec![Value::Null]),
        &table(vec![Value::String("NULL".into())]),
        true
    ));
    assert!(equal(
        &table(vec![Value::from(2), Value::from(1)]),
        &table(vec![Value::from(1), Value::from(2)]),
        false
    ));
    assert!(!equal(
        &table(vec![Value::from(1), Value::from(1)]),
        &table(vec![Value::from(1), Value::from(2)]),
        false
    ));
}
