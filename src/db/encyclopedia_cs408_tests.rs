use super::*;
use crate::{reference::Entry, study::Card};

fn packs() -> Vec<(&'static str, Vec<Entry>, Vec<Card>)> {
    [
        (
            "data_structures",
            include_str!("../../assets/data_structures.json"),
            include_str!("../../assets/data_structures_practice.json"),
        ),
        (
            "computer_organization",
            include_str!("../../assets/computer_organization.json"),
            include_str!("../../assets/computer_organization_practice.json"),
        ),
        (
            "operating_systems",
            include_str!("../../assets/operating_systems.json"),
            include_str!("../../assets/operating_systems_practice.json"),
        ),
        (
            "computer_networks",
            include_str!("../../assets/computer_networks.json"),
            include_str!("../../assets/computer_networks_practice.json"),
        ),
        (
            "mysql_internals",
            include_str!("../../assets/mysql_internals.json"),
            include_str!("../../assets/mysql_internals_practice.json"),
        ),
    ]
    .into_iter()
    .map(|(name, entries, cards)| {
        (
            name,
            serde_json::from_str(entries).unwrap(),
            serde_json::from_str(cards).unwrap(),
        )
    })
    .collect()
}

#[test]
fn cs408_four_subjects_and_mysql_are_searchable_and_have_practice() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let mut ids = std::collections::HashSet::new();
    let mut titles = std::collections::HashSet::new();
    for entry in db.reference_entries().unwrap() {
        assert!(ids.insert(entry.id));
        assert!(titles.insert((entry.language, entry.title)));
    }
    let packs = packs();
    assert_eq!(packs.len(), 5);
    let mut total = 0;
    for subject in ["数据结构", "计算机组成原理", "操作系统", "计算机网络"] {
        let pack = packs
            .iter()
            .find(|(_, entries, _)| entries.iter().any(|e| e.language == subject))
            .unwrap();
        let count = pack.1.iter().filter(|e| e.language == subject).count();
        assert_eq!(count, 24, "{subject}");
        total += count;
        assert!(
            db.reference_facets()
                .unwrap()
                .contains(&(subject.into(), subject.into()))
        );
    }
    assert_eq!(total, 96);
    assert_eq!(packs[4].1.len(), 24);
    for (_, entries, cards) in &packs {
        assert_eq!(entries.len(), cards.len());
        for (entry, card) in entries.iter().zip(cards) {
            entry.validate().unwrap();
            card.validate().unwrap();
            assert!(!entry.example.is_empty());
            assert!(entry.notes.contains("原理与边界"));
            assert_eq!(card.knowledge_ids, vec![entry.id.clone()]);
            assert!(card.training.is_some());
            let found = db
                .search_reference(&crate::reference::Filter {
                    language: entry.language.clone(),
                    category: entry.category.clone(),
                    tag: entry.tags[1].clone(),
                    query: entry.tags[2].clone(),
                    ..Default::default()
                })
                .unwrap();
            assert!(found.iter().any(|e| e.id == entry.id), "{}", entry.id);
            let practice = db
                .practice_candidates_for(crate::curriculum::MIXED, &entry.id)
                .unwrap();
            assert_eq!(practice.len(), 1);
            assert_eq!(practice[0].card.id, card.id);
        }
    }
    for query in ["MVCC", "多版本", "Read View", "undo", "快照读", "锁定读"] {
        let found = db
            .search_reference(&crate::reference::Filter {
                language: "MySQL".into(),
                query: query.into(),
                ..Default::default()
            })
            .unwrap();
        assert!(!found.is_empty(), "{query}");
    }
    for subject in ["数据结构", "计算机组成原理", "操作系统", "计算机网络"] {
        assert_eq!(db.reference_count(subject).unwrap(), 24);
    }
    assert_eq!(db.reference_count("408").unwrap(), 0);
    assert_eq!(
        db.reference_count("").unwrap(),
        crate::reference::BUILTIN_COUNT
    );
    assert_eq!(
        db.study_count("engineering").unwrap(),
        crate::curriculum::BUILTIN_COUNT
    );
}

