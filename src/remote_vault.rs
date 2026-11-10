//! Server-only storage for passwords and the writing library.
//!
//! The remote MySQL table contains age-encrypted SQLite snapshots. A dedicated
//! X25519 identity is protected with the current Windows user's DPAPI context.
//! No plaintext record or attachment is sent over SSH.

use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use age::{secrecy::ExposeSecret, x25519::Identity};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use mysql::prelude::Queryable;
use rusqlite::{Connection, MAIN_DB, OptionalExtension, params_from_iter, types::Value};
use zeroize::{Zeroize, Zeroizing};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::{
    crypto,
    error::{DeskError, Result},
    vault_config::VaultMysqlConfig,
    writing_images,
};

const REMOTE_HOST: &str = "49.234.49.192";
const REMOTE_USER: &str = "ubuntu";
const REMOTE_PORT: &str = "22";
const MYSQL_DATABASE: &str = "personaldesk_vault";
const MYSQL_TABLE: &str = "sealed_payloads";
const OBJECT_PRIVATE: &str = "private";
const OBJECT_ASSETS: &str = "assets";
const IDENTITY_FILE: &str = "PersonalDesk\\private-writing-identity.dpapi";
const REMOTE_MYSQL_COMMAND: &str = "sudo -n docker exec -i ap-mysql sh -lc 'mysql --protocol=socket -uroot -p\"$MYSQL_ROOT_PASSWORD\" --batch --skip-column-names --raw'";

enum VaultTransport {
    Direct(VaultMysqlConfig),
    Ssh,
}

impl VaultTransport {
    fn load() -> Result<Self> {
        Ok(match VaultMysqlConfig::load()? {
            Some(config) => Self::Direct(config),
            None => Self::Ssh,
        })
    }
}

const PRIVATE_TABLES: &[&str] = &[
    "credentials",
    "books",
    "book_pages",
    "blog_post_links",
    "book_documents",
    "book_page_drafts",
    "book_page_versions",
    "daily_essays",
    "documents",
    "favorites",
    "legacy_credential_imports",
    "tags",
    "writing_imports",
    "writing_logs",
];

const PRIVATE_SCOPES: &[&str] = &["books", "book_pages", "credentials", "documents"];

const PRIVATE_TEMP_TABLES: &[&str] = &[
    "credentials",
    "books",
    "book_pages",
    "blog_post_links",
    "book_documents",
    "book_page_drafts",
    "book_page_versions",
    "daily_essays",
    "writing_imports",
    "writing_logs",
    "legacy_credential_imports",
];

pub struct RemoteVault {
    identity: Identity,
    data_dir: PathBuf,
    private_revision: u64,
    assets_revision: u64,
    dirty: Arc<AtomicBool>,
    last_change: Arc<Mutex<Instant>>,
    retry_after: Instant,
}

impl RemoteVault {
    pub fn open_and_attach(desk: &mut Connection, data_dir: &Path) -> Result<Self> {
        let transport = VaultTransport::load()?;
        ensure_remote_schema(&transport)?;
        let remote_private = fetch_remote_payload(&transport, OBJECT_PRIVATE)?;
        let remote_assets = fetch_remote_payload(&transport, OBJECT_ASSETS)?;
        let identity =
            load_or_create_identity(remote_private.is_some() || remote_assets.is_some())?;

        let local_private = Zeroizing::new(serialize_private_state(desk)?);
        let local_has_private_rows = private_rows_exist(desk)?;
        let (private_plain, private_revision) = if let Some((revision, ciphertext)) = remote_private
        {
            let plaintext = decrypt(&identity, &ciphertext)?;
            if local_has_private_rows && plaintext.as_slice() != local_private.as_slice() {
                return Err(DeskError::InvalidPath(
                    "服务器和本机的账号或写作资料不一致；已保留两边数据，没有覆盖。".into(),
                ));
            }
            (plaintext, revision)
        } else {
            let ciphertext = encrypt(&identity, &local_private)?;
            store_remote_payload(&transport, OBJECT_PRIVATE, None, &ciphertext)?;
            let (revision, verified) = fetch_remote_payload(&transport, OBJECT_PRIVATE)?
                .ok_or_else(|| remote_error("服务器没有返回已保存的账号和文档资料"))?;
            let verified = decrypt(&identity, &verified)?;
            if verified.as_slice() != local_private.as_slice() {
                return Err(remote_error("账号和文档资料上传后核对不一致"));
            }
            (verified, revision)
        };

        let mut local_assets = assets_from_disk(data_dir)?;
        let local_assets_plain = Zeroizing::new(encode_assets(&local_assets)?);
        let (assets_plain, assets_revision) = if let Some((revision, ciphertext)) = remote_assets {
            let plaintext = decrypt(&identity, &ciphertext)?;
            if !local_assets.is_empty() && plaintext.as_slice() != local_assets_plain.as_slice() {
                return Err(DeskError::InvalidPath(
                    "服务器和本机的写作图片不一致；已保留两边资料，没有覆盖。".into(),
                ));
            }
            (plaintext, revision)
        } else {
            let ciphertext = encrypt(&identity, &local_assets_plain)?;
            store_remote_payload(&transport, OBJECT_ASSETS, None, &ciphertext)?;
            let (revision, verified) = fetch_remote_payload(&transport, OBJECT_ASSETS)?
                .ok_or_else(|| remote_error("服务器没有返回已保存的写作图片"))?;
            let verified = decrypt(&identity, &verified)?;
            if verified.as_slice() != local_assets_plain.as_slice() {
                return Err(remote_error("写作图片上传后核对不一致"));
            }
            (verified, revision)
        };

        for bytes in local_assets.values_mut() {
            bytes.zeroize();
        }

        let private_db = deserialize_private_state(&private_plain)?;
        attach_private_tables(desk, &private_db)?;
        writing_images::enable_remote_cache(data_dir, decode_assets(&assets_plain)?)?;
        let managed_files = managed_writing_files(desk, data_dir)?;
        sanitize_local_backups(data_dir)?;
        desk.execute(
            "INSERT INTO main.settings(key,value) VALUES('remote-vault.legacy-import-done','1') ON CONFLICT(key) DO UPDATE SET value='1'",
            [],
        )?;
        if private_rows_exist(desk)? {
            clear_local_private_rows(desk, &managed_files, true)?;
        }
        clear_local_writing_files(data_dir, &managed_files)?;

        let dirty = Arc::new(AtomicBool::new(false));
        let last_change = Arc::new(Mutex::new(Instant::now()));
        let hook_dirty = Arc::clone(&dirty);
        let hook_last_change = Arc::clone(&last_change);
        desk.update_hook(Some(
            move |_action: rusqlite::hooks::Action, database: &str, table: &str, _rowid: i64| {
                if database == "temp"
                    && (PRIVATE_TEMP_TABLES.contains(&table)
                        || table == "private_tags"
                        || table == "private_favorites"
                        || table == "private_documents")
                {
                    hook_dirty.store(true, Ordering::Release);
                    if let Ok(mut last_change) = hook_last_change.lock() {
                        *last_change = Instant::now();
                    }
                }
            },
        ))?;

        Ok(Self {
            identity,
            data_dir: data_dir.to_path_buf(),
            private_revision,
            assets_revision,
            dirty,
            last_change,
            retry_after: Instant::now(),
        })
    }

