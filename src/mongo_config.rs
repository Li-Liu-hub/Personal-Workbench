//! Local MongoDB settings for the public document library.
//!
//! 连接参数显式写入 [`ClientOptions`]（`hosts` / `credential.source`），
//! 不再拼接 `mongodb://` URI 字符串：用户名、密码或主机里的 `@ : / ? # %` 等字符
//! 不会被当作连接串语法重新解析，避免误连接或参数注入。
//! 配置文件用当前 Windows 用户的 DPAPI 保护，密码不写入仓库、不出现在日志里。

use std::{fs, net::Ipv6Addr, path::PathBuf, str::FromStr, time::Duration};

use mongodb::{
    Client,
    options::{ClientOptions, Credential, ServerAddress},
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::{
    crypto,
    error::{DeskError, Result},
};

pub const DEFAULT_HOST: &str = "10.0.0.13";
pub const DEFAULT_PORT: u16 = 27017;
pub const DEFAULT_AUTH_DATABASE: &str = "admin";
pub const DEFAULT_DATABASE: &str = "personaldesk";
pub const DEFAULT_COLLECTION: &str = "documents";
const CONFIG_VERSION: u8 = 1;
const CONFIG_FILE: &str = "PersonalDesk\\mongo-docs.dpapi";
const APP_NAME: &str = "个人办公台";
/// DNS 主机名上限；IPv6 字面量远小于该值。
const MAX_HOST_BYTES: usize = 253;
const MAX_USER_BYTES: usize = 256;
const MAX_PASSWORD_BYTES: usize = 1024;
const MAX_NAME_BYTES: usize = 120;

/// 文档库连接参数：设置界面直接编辑这组字段，保存前统一校验。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MongoSettings {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub auth_database: String,
    pub database: String,
    pub collection: String,
}

impl Default for MongoSettings {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.into(),
            port: DEFAULT_PORT,
            user: String::new(),
            password: String::new(),
            auth_database: DEFAULT_AUTH_DATABASE.into(),
            database: DEFAULT_DATABASE.into(),
            collection: DEFAULT_COLLECTION.into(),
        }
    }
}

// MongoSettings 不实现 Drop，保证设置界面与测试可以用结构体更新语法构造；
// 密码的零化放在持有者 MongoConfig 与保存路径上。
#[derive(Serialize, Deserialize)]
struct StoredConfig {
    version: u8,
    #[serde(flatten)]
    settings: MongoSettings,
}

