//! Read-only compatibility for accounts saved by older encrypted versions.
//! New account storage never creates a key or writes an encrypted record.
use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::{
    crypto::{self, VaultKey},
    error::{DeskError, Result},
    model::Credential,
};

#[derive(Default)]
pub struct ImportReport {
    imported: usize,
    unavailable: usize,
}

impl ImportReport {
    pub fn notice(&self) -> String {
        match (self.imported, self.unavailable) {
            (0, 0) => String::new(),
            (count, 0) => format!("已迁移 {count} 个旧版账号，可直接查看和编辑。"),
            (count, remaining) => format!(
                "已迁移 {count} 个账号；另有 {remaining} 条旧记录暂时无法读取，原文件已保留。"
            ),
        }
    }
}

pub fn import(desk: &mut Connection, data_dir: &Path) -> Result<ImportReport> {
    let path = data_dir.join("vault.db");
    if !path.exists() {
        return Ok(ImportReport::default());
    }
    let legacy = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    legacy.busy_timeout(std::time::Duration::from_secs(1))?;
    let has_table = |name| -> Result<bool> {
        Ok(legacy.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [name],
            |row| row.get(0),
        )?)
    };
    if !has_table("vault_entries")? {
        return Ok(ImportReport::default());
    }
    let key = if has_table("local_vault_key")? {
        let stored: Option<(String, Vec<u8>)> = legacy
            .query_row(
                "SELECT key_id,protected_dek FROM local_vault_key WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        stored.and_then(|(id, bytes)| {
            crypto::unprotect_local_key(&bytes)
                .ok()
                .map(|key| (id, VaultKey(key)))
        })
    } else {
        None
    };
    let mut statement = legacy.prepare(
        "SELECT id,key_id,nonce,ciphertext,updated_at FROM vault_entries ORDER BY rowid",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Vec<u8>>(2)?,
            row.get::<_, Vec<u8>>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let transaction = desk.transaction()?;
    let mut report = ImportReport::default();
    for row in rows {
        let (id, key_id, nonce, ciphertext, updated_at) = row?;
        let already_imported: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM legacy_credential_imports WHERE id=?1)",
            [&id],
            |row| row.get(0),
        )?;
        if already_imported {
            continue;
        }
        let decode = || -> Result<Credential> {
            let (current_id, current_key) = key.as_ref().ok_or(DeskError::Crypto)?;
            if current_id != &key_id {
                return Err(DeskError::Crypto);
            }
            let nonce: [u8; 12] = nonce.as_slice().try_into().map_err(|_| DeskError::Crypto)?;
            let plaintext = zeroize::Zeroizing::new(crypto::decrypt(
                current_key,
                &key_id,
                &id,
                &ciphertext,
                &nonce,
            )?);
            Ok(serde_json::from_slice(&plaintext)?)
        };
        let Ok(entry) = decode() else {
            report.unavailable += 1;
            continue;
        };
        transaction.execute("INSERT OR IGNORE INTO credentials(id,title,username,password,website,notes,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![id, entry.title, entry.username, entry.password, entry.website, entry.notes, updated_at])?;
        transaction.execute(
            "INSERT INTO legacy_credential_imports(id) VALUES(?1)",
            [&id],
        )?;
        report.imported += 1;
    }
    transaction.commit()?;
    Ok(report)
}
