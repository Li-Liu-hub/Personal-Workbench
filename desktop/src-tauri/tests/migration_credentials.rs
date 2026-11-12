use personal_desk_lib::{db::Database, model::Credential};

#[test]
fn credential_notes_survive_create_edit_and_reload() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut entry = Credential {
        id: "test-id".into(),
        title: "测试".into(),
        username: "用户".into(),
        password: "密码".into(),
        notes: "备注\n第二行".into(),
        website: "https://example.test".into(),
        tags: vec!["分类".into()],
    };
    db.save_credential(&entry).unwrap();
    entry.notes = "更新后的备注".into();
    db.save_credential(&entry).unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    let stored = db.credential("test-id").unwrap().unwrap();
    assert_eq!(stored.notes, entry.notes);
    assert_eq!(stored.username, entry.username);
    assert_eq!(stored.password, entry.password);
    assert_eq!(stored.tags, entry.tags);
}

#[test]
fn writing_revision_conflicts_preserve_existing_content() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut first = db.create_writing("文档", None, None).unwrap();
    let mut stale = first.clone();
    first.body = "已保存内容".into();
    db.finish_writing_save("test-first", &mut first).unwrap();
    stale.body = "旧版本内容".into();
    assert!(db.finish_writing_save("test-stale", &mut stale).is_err());
    assert_eq!(
        db.writing_page(&first.id).unwrap().unwrap().body,
        "已保存内容"
    );
}

#[test]
fn credential_classification_and_favorites_survive_reload() {
    use personal_desk_lib::tags::Scope;
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let entry = Credential {
        id: "c1".into(),
        title: "分类验证".into(),
        tags: vec!["旧分类".into()],
        ..Credential::default()
    };
    db.save_credential(&entry).unwrap();
    db.set_favorite("credentials", &entry.id, true).unwrap();
    db.rename_tag(Scope::Credentials, "旧分类", "新分类")
        .unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(db.credential("c1").unwrap().unwrap().tags, vec!["新分类"]);
    assert!(
        db.favorites()
            .unwrap()
            .contains(&("credentials".into(), "c1".into()))
    );
}

#[test]
fn credential_view_preference_and_other_settings_survive_reload() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    db.set_setting("view.credentials", "list").unwrap();
    db.set_setting("unrelated.preference", "keep").unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    let settings = db.settings().unwrap();
    assert_eq!(
        settings.get("view.credentials").map(String::as_str),
        Some("list")
    );
    assert_eq!(
        settings.get("unrelated.preference").map(String::as_str),
        Some("keep")
    );
}