/// 已校验的文档库连接配置。
pub struct MongoConfig {
    settings: MongoSettings,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct MongoSummary {
    pub configured: bool,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_database: String,
    pub database: String,
    pub collection: String,
}

impl MongoSummary {
    fn from_settings(settings: &MongoSettings, configured: bool) -> Self {
        Self {
            configured,
            host: settings.host.clone(),
            port: settings.port,
            user: settings.user.clone(),
            auth_database: settings.auth_database.clone(),
            database: settings.database.clone(),
            collection: settings.collection.clone(),
        }
    }
}

impl MongoConfig {
    pub fn new(mut settings: MongoSettings) -> Result<Self> {
        for value in [
            &mut settings.host,
            &mut settings.user,
            &mut settings.auth_database,
            &mut settings.database,
            &mut settings.collection,
        ] {
            *value = value.trim().to_owned();
        }
        if settings.auth_database.is_empty() {
            settings.auth_database = DEFAULT_AUTH_DATABASE.to_owned();
        }
        let config = Self { settings };
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
                "MongoDB 文档库配置版本不受支持".into(),
            ));
        }
        Self::new(stored.settings).map(Some)
    }

    pub fn save(&self) -> Result<()> {
        self.validate()?;
        let mut stored = StoredConfig {
            version: CONFIG_VERSION,
            settings: self.settings.clone(),
        };
        let mut json = serde_json::to_vec(&stored)?;
        let protected = crypto::protect_local_secret(&json);
        json.zeroize();
        stored.settings.password.zeroize();
        let protected = protected?;
        let path = config_path()?;
        let parent = path
            .parent()
            .ok_or_else(|| DeskError::InvalidPath("无法定位文档库配置目录".into()))?;
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

    pub fn summary() -> Result<MongoSummary> {
        Ok(match Self::load()? {
            Some(config) => MongoSummary::from_settings(&config.settings, true),
            None => MongoSummary::from_settings(&MongoSettings::default(), false),
        })
    }

    pub async fn client(&self) -> Result<Client> {
        Client::with_options(self.client_options()).map_err(mongo_error)
    }

    /// 显式构造连接参数：地址/端口来自结构化字段，认证库用 `credential.source`。
    fn client_options(&self) -> ClientOptions {
        ClientOptions::builder()
            .hosts(vec![ServerAddress::Tcp {
                host: self.settings.host.clone(),
                port: Some(self.settings.port),
            }])
            .app_name(Some(APP_NAME.to_owned()))
            .connect_timeout(Duration::from_secs(10))
            .server_selection_timeout(Duration::from_secs(15))
            .credential(
                Credential::builder()
                    .username(self.settings.user.clone())
                    .password(self.settings.password.clone())
                    .source(self.settings.auth_database.clone())
                    .build(),
            )
            .build()
    }

    pub async fn test_connection(&self) -> Result<()> {
        let client = self.client().await?;
        client
            .database(&self.settings.database)
            .run_command(mongodb::bson::doc! { "ping": 1 })
            .await
            .map_err(mongo_error)?;
        Ok(())
    }

    pub fn database(&self) -> &str {
        &self.settings.database
    }

    pub fn collection(&self) -> &str {
        &self.settings.collection
    }

    fn validate(&self) -> Result<()> {
        self.validate_host()?;
        if self.settings.port == 0 {
            return Err(DeskError::InvalidInput("请填写有效的 MongoDB 端口".into()));
        }
        if self.settings.user.is_empty() {
            return Err(DeskError::InvalidInput("请填写 MongoDB 用户名".into()));
        }
        if self.settings.user.len() > MAX_USER_BYTES {
            return Err(DeskError::InvalidInput(format!(
                "MongoDB 用户名不能超过 {MAX_USER_BYTES} 字节"
            )));
        }
        if self
            .settings
            .user
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        {
            return Err(DeskError::InvalidInput(
                "MongoDB 用户名不能包含空格或控制字符".into(),
            ));
        }
        if self.settings.password.is_empty() {
            return Err(DeskError::InvalidInput("请填写 MongoDB 密码".into()));
        }
        if self.settings.password.len() > MAX_PASSWORD_BYTES {
            return Err(DeskError::InvalidInput(format!(
                "MongoDB 密码不能超过 {MAX_PASSWORD_BYTES} 字节"
            )));
        }
        if self
            .settings
            .password
            .chars()
            .any(|character| character == '\0' || character.is_control())
        {
            return Err(DeskError::InvalidInput(
                "MongoDB 密码不能包含空字符或控制字符".into(),
            ));
        }
        for (label, value) in [
            ("认证库", &self.settings.auth_database),
            ("业务库", &self.settings.database),
            ("集合", &self.settings.collection),
        ] {
            if value.is_empty()
                || value.len() > MAX_NAME_BYTES
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            {
                return Err(DeskError::InvalidInput(format!(
                    "{label}只能包含字母、数字、下划线和短横线，且长度不超过 {MAX_NAME_BYTES}"
                )));
            }
        }
        Ok(())
    }

    fn validate_host(&self) -> Result<()> {
        let host = self.settings.host.as_str();
        if host.is_empty() {
            return Err(DeskError::InvalidInput(
                "请填写 MongoDB 私网地址，不能包含空格".into(),
            ));
        }
        if host.len() > MAX_HOST_BYTES {
            return Err(DeskError::InvalidInput(format!(
                "MongoDB 地址不能超过 {MAX_HOST_BYTES} 字节"
            )));
        }
        // 只接受主机名/IPv4/IPv6 字面量；`/ ? # @ [ ] :` 等 URI 语法字符在这里没有意义，
        // 出现即说明用户粘贴了连接串，直接拒绝而不是当成地址拼接。
        if let Some(character) = host.chars().find(|character| {
            character.is_whitespace()
                || character.is_control()
                || matches!(character, '/' | '?' | '#' | '@' | '[' | ']')
        }) {
            return Err(DeskError::InvalidInput(format!(
                "MongoDB 地址只能填写主机名或 IP，不能包含 '{character}' 或连接串前缀"
            )));
        }
        if host.contains(':') {
            // IPv6 字面量（不带方括号）。主机名不会包含 ':'，因此可以直接判定。
            Ipv6Addr::from_str(host)
                .map_err(|_| DeskError::InvalidInput("MongoDB 地址不是有效的 IPv6 地址".into()))?;
            return Ok(());
        }
        if host.starts_with('.') || host.ends_with('.') || host.contains("..") {
            return Err(DeskError::InvalidInput("MongoDB 地址格式无效".into()));
        }
        Ok(())
    }
}