    pub fn sync_if_due(&mut self, desk: &mut Connection, force: bool) -> Result<bool> {
        let private_dirty = self.dirty.load(Ordering::Acquire);
        let assets_dirty = writing_images::remote_assets_dirty(&self.data_dir);
        if !private_dirty && !assets_dirty {
            return Ok(false);
        }
        let now = Instant::now();
        if !force && now < self.retry_after {
            return Ok(false);
        }
        if !force
            && private_dirty
            && self
                .last_change
                .lock()
                .map(|last| last.elapsed() < Duration::from_millis(1200))
                .unwrap_or(false)
        {
            return Ok(false);
        }

        let result = self.sync_pending(desk, private_dirty, assets_dirty);
        match result {
            Ok(changed) => {
                self.retry_after = Instant::now();
                Ok(changed)
            }
            Err(error) => {
                self.retry_after = Instant::now() + Duration::from_secs(5);
                Err(error)
            }
        }
    }

    pub fn has_pending_changes(&self) -> bool {
        self.dirty.load(Ordering::Acquire) || writing_images::remote_assets_dirty(&self.data_dir)
    }

    fn sync_pending(
        &mut self,
        desk: &mut Connection,
        private_dirty: bool,
        assets_dirty: bool,
    ) -> Result<bool> {
        let transport = VaultTransport::load()?;
        if private_dirty {
            let plaintext = Zeroizing::new(serialize_private_state(desk)?);
            let ciphertext = encrypt(&self.identity, &plaintext)?;
            self.private_revision = store_remote_payload(
                &transport,
                OBJECT_PRIVATE,
                Some(self.private_revision),
                &ciphertext,
            )?;
            self.dirty.store(false, Ordering::Release);
        }
        if assets_dirty {
            let Some(mut assets) = writing_images::remote_assets_snapshot(&self.data_dir) else {
                return Err(remote_error("写作图片缓存暂时不可用"));
            };
            let plaintext = Zeroizing::new(encode_assets(&assets)?);
            let ciphertext = encrypt(&self.identity, &plaintext)?;
            for bytes in assets.values_mut() {
                bytes.zeroize();
            }
            self.assets_revision = store_remote_payload(
                &transport,
                OBJECT_ASSETS,
                Some(self.assets_revision),
                &ciphertext,
            )?;
            writing_images::mark_remote_assets_synced(&self.data_dir);
        }
        Ok(private_dirty || assets_dirty)
    }
}

fn remote_error(message: &str) -> DeskError {
    DeskError::InvalidPath(format!("个人服务器加密资料库：{message}"))
}

fn identity_path() -> Result<PathBuf> {
    let app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| remote_error("无法定位本机私钥目录"))?;
    Ok(app_data.join(IDENTITY_FILE))
}

