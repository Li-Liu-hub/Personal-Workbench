//! Local connection settings for the encrypted server vault.
//!
//! The MySQL password is protected with the current Windows user's DPAPI
//! context. It is never written to the repository or returned by the UI.

use std::{fs, path::PathBuf, time::Duration};

use mysql::{Conn, OptsBuilder, prelude::Queryable};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::{
    crypto,
    error::{DeskError, Result},
};

pub const DEFAULT_HOST: &str = "10.0.0.13";
pub const DEFAULT_PORT: u16 = 3306;
pub const DEFAULT_DATABASE: &str = "personaldesk_vault";
const CONFIG_VERSION: u8 = 1;
const CONFIG_FILE: &str = "PersonalDesk\\vault-mysql.dpapi";

#[derive(Serialize, Deserialize)]
struct StoredConfig {
    version: u8,
    host: String,
    port: u16,
    user: String,
    password: String,
    database: String,
}

/// Direct MySQL settings saved on this machine.
pub struct VaultMysqlConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    database: String,
}

#[derive(Serialize)]
pub struct VaultMysqlSummary {
    pub configured: bool,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub database: String,
}

impl VaultMysqlConfig {
    pub fn new(
        host: String,
        port: u16,
        user: String,
        password: String,
        database: String,
    ) -> Result<Self> {
        let config = Self {
            host: host.trim().to_owned(),
            port,
            user: user.trim().to_owned(),
            password,
            database: database.trim().to_owned(),
        };
        config.validate()?;
        Ok(config)
    }

    pub fn load() -> Result<Option<Self>> {
        let path = config_path()?;
        if !path.is_file() {
            return Ok(None);
        }
        let protected = fs::read(&path)?;
        let plaintext = crypto::unprotect_local_secret(&protected)?;
        let stored: StoredConfig = serde_json::from_slice(&plaintext)?;
        if stored.version != CONFIG_VERSION {
            return Err(DeskError::InvalidPath(
                "服务器加密库连接配置版本不受支持".into(),
            ));
        }
        Self::new(
            stored.host,
            stored.port,
            stored.user,
            stored.password,
            stored.database,
        )
        .map(Some)
    }

    pub fn save(&self) -> Result<()> {
        self.validate()?;
        let stored = StoredConfig {
            version: CONFIG_VERSION,
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            password: self.password.clone(),
            database: self.database.clone(),
        };
        let json = serde_json::to_vec(&stored)?;
        let protected = crypto::protect_local_secret(&json)?;
        let path = config_path()?;
        let parent = path
            .parent()
            .ok_or_else(|| DeskError::InvalidPath("无法定位本机密文库连接配置目录".into()))?;
        fs::create_dir_all(parent)?;
        let temporary = path.with_extension("new");
        fs::write(&temporary, protected)?;
        fs::rename(temporary, path)?;
        Ok(())
    }

    pub fn clear() -> Result<()> {
        let path = config_path()?;
        if path.is_file() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn summary() -> Result<VaultMysqlSummary> {
        Ok(match Self::load()? {
            Some(config) => VaultMysqlSummary {
                configured: true,
                host: config.host.clone(),
                port: config.port,
                user: config.user.clone(),
                database: config.database.clone(),
            },
            None => VaultMysqlSummary {
                configured: false,
                host: DEFAULT_HOST.into(),
                port: DEFAULT_PORT,
                user: String::new(),
                database: DEFAULT_DATABASE.into(),
            },
        })
    }

    pub fn test_connection(&self) -> Result<()> {
        let mut connection = self.connect()?;
        let value: Option<u8> = connection.exec_first("SELECT 1", ()).map_err(mysql_error)?;
        if value == Some(1) {
            Ok(())
        } else {
            Err(DeskError::InvalidPath("直连 MySQL 测试未返回结果".into()))
        }
    }

    pub fn connect(&self) -> Result<Conn> {
        // The private overlay network already encrypts traffic; the middleware
        // does not expose a native MySQL TLS certificate.
        let opts = OptsBuilder::default()
            .ip_or_hostname(Some(self.host.clone()))
            .tcp_port(self.port)
            .user(Some(self.user.clone()))
            .pass(Some(self.password.clone()))
            .prefer_socket(false)
            .ssl_opts(None)
            .tcp_connect_timeout(Some(Duration::from_secs(10)))
            .read_timeout(Some(Duration::from_secs(30)))
            .write_timeout(Some(Duration::from_secs(30)));
        Conn::new(opts).map_err(mysql_error)
    }

    pub fn database(&self) -> &str {
        &self.database
    }

    fn validate(&self) -> Result<()> {
        if self.host.is_empty() || self.host.chars().any(char::is_whitespace) {
            return Err(DeskError::InvalidInput(
                "请填写服务器私网地址，不能包含空格".into(),
            ));
        }
        if self.port == 0 {
            return Err(DeskError::InvalidInput("请填写有效的 MySQL 端口".into()));
        }
        if self.user.is_empty() {
            return Err(DeskError::InvalidInput("请填写 MySQL 用户名".into()));
        }
        if self.password.is_empty() {
            return Err(DeskError::InvalidInput("请填写 MySQL 密码".into()));
        }
        if self.database.is_empty()
            || self.database.len() > 64
            || !self
                .database
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(DeskError::InvalidInput(
                "数据库名只能包含字母、数字和下划线".into(),
            ));
        }
        Ok(())
    }
}

impl Drop for VaultMysqlConfig {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

fn config_path() -> Result<PathBuf> {
    let app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| DeskError::InvalidPath("无法定位本机应用数据目录".into()))?;
    Ok(app_data.join(CONFIG_FILE))
}

fn mysql_error(error: mysql::Error) -> DeskError {
    DeskError::InvalidPath(format!("直连 MySQL 失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(database: &str, password: &str) -> Result<VaultMysqlConfig> {
        VaultMysqlConfig::new(
            DEFAULT_HOST.into(),
            DEFAULT_PORT,
            "desktop".into(),
            password.into(),
            database.into(),
        )
    }

    #[test]
    fn rejects_invalid_connection_fields() {
        assert!(config("personaldesk_vault", "").is_err());
        assert!(config("bad-name", "secret").is_err());
        assert!(
            VaultMysqlConfig::new(
                "host name".into(),
                DEFAULT_PORT,
                "desktop".into(),
                "secret".into(),
                DEFAULT_DATABASE.into()
            )
            .is_err()
        );
    }

    #[test]
    fn accepts_private_address_and_database_name() {
        let config = config(DEFAULT_DATABASE, "secret").unwrap();
        assert_eq!(config.database(), DEFAULT_DATABASE);
        assert_eq!(config.host, DEFAULT_HOST);
        assert_eq!(config.port, DEFAULT_PORT);
    }
}
