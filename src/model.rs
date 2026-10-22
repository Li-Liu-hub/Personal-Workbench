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

#[derive(Clone, Debug, Default, Hash)]
pub struct CredentialSummary {
    pub id: String,
    pub title: String,
    pub username: String,
    pub password: String,
    pub website: String,
    pub notes: String,
    pub tags: Vec<String>,
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
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub model: String,
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

#[cfg(test)]
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub category: String,
    pub title: String,
    pub detail: String,
    pub path: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    Reference,
    Practice,
    Words,
    Books,
    Credentials,
    #[default]
    Folders,
    Documents,
    Inventory,
    Websites,
    Contacts,
    Games,
    Tools,
    Settings,
}

pub fn validate_date(value: &str) -> crate::error::Result<()> {
    normalize_date(value).map(|_| ())
}
pub fn normalize_date(value: &str) -> crate::error::Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    for format in ["%Y-%m-%d", "%Y/%m/%d", "%Y.%m.%d", "%Y%m%d", "%Y年%m月%d日"] {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(value, format) {
            return Ok(date.format("%Y-%m-%d").to_string());
        }
    }
    Err(crate::error::DeskError::InvalidDate(
        "日期无效，请填写 YYYY-MM-DD，例如 2026-10-20；也可留空".into(),
    ))
}