fn load_or_create_identity(remote_has_data: bool) -> Result<Identity> {
    let path = identity_path()?;
    if path.is_file() {
        let protected = std::fs::read(path)?;
        let secret = crypto::unprotect_local_secret(&protected)?;
        let text =
            std::str::from_utf8(&secret).map_err(|_| remote_error("本机私钥文件无法读取"))?;
        return Identity::from_str(text).map_err(|_| remote_error("本机私钥文件已损坏"));
    }
    if remote_has_data {
        return Err(remote_error(
            "服务器已有密文，但本机解密私钥缺失；没有生成新钥匙或覆盖服务器数据。",
        ));
    }
    let identity = Identity::generate();
    let secret = identity.to_string();
    let protected = crypto::protect_local_secret(secret.expose_secret().as_bytes())?;
    let path = identity_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| remote_error("无法定位本机私钥目录"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = path.with_extension("new");
    std::fs::write(&temporary, protected)?;
    std::fs::rename(temporary, path)?;
    Ok(identity)
}

fn encrypt(identity: &Identity, plaintext: &[u8]) -> Result<Vec<u8>> {
    age::encrypt(&identity.to_public(), plaintext).map_err(|_| DeskError::Crypto)
}

fn decrypt(identity: &Identity, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    age::decrypt(identity, ciphertext)
        .map(Zeroizing::new)
        .map_err(|_| remote_error("服务器密文无法由本机私钥解开；没有覆盖服务器数据。"))
}

fn ensure_remote_schema(transport: &VaultTransport) -> Result<()> {
    match transport {
        VaultTransport::Direct(config) => {
            let mut connection = config.connect()?;
            let exists: Option<u8> = connection
                .exec_first(
                    "SELECT 1 FROM information_schema.tables WHERE table_schema=? AND table_name=? LIMIT 1",
                    (config.database(), MYSQL_TABLE),
                )
                .map_err(mysql_error)?;
            if exists.is_none() {
                connection
                    .query_drop(format!(
                        "CREATE TABLE IF NOT EXISTS `{}`.`{MYSQL_TABLE}` (object_id VARCHAR(24) NOT NULL PRIMARY KEY, revision BIGINT UNSIGNED NOT NULL DEFAULT 1, ciphertext LONGBLOB NOT NULL, updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP) ENGINE=InnoDB",
                        config.database()
                    ))
                    .map_err(mysql_error)?;
            }
            Ok(())
        }
        VaultTransport::Ssh => {
            run_mysql_ssh(&format!(
                "CREATE DATABASE IF NOT EXISTS `{MYSQL_DATABASE}` CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;\nCREATE TABLE IF NOT EXISTS `{MYSQL_DATABASE}`.`{MYSQL_TABLE}` (object_id VARCHAR(24) NOT NULL PRIMARY KEY, revision BIGINT UNSIGNED NOT NULL DEFAULT 1, ciphertext LONGBLOB NOT NULL, updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP) ENGINE=InnoDB;\nSELECT 1;\n"
            ))?;
            Ok(())
        }
    }
}

fn fetch_remote_payload(
    transport: &VaultTransport,
    object_id: &str,
) -> Result<Option<(u64, Vec<u8>)>> {
    if object_id != OBJECT_PRIVATE && object_id != OBJECT_ASSETS {
        return Err(remote_error("资料类型无效"));
    }
    match transport {
        VaultTransport::Direct(config) => {
            let mut connection = config.connect()?;
            connection
                .exec_first(
                    format!(
                        "SELECT revision,ciphertext FROM `{}`.`{MYSQL_TABLE}` WHERE object_id=? LIMIT 1",
                        config.database()
                    ),
                    (object_id,),
                )
                .map_err(mysql_error)
        }
        VaultTransport::Ssh => fetch_remote_payload_ssh(object_id),
    }
}

fn fetch_remote_payload_ssh(object_id: &str) -> Result<Option<(u64, Vec<u8>)>> {
    let output = run_mysql_ssh(&format!(
        "SELECT revision,TO_BASE64(ciphertext) FROM `{MYSQL_DATABASE}`.`{MYSQL_TABLE}` WHERE object_id='{object_id}';\n"
    ))?;
    let line = output.trim();
    if line.is_empty() {
        return Ok(None);
    }
    let (revision, encoded) = line
        .split_once('\t')
        .ok_or_else(|| remote_error("服务器返回的密文格式无效"))?;
    let revision = revision
        .parse::<u64>()
        .map_err(|_| remote_error("服务器密文版本号无效"))?;
    let encoded = encoded
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    BASE64
        .decode(encoded)
        .map(|ciphertext| Some((revision, ciphertext)))
        .map_err(|_| remote_error("服务器返回的密文编码无效"))
}

fn store_remote_payload(
    transport: &VaultTransport,
    object_id: &str,
    expected_revision: Option<u64>,
    ciphertext: &[u8],
) -> Result<u64> {
    if object_id != OBJECT_PRIVATE && object_id != OBJECT_ASSETS {
        return Err(remote_error("资料类型无效"));
    }
    if let VaultTransport::Direct(config) = transport {
        let mut connection = config.connect()?;
        match expected_revision {
            None => connection
                .exec_drop(
                    format!(
                        "INSERT IGNORE INTO `{}`.`{MYSQL_TABLE}` (object_id,revision,ciphertext) VALUES (?,1,?)",
                        config.database()
                    ),
                    (object_id, ciphertext),
                )
                .map_err(mysql_error)?,
            Some(revision) => connection
                .exec_drop(
                    format!(
                        "UPDATE `{}`.`{MYSQL_TABLE}` SET ciphertext=?,revision=revision+1 WHERE object_id=? AND revision=?",
                        config.database()
                    ),
                    (ciphertext, object_id, revision),
                )
                .map_err(mysql_error)?,
        }
        if connection.affected_rows() != 1 {
            return Err(remote_error(
                "服务器资料已被另一台程序更新；本次改动仍保留在当前运行内存中，请先重新打开程序同步。",
            ));
        }
        return Ok(expected_revision.unwrap_or(0) + 1);
    }
    let encoded = BASE64.encode(ciphertext);
    let sql = match expected_revision {
        None => format!(
            "INSERT IGNORE INTO `{MYSQL_DATABASE}`.`{MYSQL_TABLE}` (object_id,revision,ciphertext) VALUES ('{object_id}',1,FROM_BASE64('{encoded}'));\nSELECT ROW_COUNT();\n"
        ),
        Some(revision) => format!(
            "UPDATE `{MYSQL_DATABASE}`.`{MYSQL_TABLE}` SET revision=revision+1,ciphertext=FROM_BASE64('{encoded}') WHERE object_id='{object_id}' AND revision={revision};\nSELECT ROW_COUNT();\n"
        ),
    };
    let output = run_mysql_ssh(&sql)?;
    if output.trim().parse::<u64>().ok() != Some(1) {
        return Err(remote_error(
            "服务器资料已被另一台程序更新；本次改动仍保留在当前运行内存中，请先重新打开程序同步。",
        ));
    }
    Ok(expected_revision.unwrap_or(0) + 1)
}

fn mysql_error(error: mysql::Error) -> DeskError {
    remote_error(&format!("直连 MySQL 请求失败：{error}"))
}

fn run_mysql_ssh(sql: &str) -> Result<String> {
    let ssh = ssh_executable()?;
    let user_profile = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| remote_error("无法定位 SSH 私钥目录"))?;
    let key = user_profile.join(".ssh").join("server_management_ed25519");
    if !key.is_file() {
        return Err(remote_error("没有找到服务器 SSH 登录私钥"));
    }
    let mut child = Command::new(ssh)
        .args([
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "ConnectTimeout=10",
            "-i",
        ])
        .arg(key)
        .args(["-p", REMOTE_PORT])
        .arg(format!("{REMOTE_USER}@{REMOTE_HOST}"))
        .arg(REMOTE_MYSQL_COMMAND)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| remote_error("无法启动 SSH 客户端"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| remote_error("无法读取服务器查询结果"))?;
    let stdout_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.read_to_end(&mut bytes);
        (bytes, result)
    });
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| remote_error("无法读取服务器错误信息"))?;
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stderr.read_to_end(&mut bytes);
        (bytes, result)
    });
    let write_result = child
        .stdin
        .take()
        .ok_or_else(|| remote_error("无法向服务器提交数据库语句"))?
        .write_all(sql.as_bytes());
    let status = child.wait()?;
    let (stdout, stdout_result) = stdout_reader
        .join()
        .map_err(|_| remote_error("无法读取服务器查询结果"))?;
    stdout_result?;
    let (stderr, stderr_result) = stderr_reader
        .join()
        .map_err(|_| remote_error("无法读取服务器错误信息"))?;
    stderr_result?;
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr);
        let detail = detail.trim();
        return Err(remote_error(if detail.is_empty() {
            "SSH 或 MySQL 命令失败"
        } else {
            detail
        }));
    }
    if let Err(error) = write_result {
        return Err(remote_error(&format!(
            "无法向服务器提交数据库语句：{error}"
        )));
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

fn ssh_executable() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        let windows = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .ok_or_else(|| remote_error("无法定位 Windows SSH 客户端"))?;
        let path = windows.join("System32\\OpenSSH\\ssh.exe");
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(remote_error("没有找到 Windows OpenSSH 客户端"))
}

