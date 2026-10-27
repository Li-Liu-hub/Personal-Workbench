use super::encyclopedia::{deep_card_hash, deep_entry_hash};
use super::*;
use crate::{reference::Entry, study::Card};
use std::collections::HashMap;

fn pack<T: serde::de::DeserializeOwned>(text: &str) -> Vec<T> {
    serde_json::from_str(text).unwrap()
}

fn deep_pack() -> (Vec<Entry>, Vec<Card>) {
    (
        pack(include_str!("../../assets/deep_readings.json")),
        pack(include_str!("../../assets/deep_readings_practice.json")),
    )
}

fn upgrade_pack() -> (Vec<Entry>, Vec<Card>) {
    (
        pack(include_str!("../../assets/deep_upgrades.json")),
        pack(include_str!("../../assets/deep_upgrades_practice.json")),
    )
}

fn seeded_packs() -> (Vec<Entry>, Vec<Card>) {
    let entry_packs = [
        include_str!("../../assets/data_structures.json"),
        include_str!("../../assets/computer_organization.json"),
        include_str!("../../assets/operating_systems.json"),
        include_str!("../../assets/computer_networks.json"),
        include_str!("../../assets/mysql_internals.json"),
    ];
    let card_packs = [
        include_str!("../../assets/data_structures_practice.json"),
        include_str!("../../assets/computer_organization_practice.json"),
        include_str!("../../assets/operating_systems_practice.json"),
        include_str!("../../assets/computer_networks_practice.json"),
        include_str!("../../assets/mysql_internals_practice.json"),
    ];
    (
        entry_packs.into_iter().flat_map(pack::<Entry>).collect(),
        card_packs.into_iter().flat_map(pack::<Card>).collect(),
    )
}

#[test]
fn deep_readings_import_with_linked_practice_and_search() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let (entries, cards) = deep_pack();
    assert_eq!(entries.len(), 30);
    assert_eq!(cards.len(), 30);
    assert_eq!(db.reference_count("Java").unwrap(), 234);
    assert_eq!(db.reference_count("后端").unwrap(), 70);
    for entry in &entries {
        let stored = db.reference_entry(&entry.id).unwrap().unwrap();
        assert_eq!(stored.entry_type, "精读");
        assert_eq!(stored.source, entry.source);
        assert!(stored.notes.chars().count() > 300, "{}", entry.id);
        let found = db
            .search_reference(&crate::reference::Filter {
                language: entry.language.clone(),
                query: entry.tags[2].clone(),
                ..Default::default()
            })
            .unwrap();
        assert!(found.iter().any(|item| item.id == entry.id), "{}", entry.id);
    }
    for card in &cards {
        card.validate().unwrap();
        let stored = db.study_card(&card.id).unwrap().unwrap();
        assert_eq!(stored.rubric.len(), card.rubric.len());
        for knowledge_id in &card.knowledge_ids {
            assert!(
                db.knowledge_cards(knowledge_id)
                    .unwrap()
                    .iter()
                    .any(|item| item.id == card.id),
                "{}",
                card.id
            );
        }
    }
    let found = db
        .search_reference(&crate::reference::Filter {
            query: "happens-before".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(
        found
            .iter()
            .any(|item| item.id == "ref:deep:java-jmm-happens-before")
    );
}

#[test]
fn upgrade_fingerprints_match_python_canonical_json() {
    let entry_hashes: HashMap<String, Vec<String>> =
        serde_json::from_str(include_str!("../../assets/deep_upgrade_entry_hashes.json")).unwrap();
    let card_hashes: HashMap<String, Vec<String>> =
        serde_json::from_str(include_str!("../../assets/deep_upgrade_card_hashes.json")).unwrap();
    let (entries, cards) = seeded_packs();
    let mut matched_entries = 0;
    for entry in &entries {
        if let Some(allowed) = entry_hashes.get(&entry.id) {
            assert!(
                allowed.contains(&deep_entry_hash(entry).unwrap()),
                "{}",
                entry.id
            );
            matched_entries += 1;
        }
    }
    let mut matched_cards = 0;
    for card in &cards {
        if let Some(allowed) = card_hashes.get(&card.id) {
            assert!(
                allowed.contains(&deep_card_hash(card).unwrap()),
                "{}",
                card.id
            );
            matched_cards += 1;
        }
    }
    assert_eq!(matched_entries, 30);
    assert_eq!(matched_cards, 30);
}

#[test]
fn deep_upgrades_rewrite_untouched_seed_content() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let (entries, cards) = upgrade_pack();
    for seed in &entries {
        let stored = db.reference_entry(&seed.id).unwrap().unwrap();
        assert_eq!(stored.notes, seed.notes, "{}", seed.id);
        assert_eq!(stored.syntax, seed.syntax);
        assert_eq!(stored.example, seed.example);
        assert_eq!(stored.entry_type, "精读");
        assert_eq!(stored.language, seed.language);
        assert_eq!(stored.title, seed.title);
        assert!(stored.tags.len() >= seed.tags.len());
    }
    for seed in &cards {
        let stored = db.study_card(&seed.id).unwrap().unwrap();
        assert_eq!(stored.prompt, seed.prompt, "{}", seed.id);
        assert_eq!(stored.answer, seed.answer);
        assert_eq!(stored.rubric, seed.rubric);
    }
}

