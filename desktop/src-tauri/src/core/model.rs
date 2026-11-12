use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 快捷路径索引记录；包含名称、原生路径、是否为文件夹和置顶状态。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Shortcut {
    pub id: String,
    pub name: String,
    pub path: std::path::PathBuf,
    pub is_dir: bool,
    pub pinned: bool,
}

/// 账号密码记录；包含标题、用户名、密码、网站、备注和分类标签。
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

#[derive(Serialize)]
pub struct CredentialSearchItem {
    pub id: String,
    pub title: String,
    pub username: String,
    pub website: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentRecord {
    pub path: std::path::PathBuf,
    pub title: String,
}

/// 物品清单记录；包含数量、位置、品牌型号、购买/处置信息和分类标签。
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

/// 网站收藏记录；包含标题、URL、分类和备注。
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

    pub fn validate(&self) -> crate::error::Result<()> {
        if self.id.is_empty() || self.name.trim().is_empty() {
            return Err(crate::error::DeskError::InvalidInput(
                "请填写物品名称".into(),
            ));
        }
        if self.quantity < 0 {
            return Err(crate::error::DeskError::InvalidInput(
                "数量不能小于零".into(),
            ));
        }
        Ok(())
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

    pub fn validate(&self) -> crate::error::Result<()> {
        if self.id.is_empty() || self.title.trim().is_empty() {
            return Err(crate::error::DeskError::InvalidInput(
                "名称和编号不能为空".into(),
            ));
        }
        Ok(())
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
    Essays,
    Books,
    Credentials,
    #[default]
    Folders,
    Documents,
    Inventory,
    Websites,
    Contacts,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_formats_share_one_canonical_value_and_validate_leap_years() {
        for value in [
            "2024-02-29",
            "2024/02/29",
            "2024.02.29",
            "20240229",
            " 2024年02月29日 ",
        ] {
            assert_eq!(normalize_date(value).unwrap(), "2024-02-29");
        }
        assert_eq!(normalize_date("  ").unwrap(), "");
        for value in ["2025-02-29", "2026-13-01", "2026-04-31", "tomorrow"] {
            assert!(normalize_date(value).is_err(), "{value}");
        }
    }

    #[test]
    fn credential_validation_requires_an_id_and_a_name() {
        let valid = Credential {
            id: "credential-1".into(),
            title: "邮箱".into(),
            ..Credential::default()
        };
        valid.validate().unwrap();

        let missing_title = Credential {
            title: "  ".into(),
            ..valid.clone()
        };
        assert_eq!(
            missing_title.validate().unwrap_err().to_string(),
            "名称和编号不能为空"
        );
        let missing_id = Credential {
            id: String::new(),
            ..valid
        };
        assert_eq!(
            missing_id.validate().unwrap_err().to_string(),
            "名称和编号不能为空"
        );
    }

    #[test]
    fn inventory_validation_requires_a_name_and_a_non_negative_quantity() {
        let valid = InventoryItem {
            id: "item-1".into(),
            name: "键盘".into(),
            quantity: 2,
            ..InventoryItem::default()
        };
        valid.validate().unwrap();

        let missing_name = InventoryItem {
            name: "  ".into(),
            ..valid.clone()
        };
        assert_eq!(
            missing_name.validate().unwrap_err().to_string(),
            "请填写物品名称"
        );
        let negative = InventoryItem {
            quantity: -1,
            ..valid
        };
        assert_eq!(
            negative.validate().unwrap_err().to_string(),
            "数量不能小于零"
        );
    }
}