fn serialize_private_state(source: &Connection) -> Result<Vec<u8>> {
    let private = Connection::open_in_memory()?;
    private.pragma_update(None, "foreign_keys", "OFF")?;
    for table in PRIVATE_TABLES {
        let source_table = match *table {
            "tags" => "private_tags",
            "favorites" => "private_favorites",
            "documents" => "private_documents",
            _ => table,
        };
        let schema = if exists_in_schema(source, "temp", source_table)? {
            "temp"
        } else {
            "main"
        };
        let filter = match *table {
            "tags" => Some("scope IN ('books','book_pages','credentials','documents')"),
            "favorites" => Some("scope IN ('books','book_pages','credentials','documents')"),
            "documents" => Some("path IN (SELECT document_path FROM book_documents)"),
            _ => None,
        };
        copy_table(
            source,
            schema,
            source_table,
            &private,
            "main",
            table,
            filter,
        )?;
    }
    let data = private.serialize(MAIN_DB)?;
    Ok(data.to_vec())
}

fn deserialize_private_state(bytes: &[u8]) -> Result<Connection> {
    let mut connection = Connection::open_in_memory()?;
    connection.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), false)?;
    connection.pragma_update(None, "foreign_keys", "OFF")?;
    Ok(connection)
}

fn exists_in_schema(connection: &Connection, schema: &str, table: &str) -> Result<bool> {
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM {schema}.sqlite_master WHERE type='table' AND name=?1)"
    );
    Ok(connection.query_row(&sql, [table], |row| row.get(0))?)
}

