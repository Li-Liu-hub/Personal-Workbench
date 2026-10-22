use super::*;
use crate::db::Database;

#[test]
fn builtin_reference_has_eight_languages_with_valid_original_examples() {
    let entries: Vec<Entry> =
        serde_json::from_str(include_str!("../../assets/reference.json")).unwrap();
    assert_eq!(entries.len(), 128);
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
        assert_eq!(rows.len(), 16);
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
    assert_eq!(db.search_reference(&Filter::default()).unwrap().len(), 128);
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
    assert_eq!(db.reference_entries().unwrap().len(), 127);
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
            serde_json::from_reader(zip.by_name("指令大全.json").unwrap()).unwrap();
        assert!(entries.contains(&item));
        assert!(!entries.iter().any(|e| e.id == removed.id));
        let favorites: Vec<(String, String)> =
            serde_json::from_reader(zip.by_name("收藏.json").unwrap()).unwrap();
        assert!(favorites.contains(&("reference".into(), item.id.clone())));
    }
}
