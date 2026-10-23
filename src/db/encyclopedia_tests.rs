use super::*;

#[test]
fn encyclopedia_ecosystem_is_searchable_linked_and_replay_preserves_edits() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.reference_count("").unwrap(),
        crate::reference::BUILTIN_COUNT
    );
    assert_eq!(db.reference_count("Java").unwrap(), 173);
    let additions: Vec<crate::reference::Entry> =
        serde_json::from_str(include_str!("../../assets/java_ecosystem.json")).unwrap();
    assert_eq!(additions.len(), 133);
    for entry in additions {
        entry.validate().unwrap();
        assert!(entry.notes.contains("版本前提"));
        assert!(entry.example.len() > 20);
        assert!(
            !db.practice_candidates_for(crate::curriculum::MIXED, &entry.id)
                .unwrap()
                .is_empty()
        );
    }
    for (query, id) in [
        ("Java 锁升级", "lock-states"),
        ("Spring 事务 传播", "tx-propagation"),
        ("MyBatis #{}", "mybatis-params"),
        ("虚拟线程 pinning", "virtual-monitor"),
        ("Spring Boot 配置", "properties"),
    ] {
        assert!(
            db.search_reference(&crate::reference::Filter {
                query: query.into(),
                ..Default::default()
            })
            .unwrap()
            .iter()
            .any(|e| e.id == format!("ref:java-eco:{id}")),
            "{query}"
        );
    }
    let mut customized = db.reference_entry("ref:java-eco:ioc-di").unwrap().unwrap();
    customized.notes = "个人补充内容".into();
    let customized = db.save_reference(&customized).unwrap();
    let deleted = db.reference_entry("ref:java-eco:jwt").unwrap().unwrap();
    db.delete_reference(&deleted).unwrap();
    db.set_setting("encyclopedia.ecosystem.version", "old")
        .unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.reference_entry(&customized.id).unwrap(),
        Some(customized)
    );
    assert!(db.reference_entry(&deleted.id).unwrap().is_none());
    assert_eq!(
        db.reference_count("").unwrap(),
        crate::reference::BUILTIN_COUNT - 1
    );
}