fn copy_table(
    source: &Connection,
    source_schema: &str,
    source_table: &str,
    destination: &Connection,
    destination_schema: &str,
    destination_table: &str,
    filter: Option<&str>,
) -> Result<()> {
    let source_sql =
        format!("SELECT sql FROM {source_schema}.sqlite_master WHERE type='table' AND name=?1");
    let schema_sql: Option<String> = source
        .query_row(&source_sql, [source_table], |row| row.get(0))
        .optional()?;
    let Some(schema_sql) = schema_sql else {
        return Ok(());
    };
    if !exists_in_schema(destination, destination_schema, destination_table)? {
        let create_sql = rename_create_table(&schema_sql, source_table, destination_table)?;
        destination.execute_batch(&create_sql)?;
    }
    let pragma = format!("PRAGMA {source_schema}.table_info(\"{source_table}\")");
    let columns = source
        .prepare(&pragma)?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if columns.is_empty() {
        return Ok(());
    }
    let quoted_columns = columns
        .iter()
        .map(|column| format!("\"{}\"", column.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(",");
    let where_sql = filter
        .map(|value| format!(" WHERE {value}"))
        .unwrap_or_default();
    let select_sql = format!(
        "SELECT {quoted_columns} FROM {source_schema}.\"{source_table}\"{where_sql} ORDER BY rowid"
    );
    let placeholders = (1..=columns.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let insert_sql = format!(
        "INSERT INTO {destination_schema}.\"{destination_table}\" ({quoted_columns}) VALUES ({placeholders})"
    );
    let mut statement = source.prepare(&select_sql)?;
    let column_count = columns.len();
    let rows = statement.query_map([], |row| {
        (0..column_count)
            .map(|index| row.get::<_, Value>(index))
            .collect::<std::result::Result<Vec<_>, _>>()
    })?;
    for row in rows {
        let values = row?;
        destination.execute(&insert_sql, params_from_iter(values.iter()))?;
    }
    Ok(())
}

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
    Err(remote_error("本地资料表结构无法安全复制"))
}

fn private_rows_exist(connection: &Connection) -> Result<bool> {
    for table in [
        "credentials",
        "books",
        "book_pages",
        "blog_post_links",
        "book_documents",
        "book_page_drafts",
        "book_page_versions",
        "writing_imports",
        "writing_logs",
        "legacy_credential_imports",
        "daily_essays",
    ] {
        if !exists_in_schema(connection, "main", table)? {
            continue;
        }
        let exists = connection.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM main.\"{table}\" LIMIT 1)"),
            [],
            |row| row.get::<_, bool>(0),
        )?;
        if exists {
            return Ok(true);
        }
    }
    if exists_in_schema(connection, "main", "documents")?
        && exists_in_schema(connection, "main", "book_documents")?
        && connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM main.documents WHERE path IN (SELECT document_path FROM main.book_documents))",
            [],
            |row| row.get::<_, bool>(0),
        )?
    {
        return Ok(true);
    }
    for table in ["favorites", "tags"] {
        if exists_in_schema(connection, "main", table)?
            && connection.query_row(
                &format!(
                    "SELECT EXISTS(SELECT 1 FROM main.\"{table}\" WHERE scope IN ('books','book_pages','credentials','documents'))"
                ),
                [],
                |row| row.get::<_, bool>(0),
            )?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn attach_private_tables(desk: &mut Connection, private: &Connection) -> Result<()> {
    for table in PRIVATE_TEMP_TABLES {
        create_temp_copy(desk, private, table, table)?;
    }
    create_temp_copy(desk, private, "tags", "private_tags")?;
    create_temp_copy(desk, private, "favorites", "private_favorites")?;
    create_temp_copy(desk, private, "documents", "private_documents")?;
    desk.execute_batch(
        "CREATE TEMP VIEW IF NOT EXISTS live_books AS
         WITH RECURSIVE visible(id) AS (
             SELECT id FROM books WHERE parent_id IS NULL AND deleted=0
             UNION SELECT b.id FROM books b JOIN visible v ON b.parent_id=v.id WHERE b.deleted=0
         ) SELECT b.rowid AS rowid,b.* FROM books b JOIN visible v ON b.id=v.id;",
    )?;
    Ok(())
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
    if matches!(
        destination_table,
        "book_pages" | "book_documents" | "daily_essays"
    ) {
        schema_sql = schema_sql
            .replace(" REFERENCES books(id)", "")
            .replace(" REFERENCES book_pages(id)", "");
    }
    destination.execute_batch(&schema_sql)?;
    copy_table(
        source,
        "main",
        source_table,
        destination,
        "temp",
        destination_table,
        None,
    )?;
    Ok(())
}

fn clear_local_private_rows(
    desk: &mut Connection,
    managed_files: &[PathBuf],
    vacuum_file: bool,
) -> Result<()> {
    desk.pragma_update(Some("main"), "secure_delete", "ON")?;
    let tx = desk.transaction()?;
    let has_book_documents = exists_in_schema(&tx, "main", "book_documents")?;
    let has_file_index = exists_in_schema(&tx, "main", "file_index")?;
    if has_book_documents && has_file_index {
        tx.execute(
            "DELETE FROM main.file_index WHERE path_display IN (SELECT document_path FROM main.book_documents)",
            [],
        )?;
    }
    if has_book_documents && exists_in_schema(&tx, "main", "documents")? {
        tx.execute(
            "DELETE FROM main.documents WHERE path IN (SELECT document_path FROM main.book_documents)",
            [],
        )?;
    }
    if has_book_documents && exists_in_schema(&tx, "main", "index_undo")? {
        tx.execute(
            "DELETE FROM main.index_undo WHERE scope='documents' AND record_key IN (SELECT document_path FROM main.book_documents)",
            [],
        )?;
    }
    for file in managed_files {
        let path = file.to_string_lossy().into_owned();
        let display = crate::files::shell_path(file)
            .to_string_lossy()
            .into_owned();
        if has_file_index {
            tx.execute(
                "DELETE FROM main.file_index WHERE path_display IN (?1,?2)",
                rusqlite::params![path, display],
            )?;
        }
        if exists_in_schema(&tx, "main", "documents")? {
            tx.execute(
                "DELETE FROM main.documents WHERE path IN (?1,?2)",
                rusqlite::params![path, display],
            )?;
        }
        if exists_in_schema(&tx, "main", "index_undo")? {
            tx.execute(
                "DELETE FROM main.index_undo WHERE scope='documents' AND record_key IN (?1,?2)",
                rusqlite::params![path, display],
            )?;
        }
    }
    for table in [
        "daily_essays",
        "book_page_drafts",
        "book_page_versions",
        "blog_post_links",
        "book_documents",
        "writing_imports",
        "writing_logs",
        "book_pages",
        "books",
        "credentials",
        "legacy_credential_imports",
    ] {
        if exists_in_schema(&tx, "main", table)? {
            tx.execute(&format!("DELETE FROM main.\"{table}\""), [])?;
        }
    }
    if exists_in_schema(&tx, "main", "favorites")? {
        tx.execute(
            "DELETE FROM main.favorites WHERE scope IN ('books','book_pages','credentials','documents')",
            [],
        )?;
    }
    if exists_in_schema(&tx, "main", "tags")? {
        tx.execute(
            "DELETE FROM main.tags WHERE scope IN ('books','book_pages','credentials','documents')",
            [],
        )?;
    }
    if has_file_index && exists_in_schema(&tx, "main", "file_index_fts")? {
        tx.execute_batch("INSERT INTO main.file_index_fts(file_index_fts) VALUES('rebuild');")?;
    }
    tx.commit()?;
    if vacuum_file {
        desk.execute_batch(
            "PRAGMA main.wal_checkpoint(TRUNCATE); VACUUM main; PRAGMA main.wal_checkpoint(TRUNCATE);",
        )?;
    }
    Ok(())
}

fn managed_writing_files(desk: &Connection, data_dir: &Path) -> Result<Vec<PathBuf>> {
    if !exists_in_schema(desk, "main", "writing_imports")? {
        return Ok(Vec::new());
    }
    let root = data_dir.join("documents");
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let root = root.canonicalize()?;
    let mut statement = desk.prepare(
        "SELECT source FROM writing_imports WHERE source LIKE 'managed:%' ORDER BY source",
    )?;
    let sources = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut files = Vec::new();
    for source in sources {
        let Some(relative) = source.strip_prefix("managed:") else {
            continue;
        };
        let path = Path::new(relative);
        if path.is_absolute()
            || path
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            continue;
        }
        let candidate = root.join(path);
        let Some(file_name) = candidate.file_name() else {
            continue;
        };
        let Some(parent) = candidate.parent() else {
            continue;
        };
        let Ok(parent) = parent.canonicalize() else {
            continue;
        };
        if !parent.starts_with(&root) {
            continue;
        }
        let candidate = parent.join(file_name);
        if candidate.is_file() {
            let Ok(canonical) = candidate.canonicalize() else {
                continue;
            };
            if canonical.starts_with(&root) {
                files.push(canonical);
            }
        } else {
            files.push(candidate);
        }
    }
    Ok(files)
}

