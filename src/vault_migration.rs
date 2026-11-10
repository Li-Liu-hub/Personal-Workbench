//! 一次性数据迁移：把旧版保存在私网加密库中的写作资料合并回本机数据库。
//!
//! 该模块同时参与 release 构建（由 `remote_vault` 调用）与 debug 测试构建（单元回归），
//! 因此不依赖网络、SSH、MySQL 或 age 传输层：调用方负责解密并传入内存中的快照。
//! 一旦迁移完成就不再重跑，所以逻辑必须可重复执行、失败时不改动任何一端。
//!
//! 迁移边界（0.15.76 起）：
//! - 仍留在私网加密库的只有账号资料：`credentials`、`legacy_credential_imports`
//!   以及 `tags`/`favorites` 中 `scope='credentials'` 的行；
//! - 写作资料全部回到本机 `desk.db`：`books`、`book_pages`、`book_documents`、
//!   `book_page_drafts`、`book_page_versions`、`daily_essays`、`documents`、
//!   `writing_imports`、`writing_logs`、`blog_post_links`，以及 `tags`/`favorites`
//!   中 `scope` 为 `books`/`book_pages`/`documents` 的行。
//!
//! 执行顺序（任何一步失败都不会改动远端；本机改动是事务内合并，失败即回滚）：
//! 1. 校验本机库；
//! 2. 事务内把远端快照中的写作表逐行 `INSERT OR IGNORE` 合并进本机（本机已有行不覆盖）；
//! 3. 事务内逐表核对：远端每一行都必须能在本机找到（行指纹集合包含），缺一即回滚报错；
//! 4. 写作图片写入本机后回读比对字节，冲突时另存副本、绝不覆盖本机文件；
//! 5. 写出恢复快照 `backups/remote-writing-<YYYYMMDD-HHMMSS>.db`（纯写作表明文，可直接用
//!    SQLite 工具打开）与 `backups/remote-writing-<YYYYMMDD-HHMMSS>.payload.age`（原始密文，
//!    需本机私钥才能解开），并回读校验快照可解析、逐表行数与远端一致；
//! 6. 最后才写 `settings['remote-vault.writing-local-migrated']='1'`。
//!
//! 远端裁剪（把私密载荷裁成只剩账号表、删除远端 assets 对象）不在这里做，
//! 由 `remote_vault` 在本函数成功返回后再提交，确保任何失败都不会丢远端数据。
//!
//! 人工恢复方法：关闭程序后，把 `backups/remote-writing-*.db` 复制为新的 `desk.db`
//! （或只从该库读回写作表）即可找回远端原始写作数据；`.payload.age` 是远端原始密文，
//! 只有配合本机 `%LOCALAPPDATA%\PersonalDesk\private-writing-identity.dpapi` 才能解密。

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    hash::{Hash, Hasher},
    io::Write,
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rusqlite::{Connection, MAIN_DB, OptionalExtension, params_from_iter, types::Value};

use crate::error::{DeskError, Result};

/// 旧版私网加密库里保存的写作表；迁移后这些表只在本机 `desk.db`。
pub(crate) const WRITING_TABLES: &[&str] = &[
    "books",
    "book_pages",
    "book_documents",
    "book_page_drafts",
    "book_page_versions",
    "daily_essays",
    "documents",
    "writing_imports",
    "writing_logs",
    "blog_post_links",
];

/// 仍属于写作资料的标签/收藏作用域（`credentials` 仍留在私网库）。
pub(crate) const WRITING_SCOPES: &[&str] = &["books", "book_pages", "documents"];

/// 迁移成功标记，写在 `settings` 表；只在本地全部步骤成功后写入。
pub(crate) const MIGRATION_MARKER: &str = "remote-vault.writing-local-migrated";

/// 恢复快照前缀，文件放在 `<数据目录>/backups/`。
pub(crate) const RECOVERY_PREFIX: &str = "remote-writing-";
const RECOVERY_DATABASE_SUFFIX: &str = ".db";
const RECOVERY_PAYLOAD_SUFFIX: &str = ".payload.age";

/// 私网加密库中仍保密的账号表（写作表不再包含在内）。
const ACCOUNT_TABLES: &[&str] = &[
    "credentials",
    "favorites",
    "legacy_credential_imports",
    "tags",
];

/// 挂到 `temp` 架构、UI 通过它读写账号资料的表。
const ACCOUNT_TEMP_TABLES: &[&str] = &["credentials", "legacy_credential_imports"];

fn migration_error(message: impl Into<String>) -> DeskError {
    DeskError::InvalidInput(format!("写作资料迁移：{}", message.into()))
}

/// 写作资料作用作用域过滤条件，例如 `scope IN ('books','book_pages','documents')`。
fn writing_scope_filter() -> String {
    format!(
        "scope IN ({})",
        WRITING_SCOPES
            .iter()
            .map(|scope| format!("'{scope}'"))
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub(crate) fn exists_in_schema(connection: &Connection, schema: &str, table: &str) -> Result<bool> {
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM {schema}.sqlite_master WHERE type='table' AND name=?1)"
    );
    Ok(connection.query_row(&sql, [table], |row| row.get(0))?)
}

/// 把 `CREATE TABLE <source>` 改写成 `CREATE TABLE <destination>`，保留列定义与约束。
fn rename_create_table(sql: &str, source_table: &str, destination_table: &str) -> Result<String> {
    for prefix in ["CREATE TABLE IF NOT EXISTS ", "CREATE TABLE "] {
        let expected = format!("{prefix}{source_table}");
        if sql.starts_with(&expected) {
            return Ok(format!(
                "{}{}{}",
                prefix,
                destination_table,
                &sql[expected.len()..]
            ));
        }
    }
    Err(migration_error(format!(
        "本机表 {source_table} 的结构无法安全复制到 {destination_table}"
    )))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CopyMode {
    /// 逐行插入，冲突直接报错（用于序列化私密载荷、生成账号快照与恢复快照）。
    Strict,
    /// 合并：`INSERT OR IGNORE`，本机已有行/冲突行原样保留（用于迁移）。
    Merge,
}

struct TableCopy<'a> {
    from_schema: &'a str,
    from_table: &'a str,
    to_schema: &'a str,
    to_table: &'a str,
    filter: Option<&'a str>,
    mode: CopyMode,
}