#[test]
fn cs408_mysql_upgrade_after_existing_markers_and_replay_preserve_personal_data() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let packs = packs();
    // Model an existing installation whose older encyclopedia packs are complete.
    for (name, entries, cards) in &packs {
        for card in cards {
            db.desk
                .execute("DELETE FROM study_knowledge WHERE card_id=?1", [&card.id])
                .unwrap();
            db.desk
                .execute("DELETE FROM study_cards WHERE id=?1", [&card.id])
                .unwrap();
        }
        for entry in entries {
            db.desk
                .execute("DELETE FROM command_entries WHERE id=?1", [&entry.id])
                .unwrap();
        }
        db.desk
            .execute(
                "DELETE FROM settings WHERE key=?1",
                [format!("encyclopedia.{name}.version")],
            )
            .unwrap();
    }
    let mut old = db.reference_entry("ref:java-eco:ioc-di").unwrap().unwrap();
    old.notes = "旧安装个人笔记".into();
    let old = db.save_reference(&old).unwrap();
    assert_eq!(
        db.setting("encyclopedia.other_languages.version")
            .unwrap()
            .as_deref(),
        Some("1")
    );
    drop(db);
    let mut db = Database::open(dir.path()).unwrap();
    assert_eq!(db.reference_entry(&old.id).unwrap(), Some(old));
    assert_eq!(
        db.reference_count("").unwrap(),
        crate::reference::BUILTIN_COUNT
    );
    let mut kept = vec![];
    let mut removed = vec![];
    let mut saved_cards = vec![];
    let mut attempts = vec![];
    for (name, entries, cards) in &packs {
        let mut item = db.reference_entry(&entries[0].id).unwrap().unwrap();
        item.notes = format!("{name} 个人补充");
        let item = db.save_reference(&item).unwrap();
        db.set_favorite("reference", &item.id, true).unwrap();
        kept.push(item);
        let item = db.reference_entry(&entries[1].id).unwrap().unwrap();
        db.delete_reference(&item).unwrap();
        removed.push(item.id);
        let mut card = db.study_card(&cards[0].id).unwrap().unwrap();
        card.prompt.push_str("\n个人练习要求");
        db.desk
            .execute(
                "UPDATE study_cards SET card_json=?2 WHERE id=?1",
                params![card.id, serde_json::to_string(&card).unwrap()],
            )
            .unwrap();
        let pending = crate::study::Pending {
            id: format!("{name}-retention"),
            card_id: card.id.clone(),
            answer: "原有作答".into(),
            at: "2026-10-24T22:00:00+08:00".into(),
        };
        let attempt = db
            .record_study_attempt(&pending, crate::study::Grade::unknown(&card), "test", 1)
            .unwrap();
        attempts.push((card.id.clone(), serde_json::to_value(attempt).unwrap()));
        saved_cards.push(card);
        db.set_setting(&format!("encyclopedia.{name}.version"), "old")
            .unwrap();
    }
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    for item in &kept {
        assert_eq!(db.reference_entry(&item.id).unwrap().as_ref(), Some(item));
        assert!(
            db.favorites()
                .unwrap()
                .contains(&("reference".into(), item.id.clone()))
        );
    }
    for id in removed {
        assert!(db.reference_entry(&id).unwrap().is_none());
    }
    for card in saved_cards {
        assert_eq!(
            db.study_card(&card.id).unwrap().unwrap().prompt,
            card.prompt
        );
    }
    for (id, attempt) in attempts {
        assert_eq!(
            serde_json::to_value(&db.study_attempts(&id).unwrap()[0]).unwrap(),
            attempt
        );
    }
    assert_eq!(
        db.reference_count("").unwrap(),
        crate::reference::BUILTIN_COUNT - 5
    );
    assert_eq!(
        db.desk
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn cs408_mysql_reference_export_includes_linked_exercises() {
    let dir = tempfile::tempdir().unwrap();
    Database::open(dir.path()).unwrap();
    let output = dir.path().join("encyclopedia.zip");
    crate::export::write(dir.path(), &output, crate::export::Scope::Reference).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(output).unwrap()).unwrap();
    let entries: Vec<Entry> =
        serde_json::from_reader(zip.by_name("百科全书.json").unwrap()).unwrap();
    let bundle: crate::study::Bundle =
        serde_json::from_reader(zip.by_name("百科练习.json").unwrap()).unwrap();
    for (_, expected_entries, expected_cards) in packs() {
        for entry in expected_entries {
            assert!(entries.iter().any(|e| e.id == entry.id));
        }
        for card in expected_cards {
            assert!(bundle.cards.iter().any(|c| c.id == card.id));
        }
    }
}
