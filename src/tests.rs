use std::fs;

use crate::{
    db::Database,
    files,
    model::{Credential, InventoryItem},
};

#[test]
fn document_labels_reuse_legacy_path_keys_without_creating_a_duplicate_alias() {
    let directory = tempfile::tempdir().unwrap();
    let base = directory.path().join("data");
    drop(Database::open(&base).unwrap());
    let document = directory.path().join("旧文档.txt");
    fs::write(&document, "synthetic").unwrap();
    let legacy = files::shell_path(&document.canonicalize().unwrap())
        .to_string_lossy()
        .into_owned();
    let connection = rusqlite::Connection::open(base.join("desk.db")).unwrap();
    connection
        .execute(
            "INSERT INTO documents(path,format,title) VALUES(?1,'txt','旧标题')",
            [&legacy],
        )
        .unwrap();
    drop(connection);
    let mut db = Database::open(&base).unwrap();
    let key = db
        .document_record_key(&document.canonicalize().unwrap())
        .unwrap();
    assert_eq!(key, legacy);
    db.set_record_tags(crate::tags::Scope::Documents, &key, &["工作".into()])
        .unwrap();
    assert_eq!(db.document_records().unwrap().len(), 1);
    assert_eq!(
        db.record_tags(crate::tags::Scope::Documents, &legacy)
            .unwrap(),
        ["工作"]
    );
    assert_eq!(db.document_records().unwrap()[0].title, "旧标题");
}

#[test]
fn additional_module_labels_survive_edits_restart_rename_delete_and_scoped_exports() {
    use crate::{contacts::Contact, export::Scope as E, model::Website, tags::Scope as T};
    let directory = tempfile::tempdir().unwrap();
    let base = directory.path().join("data");
    let mut db = Database::open(&base).unwrap();
    let folder = directory.path().join("folder");
    fs::create_dir(&folder).unwrap();
    db.save_shortcut("folder", &folder, true).unwrap();
    let shortcut = db.shortcuts().unwrap().remove(0);
    let document = base.join("document.md");
    fs::write(&document, "fixture").unwrap();
    db.set_document_title(&document, "文档原名").unwrap();
    let document = db.document_records().unwrap().remove(0);
    let mut website = Website::new();
    website.title = "站点".into();
    website.url = "https://example.test".into();
    db.save_website(&website).unwrap();
    let mut contact = Contact::new();
    contact.name = "示例联系人".into();
    db.save_contact(&contact).unwrap();
    let program = directory.path().join("tool.exe");
    fs::write(&program, b"MZ synthetic").unwrap();
    let tool = crate::launcher::AppTool::new(program);
    db.save_app_tool(&tool).unwrap();
    let cases = [
        (T::Shortcuts, shortcut.id.clone(), E::Folders),
        (
            T::Documents,
            document.path.to_string_lossy().into_owned(),
            E::Documents,
        ),
        (T::Websites, website.id.clone(), E::Websites),
        (T::Contacts, contact.id.clone(), E::Contacts),
        (T::Tools, tool.id.clone(), E::Tools),
    ];
    for (scope, id, _) in &cases {
        db.set_record_tags(*scope, id, &["工作".into(), "工作".into()])
            .unwrap();
        assert_eq!(db.record_tags(*scope, id).unwrap(), ["工作"]);
    }
    // Ordinary edits do not overwrite labels stored outside a record's editor.
    website.title = "站点新名".into();
    db.save_website(&website).unwrap();
    contact.school = "示例学校".into();
    db.save_contact(&contact).unwrap();
    db.save_app_tool(&tool).unwrap();
    db.set_document_title(&document.path, "文档新名").unwrap();
    drop(db);
    let mut db = Database::open(&base).unwrap();
    for (scope, id, export_scope) in &cases {
        assert_eq!(db.record_tags(*scope, id).unwrap(), ["工作"]);
        db.rename_tag(*scope, "工作", "学习").unwrap();
        assert_eq!(db.record_tags(*scope, id).unwrap(), ["学习"]);
        let output = directory.path().join(format!("{}.zip", scope.key()));
        crate::export::write(&base, &output, *export_scope).unwrap();
        let mut archive = zip::ZipArchive::new(fs::File::open(output).unwrap()).unwrap();
        let assignments = archive_json(&mut archive, "条目标签.json");
        assert_eq!(assignments.as_array().unwrap().len(), 1);
        assert_eq!(assignments[0]["id"], *id);
        assert_eq!(assignments[0]["tags"], serde_json::json!(["学习"]));
        db.delete_tag(*scope, "学习").unwrap();
        assert!(db.record_tags(*scope, id).unwrap().is_empty());
        db.set_record_tags(*scope, id, &[]).unwrap();
    }
}