impl Drop for MongoConfig {
    fn drop(&mut self) {
        self.settings.password.zeroize();
    }
}

fn config_path() -> Result<PathBuf> {
    let app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| DeskError::InvalidPath("无法定位本机应用数据目录".into()))?;
    Ok(app_data.join(CONFIG_FILE))
}

fn mongo_error(error: mongodb::error::Error) -> DeskError {
    DeskError::InvalidPath(format!("MongoDB 文档库操作失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> MongoSettings {
        MongoSettings {
            user: "desktop".into(),
            password: "secret".into(),
            ..MongoSettings::default()
        }
    }

    #[test]
    fn summary_defaults_point_at_private_document_library() {
        let summary = MongoSummary::from_settings(&MongoSettings::default(), false);
        assert!(!summary.configured);
        assert_eq!(summary.host, DEFAULT_HOST);
        assert_eq!(summary.port, DEFAULT_PORT);
        assert_eq!(summary.auth_database, DEFAULT_AUTH_DATABASE);
        assert_eq!(summary.database, DEFAULT_DATABASE);
        assert_eq!(summary.collection, DEFAULT_COLLECTION);
        assert!(summary.user.is_empty());
    }

    #[test]
    fn credentials_with_uri_syntax_are_kept_literal_and_never_reparsed() {
        let mut values = settings();
        values.password = "p@ss:w/rd?#%25".into();
        let config = MongoConfig::new(values).unwrap();
        let options = config.client_options();
        assert_eq!(
            options.hosts,
            vec![ServerAddress::Tcp {
                host: DEFAULT_HOST.to_owned(),
                port: Some(DEFAULT_PORT),
            }]
        );
        let credential = options.credential.expect("必须显式设置认证信息");
        assert_eq!(credential.username.as_deref(), Some("desktop"));
        // 密码原样进入结构化字段，没有被 URI 解析器百分号解码或截断。
        assert_eq!(credential.password.as_deref(), Some("p@ss:w/rd?#%25"));
        assert_eq!(credential.source.as_deref(), Some(DEFAULT_AUTH_DATABASE));
        assert_eq!(config.database(), DEFAULT_DATABASE);
        assert_eq!(config.collection(), DEFAULT_COLLECTION);
    }

    #[test]
    fn empty_auth_database_falls_back_to_admin() {
        let mut values = settings();
        values.auth_database = "  ".into();
        let config = MongoConfig::new(values).unwrap();
        assert_eq!(
            config
                .client_options()
                .credential
                .unwrap()
                .source
                .as_deref(),
            Some(DEFAULT_AUTH_DATABASE)
        );
    }

    #[test]
    fn rejects_invalid_connection_fields() {
        let mut values = settings();
        values.host = "host name".into();
        assert!(MongoConfig::new(values).is_err());

        let mut values = settings();
        values.host = "mongodb://10.0.0.13:27017".into();
        assert!(MongoConfig::new(values).is_err());

        let mut values = settings();
        values.host = "10.0.0.13/admin".into();
        assert!(MongoConfig::new(values).is_err());

        let mut values = settings();
        values.host = "gggg::1".into();
        assert!(MongoConfig::new(values).is_err());

        let mut values = settings();
        values.port = 0;
        assert!(MongoConfig::new(values).is_err());

        let mut values = settings();
        values.user = String::new();
        assert!(MongoConfig::new(values).is_err());

        let mut values = settings();
        values.password = String::new();
        assert!(MongoConfig::new(values).is_err());

        let mut values = settings();
        values.password = "x".repeat(MAX_PASSWORD_BYTES + 1);
        assert!(MongoConfig::new(values).is_err());

        let mut values = settings();
        values.database = "bad name".into();
        assert!(MongoConfig::new(values).is_err());

        let mut values = settings();
        values.collection = "bad.collection".into();
        assert!(MongoConfig::new(values).is_err());
    }

    #[test]
    fn accepts_ipv4_ipv6_and_hostname() {
        for host in ["10.0.0.13", "mongo.internal", "::1", "fe80::1"] {
            let mut values = settings();
            values.host = host.into();
            assert!(MongoConfig::new(values).is_ok(), "host={host}");
        }
    }
}