fn clear_local_writing_files(data_dir: &Path, managed_files: &[PathBuf]) -> Result<()> {
    for directory in [
        data_dir.join("writing-assets"),
        data_dir.join("data").join("picture"),
        data_dir.join("recovery"),
    ] {
        if directory.exists() {
            std::fs::remove_dir_all(directory)?;
        }
    }
    for file in managed_files {
        if file.is_file() {
            std::fs::remove_file(file)?;
        }
    }
    Ok(())
}

fn sanitize_local_backups(data_dir: &Path) -> Result<()> {
    let backups = data_dir.join("backups");
    if !backups.is_dir() {
        return Ok(());
    }
    let marker = data_dir.join(".remote-vault-backups-sanitized-v1");
    let marker_time = marker
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok();
    let mut databases = Vec::new();
    let mut archives = Vec::new();
    for entry in walkdir::WalkDir::new(&backups).follow_links(false) {
        let entry = entry.map_err(|error| remote_error(&error.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        if let Some(marker_time) = marker_time
            && entry
                .path()
                .metadata()
                .and_then(|metadata| metadata.modified())
                .is_ok_and(|modified| modified <= marker_time)
        {
            continue;
        }
        match entry.path().extension().and_then(|value| value.to_str()) {
            Some("db" | "sqlite") => databases.push(entry.path().to_path_buf()),
            Some("zip") => archives.push(entry.path().to_path_buf()),
            _ => {}
        }
    }
    let changed = !databases.is_empty() || !archives.is_empty();
    for database in &databases {
        sanitize_backup_database(database, data_dir)
            .map_err(|error| remote_error(&format!("本机备份 {}：{error}", database.display())))?;
    }
    for archive in &archives {
        sanitize_backup_archive(archive, data_dir).map_err(|error| {
            remote_error(&format!("本机 ZIP 备份 {}：{error}", archive.display()))
        })?;
    }
    if !marker.is_file() || changed {
        std::fs::write(
            &marker,
            b"PersonalDesk private account and writing backups were removed.\n",
        )?;
    }
    Ok(())
}

fn has_private_schema(connection: &Connection) -> Result<bool> {
    for table in [
        "credentials",
        "book_page_versions",
        "daily_essays",
        "writing_imports",
        "legacy_credential_imports",
    ] {
        if exists_in_schema(connection, "main", table)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn sanitize_backup_database(path: &Path, data_dir: &Path) -> Result<()> {
    let mut connection = Connection::open(path)?;
    if !has_private_schema(&connection)? || !private_rows_exist(&connection)? {
        return Ok(());
    }
    let managed_files = managed_writing_files(&connection, data_dir)?;
    clear_local_private_rows(&mut connection, &managed_files, true)?;
    connection
        .execute_batch("PRAGMA main.wal_checkpoint(TRUNCATE); PRAGMA main.journal_mode=DELETE;")?;
    drop(connection);
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = PathBuf::from(format!("{}{}", path.to_string_lossy(), suffix));
        if sidecar.is_file() {
            std::fs::remove_file(sidecar)?;
        }
    }
    Ok(())
}

fn sanitize_backup_archive(path: &Path, data_dir: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| remote_error("本机备份目录无效"))?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    let private_paths = private_paths_from_archive(path, data_dir)?;
    let mut source = ZipArchive::new(std::fs::File::open(path)?)?;
    let mut destination = ZipWriter::new(temporary.reopen()?);
    for index in 0..source.len() {
        let mut entry = source.by_index(index)?;
        let name = entry.name().to_owned();
        if entry.is_dir() || private_export_member(&name) {
            continue;
        }
        let mut bytes = Vec::with_capacity(entry.size().min(16 * 1024 * 1024) as usize);
        entry.read_to_end(&mut bytes)?;
        if name == "data/desk.db" {
            bytes = sanitize_sqlite_bytes(&bytes, data_dir)
                .map_err(|error| remote_error(&format!("ZIP 内数据库清理失败：{error}")))?;
        } else if name == "自定义标签.json" || name == "条目标签.json" {
            let Some(filtered) = filter_private_scope_json(&bytes)? else {
                continue;
            };
            bytes = filtered;
        } else if name == "文档清单.json" {
            let Some(filtered) = filter_private_document_list(&bytes, &private_paths)? else {
                continue;
            };
            bytes = filtered;
        }
        destination.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )?;
        destination.write_all(&bytes)?;
    }
    let output = destination.finish()?;
    output.sync_all()?;
    drop(output);
    drop(source);
    temporary
        .persist(path)
        .map_err(|error| DeskError::Io(error.error))?;
    Ok(())
}

fn private_export_member(name: &str) -> bool {
    matches!(
        name,
        "账号密码.json"
            | "图书文档.json"
            | "图书.json"
            | "图书页面.json"
            | "写作文档.json"
            | "导出说明.txt"
            | "legacy/vault.db"
            | "legacy/vault.db-wal"
            | "legacy/vault.db-shm"
    ) || name.starts_with("books/")
        || name.starts_with("writing/")
        || name.starts_with("writing-assets/")
        || name.starts_with("data/picture/")
        || name.starts_with("data/recovery/")
        || (name.starts_with("data/documents/") && is_managed_text_path(name))
        || matches!(
            name,
            "data/desk.db-wal" | "data/desk.db-shm" | "data/desk.db-journal"
        )
}

fn is_managed_text_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("md" | "markdown" | "txt")
    )
}