#[derive(Default)]
struct CopyOutcome {
    inserted: usize,
    /// 源表每一行的内容指纹；迁移核对时用来证明“远端行都到了本机”。
    row_fingerprints: Vec<u64>,
}

/// 复制一张表。返回实际插入行数与源行指纹，供迁移核对使用。
fn copy_table(
    source: &Connection,
    destination: &Connection,
    plan: &TableCopy<'_>,
) -> Result<CopyOutcome> {
    let schema_sql: Option<String> = source
        .query_row(
            &format!(
                "SELECT sql FROM {}.sqlite_master WHERE type='table' AND name=?1",
                plan.from_schema
            ),
            [plan.from_table],
            |row| row.get(0),
        )
        .optional()?;
    let Some(schema_sql) = schema_sql else {
        return Ok(CopyOutcome::default());
    };
    if !exists_in_schema(destination, plan.to_schema, plan.to_table)? {
        let create_sql = rename_create_table(&schema_sql, plan.from_table, plan.to_table)?;
        destination.execute_batch(&create_sql)?;
    }
    let pragma = format!(
        "PRAGMA {}.table_info(\"{}\")",
        plan.from_schema, plan.from_table
    );
    let columns = source
        .prepare(&pragma)?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if columns.is_empty() {
        return Ok(CopyOutcome::default());
    }
    let quoted_columns = columns
        .iter()
        .map(|column| format!("\"{}\"", column.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(",");
    let where_sql = plan
        .filter
        .map(|value| format!(" WHERE {value}"))
        .unwrap_or_default();
    let select_sql = format!(
        "SELECT {quoted_columns} FROM {}.\"{}\"{where_sql} ORDER BY rowid",
        plan.from_schema, plan.from_table
    );
    let placeholders = (1..=columns.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let insert_sql = format!(
        "{} INTO {}.\"{}\" ({quoted_columns}) VALUES ({placeholders})",
        match plan.mode {
            CopyMode::Strict => "INSERT",
            CopyMode::Merge => "INSERT OR IGNORE",
        },
        plan.to_schema,
        plan.to_table
    );
    let mut statement = source.prepare(&select_sql)?;
    let column_count = columns.len();
    let rows = statement.query_map([], |row| {
        (0..column_count)
            .map(|index| row.get::<_, Value>(index))
            .collect::<std::result::Result<Vec<_>, _>>()
    })?;
    let mut outcome = CopyOutcome::default();
    for row in rows {
        let values = row?;
        outcome.row_fingerprints.push(row_fingerprint(&values));
        destination
            .execute(&insert_sql, params_from_iter(values.iter()))
            .map_err(|error| {
                migration_error(format!(
                    "写入本机表 {} 失败（来源 {}.{}）：{error}",
                    plan.to_table, plan.from_schema, plan.from_table
                ))
            })?;
        outcome.inserted += destination.changes() as usize;
    }
    Ok(outcome)
}

/// 逐列计算行指纹。`EXCEPT` 级的内容比较在跨连接场景无法直接做，
/// 这里用同一进程内的确定性哈希比较“远端行是否都存在于本机”。
fn row_fingerprint(values: &[Value]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for value in values {
        match value {
            Value::Null => 0u8.hash(&mut hasher),
            Value::Integer(value) => {
                1u8.hash(&mut hasher);
                value.hash(&mut hasher);
            }
            Value::Real(value) => {
                2u8.hash(&mut hasher);
                value.to_bits().hash(&mut hasher);
            }
            Value::Text(value) => {
                3u8.hash(&mut hasher);
                value.hash(&mut hasher);
            }
            Value::Blob(value) => {
                4u8.hash(&mut hasher);
                value.hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

fn table_fingerprints(
    connection: &Connection,
    schema: &str,
    table: &str,
    filter: Option<&str>,
) -> Result<HashSet<u64>> {
    let where_sql = filter
        .map(|value| format!(" WHERE {value}"))
        .unwrap_or_default();
    let sql = format!("SELECT * FROM {schema}.\"{table}\"{where_sql}");
    let mut statement = connection.prepare(&sql)?;
    let column_count = statement.column_count();
    let rows = statement.query_map([], |row| {
        (0..column_count)
            .map(|index| row.get::<_, Value>(index))
            .collect::<std::result::Result<Vec<_>, _>>()
    })?;
    let mut fingerprints = HashSet::new();
    for row in rows {
        fingerprints.insert(row_fingerprint(&row?));
    }
    Ok(fingerprints)
}

fn table_row_count(
    connection: &Connection,
    schema: &str,
    table: &str,
    filter: Option<&str>,
) -> Result<i64> {
    if !exists_in_schema(connection, schema, table)? {
        return Ok(0);
    }
    let where_sql = filter
        .map(|value| format!(" WHERE {value}"))
        .unwrap_or_default();
    let sql = format!("SELECT COUNT(*) FROM {schema}.\"{table}\"{where_sql}");
    Ok(connection.query_row(&sql, [], |row| row.get(0))?)
}

/// 本机 `desk.db` 是否还有账号资料行（用于启动时的冲突检测）。
pub(crate) fn private_rows_exist(connection: &Connection) -> Result<bool> {
    for table in ["credentials", "legacy_credential_imports"] {
        if table_row_count(connection, "main", table, None)? > 0 {
            return Ok(true);
        }
    }
    for table in ["favorites", "tags"] {
        if table_row_count(connection, "main", table, Some("scope='credentials'"))? > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 远端私密载荷里是否还有写作资料（旧格式）。
pub(crate) fn writing_rows_present(connection: &Connection) -> Result<bool> {
    for table in WRITING_TABLES {
        if table_row_count(connection, "main", table, None)? > 0 {
            return Ok(true);
        }
    }
    let scope_filter = writing_scope_filter();
    for table in ["tags", "favorites"] {
        if table_row_count(connection, "main", table, Some(&scope_filter))? > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 仍留在私网加密库的账号表与过滤条件；启动时用它比较两边账号资料是否一致。
fn account_tables() -> [(&'static str, Option<&'static str>); 4] {
    [
        ("credentials", None),
        ("legacy_credential_imports", None),
        ("tags", Some("scope='credentials'")),
        ("favorites", Some("scope='credentials'")),
    ]
}

/// 比较本机与远端载荷的账号资料是否完全一致。
///
/// 双方都有账号行且内容不同时必须报错保留两边，不能静默覆盖任何一端。
pub(crate) fn account_state_matches(local: &Connection, remote: &Connection) -> Result<bool> {
    for (table, filter) in account_tables() {
        let local_fingerprints = table_fingerprints(local, "main", table, filter)?;
        let remote_fingerprints = table_fingerprints(remote, "main", table, filter)?;
        if local_fingerprints != remote_fingerprints {
            return Ok(false);
        }
    }
    Ok(true)
}

/// 把本机账号资料序列化成私密载荷的 SQLite 快照（只含账号表，绝不含写作表）。
pub(crate) fn serialize_private_state(source: &Connection) -> Result<Vec<u8>> {
    let private = Connection::open_in_memory()?;
    private.pragma_update(None, "foreign_keys", "OFF")?;
    for table in ACCOUNT_TABLES {
        // 挂载后账号表在 temp：credentials / legacy_credential_imports 由 attach_account_tables
        // 复制成同名 temp 表，未限定 schema 的 SQL 也会先命中 temp；标签/收藏则是
        // temp.private_tags / temp.private_favorites。挂载前（首次启动、旧数据）都在 main。
        // 这里必须按真实位置读取：读 main.credentials 会得到空表，上传后覆盖服务器账号。
        let (schema, source_table) = match *table {
            "tags" if exists_in_schema(source, "temp", "private_tags")? => ("temp", "private_tags"),
            "favorites" if exists_in_schema(source, "temp", "private_favorites")? => {
                ("temp", "private_favorites")
            }
            "credentials" | "legacy_credential_imports"
                if exists_in_schema(source, "temp", table)? =>
            {
                ("temp", *table)
            }
            _ => ("main", *table),
        };
        let filter = match *table {
            "tags" | "favorites" => Some("scope='credentials'"),
            _ => None,
        };
        copy_table(
            source,
            &private,
            &TableCopy {
                from_schema: schema,
                from_table: source_table,
                to_schema: "main",
                to_table: table,
                filter,
                mode: CopyMode::Strict,
            },
        )?;
    }
    Ok(private.serialize(MAIN_DB)?.to_vec())
}

/// 打开调用方解密后的私密载荷快照（SQLite 序列化字节）。
pub(crate) fn deserialize_private_state(bytes: &[u8]) -> Result<Connection> {
    let mut connection = Connection::open_in_memory()?;
    connection.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), false)?;
    connection.pragma_update(None, "foreign_keys", "OFF")?;
    Ok(connection)
}

/// 只保留账号资料的快照：迁移成功后用它覆盖远端私密载荷。
pub(crate) fn account_snapshot(private: &Connection) -> Result<Connection> {
    let account = Connection::open_in_memory()?;
    account.pragma_update(None, "foreign_keys", "OFF")?;
    for table in ["credentials", "legacy_credential_imports"] {
        copy_table(
            private,
            &account,
            &TableCopy {
                from_schema: "main",
                from_table: table,
                to_schema: "main",
                to_table: table,
                filter: None,
                mode: CopyMode::Strict,
            },
        )?;
    }
    for table in ["tags", "favorites"] {
        copy_table(
            private,
            &account,
            &TableCopy {
                from_schema: "main",
                from_table: table,
                to_schema: "main",
                to_table: table,
                filter: Some("scope='credentials'"),
                mode: CopyMode::Strict,
            },
        )?;
    }
    Ok(account)
}

fn create_temp_copy(
    destination: &Connection,
    source: &Connection,
    source_table: &str,
    destination_table: &str,
) -> Result<()> {
    let schema_sql: Option<String> = source
        .query_row(
            "SELECT sql FROM main.sqlite_master WHERE type='table' AND name=?1",
            [source_table],
            |row| row.get(0),
        )
        .optional()?;
    let Some(mut schema_sql) = schema_sql else {
        return Ok(());
    };
    schema_sql = rename_create_table(&schema_sql, source_table, destination_table)?
        .replacen(
            "CREATE TABLE IF NOT EXISTS",
            "CREATE TEMP TABLE IF NOT EXISTS",
            1,
        )
        .replacen("CREATE TABLE", "CREATE TEMP TABLE", 1);
    destination.execute_batch(&schema_sql)?;
    copy_table(
        source,
        destination,
        &TableCopy {
            from_schema: "main",
            from_table: source_table,
            to_schema: "temp",
            to_table: destination_table,
            filter: None,
            mode: CopyMode::Strict,
        },
    )?;
    Ok(())
}

/// 把账号表挂到 `temp` 架构供 UI 读写；写作表始终读写本机 `main`。
pub(crate) fn attach_account_tables(desk: &mut Connection, private: &Connection) -> Result<()> {
    for table in ACCOUNT_TEMP_TABLES {
        create_temp_copy(desk, private, table, table)?;
    }
    create_temp_copy(desk, private, "tags", "private_tags")?;
    create_temp_copy(desk, private, "favorites", "private_favorites")?;
    Ok(())
}

/// 清理本机账号行；写作行在 `main` 且不在清理名单里，绝不误删。
pub(crate) fn clear_local_private_rows(desk: &mut Connection, vacuum_file: bool) -> Result<()> {
    desk.pragma_update(Some("main"), "secure_delete", "ON")?;
    let tx = desk.transaction()?;
    for table in ["credentials", "legacy_credential_imports"] {
        if exists_in_schema(&tx, "main", table)? {
            tx.execute(&format!("DELETE FROM main.\"{table}\""), [])?;
        }
    }
    for table in ["favorites", "tags"] {
        if exists_in_schema(&tx, "main", table)? {
            tx.execute(
                &format!("DELETE FROM main.\"{table}\" WHERE scope='credentials'"),
                [],
            )?;
        }
    }
    tx.commit()?;
    if vacuum_file {
        desk.execute_batch(
            "PRAGMA main.wal_checkpoint(TRUNCATE); VACUUM main; PRAGMA main.wal_checkpoint(TRUNCATE);",
        )?;
    }
    Ok(())
}

/// 迁移标记是否已写入（只在本地迁移全部成功后写入）。
pub(crate) fn migration_completed(desk: &Connection) -> Result<bool> {
    let value: Option<String> = desk
        .query_row(
            "SELECT value FROM main.settings WHERE key=?1",
            [MIGRATION_MARKER],
            |row| row.get(0),
        )
        .optional()?;
    Ok(value.as_deref() == Some("1"))
}

fn mark_migration_completed(desk: &Connection) -> Result<()> {
    desk.execute(
        "INSERT INTO main.settings(key,value) VALUES(?1,'1') ON CONFLICT(key) DO UPDATE SET value='1'",
        [MIGRATION_MARKER],
    )?;
    Ok(())
}

fn validate_asset_path(path: &str) -> Result<()> {
    if !(path.starts_with("writing-assets/") || path.starts_with("data/picture/"))
        || path.contains(':')
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(migration_error(format!("写作图片路径无效：{path}")));
    }
    Ok(())
}

/// 解开旧版远端 `assets` 对象：JSON 的 `{相对路径: base64 字节}`。
pub(crate) fn decode_assets(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    let encoded: BTreeMap<String, String> = serde_json::from_slice(bytes)
        .map_err(|error| migration_error(format!("远端写作图片清单无法解析：{error}")))?;
    let mut assets = BTreeMap::new();
    for (path, data) in encoded {
        validate_asset_path(&path)?;
        let decoded = BASE64
            .decode(data)
            .map_err(|_| migration_error(format!("远端写作图片编码无效：{path}")))?;
        assets.insert(path, decoded);
    }
    Ok(assets)
}

/// 图片落盘结果；冲突文件另存为 `<原名>.remote-restored-<内容指纹><扩展名>`，绝不覆盖本机文件。
#[derive(Debug, Default)]
pub(crate) struct AssetRestoreReport {
    pub written: usize,
    pub skipped: usize,
    pub conflicts: Vec<String>,
}

fn restore_assets_to_disk(
    data_dir: &Path,
    assets: &BTreeMap<String, Vec<u8>>,
) -> Result<AssetRestoreReport> {
    let root = data_dir.canonicalize()?;
    let mut report = AssetRestoreReport::default();
    for (relative, bytes) in assets {
        validate_asset_path(relative)?;
        let path = data_dir.join(relative);
        let parent = path
            .parent()
            .ok_or_else(|| migration_error(format!("写作图片目录无效：{relative}")))?;
        fs::create_dir_all(parent)?;
        let canonical_parent = parent.canonicalize()?;
        if !canonical_parent.starts_with(&root) {
            return Err(migration_error(format!("写作图片路径越界：{relative}")));
        }
        if path.is_file() {
            if fs::read(&path)? == *bytes {
                report.skipped += 1;
                continue;
            }
            let file_stem = path
                .file_stem()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| "image".into());
            let extension = path
                .extension()
                .map(|value| format!(".{}", value.to_string_lossy()))
                .unwrap_or_default();
            let conflict = parent.join(format!(
                "{file_stem}.remote-restored-{:016x}{extension}",
                bytes_fingerprint(bytes)
            ));
            write_file_verified(&conflict, bytes)?;
            report.conflicts.push(relative.clone());
            continue;
        }
        write_file_verified(&path, bytes)?;
        report.written += 1;
    }
    Ok(report)
}

fn bytes_fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// 原子写入 + 回读比对：先写同目录临时文件，读回一致后再改名到目标路径。
fn write_file_verified(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.is_file() {
        if fs::read(path)? == bytes {
            return Ok(());
        }
        return Err(migration_error(format!(
            "目标文件已存在且内容不同，未覆盖：{}",
            path.display()
        )));
    }
    let parent = path
        .parent()
        .ok_or_else(|| migration_error(format!("目标目录无效：{}", path.display())))?;
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "writing".into());
    let temporary = parent.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    let write_result = (|| -> Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        if fs::read(&temporary)? != bytes {
            return Err(migration_error(format!(
                "写入后回读不一致：{}",
                path.display()
            )));
        }
        fs::rename(&temporary, path)?;
        if fs::read(path)? != bytes {
            return Err(migration_error(format!(
                "改名后回读不一致：{}",
                path.display()
            )));
        }
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

/// 只含写作表的恢复快照（明文，可直接用 SQLite 工具打开；不含任何账号资料）。
fn writing_only_snapshot(remote: &Connection) -> Result<Vec<u8>> {
    let snapshot = Connection::open_in_memory()?;
    snapshot.pragma_update(None, "foreign_keys", "OFF")?;
    for table in WRITING_TABLES {
        copy_table(
            remote,
            &snapshot,
            &TableCopy {
                from_schema: "main",
                from_table: table,
                to_schema: "main",
                to_table: table,
                filter: None,
                mode: CopyMode::Strict,
            },
        )?;
    }
    let scope_filter = writing_scope_filter();
    for table in ["tags", "favorites"] {
        copy_table(
            remote,
            &snapshot,
            &TableCopy {
                from_schema: "main",
                from_table: table,
                to_schema: "main",
                to_table: table,
                filter: Some(&scope_filter),
                mode: CopyMode::Strict,
            },
        )?;
    }
    Ok(snapshot.serialize(MAIN_DB)?.to_vec())
}

/// 回读快照并逐表核对行数；恢复快照必须真的能用。
fn verify_writing_snapshot(bytes: &[u8], remote: &Connection) -> Result<()> {
    let snapshot = deserialize_private_state(bytes)?;
    for table in WRITING_TABLES {
        let expected = table_row_count(remote, "main", table, None)?;
        let actual = table_row_count(&snapshot, "main", table, None)?;
        if expected != actual {
            return Err(migration_error(format!(
                "恢复快照行数核对失败：表 {table} 远端 {expected} 行、快照 {actual} 行"
            )));
        }
    }
    let scope_filter = writing_scope_filter();
    for table in ["tags", "favorites"] {
        let expected = table_row_count(remote, "main", table, Some(&scope_filter))?;
        let actual = table_row_count(&snapshot, "main", table, Some(&scope_filter))?;
        if expected != actual {
            return Err(migration_error(format!(
                "恢复快照行数核对失败：表 {table} 远端 {expected} 行、快照 {actual} 行"
            )));
        }
    }
    Ok(())
}

/// 已有恢复快照时复用（重复启动/重试不堆积文件），否则新建一份并回读校验。
fn ensure_recovery_snapshot(
    data_dir: &Path,
    remote: &Connection,
    remote_ciphertext: Option<&[u8]>,
) -> Result<Vec<PathBuf>> {
    let backups = data_dir.join("backups");
    fs::create_dir_all(&backups)?;
    if let Some(existing) = existing_recovery_database(&backups)? {
        let mut paths = vec![existing.clone()];
        if let Some(ciphertext) = remote_ciphertext {
            let stem = existing
                .file_stem()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("{}unknown", RECOVERY_PREFIX));
            let payload = backups.join(format!("{stem}{RECOVERY_PAYLOAD_SUFFIX}"));
            write_file_verified(&payload, ciphertext)?;
            paths.push(payload);
        }
        return Ok(paths);
    }
    let bytes = writing_only_snapshot(remote)?;
    verify_writing_snapshot(&bytes, remote)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let database = backups.join(format!(
        "{RECOVERY_PREFIX}{stamp}{RECOVERY_DATABASE_SUFFIX}"
    ));
    write_file_verified(&database, &bytes)?;
    let mut paths = vec![database];
    if let Some(ciphertext) = remote_ciphertext {
        let payload = backups.join(format!("{RECOVERY_PREFIX}{stamp}{RECOVERY_PAYLOAD_SUFFIX}"));
        write_file_verified(&payload, ciphertext)?;
        paths.push(payload);
    }
    Ok(paths)
}

fn existing_recovery_database(backups: &Path) -> Result<Option<PathBuf>> {
    if !backups.is_dir() {
        return Ok(None);
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(backups)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(RECOVERY_PREFIX)
            && name.ends_with(RECOVERY_DATABASE_SUFFIX)
            && entry.file_type()?.is_file()
        {
            candidates.push(entry.path());
        }
    }
    candidates.sort();
    Ok(candidates.into_iter().next())
}

/// 单张表的迁移核对结果。
#[derive(Debug)]
pub(crate) struct TableMigrationStats {
    pub table: String,
    pub remote_rows: i64,
    pub local_rows_before: i64,
    pub inserted: usize,
    pub local_rows_after: i64,
}

/// 迁移报告：远端裁剪由 `remote_vault` 在本函数成功返回后再提交。
#[derive(Debug, Default)]
pub(crate) struct MigrationReport {
    pub tables: Vec<TableMigrationStats>,
    pub assets: AssetRestoreReport,
    pub snapshot: Vec<PathBuf>,
}

impl MigrationReport {
    /// 人类可读的一行摘要，启动日志与汇报使用。
    pub(crate) fn summary(&self) -> String {
        let tables = self
            .tables
            .iter()
            .map(|stats| {
                format!(
                    "{}(远端 {} 行/本机迁移前 {} 行/新增 {}/迁移后 {} 行)",
                    stats.table,
                    stats.remote_rows,
                    stats.local_rows_before,
                    stats.inserted,
                    stats.local_rows_after
                )
            })
            .collect::<Vec<_>>()
            .join("、");
        let assets = if self.assets.written + self.assets.skipped + self.assets.conflicts.len() == 0
        {
            "无".to_owned()
        } else {
            format!(
                "新增 {}、已存在 {}、冲突另存 {}",
                self.assets.written,
                self.assets.skipped,
                self.assets.conflicts.len()
            )
        };
        let snapshot = self
            .snapshot
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("、");
        format!("写作表：{tables}；图片：{assets}；恢复快照：{snapshot}")
    }
}

/// 事务内合并一张表，并核对“远端每一行都到了本机”。
fn merge_and_verify_table(
    desk: &Connection,
    remote: &Connection,
    table: &str,
    filter: Option<&str>,
) -> Result<TableMigrationStats> {
    let remote_rows = table_row_count(remote, "main", table, filter)?;
    let local_rows_before = table_row_count(desk, "main", table, filter)?;
    let outcome = copy_table(
        remote,
        desk,
        &TableCopy {
            from_schema: "main",
            from_table: table,
            to_schema: "main",
            to_table: table,
            filter,
            mode: CopyMode::Merge,
        },
    )?;
    let local_rows_after = table_row_count(desk, "main", table, filter)?;
    if local_rows_after < local_rows_before {
        return Err(migration_error(format!(
            "表 {table} 迁移后本机行数反而减少（{local_rows_before} → {local_rows_after}），已回滚"
        )));
    }
    let local_fingerprints = table_fingerprints(desk, "main", table, filter)?;
    let missing = outcome
        .row_fingerprints
        .iter()
        .filter(|fingerprint| !local_fingerprints.contains(fingerprint))
        .count();
    if missing > 0 {
        return Err(migration_error(format!(
            "表 {table} 核对失败：远端 {remote_rows} 行中有 {missing} 行未能在本机找到\
             （可能是同主键内容冲突或表结构差异）；已回滚本机改动，远端与本机数据均保留"
        )));
    }
    if local_rows_after < remote_rows {
        return Err(migration_error(format!(
            "表 {table} 行数核对失败：远端 {remote_rows} 行、本机仅 {local_rows_after} 行；已回滚"
        )));
    }
    Ok(TableMigrationStats {
        table: table.to_owned(),
        remote_rows,
        local_rows_before,
        inserted: outcome.inserted,
        local_rows_after,
    })
}

/// 把旧版远端私密载荷里的写作资料合并回本机。
///
/// 入参均为纯数据，无网络访问：`remote_payload` 是调用方解密后的私密载荷 SQLite 快照，
/// `assets` 是旧版远端 `assets` 对象的解包结果，`remote_ciphertext` 是远端原始密文
/// （只用于写出 `.payload.age` 恢复副本，可为 `None`）。
pub(crate) fn migrate_writing_to_local(
    desk: &mut Connection,
    remote_payload: &[u8],
    data_dir: &Path,
    assets: Option<&BTreeMap<String, Vec<u8>>>,
    remote_ciphertext: Option<&[u8]>,
) -> Result<MigrationReport> {
    // 1. 打开并校验本机库。
    if !exists_in_schema(desk, "main", "settings")? {
        return Err(migration_error(
            "本机数据库缺少 settings 表，已停止迁移（远端与本机均未改动）",
        ));
    }
    let remote = deserialize_private_state(remote_payload)?;

    let mut report = MigrationReport::default();
    if !writing_rows_present(&remote)? {
        return Ok(report);
    }
    let scope_filter = writing_scope_filter();

    // 2/3. 事务内逐表合并 + 核对；任何一张表失败都回滚，本机与远端保持原样。
    {
        let tx = desk.transaction()?;
        // 写作表之间存在外键（book_pages→books 等）；合并阶段允许按表分批写，
        // 到 COMMIT 时再统一检查，真正的孤儿行仍会失败回滚。
        tx.pragma_update(None, "defer_foreign_keys", "ON")?;
        for table in WRITING_TABLES {
            if exists_in_schema(&remote, "main", table)? {
                report
                    .tables
                    .push(merge_and_verify_table(&tx, &remote, table, None)?);
            }
        }
        for table in ["tags", "favorites"] {
            if exists_in_schema(&remote, "main", table)? {
                report.tables.push(merge_and_verify_table(
                    &tx,
                    &remote,
                    table,
                    Some(&scope_filter),
                )?);
            }
        }
        tx.commit()?;
    }

    // 4. 图片落盘并回读校验；冲突另存副本，不覆盖本机。
    if let Some(assets) = assets {
        report.assets = restore_assets_to_disk(data_dir, assets)?;
    }

    // 5. 恢复快照（纯写作表 + 原始密文）。
    report.snapshot = ensure_recovery_snapshot(data_dir, &remote, remote_ciphertext)?;

    // 6. 全部成功后才写迁移标记。
    mark_migration_completed(desk)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    // 两个临时 SQLite 库：本机 desk.db 与旧版远端载荷；不连接任何真实服务器。
    use super::*;
    use rusqlite::Connection;
    use std::path::PathBuf;

    const WRITING_SCHEMA: &str = "
        CREATE TABLE books(id TEXT PRIMARY KEY,title TEXT NOT NULL,deleted INTEGER NOT NULL DEFAULT 0,updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE TABLE book_pages(id TEXT PRIMARY KEY,book_id TEXT NOT NULL REFERENCES books(id),title TEXT NOT NULL,body TEXT NOT NULL DEFAULT '',revision INTEGER NOT NULL DEFAULT 1,deleted INTEGER NOT NULL DEFAULT 0,updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE TABLE blog_post_links(local_page_id TEXT PRIMARY KEY,remote_post_id TEXT NOT NULL UNIQUE);
        CREATE TABLE book_documents(document_path TEXT PRIMARY KEY,book_id TEXT NOT NULL REFERENCES books(id),position INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE book_page_drafts(id TEXT PRIMARY KEY,book_id TEXT NOT NULL,writer TEXT NOT NULL,page_json TEXT NOT NULL);
        CREATE TABLE book_page_versions(page_id TEXT NOT NULL,revision INTEGER NOT NULL,title TEXT NOT NULL,body TEXT NOT NULL,tags_json TEXT NOT NULL,PRIMARY KEY(page_id,revision));
        CREATE TABLE daily_essays(day TEXT PRIMARY KEY,topic_id TEXT NOT NULL,topic_json TEXT NOT NULL,page_id TEXT UNIQUE REFERENCES book_pages(id));
        CREATE TABLE documents(path TEXT PRIMARY KEY,format TEXT NOT NULL,title TEXT NOT NULL DEFAULT '',last_opened TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE TABLE writing_imports(source TEXT PRIMARY KEY,page_id TEXT NOT NULL);
        CREATE TABLE writing_logs(day TEXT PRIMARY KEY,page_id TEXT NOT NULL);
        CREATE TABLE tags(scope TEXT NOT NULL,name TEXT NOT NULL,PRIMARY KEY(scope,name));
        CREATE TABLE favorites(scope TEXT NOT NULL,id TEXT NOT NULL,PRIMARY KEY(scope,id));
    ";

    const ACCOUNT_SCHEMA: &str = "
        CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
        CREATE TABLE credentials(id TEXT PRIMARY KEY,title TEXT NOT NULL,username TEXT NOT NULL,password TEXT NOT NULL,website TEXT NOT NULL,notes TEXT NOT NULL,updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE TABLE legacy_credential_imports(id TEXT PRIMARY KEY);
    ";

    struct Fixture {
        directory: tempfile::TempDir,
        desk: Connection,
        remote: Connection,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let desk = Connection::open(directory.path().join("desk.db")).unwrap();
            desk.execute_batch(ACCOUNT_SCHEMA).unwrap();
            desk.execute_batch(WRITING_SCHEMA).unwrap();
            let remote = Connection::open(directory.path().join("remote.db")).unwrap();
            remote.execute_batch(ACCOUNT_SCHEMA).unwrap();
            remote.execute_batch(WRITING_SCHEMA).unwrap();
            Self {
                directory,
                desk,
                remote,
            }
        }

        fn payload(&self) -> Vec<u8> {
            self.remote.serialize(MAIN_DB).unwrap().to_vec()
        }

        fn data_dir(&self) -> PathBuf {
            self.directory.path().to_path_buf()
        }

        fn add_book(&self, connection: &Connection, id: &str, title: &str) {
            connection
                .execute(
                    "INSERT INTO main.books(id,title) VALUES(?1,?2)",
                    [id, title],
                )
                .unwrap();
        }

        fn migrate(
            &mut self,
            assets: Option<&BTreeMap<String, Vec<u8>>>,
        ) -> Result<MigrationReport> {
            let payload = self.payload();
            let data_dir = self.data_dir();
            migrate_writing_to_local(
                &mut self.desk,
                &payload,
                &data_dir,
                assets,
                Some(b"ciphertext-bytes"),
            )
        }
    }

    fn tiny_png() -> Vec<u8> {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            1,
            image::Rgba([7, 8, 9, 255]),
        ));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    fn local_count(connection: &Connection, table: &str) -> i64 {
        table_row_count(connection, "main", table, None).unwrap()
    }

    fn marker(desk: &Connection) -> Option<String> {
        desk.query_row(
            "SELECT value FROM main.settings WHERE key=?1",
            [MIGRATION_MARKER],
            |row| row.get(0),
        )
        .optional()
        .unwrap()
    }

    fn recovery_files(data_dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let backups = data_dir.join("backups");
        if !backups.is_dir() {
            return files;
        }
        for entry in fs::read_dir(backups).unwrap() {
            files.push(entry.unwrap().path());
        }
        files.sort();
        files
    }

    #[test]
    fn migrates_writing_rows_images_and_recovery_snapshot() {
        let mut fixture = Fixture::new();
        fixture.add_book(&fixture.remote, "book-1", "远端图书");
        fixture
            .remote
            .execute(
                "INSERT INTO main.book_pages(id,book_id,title,body) VALUES('page-1','book-1','页面','正文')",
                [],
            )
            .unwrap();
        fixture
            .remote
            .execute(
                "INSERT INTO main.tags(scope,name) VALUES('books','写作标签'),('credentials','账号标签')",
                [],
            )
            .unwrap();
        fixture
            .remote
            .execute(
                "INSERT INTO main.credentials(id,title,username,password,website,notes) VALUES('c1','账号','u','p','w','n')",
                [],
            )
            .unwrap();
        let png = tiny_png();
        let assets = BTreeMap::from([
            ("writing-assets/remote.png".to_owned(), png.clone()),
            (
                "data/picture/00000000-0000-4000-8000-000000000000/remote.png".to_owned(),
                png.clone(),
            ),
        ]);

        let report = fixture.migrate(Some(&assets)).unwrap();

        // 写作行落到本机，账号行没有进本机 main。
        assert_eq!(local_count(&fixture.desk, "books"), 1);
        assert_eq!(local_count(&fixture.desk, "book_pages"), 1);
        assert_eq!(local_count(&fixture.desk, "credentials"), 0);
        assert_eq!(local_count(&fixture.desk, "tags"), 1);
        assert!(!report.tables.is_empty());
        assert!(report.summary().contains("books"));

        // 图片落盘且字节一致。
        for (relative, expected) in &assets {
            let path = fixture.data_dir().join(relative);
            assert_eq!(&fs::read(&path).unwrap(), expected);
        }
        assert_eq!(report.assets.written, 2);

        // 快照：一个明文 .db + 一个原始密文；明文库不含账号表。
        let backups = fixture.data_dir().join("backups");
        let database = existing_recovery_database(&backups).unwrap().unwrap();
        let payload = backups.join(format!(
            "{}{RECOVERY_PAYLOAD_SUFFIX}",
            database.file_stem().unwrap().to_string_lossy()
        ));
        assert_eq!(fs::read(&payload).unwrap(), b"ciphertext-bytes");
        let snapshot = deserialize_private_state(&fs::read(&database).unwrap()).unwrap();
        assert_eq!(local_count(&snapshot, "books"), 1);
        assert!(!exists_in_schema(&snapshot, "main", "credentials").unwrap());

        assert!(migration_completed(&fixture.desk).unwrap());
        assert_eq!(marker(&fixture.desk).as_deref(), Some("1"));
        // 远端原样，没有被迁移模块改动。
        assert_eq!(local_count(&fixture.remote, "books"), 1);
        assert_eq!(local_count(&fixture.remote, "credentials"), 1);
    }

    #[test]
    fn repeated_migration_is_idempotent() {
        let mut fixture = Fixture::new();
        fixture.add_book(&fixture.remote, "book-1", "远端图书");
        fixture
            .remote
            .execute(
                "INSERT INTO main.tags(scope,name) VALUES('documents','文档标签')",
                [],
            )
            .unwrap();
        let png = tiny_png();
        let assets = BTreeMap::from([("writing-assets/a.png".to_owned(), png)]);

        let first = fixture.migrate(Some(&assets)).unwrap();
        assert_eq!(first.assets.written, 1);
        let snapshot_after_first = recovery_files(&fixture.data_dir());
        assert_eq!(local_count(&fixture.desk, "books"), 1);

        let second = fixture.migrate(Some(&assets)).unwrap();
        assert_eq!(local_count(&fixture.desk, "books"), 1);
        assert_eq!(local_count(&fixture.desk, "tags"), 1);
        let all_inserted: usize = second.tables.iter().map(|stats| stats.inserted).sum();
        assert_eq!(all_inserted, 0, "重复迁移不应重复插入任何行");
        assert_eq!(second.assets.written, 0);
        assert_eq!(second.assets.skipped, 1);
        // 不堆积新的快照文件。
        assert_eq!(recovery_files(&fixture.data_dir()), snapshot_after_first);
        assert!(migration_completed(&fixture.desk).unwrap());
    }

    #[test]
    fn conflicting_row_content_is_rejected_without_touching_either_side() {
        let mut fixture = Fixture::new();
        // 本机已有同主键行，内容与远端不同：必须拒绝、保留本机内容、不写远端。
        fixture.add_book(&fixture.desk, "book-1", "本机版本");
        fixture.add_book(&fixture.remote, "book-1", "远端版本");
        fixture.add_book(&fixture.remote, "book-2", "远端第二本");

        let error = fixture.migrate(None).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("核对失败"), "实际错误：{message}");
        assert!(message.contains("books"), "实际错误：{message}");

        // 本机：原行内容不变，第二本没有插入（事务回滚）。
        assert_eq!(
            fixture
                .desk
                .query_row(
                    "SELECT title FROM main.books WHERE id='book-1'",
                    [],
                    |row| { row.get::<_, String>(0) }
                )
                .unwrap(),
            "本机版本"
        );
        assert_eq!(local_count(&fixture.desk, "books"), 1);
        assert!(!migration_completed(&fixture.desk).unwrap());
        assert!(recovery_files(&fixture.data_dir()).is_empty());
        // 远端：原样保留。
        assert_eq!(local_count(&fixture.remote, "books"), 2);
        assert_eq!(
            fixture
                .remote
                .query_row(
                    "SELECT title FROM main.books WHERE id='book-1'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "远端版本"
        );
    }

    #[test]
    fn keeps_local_writing_rows_and_fills_missing_remote_rows() {
        let mut fixture = Fixture::new();
        fixture.add_book(&fixture.desk, "local-only", "本机新增");
        fixture.add_book(&fixture.remote, "remote-1", "远端一");
        fixture.add_book(&fixture.remote, "remote-2", "远端二");
        fixture
            .desk
            .execute(
                "INSERT INTO main.documents(path,format,title) VALUES('D:/notes/local.md','md','本机文档')",
                [],
            )
            .unwrap();

        fixture.migrate(None).unwrap();

        assert_eq!(local_count(&fixture.desk, "books"), 3);
        assert_eq!(local_count(&fixture.desk, "documents"), 1);
        let local_title: String = fixture
            .desk
            .query_row(
                "SELECT title FROM main.books WHERE id='local-only'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(local_title, "本机新增");
        assert_eq!(local_count(&fixture.desk, "credentials"), 0);
    }

    #[test]
    fn asset_conflict_keeps_local_copy_and_saves_remote_copy_beside_it() {
        let mut fixture = Fixture::new();
        fixture.add_book(&fixture.remote, "book-1", "远端图书");
        let local_bytes = tiny_png();
        let remote_bytes = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([1, 2, 3, 255]),
        ));
        let mut cursor = std::io::Cursor::new(Vec::new());
        remote_bytes
            .write_to(&mut cursor, image::ImageFormat::Png)
            .unwrap();
        let remote_bytes = cursor.into_inner();

        let relative = "writing-assets/same-name.png";
        let local_path = fixture.data_dir().join(relative);
        fs::create_dir_all(local_path.parent().unwrap()).unwrap();
        fs::write(&local_path, &local_bytes).unwrap();

        let assets = BTreeMap::from([(relative.to_owned(), remote_bytes.clone())]);
        let report = fixture.migrate(Some(&assets)).unwrap();

        // 本机原文件保持不变，远端图片另存副本。
        assert_eq!(fs::read(&local_path).unwrap(), local_bytes);
        assert_eq!(report.assets.conflicts.len(), 1);
        let restored = fs::read_dir(local_path.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains("remote-restored-")
            })
            .expect("应保存远端图片副本");
        assert_eq!(fs::read(restored).unwrap(), remote_bytes);
    }

    #[test]
    fn account_snapshot_and_serialize_never_include_writing_tables() {
        let fixture = Fixture::new();
        fixture.add_book(&fixture.desk, "book-1", "本机图书");
        fixture
            .desk
            .execute(
                "INSERT INTO main.credentials(id,title,username,password,website,notes) VALUES('c1','账号','u','p','w','n')",
                [],
            )
            .unwrap();
        fixture
            .desk
            .execute(
                "INSERT INTO main.tags(scope,name) VALUES('credentials','账号标签'),('books','写作标签')",
                [],
            )
            .unwrap();

        let payload = serialize_private_state(&fixture.desk).unwrap();
        let private = deserialize_private_state(&payload).unwrap();
        assert_eq!(local_count(&private, "credentials"), 1);
        assert!(!exists_in_schema(&private, "main", "books").unwrap());
        assert_eq!(
            table_row_count(&private, "main", "tags", Some("scope='credentials'")).unwrap(),
            1
        );

        let account = account_snapshot(&private).unwrap();
        assert_eq!(local_count(&account, "credentials"), 1);
        assert_eq!(local_count(&account, "tags"), 1);
    }

    #[test]
    fn serialize_reads_attached_account_tables_instead_of_empty_local_copy() {
        let mut fixture = Fixture::new();
        // 远端载荷里有 2 条账号；挂载后写入走 temp，本机 main.credentials 会被清空。
        for (id, title) in [("a", "远端账号一"), ("b", "远端账号二")] {
            fixture
                .remote
                .execute(
                    "INSERT INTO main.credentials(id,title,username,password,website,notes) VALUES(?1,?2,'u','p','w','n')",
                    [id, title],
                )
                .unwrap();
        }
        let remote = deserialize_private_state(&fixture.payload()).unwrap();
        attach_account_tables(&mut fixture.desk, &remote).unwrap();
        clear_local_private_rows(&mut fixture.desk, false).unwrap();
        assert_eq!(local_count(&fixture.desk, "credentials"), 0);

        // 同步上传的快照必须来自已挂载的 temp 账号表，否则会把服务器账号清空。
        let payload = serialize_private_state(&fixture.desk).unwrap();
        let snapshot = deserialize_private_state(&payload).unwrap();
        assert_eq!(
            local_count(&snapshot, "credentials"),
            2,
            "同步序列化必须读取 temp 账号表，而不是空的 main.credentials"
        );
    }

    #[test]
    fn clear_local_private_rows_only_removes_account_rows() {
        let mut fixture = Fixture::new();
        fixture.add_book(&fixture.desk, "book-1", "本机图书");
        fixture
            .desk
            .execute(
                "INSERT INTO main.credentials(id,title,username,password,website,notes) VALUES('c1','账号','u','p','w','n')",
                [],
            )
            .unwrap();
        fixture
            .desk
            .execute(
                "INSERT INTO main.tags(scope,name) VALUES('credentials','账号标签'),('books','写作标签')",
                [],
            )
            .unwrap();

        clear_local_private_rows(&mut fixture.desk, false).unwrap();

        assert_eq!(local_count(&fixture.desk, "credentials"), 0);
        assert_eq!(local_count(&fixture.desk, "books"), 1);
        assert_eq!(local_count(&fixture.desk, "tags"), 1);
        assert_eq!(
            fixture
                .desk
                .query_row("SELECT scope FROM main.tags", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "books"
        );
    }

    #[test]
    fn account_boundary_helpers_only_touch_account_data() {
        let mut fixture = Fixture::new();
        fixture.add_book(&fixture.desk, "book-1", "本机图书");
        let remote_state = deserialize_private_state(&fixture.payload()).unwrap();
        // 两边账号都为空时一致；写作行不算账号资料。
        assert!(account_state_matches(&fixture.desk, &remote_state).unwrap());
        assert!(!private_rows_exist(&fixture.desk).unwrap());

        fixture
            .desk
            .execute(
                "INSERT INTO main.credentials(id,title,username,password,website,notes) VALUES('c1','账号','u','p','w','n')",
                [],
            )
            .unwrap();
        assert!(private_rows_exist(&fixture.desk).unwrap());
        // 远端没有账号行、本机有：启动时必须判定不一致，不能静默覆盖。
        assert!(!account_state_matches(&fixture.desk, &remote_state).unwrap());

        // 账号表挂到 temp；写作表绝不复制到 temp。
        attach_account_tables(&mut fixture.desk, &remote_state).unwrap();
        assert!(exists_in_schema(&fixture.desk, "temp", "credentials").unwrap());
        assert!(exists_in_schema(&fixture.desk, "temp", "private_tags").unwrap());
        assert!(!exists_in_schema(&fixture.desk, "temp", "books").unwrap());
        assert!(!exists_in_schema(&fixture.desk, "temp", "documents").unwrap());
    }

    #[test]
    fn rejects_unknown_writing_asset_path() {
        let encoded = serde_json::to_vec(&BTreeMap::from([(
            "../outside.png".to_owned(),
            BASE64.encode(b"x"),
        )]))
        .unwrap();
        assert!(decode_assets(&encoded).is_err());
    }
}
