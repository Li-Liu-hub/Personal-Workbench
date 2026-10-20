use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Shortcut {
    pub id: String,
    pub name: String,
    pub path: std::path::PathBuf,
    pub is_dir: bool,
    pub pinned: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Credential {
    pub id: String,
    pub title: String,
    pub username: String,
    pub password: String,
    pub website: String,
    pub notes: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CredentialSummary {
    pub id: String,
    pub title: String,
    pub username: String,
    pub password: String,
    pub website: String,
    pub notes: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct FileItem {
    pub name: String,
    pub path: std::path::PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentRecord {
    pub path: std::path::PathBuf,
    pub title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct InventoryItem {
    pub id: String,
    pub name: String,
    pub category: String,
    pub quantity: i64,
    pub location: String,
    pub notes: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub purchase_date: String,
    #[serde(default)]
    pub disposal_date: String,
    #[serde(default)]
    pub purchase_price_cents: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Website {
    pub id: String,
    pub title: String,
    pub url: String,
    pub category: String,
    pub notes: String,
}

impl Website {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            ..Self::default()
        }
    }
}

impl InventoryItem {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            quantity: 1,
            ..Self::default()
        }
    }
}

impl Credential {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: String::new(),
            username: String::new(),
            password: String::new(),
            website: String::new(),
            notes: String::new(),
            tags: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub category: String,
    pub title: String,
    pub detail: String,
    pub path: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    Browser,
    Books,
    Terminal,
    Credentials,
    #[default]
    Folders,
    Documents,
    Inventory,
    Websites,
    Contacts,
    Games,
    Tools,
    Codex,
    Settings,
}

pub fn validate_date(value: &str) -> crate::error::Result<()> {
    if !value.trim().is_empty()
        && (value.len() != 10 || chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err())
    {
        return Err(crate::error::DeskError::InvalidPath(
            "日期请填写 YYYY-MM-DD，例如 2026-10-20".into(),
        ));
    }
    Ok(())
}
