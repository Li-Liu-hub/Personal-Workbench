use std::path::{Path, PathBuf};

mod contacts;
mod curriculum;
mod library;
mod reference;
#[cfg(test)]
mod reference_tests;
mod study;
mod tag_cache;
mod writing;
pub(crate) use writing::WRITING_ROOT;

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::{
    error::{DeskError, Result},
    legacy_credentials,
    model::{Credential, CredentialSummary, DocumentRecord, InventoryItem, Shortcut, Website},
    tags::{self, Scope as TagScope, Tag},
};

pub struct Database {
    desk: Connection,
    tag_cache: std::cell::RefCell<tag_cache::Cache>,
    pub migration_notice: String,
}

impl Database {
    pub fn remove_index(&mut self, scope: &str, key: &str) -> Result<()> {
        let (table, column) = match scope {
            "documents" => ("documents", "path"),
            "shortcuts" => ("shortcuts", "id"),
            _ => return Err(DeskError::UnsupportedFormat),
        };
        let key = if scope == "documents" {
            let normalized = Path::new(key)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(key));
            self.desk
                .query_row(
                    "SELECT path FROM documents WHERE path=?1 OR path=?2 ORDER BY path=?1 DESC LIMIT 1",
                    params![key, normalized.to_string_lossy().as_ref()],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .unwrap_or_else(|| key.to_owned())
        } else {
            key.to_owned()
        };
        let tx = self.desk.transaction()?;
        if tx.execute(
            &format!("UPDATE {table} SET hidden=1 WHERE {column}=?1 AND hidden=0"),
            [&key],
        )? > 0
        {
            tx.execute(
                "INSERT INTO index_undo(scope,record_key) VALUES(?1,?2)",
                params![scope, key],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn index_undo_available(&self) -> Result<bool> {
        Ok(self.desk.query_row("SELECT EXISTS(SELECT 1 FROM index_undo u WHERE (scope='documents' AND EXISTS(SELECT 1 FROM documents WHERE path=u.record_key AND hidden=1)) OR (scope='shortcuts' AND EXISTS(SELECT 1 FROM shortcuts WHERE id=u.record_key AND hidden=1)))",[],|r|r.get(0))?)
    }
    pub fn undo_index_removal(&mut self) -> Result<bool> {
        let tx = self.desk.transaction()?;
        let mut restored = false;
        loop {
            let item = tx
                .query_row(
                    "SELECT id,scope,record_key FROM index_undo ORDER BY id DESC LIMIT 1",
                    [],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?;
            let Some((id, scope, key)) = item else {
                break;
            };
            let (table, column) = match scope.as_str() {
                "documents" => ("documents", "path"),
                "shortcuts" => ("shortcuts", "id"),
                _ => return Err(DeskError::UnsupportedFormat),
            };
            restored = tx.execute(
                &format!("UPDATE {table} SET hidden=0 WHERE {column}=?1 AND hidden=1"),
                [key],
            )? > 0;
            tx.execute("DELETE FROM index_undo WHERE id=?1", [id])?;
            if restored {
                break;
            }
        }
        tx.commit()?;
        Ok(restored)
    }
    pub fn favorites(&self) -> Result<Vec<(String, String)>> {
        Ok(self
            .desk
            .prepare("SELECT scope,id FROM favorites ORDER BY scope,id")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn set_favorite(&mut self, scope: &str, id: &str, value: bool) -> Result<()> {
        if value {
            self.desk.execute(
                "INSERT OR IGNORE INTO favorites(scope,id) VALUES(?1,?2)",
                params![scope, id],
            )?;
        } else {
            self.desk.execute(
                "DELETE FROM favorites WHERE scope=?1 AND id=?2",
                params![scope, id],
            )?;
        }
        Ok(())
    }
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir.join("documents"))?;
        std::fs::create_dir_all(data_dir.join("recovery"))?;
        let mut desk = Connection::open(data_dir.join("desk.db"))?;
        desk.pragma_update(None, "journal_mode", "WAL")?;
        desk.pragma_update(None, "foreign_keys", "ON")?;
        if desk.table_exists(None, "contacts")? {
            let has_birthday = desk
                .prepare("PRAGMA table_info(contacts)")?
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .iter()
                .any(|name| name == "birthday");
            if !has_birthday {
                let integrity: String =
                    desk.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
                if integrity != "ok" {
                    return Err(DeskError::InvalidPath(
                        "数据库检查未通过，已停止升级并保留原库".into(),
                    ));
                }
                let backup = data_dir
                    .join("backups")
                    .join(format!("schema-0.3.0-{}", Uuid::new_v4()));
                std::fs::create_dir_all(&backup)?;
                Self::snapshot_connection(&desk, &backup.join("desk.db"))?;
            }
        }
        desk.busy_timeout(std::time::Duration::from_secs(3))?;
        desk.execute_batch(
            "CREATE TABLE IF NOT EXISTS shortcuts(
               id TEXT PRIMARY KEY, name TEXT NOT NULL, path_native BLOB NOT NULL UNIQUE, path_display TEXT NOT NULL,
               is_dir INTEGER NOT NULL, pinned INTEGER NOT NULL DEFAULT 0,
               created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS file_index(
               id TEXT PRIMARY KEY, root_id TEXT NOT NULL, path_native BLOB NOT NULL UNIQUE, path_display TEXT NOT NULL,
               title TEXT NOT NULL, body TEXT NOT NULL DEFAULT '',
               modified_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS file_index_fts USING fts5(
               title, path_display, body, content='file_index', content_rowid='rowid', tokenize='trigram'
             );
             CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS app_tools(id TEXT PRIMARY KEY,name TEXT NOT NULL,path_native BLOB NOT NULL,notes TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS credentials(
               id TEXT PRIMARY KEY, title TEXT NOT NULL, username TEXT NOT NULL,
               password TEXT NOT NULL, website TEXT NOT NULL, notes TEXT NOT NULL,
               updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS legacy_credential_imports(id TEXT PRIMARY KEY);
             CREATE TABLE IF NOT EXISTS favorites(scope TEXT NOT NULL,id TEXT NOT NULL,PRIMARY KEY(scope,id));
             CREATE TABLE IF NOT EXISTS tags(scope TEXT NOT NULL, name TEXT NOT NULL, PRIMARY KEY(scope,name));
             CREATE TABLE IF NOT EXISTS contacts(
               id TEXT PRIMARY KEY, name TEXT NOT NULL, school TEXT NOT NULL, work TEXT NOT NULL,
               phone TEXT NOT NULL, national_id TEXT NOT NULL, address TEXT NOT NULL,
               gender TEXT NOT NULL, hobbies TEXT NOT NULL, chat_notes TEXT NOT NULL,
               updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS contact_chats(
               id TEXT PRIMARY KEY, contact_id TEXT NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
               position INTEGER NOT NULL, preview_json TEXT NOT NULL, original BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS contact_chats_contact ON contact_chats(contact_id,position);
             CREATE TABLE IF NOT EXISTS websites(
               id TEXT PRIMARY KEY, title TEXT NOT NULL, url TEXT NOT NULL,
               category TEXT NOT NULL DEFAULT '', notes TEXT NOT NULL DEFAULT '',
               updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS documents(path TEXT PRIMARY KEY, format TEXT NOT NULL, title TEXT NOT NULL DEFAULT '',
               last_opened TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
             CREATE TABLE IF NOT EXISTS inventory_items(
               id TEXT PRIMARY KEY, name TEXT NOT NULL, category TEXT NOT NULL DEFAULT '',
               quantity INTEGER NOT NULL DEFAULT 1, location TEXT NOT NULL DEFAULT '', notes TEXT NOT NULL DEFAULT '',
               created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
               updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );"
        )?;
        // Add the title field without changing existing paths or document contents.
        let has_title = {
            let mut statement = desk.prepare("PRAGMA table_info(documents)")?;
            statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .iter()
                .any(|name| name == "title")
        };
        if !has_title {
            desk.execute(
                "ALTER TABLE documents ADD COLUMN title TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        desk.execute_batch("CREATE TABLE IF NOT EXISTS books(id TEXT PRIMARY KEY,title TEXT NOT NULL,deleted INTEGER NOT NULL DEFAULT 0,updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
            CREATE TABLE IF NOT EXISTS book_pages(id TEXT PRIMARY KEY,book_id TEXT NOT NULL REFERENCES books(id),title TEXT NOT NULL,body TEXT NOT NULL DEFAULT '',revision INTEGER NOT NULL DEFAULT 1,deleted INTEGER NOT NULL DEFAULT 0,updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
            CREATE TABLE IF NOT EXISTS book_documents(document_path TEXT PRIMARY KEY,book_id TEXT NOT NULL REFERENCES books(id),position INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS book_page_drafts(id TEXT PRIMARY KEY,book_id TEXT NOT NULL,writer TEXT NOT NULL,page_json TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS book_page_versions(page_id TEXT NOT NULL,revision INTEGER NOT NULL,title TEXT NOT NULL,body TEXT NOT NULL,tags_json TEXT NOT NULL,PRIMARY KEY(page_id,revision));")?;
        for (table, column, kind) in [
            ("documents", "hidden", "INTEGER NOT NULL DEFAULT 0"),
            ("shortcuts", "hidden", "INTEGER NOT NULL DEFAULT 0"),
            ("inventory_items", "brand", "TEXT NOT NULL DEFAULT ''"),
            ("inventory_items", "model", "TEXT NOT NULL DEFAULT ''"),
            ("contacts", "birthday", "TEXT NOT NULL DEFAULT ''"),
            ("contacts", "major", "TEXT NOT NULL DEFAULT ''"),
            ("contacts", "photo", "BLOB NOT NULL DEFAULT X''"),
            (
                "inventory_items",
                "purchase_date",
                "TEXT NOT NULL DEFAULT ''",
            ),
            ("inventory_items", "purchase_price_cents", "INTEGER"),
            (
                "inventory_items",
                "disposal_date",
                "TEXT NOT NULL DEFAULT ''",
            ),
        ] {
            let exists = desk
                .prepare(&format!("PRAGMA table_info({table})"))?
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .iter()
                .any(|name| name == column);
            if !exists {
                desk.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {kind}"))?;
            }
        }
        desk.execute_batch("CREATE TABLE IF NOT EXISTS index_undo(id INTEGER PRIMARY KEY AUTOINCREMENT,scope TEXT NOT NULL,record_key TEXT NOT NULL)")?;
        // Additive migration leaves existing accounts and quantities untouched.
        for scope in TagScope::ALL {
            let table = scope.table();
            let has_tags = desk
                .prepare(&format!("PRAGMA table_info({table})"))?
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .iter()
                .any(|column| column == "tags_json");
            if !has_tags {
                desk.execute(
                    &format!("ALTER TABLE {table} ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]'"),
                    [],
                )?;
            }
        }
        migrate_inventory_categories(&mut desk)?;
        migrate_website_categories(&mut desk)?;
        let migration_notice = match legacy_credentials::import(&mut desk, data_dir) {
            Ok(report) => report.notice(),
            Err(_) => "旧版账号暂时无法读取，原文件已保留；可以继续添加和使用新账号。".into(),
        };
        let mut database = Self {
            desk,
            tag_cache: Default::default(),
            migration_notice,
        };
        database.prepare_writing(data_dir)?;
        database.prepare_study()?;
        database.prepare_reference()?;
        Ok(database)
    }

    pub fn save_shortcut(&mut self, name: &str, path: &Path, is_dir: bool) -> Result<()> {
        let display_path = path.to_string_lossy().into_owned();
        self.desk.execute("INSERT INTO shortcuts(id,name,path_native,path_display,is_dir,pinned) VALUES(?1,?2,?3,?4,?5,0)",
            params![Uuid::new_v4().to_string(), name.trim(), encode_path(path), display_path, is_dir])?;
        Ok(())
    }

    pub fn shortcuts(&self) -> Result<Vec<Shortcut>> {
        let mut stmt = self.desk.prepare("SELECT id,name,path_native,is_dir,pinned FROM shortcuts WHERE hidden=0 ORDER BY pinned DESC,name COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, bool>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (id, name, path, is_dir, pinned) = row?;
            Ok(Shortcut {
                id,
                name,
                path: decode_path(&path)?,
                is_dir,
                pinned,
            })
        })
        .collect::<std::result::Result<Vec<_>, DeskError>>()
    }

    #[cfg(test)]
    pub fn remove_shortcut(&mut self, id: &str) -> Result<()> {
        let tx = self.desk.transaction()?;
        tx.execute(
            "INSERT INTO file_index_fts(file_index_fts,rowid,title,path_display,body)
            SELECT 'delete',rowid,title,path_display,body FROM file_index WHERE root_id=?1",
            [id],
        )?;
        tx.execute("DELETE FROM file_index WHERE root_id=?1", [id])?;
        tx.execute("DELETE FROM shortcuts WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn save_credential(&mut self, entry: &Credential) -> Result<()> {
        let values = tags::normalize(&entry.tags)?;
        let tx = self.desk.transaction()?;
        ensure_tags(&tx, TagScope::Credentials, &values)?;
        tx.execute(
            "INSERT INTO credentials(id,title,username,password,website,notes,tags_json) VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(id) DO UPDATE SET title=excluded.title,username=excluded.username,
             password=excluded.password,website=excluded.website,notes=excluded.notes,tags_json=excluded.tags_json,updated_at=CURRENT_TIMESTAMP",
            params![entry.id, entry.title, entry.username, entry.password, entry.website, entry.notes, serde_json::to_string(&values)?],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn credential_summaries(&self) -> Result<Vec<CredentialSummary>> {
        let mut statement = self.desk.prepare(
            "SELECT id,title,username,website,tags_json,password,notes FROM credentials ORDER BY updated_at DESC,rowid DESC",
        )?;
        Ok(statement
            .query_map([], |row| {
                Ok(CredentialSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    username: row.get(2)?,
                    password: row.get(5)?,
                    website: row.get(3)?,
                    notes: row.get(6)?,
                    tags: read_tags(row, 4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn credential(&self, id: &str) -> Result<Option<Credential>> {
        Ok(self
            .desk
            .query_row(
                "SELECT id,title,username,password,website,notes,tags_json FROM credentials WHERE id=?1",
                [id],
                |row| {
                    Ok(Credential {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        username: row.get(2)?,
                        password: row.get(3)?,
                        website: row.get(4)?,
                        notes: row.get(5)?,
                        tags: read_tags(row, 6)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn delete_credential(&mut self, id: &str) -> Result<()> {
        // Retain the migration marker so a deleted old account is never imported again.
        self.desk
            .execute("DELETE FROM credentials WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn tags(&self, scope: TagScope) -> Result<Vec<String>> {
        Ok(self
            .desk
            .prepare("SELECT name FROM tags WHERE scope=?1 ORDER BY name COLLATE NOCASE")?
            .query_map([scope.key()], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn tag_catalog(&self) -> Result<Vec<Tag>> {
        let mut result = Vec::new();
        for scope in TagScope::ALL {
            result.extend(
                self.tags(scope)?
                    .into_iter()
                    .map(|name| Tag { scope, name }),
            );
        }
        Ok(result)
    }

    pub fn create_tag(&mut self, scope: TagScope, name: &str) -> Result<()> {
        let name = tags::name(name)?;
        if self.tags(scope)?.contains(&name) {
            return Err(DeskError::AlreadyExists);
        }
        self.desk.execute(
            "INSERT INTO tags(scope,name) VALUES(?1,?2)",
            params![scope.key(), name],
        )?;
        Ok(())
    }

    pub fn rename_tag(&mut self, scope: TagScope, original: &str, name: &str) -> Result<()> {
        let name = tags::name(name)?;
        if original == name {
            return Ok(());
        }
        let tx = self.desk.transaction()?;
        let changed = tx.execute(
            "UPDATE tags SET name=?1 WHERE scope=?2 AND name=?3",
            params![name, scope.key(), original],
        )?;
        if changed == 0 {
            return Err(DeskError::InvalidPath("标签已不存在，请重新选择".into()));
        }
        tx.execute(&format!("UPDATE {} SET tags_json=(SELECT json_group_array(CASE WHEN value=?1 THEN ?2 ELSE value END) FROM json_each(tags_json))
            WHERE EXISTS(SELECT 1 FROM json_each(tags_json) WHERE value=?1)",scope.table()), params![original,name])?;
        sync_inventory_category(&tx, scope)?;
        if scope == TagScope::BookPages {
            rewrite_page_draft_tags(&tx, original, Some(&name))?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_tag(&mut self, scope: TagScope, name: &str) -> Result<()> {
        let tx = self.desk.transaction()?;
        tx.execute(
            "DELETE FROM tags WHERE scope=?1 AND name=?2",
            params![scope.key(), name],
        )?;
        tx.execute(&format!("UPDATE {} SET tags_json=(SELECT json_group_array(value) FROM json_each(tags_json) WHERE value<>?1)
            WHERE EXISTS(SELECT 1 FROM json_each(tags_json) WHERE value=?1)",scope.table()), [name])?;
        sync_inventory_category(&tx, scope)?;
        if scope == TagScope::BookPages {
            rewrite_page_draft_tags(&tx, name, None)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_record_tags(&mut self, scope: TagScope, id: &str, values: &[String]) -> Result<()> {
        let values = tags::normalize(values)?;
        let tx = self.desk.transaction()?;
        let changed = tx.execute(
            &format!(
                "UPDATE {} SET tags_json=?1 WHERE {}=?2",
                scope.table(),
                scope.id_column()
            ),
            params![serde_json::to_string(&values)?, id],
        )?;
        if changed == 0 {
            return Err(DeskError::InvalidPath("这条记录已不存在".into()));
        }
        ensure_tags(&tx, scope, &values)?;
        sync_inventory_category(&tx, scope)?;
        tx.commit()?;
        Ok(())
    }

    #[cfg(test)]
    pub fn record_tags(&self, scope: TagScope, id: &str) -> Result<Vec<String>> {
        Ok(self
            .desk
            .query_row(
                &format!(
                    "SELECT tags_json FROM {} WHERE {}=?1",
                    scope.table(),
                    scope.id_column()
                ),
                [id],
                |row| read_tags(row, 0),
            )
            .optional()?
            .unwrap_or_default())
    }

    pub fn tagged_records(&self, scope: TagScope) -> Result<Vec<tags::RecordTags>> {
        Ok(self
            .desk
            .prepare(&format!(
                "SELECT {},tags_json FROM {} WHERE tags_json<>'[]' ORDER BY 1",
                scope.id_column(),
                scope.table()
            ))?
            .query_map([], |row| {
                Ok(tags::RecordTags {
                    scope,
                    id: row.get(0)?,
                    tags: read_tags(row, 1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn credentials(&self) -> Result<Vec<Credential>> {
        let mut statement = self.desk.prepare("SELECT id,title,username,password,website,notes,tags_json FROM credentials ORDER BY updated_at DESC,rowid DESC")?;
        Ok(statement
            .query_map([], |row| {
                Ok(Credential {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    username: row.get(2)?,
                    password: row.get(3)?,
                    website: row.get(4)?,
                    notes: row.get(5)?,
                    tags: read_tags(row, 6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn websites(&self) -> Result<Vec<Website>> {
        let mut statement = self.desk.prepare(
            "SELECT id,title,url,category,notes FROM websites ORDER BY updated_at DESC,rowid DESC",
        )?;
        Ok(statement
            .query_map([], |row| {
                Ok(Website {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    url: row.get(2)?,
                    category: row.get(3)?,
                    notes: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    #[cfg(test)]
    pub fn save_website(&mut self, item: &Website) -> Result<()> {
        let existing: Option<String> = self
            .desk
            .query_row(
                "SELECT category FROM websites WHERE id=?1",
                [&item.id],
                |r| r.get(0),
            )
            .optional()?;
        let tags = if existing.is_some() {
            self.record_tags(TagScope::Websites, &item.id)?
        } else if item.category.trim().is_empty() {
            vec![]
        } else {
            vec![item.category.trim().to_owned()]
        };
        self.save_website_classified(item, &tags)
    }

    pub fn save_website_classified(&mut self, item: &Website, tags: &[String]) -> Result<()> {
        let values = crate::tags::normalize(tags)?;
        let tx = self.desk.transaction()?;
        ensure_tags(&tx, TagScope::Websites, &values)?;
        tx.execute("INSERT INTO websites(id,title,url,category,notes,tags_json) VALUES(?1,?2,?3,?4,?5,?6)
            ON CONFLICT(id) DO UPDATE SET title=excluded.title,url=excluded.url,category=excluded.category,notes=excluded.notes,tags_json=excluded.tags_json,updated_at=CURRENT_TIMESTAMP",
            params![item.id, item.title, item.url, values.join(" · "), item.notes, serde_json::to_string(&values)?])?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_website(&mut self, id: &str) -> Result<()> {
        self.desk
            .execute("DELETE FROM websites WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn set_setting(&mut self, key: &str, value: &str) -> Result<()> {
        self.desk.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value])?;
        Ok(())
    }

    pub fn app_tools(&self) -> Result<Vec<crate::launcher::AppTool>> {
        let mut stmt = self.desk.prepare(
            "SELECT id,name,path_native,notes FROM app_tools ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (id, name, path, notes) = row?;
            Ok(crate::launcher::AppTool {
                id,
                name,
                path: decode_path(&path)?,
                notes,
            })
        })
        .collect()
    }
    pub fn save_app_tool(&mut self, item: &crate::launcher::AppTool) -> Result<()> {
        if item.name.trim().is_empty() {
            return Err(DeskError::InvalidPath("请填写工具名称".into()));
        }
        let path = crate::launcher::prepare(&item.path)?;
        self.desk.execute("INSERT INTO app_tools(id,name,path_native,notes) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET name=excluded.name,path_native=excluded.path_native,notes=excluded.notes",params![item.id,item.name.trim(),encode_path(&path),item.notes])?;
        Ok(())
    }
    pub fn delete_app_tool(&mut self, id: &str) -> Result<()> {
        self.desk
            .execute("DELETE FROM app_tools WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .desk
            .prepare_cached("SELECT value FROM settings WHERE key=?1")?
            .query_row([key], |r| r.get(0))
            .optional()?)
    }

    pub fn settings(&self) -> Result<std::collections::BTreeMap<String, String>> {
        let mut statement = self
            .desk
            .prepare("SELECT key,value FROM settings ORDER BY key")?;
        Ok(statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?)
    }

    pub fn snapshot(&self, destination: &Path) -> Result<()> {
        Self::snapshot_connection(&self.desk, destination)
    }

    pub fn import_records(
        &mut self,
        accounts: &[Credential],
        websites: &[Website],
    ) -> Result<(usize, usize)> {
        let transaction = self.desk.transaction()?;
        let mut added_accounts = 0;
        let mut added_websites = 0;
        for item in accounts {
            let exists: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM credentials WHERE title=?1 AND username=?2 AND password=?3 AND website=?4 AND notes=?5)",
                params![item.title, item.username, item.password, item.website, item.notes], |row| row.get(0))?;
            if !exists {
                let values = tags::normalize(&item.tags)?;
                ensure_tags(&transaction, TagScope::Credentials, &values)?;
                transaction.execute("INSERT INTO credentials(id,title,username,password,website,notes,tags_json) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![item.id,item.title,item.username,item.password,item.website,item.notes,serde_json::to_string(&values)?])?;
                added_accounts += 1;
            }
        }
        for item in websites {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM websites WHERE url=?1)",
                [&item.url],
                |row| row.get(0),
            )?;
            if !exists {
                let values = if item.category.trim().is_empty() {
                    vec![]
                } else {
                    crate::tags::normalize(std::slice::from_ref(&item.category))?
                };
                ensure_tags(&transaction, TagScope::Websites, &values)?;
                transaction.execute(
                    "INSERT INTO websites(id,title,url,category,notes,tags_json) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![item.id, item.title, item.url, item.category, item.notes, serde_json::to_string(&values)?],
                )?;
                added_websites += 1;
            }
        }
        transaction.commit()?;
        Ok((added_accounts, added_websites))
    }

    pub fn snapshot_connection(source: &Connection, destination: &Path) -> Result<()> {
        let mut destination = Connection::open(destination)?;
        let backup = rusqlite::backup::Backup::new(source, &mut destination)?;
        // Bound retries so a locked source reports failure instead of hanging forever.
        for _ in 0..300 {
            match backup.step(256)? {
                rusqlite::backup::StepResult::Done => return Ok(()),
                rusqlite::backup::StepResult::More => {}
                _ => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
        Err(DeskError::InvalidPath("数据库正忙，请稍后重新导出".into()))
    }

    #[cfg(test)]
    pub fn index_file(
        &mut self,
        root_id: &str,
        path: &Path,
        title: &str,
        body: &str,
    ) -> Result<()> {
        let path_display = path.to_string_lossy().into_owned();
        let path_native = encode_path(path);
        let tx = self.desk.transaction()?;
        tx.execute(
            "INSERT INTO file_index_fts(file_index_fts,rowid,title,path_display,body)
          SELECT 'delete',rowid,title,path_display,body FROM file_index WHERE path_native=?1",
            [&path_native],
        )?;
        tx.execute("INSERT INTO file_index(id,root_id,path_native,path_display,title,body) VALUES(?1,?2,?3,?4,?5,?6)
          ON CONFLICT(path_native) DO UPDATE SET root_id=excluded.root_id,path_display=excluded.path_display,title=excluded.title,body=excluded.body,modified_at=CURRENT_TIMESTAMP",
          params![Uuid::new_v4().to_string(),root_id,path_native,path_display,title,body])?;
        let rowid: i64 = tx.query_row(
            "SELECT rowid FROM file_index WHERE path_native=?1",
            [&path_native],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO file_index_fts(rowid,title,path_display,body) VALUES(?1,?2,?3,?4)",
            params![rowid, title, path_display, body],
        )?;
        tx.commit()?;
        Ok(())
    }

    #[cfg(test)]
    pub fn search_files(&self, query: &str) -> Result<Vec<(String, String)>> {
        let mut out = Vec::new();
        if query.chars().count() >= 3 {
            let literal = format!("\"{}\"", query.replace('"', "\"\""));
            let mut stmt = self.desk.prepare("SELECT i.title,i.path_display FROM file_index_fts f JOIN file_index i ON i.rowid=f.rowid
              WHERE file_index_fts MATCH ?1 ORDER BY rank LIMIT 100")?;
            let rows = stmt.query_map([literal], |r| Ok((r.get(0)?, r.get(1)?)))?;
            for row in rows {
                out.push(row?);
            }
        }
        if query.chars().count() < 3 {
            let pattern = format!(
                "%{}%",
                query
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            let mut stmt = self.desk.prepare("SELECT title,path_display FROM file_index WHERE title LIKE ?1 ESCAPE '\\' OR path_display LIKE ?1 ESCAPE '\\' OR body LIKE ?1 ESCAPE '\\' LIMIT 100")?;
            let rows = stmt.query_map([pattern], |r| Ok((r.get(0)?, r.get(1)?)))?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn touch_document(&mut self, path: &Path) -> Result<()> {
        let path = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .into_owned();
        let format = Path::new(&path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("txt")
            .to_ascii_lowercase();
        self.desk.execute("INSERT INTO documents(path,format) VALUES(?1,?2) ON CONFLICT(path) DO UPDATE SET last_opened=CURRENT_TIMESTAMP,hidden=0",params![path,format])?;
        Ok(())
    }

    pub fn document_record_key(&mut self, path: &Path) -> Result<String> {
        let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let display = crate::files::shell_path(&normalized);
        let existing: Option<String> = self
            .desk
            .query_row(
                "SELECT path FROM documents WHERE path IN (?1,?2,?3) ORDER BY path=?1 DESC LIMIT 1",
                params![
                    path.to_string_lossy().as_ref(),
                    normalized.to_string_lossy().as_ref(),
                    display.to_string_lossy().as_ref()
                ],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(key) = existing {
            return Ok(key);
        }
        self.touch_document(&normalized)?;
        Ok(normalized.to_string_lossy().into_owned())
    }

    #[cfg(test)]
    pub fn documents(&self) -> Result<Vec<PathBuf>> {
        let mut stmt = self
            .desk
            .prepare("SELECT path FROM documents WHERE hidden=0 ORDER BY last_opened DESC, path")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut paths = Vec::new();
        for row in rows {
            paths.push(PathBuf::from(row?));
        }
        Ok(paths)
    }

    pub fn document_records(&self) -> Result<Vec<DocumentRecord>> {
        let mut statement = self.desk.prepare(
            "SELECT path,title FROM documents WHERE hidden=0 ORDER BY last_opened DESC,title<>'' DESC,path",
        )?;
        let records = statement.query_map([], |row| {
            let path = PathBuf::from(row.get::<_, String>(0)?);
            let title = visible_document_title(&path, row.get(1)?);
            Ok(DocumentRecord { path, title })
        })?;
        records
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(DeskError::from)
    }

    #[cfg(test)]
    pub fn document_title(&self, path: &Path) -> Result<String> {
        let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let title: Option<String> = self.desk.query_row("SELECT title FROM documents WHERE path=?1 OR path=?2 ORDER BY title<>'' DESC LIMIT 1", params![path.to_string_lossy().as_ref(),normalized.to_string_lossy().as_ref()], |row| row.get(0)).optional()?;
        Ok(visible_document_title(path, title.unwrap_or_default()))
    }

    #[cfg(test)]
    pub fn set_document_title(&mut self, path: &Path, title: &str) -> Result<()> {
        self.touch_document(path)?;
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let title = if title.trim().is_empty() {
            "未命名文档"
        } else {
            title.trim()
        };
        let aliases = self
            .documents()?
            .into_iter()
            .filter(|candidate| {
                candidate
                    .canonicalize()
                    .unwrap_or_else(|_| candidate.clone())
                    == path
            })
            .collect::<Vec<_>>();
        let tx = self.desk.transaction()?;
        for alias in aliases {
            tx.execute(
                "UPDATE documents SET title=?1 WHERE path=?2",
                params![title, alias.to_string_lossy().as_ref()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Remove the library record and its derived index; never remove the file here.
    #[cfg(test)]
    pub fn forget_document(&mut self, path: &Path) -> Result<()> {
        let display = path.to_string_lossy();
        let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let normalized_display = normalized.to_string_lossy();
        let native = encode_path(path);
        let tx = self.desk.transaction()?;
        tx.execute(
            "DELETE FROM documents WHERE path=?1 OR path=?2",
            params![display.as_ref(), normalized_display.as_ref()],
        )?;
        tx.execute(
            "INSERT INTO file_index_fts(file_index_fts,rowid,title,path_display,body)
             SELECT 'delete',rowid,title,path_display,body FROM file_index WHERE path_native=?1 OR path_display=?2 OR path_display=?3",
            params![native, display.as_ref(),normalized_display.as_ref()],
        )?;
        tx.execute(
            "DELETE FROM file_index WHERE path_native=?1 OR path_display=?2 OR path_display=?3",
            params![native, display.as_ref(), normalized_display.as_ref()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn inventory_items(&self) -> Result<Vec<InventoryItem>> {
        let mut stmt = self.desk.prepare(
            "SELECT id,name,category,quantity,location,notes,tags_json,purchase_date,purchase_price_cents,disposal_date,brand,model FROM inventory_items ORDER BY updated_at DESC,name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(InventoryItem {
                id: row.get(0)?,
                name: row.get(1)?,
                category: row.get(2)?,
                quantity: row.get(3)?,
                location: row.get(4)?,
                notes: row.get(5)?,
                tags: read_tags(row, 6)?,
                purchase_date: row.get(7)?,
                purchase_price_cents: row.get(8)?,
                disposal_date: row.get(9)?,
                brand: row.get(10)?,
                model: row.get(11)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(DeskError::from)
    }

    pub fn save_inventory_item(&mut self, item: &InventoryItem) -> Result<()> {
        let purchase_date = crate::model::normalize_date(&item.purchase_date)?;
        let disposal_date = crate::model::normalize_date(&item.disposal_date)?;
        if item.purchase_price_cents.is_some_and(|price| price < 0) {
            return Err(DeskError::InvalidPath("购买价格不能小于零".into()));
        }
        let mut values = tags::normalize(&item.tags)?;
        if values.is_empty() && !item.category.trim().is_empty() {
            values.push(item.category.trim().into());
        }
        let tx = self.desk.transaction()?;
        ensure_tags(&tx, TagScope::Inventory, &values)?;
        tx.execute(
            "INSERT INTO inventory_items(id,name,category,quantity,location,notes,tags_json,purchase_date,purchase_price_cents,disposal_date,brand,model) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name,category=excluded.category,quantity=excluded.quantity,
               location=excluded.location,notes=excluded.notes,tags_json=excluded.tags_json,purchase_date=excluded.purchase_date,purchase_price_cents=excluded.purchase_price_cents,disposal_date=excluded.disposal_date,brand=excluded.brand,model=excluded.model,updated_at=CURRENT_TIMESTAMP",
            params![
                item.id,
                item.name.trim(),
                values.join(" · "),
                item.quantity.max(0),
                item.location.trim(),
                item.notes.trim(),
                serde_json::to_string(&values)?,purchase_date,item.purchase_price_cents,disposal_date,item.brand.trim(),item.model.trim()
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_inventory_item(&mut self, id: &str) -> Result<()> {
        self.desk
            .execute("DELETE FROM inventory_items WHERE id=?1", [id])?;
        Ok(())
    }
}

fn sync_inventory_category(tx: &rusqlite::Transaction<'_>, scope: TagScope) -> Result<()> {
    if scope == TagScope::Inventory {
        tx.execute("UPDATE inventory_items SET category=COALESCE((SELECT group_concat(value,' · ') FROM json_each(tags_json)),'')",[])?;
    }
    if scope == TagScope::Websites {
        tx.execute("UPDATE websites SET category=COALESCE((SELECT group_concat(value,' · ') FROM json_each(tags_json)),'')",[])?;
    }
    Ok(())
}

fn rewrite_page_draft_tags(
    tx: &rusqlite::Transaction<'_>,
    original: &str,
    replacement: Option<&str>,
) -> Result<()> {
    let rows = tx
        .prepare("SELECT id,page_json FROM book_page_drafts")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (id, json) in rows {
        let mut page: crate::library::Page = serde_json::from_str(&json)?;
        if page.tags.iter().any(|tag| tag == original) {
            page.tags = page
                .tags
                .into_iter()
                .filter_map(|tag| {
                    if tag == original {
                        replacement.map(str::to_owned)
                    } else {
                        Some(tag)
                    }
                })
                .collect();
            tx.execute(
                "UPDATE book_page_drafts SET page_json=?1 WHERE id=?2",
                params![serde_json::to_string(&page)?, id],
            )?;
        }
    }
    Ok(())
}

fn migrate_website_categories(connection: &mut Connection) -> Result<()> {
    // Existing nonempty tag selections win; an explicit clear also clears category.
    let rows = connection
        .prepare("SELECT id,category FROM websites WHERE tags_json='[]' AND trim(category)<>''")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let tx = connection.transaction()?;
    for (id, category) in rows {
        let values = crate::tags::normalize(&[category])?;
        ensure_tags(&tx, TagScope::Websites, &values)?;
        tx.execute(
            "UPDATE websites SET tags_json=?1 WHERE id=?2",
            params![serde_json::to_string(&values)?, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn migrate_inventory_categories(connection: &mut Connection) -> Result<()> {
    if connection
        .query_row(
            "SELECT value FROM settings WHERE key='inventory.tags_unified_v1'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .is_some()
    {
        return Ok(());
    }
    let tx = connection.transaction()?;
    let rows = tx
        .prepare("SELECT id,category,tags_json FROM inventory_items")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (id, category, json) in rows {
        let mut values: Vec<String> = serde_json::from_str(&json)?;
        // Preserve a distinct legacy category as a classification instead of dropping it.
        if !category.trim().is_empty()
            && category.trim() != values.join(" · ")
            && !values.iter().any(|value| value == category.trim())
        {
            values.push(category.trim().to_owned());
        }
        ensure_tags(&tx, TagScope::Inventory, &values)?;
        tx.execute(
            "UPDATE inventory_items SET tags_json=?1 WHERE id=?2",
            params![serde_json::to_string(&values)?, id],
        )?;
    }
    sync_inventory_category(&tx, TagScope::Inventory)?;
    tx.execute(
        "INSERT INTO settings(key,value) VALUES('inventory.tags_unified_v1','1')",
        [],
    )?;
    tx.commit()?;
    Ok(())
}

fn visible_document_title(path: &Path, title: String) -> String {
    if title.trim().is_empty() {
        path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        title
    }
}

fn read_tags(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Vec<String>> {
    let json: String = row.get(index)?;
    serde_json::from_str(&json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn ensure_tags(tx: &rusqlite::Transaction<'_>, scope: TagScope, values: &[String]) -> Result<()> {
    for value in values {
        tx.execute(
            "INSERT OR IGNORE INTO tags(scope,name) VALUES(?1,?2)",
            params![scope.key(), value],
        )?;
    }
    Ok(())
}

fn encode_path(path: &Path) -> Vec<u8> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect()
    }
    #[cfg(not(windows))]
    {
        path.as_os_str().to_string_lossy().as_bytes().to_vec()
    }
}

fn decode_path(bytes: &[u8]) -> Result<PathBuf> {
    #[cfg(windows)]
    {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        if !bytes.len().is_multiple_of(2) {
            return Err(DeskError::InvalidPath("路径记录无效".into()));
        }
        let wide = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        Ok(PathBuf::from(OsString::from_wide(&wide)))
    }
    #[cfg(not(windows))]
    {
        Ok(PathBuf::from(String::from_utf8(bytes.to_vec()).map_err(
            |_| DeskError::InvalidPath("路径记录无效".into()),
        )?))
    }
}