#[test]
fn inventory_category_upgrade_preserves_distinct_legacy_values_and_never_restores_cleared_tags() {
    use crate::tags::Scope;
    let dir = tempfile::tempdir().unwrap();
    drop(Database::open(dir.path()).unwrap());
    let connection = rusqlite::Connection::open(dir.path().join("desk.db")).unwrap();
    connection
        .execute_batch(
            "DELETE FROM settings WHERE key='inventory.tags_unified_v1';
        INSERT INTO inventory_items(id,name,category,quantity,location,notes,tags_json)
        VALUES('old-item','设备','数码',2,'书桌','原备注','[\"已买\"]');",
        )
        .unwrap();
    drop(connection);
    let mut db = Database::open(dir.path()).unwrap();
    let item = &db.inventory_items().unwrap()[0];
    assert_eq!(item.tags, vec!["已买", "数码"]);
    assert_eq!(item.category, "已买 · 数码");
    assert_eq!(item.notes, "原备注");
    assert_eq!(item.quantity, 2);
    db.set_record_tags(Scope::Inventory, "old-item", &[])
        .unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    let item = &db.inventory_items().unwrap()[0];
    assert!(item.tags.is_empty());
    assert!(item.category.is_empty());
    assert_eq!(item.notes, "原备注");
}

#[test]
fn custom_tags_persist_rename_and_delete_without_changing_accounts_or_inventory() {
    use crate::tags::{Filter, Scope};
    let directory = tempfile::tempdir().unwrap();
    let mut db = Database::open(directory.path()).unwrap();
    let mut account = Credential::new();
    account.title = "测试账号".into();
    account.password = "synthetic-visible".into();
    account.tags = vec!["工作".into(), "学习".into(), "工作".into()];
    db.save_credential(&account).unwrap();
    let mut item = InventoryItem::new();
    item.name = "显示器".into();
    item.quantity = 3;
    item.tags = vec!["工作".into()];
    db.save_inventory_item(&item).unwrap();
    db.create_tag(Scope::Credentials, "暂未使用").unwrap();
    assert!(db.create_tag(Scope::Credentials, "工作").is_err());
    db.rename_tag(Scope::Credentials, "工作", "办公").unwrap();
    assert!(db.rename_tag(Scope::Credentials, "办公", "学习").is_err());
    drop(db);
    let mut db = Database::open(directory.path()).unwrap();
    let stored = db.credential(&account.id).unwrap().unwrap();
    assert_eq!(stored.tags, vec!["办公", "学习"]);
    assert_eq!(stored.password, account.password);
    assert!(Filter::Named("学习".into()).matches(&stored.tags));
    assert!(!Filter::Untagged.matches(&stored.tags));
    assert_eq!(db.inventory_items().unwrap()[0].tags, vec!["工作"]);
    db.delete_tag(Scope::Credentials, "办公").unwrap();
    assert_eq!(db.credential_summaries().unwrap()[0].tags, vec!["学习"]);
    db.set_record_tags(Scope::Credentials, &account.id, &[])
        .unwrap();
    assert!(Filter::Untagged.matches(&db.credential(&account.id).unwrap().unwrap().tags));
    assert_eq!(db.inventory_items().unwrap()[0].quantity, 3);
    assert_eq!(db.inventory_items().unwrap()[0].tags, vec!["工作"]);
    assert_eq!(db.credentials().unwrap().len(), 1);
    assert!(
        db.tags(Scope::Credentials)
            .unwrap()
            .contains(&"暂未使用".to_owned())
    );
    assert!(
        db.set_record_tags(Scope::Credentials, "missing", &["新标签".into()])
            .is_err()
    );
    assert!(
        !db.tags(Scope::Credentials)
            .unwrap()
            .contains(&"新标签".to_owned())
    );
}

