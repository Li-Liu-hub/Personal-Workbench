use super::*;
use crate::{reference::Entry, study::Card};

fn catalog() -> (Vec<Entry>, Vec<Card>) {
    (
        serde_json::from_str(include_str!("../../assets/other_languages.json")).unwrap(),
        serde_json::from_str(include_str!("../../assets/other_languages_practice.json")).unwrap(),
    )
}

#[test]
fn other_languages_cover_seven_catalogs_with_exact_valid_practice_links() {
    let (entries, cards) = catalog();
    assert_eq!(entries.len(), 112);
    assert_eq!(cards.len(), entries.len());
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let mut ids = std::collections::HashSet::new();
    let mut titles = std::collections::HashSet::new();
    for entry in db.reference_entries().unwrap() {
        assert!(ids.insert(entry.id));
        assert!(titles.insert((entry.language, entry.title)));
    }
    for language in ["JavaScript", "C", "C++", "Rust", "Go", "Linux", "MySQL"] {
        assert_eq!(
            entries.iter().filter(|e| e.language == language).count(),
            16
        );
    }
    for (entry, card) in entries.iter().zip(&cards) {
        entry.validate().unwrap();
        card.validate().unwrap();
        assert!(entry.notes.contains("版本前提"));
        assert!(entry.example.len() > 30);
        assert_eq!(
            card.knowledge_ids.as_slice(),
            std::slice::from_ref(&entry.id)
        );
        assert!(card.tags.contains(&entry.language));
        assert!(card.training.is_some());
        assert!(db.reference_entry(&entry.id).unwrap().is_some());
        let candidates = db
            .practice_candidates_for(crate::curriculum::MIXED, &entry.id)
            .unwrap();
        assert_eq!(candidates.len(), 1, "{}", entry.id);
        assert_eq!(candidates[0].card.id, card.id);
        assert!(
            db.search_reference(&crate::reference::Filter {
                language: entry.language.clone(),
                category: entry.category.clone(),
                tag: "进阶专题".into(),
                query: entry.tags.last().unwrap().clone(),
                ..Default::default()
            })
            .unwrap()
            .iter()
            .any(|e| e.id == entry.id),
            "{}",
            entry.id
        );
    }
    assert_eq!(
        db.reference_count("").unwrap(),
        crate::reference::BUILTIN_COUNT
    );
}

#[test]
fn other_languages_upgrade_runs_after_java_marker_and_replay_preserves_personal_work() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let (entries, cards) = catalog();
    // Simulate a Java-only installation: keep its completed marker and all older data.
    for card in &cards {
        db.desk
            .execute("DELETE FROM study_knowledge WHERE card_id=?1", [&card.id])
            .unwrap();
        db.desk
            .execute("DELETE FROM study_cards WHERE id=?1", [&card.id])
            .unwrap();
    }
    for entry in &entries {
        db.desk
            .execute("DELETE FROM command_entries WHERE id=?1", [&entry.id])
            .unwrap();
    }
    db.desk
        .execute(
            "DELETE FROM settings WHERE key='encyclopedia.other_languages.version'",
            [],
        )
        .unwrap();
    assert_eq!(
        db.setting("encyclopedia.ecosystem.version")
            .unwrap()
            .as_deref(),
        Some("1")
    );
    let mut java = db.reference_entry("ref:java-eco:ioc-di").unwrap().unwrap();
    java.notes = "保留原 Java 笔记".into();
    let java = db.save_reference(&java).unwrap();
    drop(db);
    let mut db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.reference_count("").unwrap(),
        crate::reference::BUILTIN_COUNT
    );
    assert_eq!(db.reference_entry(&java.id).unwrap(), Some(java));

    let mut edited = vec![];
    let mut deleted = vec![];
    for language in ["JavaScript", "C", "C++", "Rust", "Go", "Linux", "MySQL"] {
        let mut rows = entries.iter().filter(|e| e.language == language);
        let mut entry = db
            .reference_entry(&rows.next().unwrap().id)
            .unwrap()
            .unwrap();
        entry.notes = format!("{language} 的个人补充");
        edited.push(db.save_reference(&entry).unwrap());
        let entry = db
            .reference_entry(&rows.next().unwrap().id)
            .unwrap()
            .unwrap();
        db.delete_reference(&entry).unwrap();
        deleted.push(entry.id);
    }
    db.set_favorite("reference", &edited[0].id, true).unwrap();
    let mut card = db.study_card(&cards[0].id).unwrap().unwrap();
    card.prompt.push_str("\n自定义要求");
    db.desk
        .execute(
            "UPDATE study_cards SET card_json=?2 WHERE id=?1",
            params![card.id, serde_json::to_string(&card).unwrap()],
        )
        .unwrap();
    let pending = crate::study::Pending {
        id: "multilingual-upgrade-evidence".into(),
        card_id: card.id.clone(),
        answer: "个人原答案".into(),
        at: "2026-10-23T23:00:00+08:00".into(),
    };
    let attempt = db
        .record_study_attempt(&pending, crate::study::Grade::unknown(&card), "test", 1)
        .unwrap();
    db.set_setting("encyclopedia.other_languages.version", "old")
        .unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    for entry in edited {
        assert_eq!(db.reference_entry(&entry.id).unwrap(), Some(entry));
    }
    for id in deleted {
        assert!(db.reference_entry(&id).unwrap().is_none());
    }
    assert_eq!(
        db.reference_count("").unwrap(),
        crate::reference::BUILTIN_COUNT - 7
    );
    assert_eq!(
        db.study_card(&card.id).unwrap().unwrap().prompt,
        card.prompt
    );
    assert_eq!(
        serde_json::to_value(&db.study_attempts(&card.id).unwrap()[0]).unwrap(),
        serde_json::to_value(attempt).unwrap()
    );
    assert!(
        db.favorites()
            .unwrap()
            .contains(&("reference".into(), entries[0].id.clone()))
    );
    assert_eq!(
        db.desk
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn other_languages_module_export_contains_entries_and_linked_exercises() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let removed = db
        .reference_entry("ref:js-advanced:finally-return")
        .unwrap()
        .unwrap();
    let mut db = db;
    db.delete_reference(&removed).unwrap();
    let path = dir.path().join("encyclopedia.zip");
    crate::export::write(dir.path(), &path, crate::export::Scope::Reference).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let exported: Vec<Entry> =
        serde_json::from_reader(zip.by_name("百科全书.json").unwrap()).unwrap();
    assert!(!exported.iter().any(|e| e.id == removed.id));
    for entry in catalog().0.into_iter().filter(|e| e.id != removed.id) {
        assert!(exported.iter().any(|e| e.id == entry.id));
    }
    let exported_cards: crate::study::Bundle =
        serde_json::from_reader(zip.by_name("百科练习.json").unwrap()).unwrap();
    for card in catalog()
        .1
        .into_iter()
        .filter(|c| !c.knowledge_ids.contains(&removed.id))
    {
        assert!(exported_cards.cards.iter().any(|c| c.id == card.id));
    }
}