fn private_paths_from_archive(
    path: &Path,
    data_dir: &Path,
) -> Result<std::collections::HashSet<String>> {
    let mut archive = ZipArchive::new(std::fs::File::open(path)?)?;
    let mut database_bytes = None;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.name() == "data/desk.db" {
            let mut bytes = Vec::with_capacity(entry.size().min(32 * 1024 * 1024) as usize);
            entry.read_to_end(&mut bytes)?;
            database_bytes = Some(bytes);
            break;
        }
    }
    let Some(database_bytes) = database_bytes else {
        return Ok(std::collections::HashSet::new());
    };
    let directory = tempfile::tempdir()?;
    let database_path = directory.path().join("desk.db");
    std::fs::write(&database_path, &database_bytes)?;
    let connection = Connection::open(&database_path)?;
    let mut paths = std::collections::HashSet::new();
    if exists_in_schema(&connection, "main", "book_documents")? {
        let mut statement = connection.prepare("SELECT document_path FROM main.book_documents")?;
        for path in statement.query_map([], |row| row.get::<_, String>(0))? {
            let path = path?;
            paths.insert(path_key(Path::new(&path)));
        }
    }
    if exists_in_schema(&connection, "main", "writing_imports")? {
        let root = data_dir.join("documents");
        let mut statement = connection
            .prepare("SELECT source FROM main.writing_imports WHERE source LIKE 'managed:%'")?;
        for source in statement.query_map([], |row| row.get::<_, String>(0))? {
            let source = source?;
            let Some(relative) = source.strip_prefix("managed:") else {
                continue;
            };
            let relative = Path::new(relative);
            if relative.is_absolute()
                || relative
                    .components()
                    .any(|component| !matches!(component, std::path::Component::Normal(_)))
            {
                continue;
            }
            paths.insert(path_key(&root.join(relative)));
        }
    }
    Ok(paths)
}