#[test]
fn existing_tables_and_old_imports_gain_empty_tags_without_losing_values() {
    let directory = tempfile::tempdir().unwrap();
    let connection = rusqlite::Connection::open(directory.path().join("desk.db")).unwrap();
    connection.execute_batch("CREATE TABLE credentials(id TEXT PRIMARY KEY,title TEXT,username TEXT,password TEXT,website TEXT,notes TEXT,updated_at TEXT);
        INSERT INTO credentials VALUES('existing','old-title','old-user','synthetic','example.test','original-notes','2026-01-01');
        CREATE TABLE inventory_items(id TEXT PRIMARY KEY,name TEXT,category TEXT,quantity INTEGER,location TEXT,notes TEXT,created_at TEXT,updated_at TEXT);
        INSERT INTO inventory_items VALUES('item','keyboard','old-category',7,'desk','notes','2026-01-01','2026-01-01');").unwrap();
    drop(connection);
    for _ in 0..2 {
        let db = Database::open(directory.path()).unwrap();
        let account = db.credential("existing").unwrap().unwrap();
        assert_eq!(account.password, "synthetic");
        assert_eq!(account.notes, "original-notes");
        assert!(account.tags.is_empty());
        let item = &db.inventory_items().unwrap()[0];
        assert_eq!(item.quantity, 7);
        assert_eq!(item.category, "old-category");
        assert_eq!(item.tags, vec!["old-category"]);
    }
    let account: Credential = serde_json::from_value(serde_json::json!({"id":"imported","title":"old-json","username":"","password":"synthetic","website":"","notes":""})).unwrap();
    assert!(account.tags.is_empty());
}

#[test]
fn exports_include_selected_labels_and_unused_scoped_tag_catalogs() {
    use crate::{
        export::{self, Scope},
        tags::Scope as TagScope,
    };
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("app");
    let mut db = Database::open(&data).unwrap();
    let mut account = Credential::new();
    account.title = "tagged".into();
    account.tags = vec!["游戏".into()];
    db.save_credential(&account).unwrap();
    db.create_tag(TagScope::Credentials, "未分配标签").unwrap();
    db.create_tag(TagScope::Inventory, "设备").unwrap();
    for scope in [Scope::All, Scope::Credentials, Scope::Inventory] {
        let path = directory.path().join(format!("{}.zip", scope.label()));
        export::write(&data, &path, scope).unwrap();
        let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        let catalog = archive_json(&mut archive, "自定义标签.json");
        assert_eq!(
            catalog.as_array().unwrap().len(),
            if scope == Scope::All {
                3
            } else if scope == Scope::Credentials {
                2
            } else {
                1
            }
        );
        if scope != Scope::Inventory {
            assert_eq!(
                archive_json(&mut archive, "账号密码.json")[0]["tags"],
                serde_json::json!(["游戏"])
            );
        }
    }
}

#[test]
fn websites_normalize_persist_edit_and_delete_without_opening_the_browser() {
    use crate::{model::Website, websites};
    let directory = tempfile::tempdir().unwrap();
    let mut db = Database::open(directory.path()).unwrap();
    let mut item = Website::new();
    item.url = "example.test/path?q=中文".into();
    item.category = "学习".into();
    item = websites::prepare(item).unwrap();
    assert_eq!(item.title, "example.test");
    assert!(item.url.starts_with("https://example.test/"));
    db.save_website(&item).unwrap();
    drop(db);
    let mut db = Database::open(directory.path()).unwrap();
    assert_eq!(db.websites().unwrap(), vec![item.clone()]);
    item.title = "新标题".into();
    item.notes = "可搜索备注".into();
    db.save_website(&item).unwrap();
    assert_eq!(db.websites().unwrap(), vec![item.clone()]);
    db.delete_website(&item.id).unwrap();
    assert!(db.websites().unwrap().is_empty());
    for url in [
        "",
        "http://",
        "file:///C:/temp",
        "javascript://example.test",
    ] {
        item.url = url.into();
        assert!(websites::prepare(item.clone()).is_err());
    }
}

#[test]
fn bulk_import_preserves_extra_fields_skips_exact_duplicates_and_rolls_back_conflicts() {
    use crate::{import, model::Website};
    let directory = tempfile::tempdir().unwrap();
    let mut account = Credential::new();
    account.title = "测试账号".into();
    account.password = "synthetic-$*\\-secret".into();
    account.notes = "二级密码：synthetic\n第二个手机号：未明确对应".into();
    let mut website = Website::new();
    website.url = "example.test".into();
    let bundle = || import::Bundle {
        accounts: vec![account.clone()],
        websites: vec![website.clone()],
    };
    assert_eq!(import::records(directory.path(), bundle()).unwrap(), (1, 1));
    assert_eq!(import::records(directory.path(), bundle()).unwrap(), (0, 0));
    let mut db = Database::open(directory.path()).unwrap();
    let saved = db.credential(&account.id).unwrap().unwrap();
    assert_eq!(saved.password, account.password);
    assert_eq!(saved.notes, account.notes);
    let mut first = Credential::new();
    first.title = "first".into();
    let mut conflicting = first.clone();
    conflicting.title = "different".into();
    assert!(db.import_records(&[first, conflicting], &[]).is_err());
    assert_eq!(db.credentials().unwrap().len(), 1);
}

fn archive_json(archive: &mut zip::ZipArchive<fs::File>, name: &str) -> serde_json::Value {
    serde_json::from_reader(archive.by_name(name).unwrap()).unwrap()
}

#[test]
fn all_exports_include_records_documents_recovery_and_a_readable_database_snapshot() {
    use crate::{
        export::{self, Scope},
        model::Website,
    };
    use std::io::Read;
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("app");
    let external = directory.path().join("external");
    fs::create_dir(&external).unwrap();
    fs::write(
        external.join("shortcut-target-only.txt"),
        "must not traverse shortcut",
    )
    .unwrap();
    let external_document = directory.path().join("外部文档.md");
    fs::write(&external_document, "外部文档正文").unwrap();
    let mut db = Database::open(&data).unwrap();
    let mut account = Credential::new();
    account.title = "test".into();
    account.password = "synthetic-full-export".into();
    account.notes = "附加令牌：synthetic-token".into();
    db.save_credential(&account).unwrap();
    let mut website = Website::new();
    website.title = "测试网站".into();
    website.url = "https://example.test/".into();
    db.save_website(&website).unwrap();
    let mut item = InventoryItem::new();
    item.name = "测试物品".into();
    db.save_inventory_item(&item).unwrap();
    db.save_shortcut("仅保存地址", &external, true).unwrap();
    let document = data.join("documents/managed.md");
    fs::write(&document, "台内正文").unwrap();
    db.set_document_title(&document, "同名标题").unwrap();
    db.set_document_title(&external_document, "同名标题")
        .unwrap();
    fs::write(data.join("recovery/draft.recovery.txt"), "尚未保存的草稿").unwrap();
    let destination = data.join("documents/全部.zip");
    let report = export::write(&data, &destination, Scope::All).unwrap();
    assert_eq!(report.warnings, 0);
    let mut archive = zip::ZipArchive::new(fs::File::open(&destination).unwrap()).unwrap();
    assert_eq!(
        archive_json(&mut archive, "账号密码.json")[0]["password"],
        account.password
    );
    assert_eq!(
        archive_json(&mut archive, "账号密码.json")[0]["notes"],
        account.notes
    );
    assert_eq!(
        archive_json(&mut archive, "网站收藏.json")
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        archive_json(&mut archive, "物品清单.json")
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        archive_json(&mut archive, "快捷路径.json")
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let documents = archive_json(&mut archive, "文档清单.json");
    for record in documents.as_array().unwrap() {
        let Some(member) = record["archive_path"].as_str() else {
            assert_eq!(
                record["path"],
                external_document
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .as_ref()
            );
            continue;
        };
        let mut text = String::new();
        archive
            .by_name(member)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert!(["台内正文", "外部文档正文"].contains(&text.as_str()));
    }
    assert!(archive.by_name("data/recovery/draft.recovery.txt").is_ok());
    assert!(
        archive
            .file_names()
            .all(|name| !name.contains("shortcut-target-only")
                && !name.contains("全部.zip")
                && !name.contains(".tmp"))
    );
    let mut snapshot = vec![];
    archive
        .by_name("data/desk.db")
        .unwrap()
        .read_to_end(&mut snapshot)
        .unwrap();
    let path = directory.path().join("snapshot.db");
    fs::write(&path, snapshot).unwrap();
    let snapshot = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        snapshot
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        snapshot
            .query_row("SELECT password FROM credentials", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        account.password
    );
    drop(archive);
    // Re-exporting into the managed document folder must exclude both old output and temporary output.
    assert_eq!(
        export::write(&data, &destination, Scope::All)
            .unwrap()
            .warnings,
        0
    );
}

#[test]
fn module_exports_only_include_that_module_and_missing_documents_are_reported() {
    use crate::export::{self, Scope};
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("app");
    let mut db = Database::open(&data).unwrap();
    let mut account = Credential::new();
    account.title = "record".into();
    db.save_credential(&account).unwrap();
    let missing = data.join("documents/missing.md");
    db.set_document_title(&missing, "missing").unwrap();
    for (scope, filename) in [
        (Scope::Credentials, "账号密码.json"),
        (Scope::Folders, "快捷路径.json"),
        (Scope::Documents, "文档清单.json"),
        (Scope::Inventory, "物品清单.json"),
        (Scope::Websites, "网站收藏.json"),
    ] {
        let path = directory.path().join(format!("{}.zip", scope.label()));
        let report = export::write(&data, &path, scope).unwrap();
        assert_eq!(report.warnings, usize::from(scope == Scope::Documents));
        let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        assert!(archive.by_name(filename).is_ok());
        assert_eq!(
            archive.by_name("账号密码.json").is_ok(),
            scope == Scope::Credentials
        );
        assert!(archive.by_name("data/desk.db").is_err());
        let manifest = archive_json(&mut archive, "导出清单.json");
        assert_eq!(manifest["shortcut_targets_included"], false);
        if scope == Scope::Documents {
            assert!(archive_json(&mut archive, filename)[0]["archive_path"].is_null());
        }
    }
    assert!(export::write(&data, &data.join("desk.db"), Scope::All).is_err());
}

#[test]
fn unreadable_legacy_files_are_preserved_in_full_exports() {
    use crate::export::{self, Scope};
    use std::io::Read;
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("app");
    drop(Database::open(&data).unwrap());
    fs::write(data.join("vault.db"), b"unreadable-legacy-original").unwrap();
    let path = directory.path().join("export.zip");
    assert_eq!(export::write(&data, &path, Scope::All).unwrap().warnings, 1);
    let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let mut bytes = vec![];
    archive
        .by_name("legacy/vault.db")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"unreadable-legacy-original");
}

#[test]
fn failed_export_does_not_replace_previous_output_or_leave_partial_archives() {
    use crate::export::{self, Scope};
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("app");
    drop(Database::open(&data).unwrap());
    let connection = rusqlite::Connection::open(data.join("desk.db")).unwrap();
    connection
        .execute("INSERT INTO settings VALUES('invalid-text',X'00')", [])
        .unwrap();
    let path = directory.path().join("export.zip");
    fs::write(&path, b"previous-output").unwrap();
    assert!(export::write(&data, &path, Scope::All).is_err());
    assert_eq!(fs::read(path).unwrap(), b"previous-output");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[test]
fn accounts_are_plain_local_records_and_support_create_edit_delete_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let mut entry = Credential::new();
    entry.title = "测试账号".into();
    entry.username = "local-user".into();
    entry.password = "synthetic-readable-password".into();
    {
        let mut db = Database::open(directory.path()).unwrap();
        db.save_credential(&entry).unwrap();
    }
    assert!(
        !directory.path().join("vault.db").exists(),
        "new installs need no vault or key"
    );
    let connection = rusqlite::Connection::open(directory.path().join("desk.db")).unwrap();
    let stored: String = connection
        .query_row(
            "SELECT password FROM credentials WHERE id=?1",
            [&entry.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, entry.password);
    drop(connection);
    let mut db = Database::open(directory.path()).unwrap();
    assert_eq!(
        db.credential(&entry.id).unwrap().unwrap().password,
        entry.password
    );
    entry.password = "changed-synthetic-password".into();
    db.save_credential(&entry).unwrap();
    assert_eq!(
        db.credential(&entry.id).unwrap().unwrap().password,
        entry.password
    );
    db.delete_credential(&entry.id).unwrap();
    drop(db);
    assert!(
        Database::open(directory.path())
            .unwrap()
            .credential(&entry.id)
            .unwrap()
            .is_none()
    );
}

#[cfg(windows)]
fn seed_legacy_accounts(directory: &std::path::Path, entries: &[Credential]) {
    use crate::crypto::{self, VaultKey};
    let connection = rusqlite::Connection::open(directory.join("vault.db")).unwrap();
    connection.execute_batch("CREATE TABLE local_vault_key(id INTEGER PRIMARY KEY,key_id TEXT,protected_dek BLOB);
        CREATE TABLE vault_entries(id TEXT PRIMARY KEY,key_id TEXT,nonce BLOB,ciphertext BLOB,updated_at TEXT DEFAULT CURRENT_TIMESTAMP);").unwrap();
    let key = VaultKey(zeroize::Zeroizing::new(
        crypto::random_bytes::<32>().unwrap(),
    ));
    let protected = crypto::protect_local_key(&key.0).unwrap();
    connection
        .execute(
            "INSERT INTO local_vault_key VALUES(1,'old-key',?1)",
            [protected],
        )
        .unwrap();
    for entry in entries {
        let nonce = crypto::random_bytes::<12>().unwrap();
        let ciphertext = crypto::encrypt(
            &key,
            "old-key",
            &entry.id,
            &serde_json::to_vec(entry).unwrap(),
            &nonce,
        )
        .unwrap();
        connection
            .execute(
                "INSERT INTO vault_entries(id,key_id,nonce,ciphertext) VALUES(?1,'old-key',?2,?3)",
                rusqlite::params![entry.id, nonce.as_slice(), ciphertext],
            )
            .unwrap();
    }
}

#[cfg(windows)]
#[test]
fn old_accounts_migrate_without_overwriting_edits_or_restoring_deleted_accounts() {
    let directory = tempfile::tempdir().unwrap();
    let mut entry = Credential::new();
    entry.title = "旧版测试账号".into();
    entry.username = "user".into();
    entry.password = "synthetic-old-password".into();
    entry.website = "example.test".into();
    entry.notes = "备注".into();
    seed_legacy_accounts(directory.path(), &[entry.clone()]);
    let original = fs::read(directory.path().join("vault.db")).unwrap();
    {
        let mut db = Database::open(directory.path()).unwrap();
        let imported = db.credential(&entry.id).unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(&imported).unwrap(),
            serde_json::to_value(&entry).unwrap()
        );
        entry.password = "edited-after-import".into();
        db.save_credential(&entry).unwrap();
    }
    {
        let mut db = Database::open(directory.path()).unwrap();
        assert_eq!(
            db.credential(&entry.id).unwrap().unwrap().password,
            entry.password
        );
        db.delete_credential(&entry.id).unwrap();
    }
    let db = Database::open(directory.path()).unwrap();
    assert!(db.credential(&entry.id).unwrap().is_none());
    assert_eq!(
        fs::read(directory.path().join("vault.db")).unwrap(),
        original
    );
}

#[test]
fn unreadable_old_accounts_do_not_block_any_new_local_accounts() {
    for broken_database in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        if broken_database {
            fs::write(directory.path().join("vault.db"), b"unreadable old file").unwrap();
        } else {
            let connection = rusqlite::Connection::open(directory.path().join("vault.db")).unwrap();
            connection.execute_batch("CREATE TABLE local_vault_key(id INTEGER,key_id TEXT,protected_dek BLOB);
                INSERT INTO local_vault_key VALUES(1,'old-key',X'00');
                CREATE TABLE vault_entries(id TEXT,key_id TEXT,nonce BLOB,ciphertext BLOB,updated_at TEXT);
                INSERT INTO vault_entries VALUES('old-id','old-key',X'00',X'00','2026-01-01');").unwrap();
        }
        let original = fs::read(directory.path().join("vault.db")).unwrap();
        let mut db = Database::open(directory.path()).unwrap();
        assert!(!db.migration_notice.is_empty());
        let mut entry = Credential::new();
        entry.title = "新账号".into();
        entry.password = "synthetic".into();
        db.save_credential(&entry).unwrap();
        assert!(db.credential(&entry.id).unwrap().is_some());
        assert_eq!(
            fs::read(directory.path().join("vault.db")).unwrap(),
            original
        );
    }
}

#[cfg(windows)]
#[test]
fn a_failed_legacy_import_rolls_back_records_and_markers_and_can_retry() {
    let directory = tempfile::tempdir().unwrap();
    drop(Database::open(directory.path()).unwrap());
    let connection = rusqlite::Connection::open(directory.path().join("desk.db")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_import BEFORE INSERT ON credentials WHEN NEW.title='fail' BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    let mut first = Credential::new();
    first.title = "first".into();
    let mut second = Credential::new();
    second.title = "fail".into();
    seed_legacy_accounts(directory.path(), &[first, second]);
    let db = Database::open(directory.path()).unwrap();
    assert!(!db.migration_notice.is_empty());
    assert!(db.credential_summaries().unwrap().is_empty());
    let markers: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM legacy_credential_imports",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(markers, 0);
    drop(db);
    connection
        .execute_batch("DROP TRIGGER fail_import")
        .unwrap();
    let db = Database::open(directory.path()).unwrap();
    assert_eq!(db.credential_summaries().unwrap().len(), 2);
}

#[test]
fn indexed_search_updates_old_terms_and_handles_short_literal_queries() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let mut db = Database::open(directory.path()).expect("open database");
    let path = directory.path().join("项目说明.md");
    db.index_file("root", &path, "项目说明", "旧关键词 100% 完成情况")
        .expect("index file");
    assert_eq!(
        db.search_files("旧关键词").expect("search old term").len(),
        1
    );
    assert_eq!(
        db.search_files("%")
            .expect("search percent literally")
            .len(),
        1
    );

    db.index_file("root", &path, "项目说明", "新关键词 当前内容")
        .expect("update file index");
    assert!(
        db.search_files("旧关键词")
            .expect("search removed term")
            .is_empty()
    );
    assert_eq!(
        db.search_files("新关键词").expect("search new term").len(),
        1
    );
}

#[test]
fn inventory_items_can_be_created_updated_listed_and_deleted() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let mut db = Database::open(directory.path()).expect("open database");
    let mut item = InventoryItem::new();
    item.name = "降噪耳机".into();
    item.category = "数码设备".into();
    item.tags = vec!["数码设备".into()];
    item.quantity = 1;
    item.location = "书桌抽屉".into();
    item.notes = "公司备用".into();

    db.save_inventory_item(&item)
        .expect("create inventory item");
    drop(db);
    let mut db = Database::open(directory.path()).expect("reopen inventory database");
    let listed = db.inventory_items().expect("list inventory items");
    assert_eq!(listed, vec![item.clone()]);

    item.quantity = 2;
    item.notes = "家里和办公室各一副".into();
    db.save_inventory_item(&item)
        .expect("update inventory item");
    let listed = db.inventory_items().expect("list updated inventory");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].quantity, 2);
    assert_eq!(listed[0].notes, "家里和办公室各一副");

    db.delete_inventory_item(&item.id)
        .expect("delete inventory item");
    assert!(db.inventory_items().expect("list after delete").is_empty());
}

