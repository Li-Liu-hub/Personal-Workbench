use super::*;
use crate::db::Database;

#[test]
fn builtin_reference_has_eight_languages_with_valid_original_examples() {
    let entries: Vec<Entry> =
        serde_json::from_str(include_str!("../../assets/reference.json")).unwrap();
    assert_eq!(entries.len(), 358);
    let mut ids = std::collections::BTreeSet::new();
    for item in &entries {
        item.validate().unwrap();
        assert!(ids.insert(&item.id));
        assert!(
            !item.syntax.trim().is_empty()
                && !item.example.trim().is_empty()
                && !item.notes.trim().is_empty()
        );
    }
    for language in LANGUAGES {
        let rows = entries
            .iter()
            .filter(|e| e.language == language)
            .collect::<Vec<_>>();
        assert!(rows.len() >= 36);
        assert!(
            rows.iter()
                .any(|e| e.category == "基础语法" || language == "Linux" || language == "MySQL")
        );
        if !matches!(language, "Linux" | "MySQL") {
            assert!(rows.iter().any(|e| e.category == "集合与容器"));
        }
    }
}

#[test]
fn reference_search_combines_language_category_literals_and_multiple_terms() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.search_reference(&Filter::default()).unwrap().len(),
        BUILTIN_COUNT
    );
    for (alias, language) in [("JS", "JavaScript"), ("CPP", "C++"), ("golang", "Go")] {
        let rows = db
            .search_reference(&Filter {
                query: alias.into(),
                ..Default::default()
            })
            .unwrap();
        assert!(rows.iter().any(|r| r.language == language));
    }
    let rows = db
        .search_reference(&Filter {
            language: "Java".into(),
            category: "集合与容器".into(),
            query: "HashMap 键".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].title.contains("HashMap"));
    assert!(
        !db.search_reference(&Filter {
            query: "C++".into(),
            ..Default::default()
        })
        .unwrap()
        .is_empty()
    );
    let mut item = Entry::new();
    item.language = "自定义语言".into();
    item.category = "工作笔记".into();
    item.title = "literal_%\\\" 中文".into();
    item.example = "marker_token 比例 100%".into();
    let item = db.save_reference(&item).unwrap();
    for query in [
        "%",
        "_",
        "\\",
        "\"",
        "中文",
        "marker_token",
        "100%",
        "自定义语言 工作笔记",
    ] {
        assert!(
            db.search_reference(&Filter {
                query: query.into(),
                ..Default::default()
            })
            .unwrap()
            .iter()
            .any(|r| r.id == item.id),
            "{query}"
        );
    }
    assert!(
        db.search_reference(&Filter {
            query: "' OR 1=1 --".into(),
            ..Default::default()
        })
        .unwrap()
        .is_empty()
    );
}

#[test]
fn edits_reclassify_and_update_index_while_stale_writes_never_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut draft = Entry::new();
    draft.title = "unique-before-token".into();
    let first = db.save_reference(&draft).unwrap();
    let mut edited = first.clone();
    edited.title = "unique-after-token".into();
    edited.category = "自定义集合".into();
    let second = db.save_reference(&edited).unwrap();
    assert_eq!(second.revision, first.revision + 1);
    assert!(db.save_reference(&first).is_err());
    assert!(db.delete_reference(&first).is_err());
    assert!(
        db.search_reference(&Filter {
            query: "unique-before-token".into(),
            ..Default::default()
        })
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        db.search_reference(&Filter {
            category: "自定义集合".into(),
            query: "unique-after-token".into(),
            ..Default::default()
        })
        .unwrap()
        .len(),
        1
    );
    let mut invalid = second.clone();
    invalid.title.clear();
    assert!(db.save_reference(&invalid).is_err());
    assert_eq!(db.reference_entry(&second.id).unwrap(), Some(second));
}

#[test]
fn customized_and_deleted_builtins_survive_restart_and_seed_replay() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut item = db.reference_entry("ref:java:01").unwrap().unwrap();
    item.title = "我的变量笔记".into();
    let saved = db.save_reference(&item).unwrap();
    let removed = db.reference_entry("ref:js:01").unwrap().unwrap();
    db.delete_reference(&removed).unwrap();
    db.set_setting("reference.seed.version", "old").unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(db.reference_entry(&saved.id).unwrap(), Some(saved));
    assert!(db.reference_entry(&removed.id).unwrap().is_none());
    assert_eq!(db.reference_entries().unwrap().len(), BUILTIN_COUNT - 1);
}

