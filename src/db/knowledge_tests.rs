use super::*;
use crate::{curriculum, reference::Entry, study};

#[test]
fn scoped_draws_do_not_escape_the_point_and_honor_mastery_and_recent_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let id = "ref:mysql:last-value";
    let mut recent = Vec::new();
    for roll in 0..10 {
        let (card, _) = db
            .practice_choice_for(curriculum::MIXED, &Default::default(), &recent, roll, id)
            .unwrap()
            .unwrap();
        assert_eq!(card.knowledge_ids, vec![id]);
        if recent.len() == 1 {
            assert_ne!(card.id, recent[0]);
        }
        recent.push(card.id);
    }
    assert!(
        db.practice_choice_for(curriculum::MIXED, &Default::default(), &[], 1, "missing-id")
            .unwrap()
            .is_none()
    );
    let custom = Entry::new();
    assert!(
        db.practice_candidates_for("", &custom.id)
            .unwrap()
            .is_empty()
    );
    for c in db.practice_candidates_for("", id).unwrap() {
        db.desk
            .execute(
                "INSERT INTO study_progress VALUES(?1,5,5,1,100)",
                [c.card.id],
            )
            .unwrap();
    }
    let rules = study::Rules {
        mastered: study::MasteredMode::Stop,
        ..Default::default()
    };
    // Persist the rule without recomputing fixture progress from nonexistent attempts.
    db.set_setting("study.rules", &serde_json::to_string(&rules).unwrap())
        .unwrap();
    assert!(
        db.practice_choice_for(curriculum::MIXED, &Default::default(), &[], 1, id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn legacy_columns_upgrade_and_interrupted_seed_replay_preserve_edits_and_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut custom = db.reference_entry("ref:js:01").unwrap().unwrap();
    custom.notes = "我的原笔记".into();
    custom.tags = vec!["个人分类".into()];
    let custom = db.save_reference(&custom).unwrap();
    let removed = db.reference_entry("ref:mysql:rank").unwrap().unwrap();
    db.delete_reference(&removed).unwrap();
    let card = db
        .study_card("knowledge:ref:mysql:last-value")
        .unwrap()
        .unwrap();
    let p = study::Pending {
        id: "legacy-attempt".into(),
        card_id: card.id.clone(),
        answer: "不会".into(),
        at: "2026-10-23T10:00:00+08:00".into(),
    };
    db.record_study_attempt(&p, study::Grade::unknown(&card), "test", 0)
        .unwrap();
    let raw: String = db
        .desk
        .query_row(
            "SELECT attempt_json FROM study_attempts WHERE id=?1",
            [&p.id],
            |r| r.get(0),
        )
        .unwrap();
    db.set_setting("reference.seed.version", "1").unwrap();
    db.set_setting("reference.practice.version", "old").unwrap();
    // Simulate an older custom schema without the two additive columns.
    db.desk.execute_batch("ALTER TABLE command_entries DROP COLUMN entry_type; ALTER TABLE command_entries DROP COLUMN tags_json;").unwrap();
    drop(db);
    let mut db = Database::open(dir.path()).unwrap();
    let restored = db.reference_entry(&custom.id).unwrap().unwrap();
    assert_eq!(restored.notes, custom.notes);
    assert_eq!(restored.revision, custom.revision);
    assert!(db.reference_entry(&removed.id).unwrap().is_none());
    assert_eq!(
        db.study_count("engineering").unwrap(),
        curriculum::BUILTIN_COUNT
    );
    assert_eq!(
        raw,
        db.desk
            .query_row(
                "SELECT attempt_json FROM study_attempts WHERE id=?1",
                [&p.id],
                |r| r.get::<_, String>(0)
            )
            .unwrap()
    );
    db.set_setting("reference.practice.version", "old").unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.practice_candidates_for("", &card.knowledge_ids[0])
            .unwrap()
            .len(),
        2
    );
}