#[test]
fn document_gallery_keeps_all_records_and_removal_clears_the_search_index() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let mut db = Database::open(directory.path()).expect("open database");
    for index in 0..12 {
        let path = directory.path().join(format!("文档-{index}.md"));
        fs::write(&path, "保留原始文件").expect("write document");
        db.touch_document(&path).expect("add document");
    }
    assert_eq!(db.documents().expect("list all documents").len(), 12);
    let path = directory.path().join("文档-0.md");
    db.index_file("root", &path, "测试文档", "独有检索关键词")
        .expect("index document");
    assert_eq!(db.search_files("独有检索关键词").expect("search").len(), 1);
    db.forget_document(&path).expect("remove document record");
    assert_eq!(db.documents().expect("remaining documents").len(), 11);
    assert!(
        db.search_files("独有检索关键词")
            .expect("search removed document")
            .is_empty()
    );
    assert!(
        path.exists(),
        "database removal must not delete the physical file"
    );
}

#[test]
fn deleting_a_shortcut_clears_its_index_but_keeps_the_original_folder() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let root = directory.path().join("真实工作目录");
    fs::create_dir(&root).expect("create folder");
    let document = root.join("说明.md");
    fs::write(&document, "保留的原始正文").expect("write source");
    let mut db = Database::open(directory.path()).expect("open database");
    db.save_shortcut("工作目录", &root, true)
        .expect("save shortcut");
    let id = db.shortcuts().expect("list shortcuts")[0].id.clone();
    db.index_file(&id, &document, "说明文档", "保留的原始正文")
        .expect("index source");
    db.remove_shortcut(&id).expect("delete shortcut");
    assert!(db.shortcuts().expect("remaining shortcuts").is_empty());
    assert!(
        db.search_files("保留的原始正文")
            .expect("search removed root")
            .is_empty()
    );
    assert_eq!(
        fs::read_to_string(document).expect("source remains"),
        "保留的原始正文"
    );
}