#[test]
fn reference_export_keeps_custom_fields_and_excludes_deleted_entries() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut item = Entry::new();
    item.title = "我的收藏".into();
    item.example = "let value = 42;".into();
    let item = db.save_reference(&item).unwrap();
    db.set_favorite("reference", &item.id, true).unwrap();
    let removed = db.reference_entry("ref:c:01").unwrap().unwrap();
    db.delete_reference(&removed).unwrap();
    for scope in [crate::export::Scope::Reference, crate::export::Scope::All] {
        let path = dir.path().join(format!("{scope:?}.zip"));
        crate::export::write(dir.path(), &path, scope).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let entries: Vec<Entry> =
            serde_json::from_reader(zip.by_name("百科全书.json").unwrap()).unwrap();
        assert!(entries.contains(&item));
        assert!(!entries.iter().any(|e| e.id == removed.id));
        let favorites: Vec<(String, String)> =
            serde_json::from_reader(zip.by_name("收藏.json").unwrap()).unwrap();
        assert!(favorites.contains(&("reference".into(), item.id.clone())));
    }
}

#[test]
fn knowledge_metadata_search_and_exact_practice_links_cover_the_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let entries = db.reference_entries().unwrap();
    for item in &entries {
        assert!(!item.tags.is_empty(), "{}", item.id);
        let cards = db
            .practice_candidates_for(crate::curriculum::MIXED, &item.id)
            .unwrap();
        assert!(!cards.is_empty(), "missing exercises: {}", item.id);
        for c in &cards {
            c.card.validate().unwrap();
            assert!(c.card.knowledge_ids.contains(&item.id));
            assert!(
                c.card.tags.contains(&item.language),
                "entry {} card {} tags {:?} lacks language {}",
                item.id,
                c.card.id,
                c.card.tags,
                item.language
            );
        }
    }
    for (query, expected) in [
        ("JS 原型对象", "ref:js:prototype-object"),
        ("对象解构", "ref:js:destructure-object"),
        ("解构 默认参数", "ref:js:destructure-params"),
        ("MySQL LAST_VALUE", "ref:mysql:last-value"),
        ("PERCENT_RANK", "ref:mysql:percent-rank"),
    ] {
        assert!(
            db.search_reference(&Filter {
                query: query.into(),
                ..Default::default()
            })
            .unwrap()
            .iter()
            .any(|e| e.id == expected),
            "{query}"
        );
    }
    let mut filter = Filter {
        language: "MySQL".into(),
        category: "窗口函数·排名".into(),
        entry_type: "函数".into(),
        tag: "RANK".into(),
        query: "并列".into(),
    };
    let rows = db.search_reference(&filter).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "ref:mysql:rank");
    filter.tag = "不存在的标签".into();
    assert!(db.search_reference(&filter).unwrap().is_empty());
    let mut item = db
        .reference_entry("ref:js:prototype-chain")
        .unwrap()
        .unwrap();
    item.tags = vec!["我的原型笔记".into()];
    item.entry_type = "自定义概念".into();
    db.save_reference(&item).unwrap();
    assert_eq!(
        db.search_reference(&Filter {
            query: "我的原型笔记".into(),
            entry_type: "自定义概念".into(),
            ..Default::default()
        })
        .unwrap()
        .len(),
        1
    );
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.reference_entry(&item.id).unwrap().unwrap().tags,
        item.tags
    );
}

#[test]
fn window_labs_have_real_ties_and_reject_wrong_results_without_changing_data() {
    let cards: Vec<crate::study::Card> =
        serde_json::from_str(include_str!("../../assets/reference_practice.json")).unwrap();
    let labs = cards
        .iter()
        .filter_map(|c| c.sql.as_ref().map(|lab| (c, lab)))
        .collect::<Vec<_>>();
    assert_eq!(labs.len(), 18);
    for (card, lab) in labs {
        let reference = crate::sql_practice::run(lab, &lab.reference);
        assert_eq!(
            reference.matches,
            Some(true),
            "{}: {:?}",
            card.id,
            reference.error
        );
        assert!(reference.rolled_back);
        let wrong = crate::sql_practice::run(lab, "SELECT id FROM sales WHERE 0");
        assert_eq!(wrong.matches, Some(false));
        assert!(wrong.rolled_back);
        let damaged =
            crate::sql_practice::run(lab, "UPDATE sales SET amount=0; SELECT id FROM sales;");
        assert!(damaged.rolled_back);
        assert_eq!(
            crate::sql_practice::run(lab, &lab.reference).matches,
            Some(true)
        );
    }
}