#[test]
fn deep_upgrade_replays_without_touching_personal_data() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let (entry_seeds, card_seeds) = upgrade_pack();
    let (old_entries, old_cards) = seeded_packs();
    let entry_seed = &entry_seeds[0];
    let card_seed = &card_seeds[0];
    let old_entry = old_entries
        .iter()
        .find(|entry| entry.id == entry_seed.id)
        .unwrap();
    assert_ne!(old_entry.notes, entry_seed.notes);
    let mut restored = db.reference_entry(&old_entry.id).unwrap().unwrap();
    restored.summary = old_entry.summary.clone();
    restored.syntax = old_entry.syntax.clone();
    restored.example = old_entry.example.clone();
    restored.notes = old_entry.notes.clone();
    restored.source = old_entry.source.clone();
    restored.entry_type = old_entry.entry_type.clone();
    restored.tags = old_entry.tags.clone();
    db.save_reference(&restored).unwrap();
    let old_card = old_cards
        .iter()
        .find(|card| card.id == card_seed.id)
        .unwrap();
    db.desk
        .execute(
            "UPDATE study_cards SET topic=?2,card_json=?3 WHERE id=?1",
            params![
                old_card.id,
                old_card.topic,
                serde_json::to_string(old_card).unwrap()
            ],
        )
        .unwrap();

    let mut custom = db.reference_entry(&entry_seeds[1].id).unwrap().unwrap();
    custom.notes.push_str("\n个人补充");
    let custom = db.save_reference(&custom).unwrap();
    let removed = db.reference_entry(&entry_seeds[2].id).unwrap().unwrap();
    db.delete_reference(&removed).unwrap();
    let mut custom_card = db.study_card(&card_seeds[3].id).unwrap().unwrap();
    custom_card.prompt.push_str("\n个人练习要求");
    db.desk
        .execute(
            "UPDATE study_cards SET card_json=?2 WHERE id=?1",
            params![custom_card.id, serde_json::to_string(&custom_card).unwrap()],
        )
        .unwrap();
    let pending = crate::study::Pending {
        id: "deep-upgrade-retention".into(),
        card_id: custom_card.id.clone(),
        answer: "原有作答".into(),
        at: "2026-10-27T09:00:00+08:00".into(),
    };
    let attempt = db
        .record_study_attempt(
            &pending,
            crate::study::Grade::unknown(&custom_card),
            "test",
            1,
        )
        .unwrap();

    db.desk
        .execute(
            "DELETE FROM settings WHERE key='encyclopedia.deep_upgrades.version'",
            [],
        )
        .unwrap();
    db.prepare_encyclopedia().unwrap();

    assert_eq!(
        db.reference_entry(&entry_seed.id).unwrap().unwrap().notes,
        entry_seed.notes
    );
    assert_eq!(
        db.study_card(&card_seed.id).unwrap().unwrap().prompt,
        card_seed.prompt
    );
    assert_eq!(
        db.reference_entry(&custom.id).unwrap().unwrap().notes,
        custom.notes
    );
    assert!(db.reference_entry(&removed.id).unwrap().is_none());
    assert_eq!(
        db.study_card(&custom_card.id).unwrap().unwrap().prompt,
        custom_card.prompt
    );
    assert_eq!(
        serde_json::to_value(&db.study_attempts(&custom_card.id).unwrap()[0]).unwrap(),
        serde_json::to_value(&attempt).unwrap()
    );
    assert_eq!(
        db.reference_entry(&entry_seeds[4].id)
            .unwrap()
            .unwrap()
            .notes,
        entry_seeds[4].notes
    );
    assert_eq!(
        db.desk
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}