#[test]
fn search_hides_files_that_were_removed_after_indexing() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let mut db = Database::open(directory.path()).expect("open database");
    let path = directory.path().join("测试索引.md");
    fs::write(&path, "原始内容").expect("create indexed file");
    db.index_file("root", &path, "测试索引", "原始内容")
        .expect("index file");
    assert_eq!(
        crate::search::search(&db, "测试索引")
            .expect("search existing file")
            .len(),
        1
    );
    let result = crate::search::search(&db, "测试索引").unwrap().remove(0);
    assert_eq!(result.category, "已索引文件");
    assert_eq!(result.title, "测试索引");
    assert_eq!(result.path, Some(result.detail));
    fs::remove_file(&path).expect("simulate deletion outside app");
    assert!(
        crate::search::search(&db, "测试索引")
            .expect("search stale index")
            .is_empty()
    );
}

#[test]
fn old_document_records_gain_titles_without_losing_existing_content() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let path = directory.path().join("原有笔记.md");
    fs::write(&path, "原有正文").expect("create old document");
    {
        let connection =
            rusqlite::Connection::open(directory.path().join("desk.db")).expect("old database");
        connection.execute_batch("CREATE TABLE documents(path TEXT PRIMARY KEY,format TEXT NOT NULL,last_opened TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)").expect("old schema");
        connection
            .execute(
                "INSERT INTO documents(path,format) VALUES(?1,'md')",
                [path.to_string_lossy().as_ref()],
            )
            .expect("old record");
    }
    let mut db = Database::open(directory.path()).expect("upgrade database");
    assert_eq!(db.document_title(&path).expect("legacy title"), "原有笔记");
    db.set_document_title(&path, "新标题 / 不改变文件名")
        .expect("set title");
    assert_eq!(
        db.document_title(&path).expect("new title"),
        "新标题 / 不改变文件名"
    );
    assert_eq!(
        fs::read_to_string(path).expect("unchanged file"),
        "原有正文"
    );
}

