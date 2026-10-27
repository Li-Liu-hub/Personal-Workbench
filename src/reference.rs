//! Editable, offline programming reference. Shared by UI and persistence.
use crate::error::{DeskError, Result};
use serde::{Deserialize, Serialize};

pub const LANGUAGES: [&str; 8] = [
    "JavaScript",
    "Java",
    "C",
    "C++",
    "Rust",
    "Go",
    "Linux",
    "MySQL",
];
pub const SEED_VERSION: &str = "2";
#[cfg(test)]
pub const BUILTIN_COUNT: usize = 931;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub language: String,
    pub category: String,
    pub title: String,
    pub summary: String,
    pub syntax: String,
    pub example: String,
    pub notes: String,
    pub source: String,
    #[serde(default = "default_type")]
    pub entry_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub revision: i64,
}
pub fn default_type() -> String {
    "语法".into()
}
impl Entry {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            language: "Java".into(),
            category: "基础语法".into(),
            entry_type: default_type(),
            ..Default::default()
        }
    }
    pub fn validate(&self) -> Result<()> {
        for (label, value, max) in [
            ("ID", &self.id, 128),
            ("语言", &self.language, 40),
            ("分类", &self.category, 80),
            ("标题", &self.title, 160),
        ] {
            if value.trim().is_empty() || value.chars().count() > max {
                return Err(DeskError::InvalidInput(format!(
                    "{label}不能为空，且不能超过 {max} 字"
                )));
            }
        }
        validate_tags(&self.tags)?;
        if self.entry_type.trim().is_empty() || self.entry_type.chars().count() > 40 {
            return Err(DeskError::InvalidInput(
                "类型需要填写，且不能超过 40 字".into(),
            ));
        }
        if self.summary.chars().count() > 800
            || self.syntax.len() > 32_768
            || self.example.len() > 65_536
            || self.notes.len() > 32_768
            || self.source.len() > 2048
        {
            return Err(DeskError::InvalidInput(
                "条目内容过长，请拆成多个条目".into(),
            ));
        }
        if !self.source.trim().is_empty()
            && !url::Url::parse(&self.source).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
            })
        {
            return Err(DeskError::InvalidInput(
                "参考地址需要填写完整的 http 或 https 网址".into(),
            ));
        }
        Ok(())
    }
    pub fn search_text(&self) -> String {
        [
            &self.language,
            &self.category,
            &self.title,
            &self.summary,
            &self.syntax,
            &self.example,
            &self.notes,
            &self.entry_type,
        ]
        .into_iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n")
            + "\n"
            + &self.tags.join("\n")
    }
    pub fn indexed_text(&self) -> String {
        self.search_text().to_lowercase()
    }
    pub fn markdown(&self) -> String {
        let language = match self.language.as_str() {
            "JavaScript" => "javascript",
            "Java" => "java",
            "C" => "c",
            "C++" => "cpp",
            "Rust" => "rust",
            "Go" => "go",
            "Linux" => "sh",
            "MySQL" => "sql",
            _ => "text",
        };
        let longest = [&self.syntax, &self.example]
            .into_iter()
            .flat_map(|s| s.split(|c| c != '`'))
            .map(str::len)
            .max()
            .unwrap_or(0);
        let fence = "`".repeat((longest + 1).max(3));
        if self.syntax == self.example {
            return format!(
                "{}\n\n## 示例与场景\n\n{fence}{language}\n{}\n{fence}\n\n## 理解与边界\n\n{}",
                self.summary, self.example, self.notes
            );
        }
        format!(
            "{}\n\n## 语法\n\n{fence}{language}\n{}\n{fence}\n\n## 示例\n\n{fence}{language}\n{}\n{fence}\n\n## 要点\n\n{}",
            self.summary, self.syntax, self.example, self.notes
        )
    }
}

#[derive(Clone, Debug)]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub language: String,
    pub category: String,
    pub summary: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub language: String,
    pub category: String,
    pub query: String,
    pub entry_type: String,
    pub tag: String,
}
impl Filter {
    pub fn terms(&self) -> Vec<String> {
        self.query
            .split_whitespace()
            .take(8)
            .map(|s| {
                let term = s.chars().take(128).collect::<String>().to_lowercase();
                match term.as_str() {
                    "js" => "javascript".into(),
                    "golang" => "go".into(),
                    "cpp" | "cxx" => "c++".into(),
                    _ => term,
                }
            })
            .collect()
    }
}

pub fn validate_tags(tags: &[String]) -> Result<()> {
    if tags.len() > 24
        || tags
            .iter()
            .any(|s| s.trim().is_empty() || s.chars().count() > 80)
    {
        return Err(DeskError::InvalidInput(
            "最多 24 个标签，每个标签需要 1–80 字".into(),
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests;
