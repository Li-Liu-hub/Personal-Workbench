//! Server-only storage for the private account vault.
//!
//! 0.15.76 起只有账号资料（密码、凭据标签/收藏、旧版账号导入标记）继续放在私网 MySQL 的
//! age 加密 SQLite 快照里；写作资料（图书/页面/文档/随笔/草稿/版本/日志/导入/blog 链接
//! 及其图片）全部保存在本机 `desk.db` 与数据目录。旧版远端载荷中的写作表由
//! `vault_migration` 一次性迁回本机，成功后这里才把远端载荷裁成只剩账号表。
//!
//! 远端密文由 age 的 X25519 身份解封，身份私钥用当前 Windows 用户的 DPAPI 上下文保护。
//! 不向 SSH/MySQL 发送任何明文记录或附件。

use std::{
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
use rusqlite::{Connection, MAIN_DB};
use zeroize::Zeroizing;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::{
    crypto,
    error::{DeskError, Result},
    vault_config::VaultMysqlConfig,
    vault_migration,
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

/// 仍保存在私网库的标签/收藏作用域。
const PRIVATE_SCOPES: &[&str] = &["credentials"];

/// 账号资料用到的 `temp` 表：写这些表的改动才触发远端同步。
const ACCOUNT_TEMP_TABLES: &[&str] = &["credentials", "legacy_credential_imports"];

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

pub struct RemoteVault {
    identity: Identity,
    private_revision: u64,
    dirty: Arc<AtomicBool>,
    last_change: Arc<Mutex<Instant>>,
    retry_after: Instant,
}

impl RemoteVault {
    pub fn open_and_attach(desk: &mut Connection, data_dir: &Path) -> Result<Self> {
        let transport = VaultTransport::load()?;
        ensure_remote_schema(&transport)?;
        let remote_private = fetch_remote_payload(&transport, OBJECT_PRIVATE)?;
        // 旧版可能只留下 assets 对象；此时也必须先找到本机私钥，绝不生成新钥匙覆盖旧密文。
        let remote_assets_revision = fetch_remote_revision(&transport, OBJECT_ASSETS)?;
        let identity =
            load_or_create_identity(remote_private.is_some() || remote_assets_revision.is_some())?;

        let local_private = Zeroizing::new(vault_migration::serialize_private_state(desk)?);
        let local_has_private_rows = vault_migration::private_rows_exist(desk)?;
        let mut remote_ciphertext: Option<Vec<u8>> = None;
        let (private_plain, mut private_revision) =
            if let Some((revision, ciphertext)) = remote_private {
                let plaintext = decrypt(&identity, &ciphertext)?;
                if local_has_private_rows {
                    let remote_state = vault_migration::deserialize_private_state(&plaintext)?;
                    if !vault_migration::account_state_matches(desk, &remote_state)? {
                        return Err(DeskError::InvalidPath(
                            "服务器和本机的账号资料不一致；已保留两边数据，没有覆盖。".into(),
                        ));
                    }
                }
                remote_ciphertext = Some(ciphertext);
                (plaintext, revision)
            } else {
                let ciphertext = encrypt(&identity, &local_private)?;
                store_remote_payload(&transport, OBJECT_PRIVATE, None, &ciphertext)?;
                let (revision, verified) = fetch_remote_payload(&transport, OBJECT_PRIVATE)?
                    .ok_or_else(|| remote_error("服务器没有返回已保存的账号资料"))?;
                let verified = decrypt(&identity, &verified)?;
                if verified.as_slice() != local_private.as_slice() {
                    return Err(remote_error("账号资料上传后核对不一致"));
                }
                (verified, revision)
            };

        let private_db = vault_migration::deserialize_private_state(&private_plain)?;
        if vault_migration::writing_rows_present(&private_db)? {
            // 旧版远端私密载荷仍含写作资料：先只读解出远端写作图片。
            let assets = match fetch_remote_payload(&transport, OBJECT_ASSETS)? {
                Some((_, ciphertext)) => {
                    let plaintext = decrypt(&identity, &ciphertext)?;
                    Some(vault_migration::decode_assets(&plaintext)?)
                }
                None => None,
            };
            // 快照 → 逐表合并核对 → 图片落盘校验 → 写迁移标记；任何失败都不会改动远端。
            let report = vault_migration::migrate_writing_to_local(
                desk,
                private_plain.as_slice(),
                data_dir,
                assets.as_ref(),
                remote_ciphertext.as_deref(),
            )?;
            eprintln!("个人服务器写作资料已迁移到本机：{}", report.summary());

            // 写作数据已在事务内核对、图片已回读校验，才把远端载荷裁成只剩账号表。
            let account_db = vault_migration::account_snapshot(&private_db)?;
            let account_bytes = account_db.serialize(MAIN_DB)?;
            let ciphertext = encrypt(&identity, &account_bytes)?;
            private_revision = store_remote_payload(
                &transport,
                OBJECT_PRIVATE,
                Some(private_revision),
                &ciphertext,
            )?;
            // 旧版 assets 对象里的图片已经在本机落盘并验证，这里只是清理远端副本。
            if let Err(error) = delete_remote_payload(&transport, OBJECT_ASSETS) {
                eprintln!(
                    "个人服务器旧版写作图片清理失败（本机数据已完整，下次启动会重试）：{error}"
                );
            }
            vault_migration::attach_account_tables(desk, &account_db)?;
        } else {
            if remote_assets_revision.is_some() {
                // 私密载荷已裁剪、只剩旧版 assets：只有确认本机迁移标记存在才允许删除。
                if !vault_migration::migration_completed(desk)? {
                    return Err(remote_error(
                        "服务器仍有旧版写作图片，但本机没有完成迁移标记；已保留服务器图片，没有删除。",
                    ));
                }
                if let Err(error) = delete_remote_payload(&transport, OBJECT_ASSETS) {
                    eprintln!(
                        "个人服务器旧版写作图片清理失败（本机数据已完整，下次启动会重试）：{error}"
                    );
                }
            }
            vault_migration::attach_account_tables(desk, &private_db)?;
        }

        sanitize_local_backups(data_dir)?;
        desk.execute(
            "INSERT INTO main.settings(key,value) VALUES('remote-vault.legacy-import-done','1') ON CONFLICT(key) DO UPDATE SET value='1'",
            [],
        )?;
        // 账号行改为从私网库读取后，本机 main 里只清账号行；写作行全部保留在本机。
        if vault_migration::private_rows_exist(desk)? {
            vault_migration::clear_local_private_rows(desk, true)?;
        }

        let dirty = Arc::new(AtomicBool::new(false));
        let last_change = Arc::new(Mutex::new(Instant::now()));
        let hook_dirty = Arc::clone(&dirty);
        let hook_last_change = Arc::clone(&last_change);
        desk.update_hook(Some(
            move |_action: rusqlite::hooks::Action, database: &str, table: &str, _rowid: i64| {
                if database == "temp"
                    && (ACCOUNT_TEMP_TABLES.contains(&table)
                        || table == "private_tags"
                        || table == "private_favorites")
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
            private_revision,
            dirty,
            last_change,
            retry_after: Instant::now(),
        })
    }

    pub fn sync_if_due(&mut self, desk: &mut Connection, force: bool) -> Result<bool> {
        let private_dirty = self.dirty.load(Ordering::Acquire);
        if !private_dirty {
            return Ok(false);
        }
        let now = Instant::now();
        if !force && now < self.retry_after {
            return Ok(false);
        }
        if !force
            && self
                .last_change
                .lock()
                .map(|last| last.elapsed() < Duration::from_millis(1200))
                .unwrap_or(false)
        {
            return Ok(false);
        }

        let result = self.sync_pending(desk, private_dirty);
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
        self.dirty.load(Ordering::Acquire)
    }

    /// 只上传账号资料快照；写作资料永远只在本机，不参与远端同步。
    fn sync_pending(&mut self, desk: &mut Connection, private_dirty: bool) -> Result<bool> {
        let transport = VaultTransport::load()?;
        if private_dirty {
            let plaintext = Zeroizing::new(vault_migration::serialize_private_state(desk)?);
            let ciphertext = encrypt(&self.identity, &plaintext)?;
            self.private_revision = store_remote_payload(
                &transport,
                OBJECT_PRIVATE,
                Some(self.private_revision),
                &ciphertext,
            )?;
            self.dirty.store(false, Ordering::Release);
        }
        Ok(private_dirty)
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

fn fetch_remote_revision(transport: &VaultTransport, object_id: &str) -> Result<Option<u64>> {
    if object_id != OBJECT_PRIVATE && object_id != OBJECT_ASSETS {
        return Err(remote_error("资料类型无效"));
    }
    match transport {
        VaultTransport::Direct(config) => {
            let mut connection = config.connect()?;
            connection
                .exec_first(
                    format!(
                        "SELECT revision FROM `{}`.`{MYSQL_TABLE}` WHERE object_id=? LIMIT 1",
                        config.database()
                    ),
                    (object_id,),
                )
                .map_err(mysql_error)
        }
        VaultTransport::Ssh => {
            let output = run_mysql_ssh(&format!(
                "SELECT revision FROM `{MYSQL_DATABASE}`.`{MYSQL_TABLE}` WHERE object_id='{object_id}';\n"
            ))?;
            let line = output.trim();
            if line.is_empty() {
                return Ok(None);
            }
            line.parse::<u64>()
                .map(Some)
                .map_err(|_| remote_error("服务器密文版本号无效"))
        }
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

fn delete_remote_payload(transport: &VaultTransport, object_id: &str) -> Result<()> {
    if object_id != OBJECT_PRIVATE && object_id != OBJECT_ASSETS {
        return Err(remote_error("资料类型无效"));
    }
    match transport {
        VaultTransport::Direct(config) => {
            let mut connection = config.connect()?;
            connection
                .exec_drop(
                    format!(
                        "DELETE FROM `{}`.`{MYSQL_TABLE}` WHERE object_id=?",
                        config.database()
                    ),
                    (object_id,),
                )
                .map_err(mysql_error)?;
        }
        VaultTransport::Ssh => {
            run_mysql_ssh(&format!(
                "DELETE FROM `{MYSQL_DATABASE}`.`{MYSQL_TABLE}` WHERE object_id='{object_id}';\nSELECT ROW_COUNT();\n"
            ))?;
        }
    }
    Ok(())
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
        sanitize_backup_database(database)
            .map_err(|error| remote_error(&format!("本机备份 {}：{error}", database.display())))?;
    }
    for archive in &archives {
        sanitize_backup_archive(archive).map_err(|error| {
            remote_error(&format!("本机 ZIP 备份 {}：{error}", archive.display()))
        })?;
    }
    if !marker.is_file() || changed {
        std::fs::write(
            &marker,
            "PersonalDesk backups were sanitized: only private account rows are removed; writing data is kept.\n",
        )?;
    }
    Ok(())
}

fn has_private_schema(connection: &Connection) -> Result<bool> {
    for table in ["credentials", "legacy_credential_imports"] {
        if vault_migration::exists_in_schema(connection, "main", table)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn sanitize_backup_database(path: &Path) -> Result<()> {
    let mut connection = Connection::open(path)?;
    // 写作资料已经属于本机数据，备份里必须保留；只清仍有保密要求的账号行。
    if !has_private_schema(&connection)? || !vault_migration::private_rows_exist(&connection)? {
        return Ok(());
    }
    vault_migration::clear_local_private_rows(&mut connection, true)?;
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

fn sanitize_backup_archive(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| remote_error("本机备份目录无效"))?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
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
            bytes = sanitize_sqlite_bytes(&bytes)
                .map_err(|error| remote_error(&format!("ZIP 内数据库清理失败：{error}")))?;
        } else if name == "自定义标签.json" || name == "条目标签.json" {
            let Some(filtered) = filter_private_scope_json(&bytes)? else {
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

/// 导出包中必须整份丢弃的“账号私密”文件；写作/文档/图片全部保留。
fn private_export_member(name: &str) -> bool {
    matches!(
        name,
        "账号密码.json"
            | "legacy/vault.db"
            | "legacy/vault.db-wal"
            | "legacy/vault.db-shm"
            | "data/desk.db-wal"
            | "data/desk.db-shm"
            | "data/desk.db-journal"
    )
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

fn sanitize_sqlite_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
    let directory = tempfile::tempdir()?;
    let database_path = directory.path().join("desk.db");
    std::fs::write(&database_path, bytes)?;
    let mut connection = Connection::open(&database_path)?;
    if !has_private_schema(&connection)? || !vault_migration::private_rows_exist(&connection)? {
        return Ok(bytes.to_vec());
    }
    vault_migration::clear_local_private_rows(&mut connection, true)?;
    connection
        .execute_batch("PRAGMA main.wal_checkpoint(TRUNCATE); PRAGMA main.journal_mode=DELETE;")?;
    drop(connection);
    Ok(std::fs::read(database_path)?)
}