#[test]
fn tool_edits_persist_and_removal_preserves_the_external_program() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let path = directory.path().join("工具 & 测试程序.exe");
    fs::write(&path, b"MZ synthetic executable").unwrap();
    let mut db = Database::open(&data).unwrap();
    let mut tool = crate::launcher::AppTool::new(path.clone());
    tool.name = "开发工具".into();
    db.save_app_tool(&tool).unwrap();
    tool.name = "更新名称".into();
    tool.notes = "测试备注".into();
    db.save_app_tool(&tool).unwrap();
    drop(db);
    let mut db = Database::open(&data).unwrap();
    let saved = db.app_tools().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].name, "更新名称");
    assert_eq!(saved[0].notes, "测试备注");
    assert_eq!(saved[0].path, path.canonicalize().unwrap());
    db.delete_app_tool(&tool.id).unwrap();
    assert!(db.app_tools().unwrap().is_empty());
    assert_eq!(fs::read(path).unwrap(), b"MZ synthetic executable");
}

#[test]
fn website_legacy_categories_and_labels_share_persistent_selection_and_exports() {
    use crate::{model::Website, tags::Scope as TagScope};
    let directory = tempfile::tempdir().unwrap();
    let mut db = Database::open(directory.path()).unwrap();
    let mut site = Website::new();
    site.title = "legacy-site".into();
    site.url = "https://example.test".into();
    db.save_website(&site).unwrap();
    drop(db);
    let connection = rusqlite::Connection::open(directory.path().join("desk.db")).unwrap();
    connection
        .execute(
            "UPDATE websites SET category='视频',tags_json='[]' WHERE id=?1",
            [&site.id],
        )
        .unwrap();
    drop(connection);
    db = Database::open(directory.path()).unwrap();
    assert_eq!(
        db.record_tags(TagScope::Websites, &site.id).unwrap(),
        ["视频"]
    );
    assert!(
        db.tags(TagScope::Websites)
            .unwrap()
            .contains(&"视频".to_string())
    );
    db.save_website_classified(&site, &["学习".into()]).unwrap();
    assert_eq!(db.websites().unwrap()[0].category, "学习");
    db.rename_tag(TagScope::Websites, "学习", "资料").unwrap();
    assert_eq!(db.websites().unwrap()[0].category, "资料");
    db.delete_tag(TagScope::Websites, "资料").unwrap();
    drop(db);
    db = Database::open(directory.path()).unwrap();
    assert!(
        db.record_tags(TagScope::Websites, &site.id)
            .unwrap()
            .is_empty()
    );
    assert!(db.websites().unwrap()[0].category.is_empty());
}