fn path_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
}

fn filter_private_document_list(
    bytes: &[u8],
    private_paths: &std::collections::HashSet<String>,
) -> Result<Option<Vec<u8>>> {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Ok(None);
    };
    let Some(records) = value.as_array_mut() else {
        return Ok(None);
    };
    records.retain(|record| {
        let path = record
            .get("path")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|path| private_paths.contains(&path_key(Path::new(path))));
        let archived_text = record
            .get("archive_path")
            .and_then(serde_json::Value::as_str)
            .is_some_and(is_managed_text_path);
        !path && !archived_text
    });
    Ok(Some(serde_json::to_vec(&value)?))
}

fn filter_private_scope_json(bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Ok(None);
    };
    let Some(records) = value.as_array_mut() else {
        return Ok(None);
    };
    records.retain(|record| {
        !record
            .get("scope")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|scope| PRIVATE_SCOPES.contains(&scope))
    });
    Ok(Some(serde_json::to_vec(&value)?))
}

fn sanitize_sqlite_bytes(bytes: &[u8], data_dir: &Path) -> Result<Vec<u8>> {
    let directory = tempfile::tempdir()?;
    let database_path = directory.path().join("desk.db");
    std::fs::write(&database_path, bytes)?;
    let mut connection = Connection::open(&database_path)?;
    if !has_private_schema(&connection)? || !private_rows_exist(&connection)? {
        return Ok(bytes.to_vec());
    }
    let managed_files = managed_writing_files(&connection, data_dir)?;
    clear_local_private_rows(&mut connection, &managed_files, true)?;
    connection
        .execute_batch("PRAGMA main.wal_checkpoint(TRUNCATE); PRAGMA main.journal_mode=DELETE;")?;
    drop(connection);
    Ok(std::fs::read(database_path)?)
}

fn assets_from_disk(data_dir: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut result = BTreeMap::new();
    for directory in [
        data_dir.join("writing-assets"),
        data_dir.join("data").join("picture"),
    ] {
        if !directory.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&directory).follow_links(false) {
            let entry = entry.map_err(|error| remote_error(&error.to_string()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(data_dir)
                .map_err(|_| remote_error("写作附件超出本机资料目录"))?
                .to_string_lossy()
                .replace('\\', "/");
            validate_asset_path(&relative)?;
            result.insert(relative, std::fs::read(entry.path())?);
        }
    }
    prefixless_asset_guard(&result)?;
    Ok(result)
}

fn prefixless_asset_guard(assets: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    if assets
        .keys()
        .all(|path| path.starts_with("writing-assets/") || path.starts_with("data/picture/"))
    {
        Ok(())
    } else {
        Err(remote_error("发现不属于写作库的本机附件"))
    }
}

fn encode_assets(assets: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>> {
    let mut encoded = BTreeMap::new();
    for (path, bytes) in assets {
        validate_asset_path(path)?;
        encoded.insert(path, BASE64.encode(bytes));
    }
    Ok(serde_json::to_vec(&encoded)?)
}

fn decode_assets(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    let encoded: BTreeMap<String, String> = serde_json::from_slice(bytes)?;
    let mut assets = BTreeMap::new();
    for (path, data) in encoded {
        validate_asset_path(&path)?;
        assets.insert(
            path,
            BASE64
                .decode(data)
                .map_err(|_| remote_error("服务器写作附件密文损坏"))?,
        );
    }
    Ok(assets)
}

fn validate_asset_path(path: &str) -> Result<()> {
    if !(path.starts_with("writing-assets/") || path.starts_with("data/picture/"))
        || path.contains(':')
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(remote_error("写作附件路径无效"));
    }
    Ok(())
}
