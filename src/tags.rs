use crate::error::{DeskError, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Books,
    BookPages,
    Credentials,
    Inventory,
    Shortcuts,
    Documents,
    Websites,
    Contacts,
    Tools,
}

impl Scope {
    pub const ALL: [Self; 9] = [
        Self::Books,
        Self::BookPages,
        Self::Credentials,
        Self::Inventory,
        Self::Shortcuts,
        Self::Documents,
        Self::Websites,
        Self::Contacts,
        Self::Tools,
    ];
    pub fn for_section(section: crate::model::Section) -> Option<Self> {
        use crate::model::Section;
        Some(match section {
            Section::Credentials => Self::Credentials,
            Section::Inventory => Self::Inventory,
            Section::Folders => Self::Shortcuts,
            Section::Documents => Self::BookPages,
            Section::Websites => Self::Websites,
            Section::Contacts => Self::Contacts,
            Section::Tools => Self::Tools,
            Section::Books => Self::Books,
            _ => return None,
        })
    }
    pub fn id_column(self) -> &'static str {
        if self == Self::Documents {
            "path"
        } else {
            "id"
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Books => "books",
            Self::BookPages => "book_pages",
            Self::Credentials => "credentials",
            Self::Inventory => "inventory",
            Self::Shortcuts => "shortcuts",
            Self::Documents => "documents",
            Self::Websites => "websites",
            Self::Contacts => "contacts",
            Self::Tools => "tools",
        }
    }
    pub fn table(self) -> &'static str {
        match self {
            Self::Books => "books",
            Self::BookPages => "book_pages",
            Self::Credentials => "credentials",
            Self::Inventory => "inventory_items",
            Self::Shortcuts => "shortcuts",
            Self::Documents => "documents",
            Self::Websites => "websites",
            Self::Contacts => "contacts",
            Self::Tools => "app_tools",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Books => "图书",
            Self::BookPages => "文档",
            Self::Credentials => "账号",
            Self::Inventory => "物品",
            Self::Shortcuts => "快捷路径",
            Self::Documents => "文档",
            Self::Websites => "网站",
            Self::Contacts => "联系人",
            Self::Tools => "小工具",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tag {
    pub scope: Scope,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordTags {
    pub scope: Scope,
    pub id: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Filter {
    #[default]
    All,
    Untagged,
    Named(String),
}

impl Filter {
    pub fn matches(&self, tags: &[String]) -> bool {
        match self {
            Self::All => true,
            Self::Untagged => tags.is_empty(),
            Self::Named(name) => tags.contains(name),
        }
    }
}

pub fn name(value: &str) -> Result<String> {
    let name = value.trim();
    if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
        return Err(DeskError::InvalidPath(
            "标签名称需要 1 至 40 个字符，不能包含换行。".into(),
        ));
    }
    Ok(name.to_owned())
}

pub fn normalize(values: &[String]) -> Result<Vec<String>> {
    let mut normalized = Vec::new();
    for value in values {
        let name = name(value)?;
        if !normalized.contains(&name) {
            normalized.push(name);
        }
    }
    Ok(normalized)
}