#[test]
fn upgrade_adds_contact_photos_dates_prices_and_library_without_changing_old_records() {
    use crate::{
        contacts::Contact,
        library::{Book, Page},
        model::InventoryItem,
    };
    let root = tempfile::tempdir().unwrap();
    let mut db = Database::open(root.path()).unwrap();
    let mut contact = Contact::new();
    contact.name = "合成联系人".into();
    contact.school = "合成学校".into();
    contact.chat_notes = "原有备注".into();
    db.save_contact(&contact).unwrap();
    let mut item = InventoryItem::new();
    item.name = "合成物品".into();
    db.save_inventory_item(&item).unwrap();
    drop(db);
    let old = rusqlite::Connection::open(root.path().join("desk.db")).unwrap();
    for (table, column) in [
        ("contacts", "birthday"),
        ("contacts", "major"),
        ("contacts", "photo"),
        ("inventory_items", "purchase_date"),
        ("inventory_items", "purchase_price_cents"),
    ] {
        old.execute_batch(&format!("ALTER TABLE {table} DROP COLUMN {column}"))
            .unwrap();
    }
    drop(old);
    let mut db = Database::open(root.path()).unwrap();
    assert_eq!(db.contacts().unwrap()[0], contact);
    let backups = std::fs::read_dir(root.path().join("backups"))
        .unwrap()
        .collect::<std::io::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(backups.len(), 1);
    let original = rusqlite::Connection::open(backups[0].path().join("desk.db")).unwrap();
    assert_eq!(
        original
            .query_row(
                "SELECT name FROM contacts WHERE id=?1",
                [&contact.id],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        contact.name
    );
    assert_eq!(
        original
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(original);
    let photo = root.path().join("photo.png");
    image::DynamicImage::new_rgb8(4, 4).save(&photo).unwrap();
    contact.photo = crate::contacts::import_photo(&photo).unwrap();
    contact.birthday = "2000-02-29".into();
    contact.major = "计算机科学".into();
    db.save_contact(&contact).unwrap();
    std::fs::remove_file(photo).unwrap();
    item.purchase_date = "2026-10-20".into();
    item.purchase_price_cents = Some(123456);
    db.save_inventory_item(&item).unwrap();
    let mut book = Book::new();
    book.title = "合成图书".into();
    db.save_book(&book).unwrap();
    let mut page = Page::new(book.id.clone());
    page.body = "书中正文".into();
    db.save_book_page(&mut page).unwrap();
    drop(db);
    let db = Database::open(root.path()).unwrap();
    assert_eq!(db.contacts().unwrap()[0], contact);
    assert_eq!(
        db.inventory_items().unwrap()[0].purchase_price_cents,
        Some(123456)
    );
    assert_eq!(db.book_pages(&book.id).unwrap()[0].body, "书中正文");
    drop(db);
    let target = root.path().join("all.zip");
    crate::export::write(root.path(), &target, crate::export::Scope::All).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(target).unwrap()).unwrap();
    let records: serde_json::Value =
        serde_json::from_reader(zip.by_name("联系人.json").unwrap()).unwrap();
    let photo_name = records[0]["photo_archive_path"].as_str().unwrap();
    let mut original_photo = vec![];
    use std::io::Read;
    zip.by_name(photo_name)
        .unwrap()
        .read_to_end(&mut original_photo)
        .unwrap();
    assert_eq!(original_photo, contact.photo);
    assert_eq!(records[0]["birthday"], "2000-02-29");
    assert_eq!(records[0]["major"], "计算机科学");
    assert!(zip.by_name("图书页面.json").is_ok());
}

#[test]
fn library_versions_conflicts_drafts_and_deleted_pages_survive_restart() {
    use crate::library::{Book, Page};
    let root = tempfile::tempdir().unwrap();
    let mut db = Database::open(root.path()).unwrap();
    let book = Book::new();
    db.save_book(&book).unwrap();
    let mut page = Page::new(book.id.clone());
    page.body = "first".into();
    page.tags = vec!["学习".into()];
    db.save_book_page(&mut page).unwrap();
    let mut stale = page.clone();
    page.body = "newer body".into();
    db.save_book_page(&mut page).unwrap();
    stale.body = "unsaved alternative".into();
    assert!(db.save_book_page(&mut stale).is_err());
    db.save_book_draft("old-instance", &stale).unwrap();
    db.set_library_deleted(&page.id, true, true).unwrap();
    drop(db);
    let mut db = Database::open(root.path()).unwrap();
    assert!(db.book_pages(&book.id).unwrap().is_empty());
    assert_eq!(
        db.book_drafts(&book.id, "new-instance").unwrap()[0].1.body,
        "unsaved alternative"
    );
    assert_eq!(db.deleted_library().unwrap().len(), 1);
    db.set_library_deleted(&page.id, true, false).unwrap();
    assert_eq!(db.book_pages(&book.id).unwrap()[0].body, "newer body");
    let sql = rusqlite::Connection::open(root.path().join("desk.db")).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT body FROM book_page_versions WHERE page_id=?1 AND revision=1",
            [&page.id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "first"
    );
}
