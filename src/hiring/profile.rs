use super::{Result, err};
use serde_json::{Value, json};
use std::path::Path;
#[derive(Clone)]
pub struct Field {
    pub section: String,
    pub label: String,
    pub value: String,
    pub index: usize,
}
pub struct Profile {
    pub fields: Vec<Field>,
}
pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
pub fn blank(s: &str) -> bool {
    let s = s.trim();
    s.is_empty()
        || ["待补充", "待填写", "待确认", "未知", "TODO", "TBD"]
            .iter()
            .any(|k| s.contains(k))
}
impl Profile {
    pub fn parse(text: &str) -> Self {
        let aliases: Value =
            serde_json::from_str(include_str!("../../assets/hiring/profile_aliases.json")).unwrap();
        let mut fields = Vec::<Field>::new();
        let mut section = String::new();
        let mut index = 0;
        let mut last = None;
        for raw in text.lines() {
            let line = raw.trim();
            if line.starts_with('#') {
                let title = line.trim_start_matches('#').trim();
                if line.starts_with("## ") {
                    section = aliases["SECTION_ALIASES"][title]
                        .as_str()
                        .unwrap_or(title)
                        .into();
                    index = 0;
                } else if line.starts_with("### ") {
                    index += 1;
                }
                last = None;
                continue;
            }
            let line = line.trim_start_matches(['-', '*', ' ']).replace("**", "");
            let pair = if line.starts_with('|') {
                let cells: Vec<_> = line.trim_matches('|').split('|').map(str::trim).collect();
                if cells.len() >= 2
                    && !["字段", "项目", "信息", "名称"].contains(&cells[0])
                    && !cells[0].contains("---")
                {
                    Some((cells[0].to_owned(), cells[1].to_owned()))
                } else {
                    None
                }
            } else {
                line.split_once('：')
                    .or_else(|| line.split_once(": "))
                    .map(|(k, v)| (k.trim().into(), v.trim().trim_matches('`').into()))
            };
            if let Some((label, value)) = pair {
                if label.len() > 100 || label.is_empty() {
                    continue;
                }
                fields.push(Field {
                    section: section.clone(),
                    label,
                    value,
                    index,
                });
                last = Some(fields.len() - 1);
            } else if !line.is_empty() && !line.starts_with("---") {
                if let Some(i) = last {
                    fields[i].value.push('\n');
                    fields[i].value.push_str(&line);
                } else if ["self_evaluation", "skills", "awards", "cover_letter"]
                    .contains(&section.as_str())
                {
                    fields.push(Field {
                        section: section.clone(),
                        label: section.clone(),
                        value: line,
                        index,
                    });
                    last = Some(fields.len() - 1);
                }
            }
        }
        Self { fields }
    }
    pub fn read(path: &Path) -> Result<Self> {
        Ok(Self::parse(&std::fs::read_to_string(path).map_err(err)?))
    }
    fn synonyms(label: &str) -> Vec<String> {
        let aliases: Value =
            serde_json::from_str(include_str!("../../assets/hiring/profile_aliases.json")).unwrap();
        let n = norm(label);
        let mut result = vec![n.clone()];
        for (_, group) in aliases.as_object().unwrap() {
            if let Some(group) = group.as_object() {
                for (key, values) in group {
                    if let Some(values) = values.as_array()
                        && (norm(key) == n
                            || values
                                .iter()
                                .any(|v| v.as_str().is_some_and(|s| norm(s) == n)))
                    {
                        result.extend(values.iter().filter_map(|v| v.as_str()).map(norm));
                        result.push(norm(key));
                    }
                }
            }
        }
        result
    }
    pub fn value_for(&self, field: &Value) -> Option<String> {
        let kind = field["kind"].as_str().unwrap_or("");
        let input = field["input_type"].as_str().unwrap_or("");
        let label = field["label"].as_str().unwrap_or("");
        let hints = format!(
            "{} {} {} {}",
            label,
            field["name"].as_str().unwrap_or(""),
            field["placeholder"].as_str().unwrap_or(""),
            field["aria_label"].as_str().unwrap_or("")
        );
        if kind == "checkbox"
            || input == "password"
            || [
                "验证码",
                "动态码",
                "校验码",
                "captcha",
                "otp",
                "verificationcode",
            ]
            .iter()
            .any(|s| norm(&hints).contains(s))
        {
            return None;
        }
        if field["disabled"] == true || field["readonly"] == true {
            return None;
        }
        let context = field["section"].as_str().unwrap_or("");
        let aliases: Value =
            serde_json::from_str(include_str!("../../assets/hiring/profile_aliases.json")).unwrap();
        let context_section = aliases["SECTION_ALIASES"].as_object().and_then(|a| {
            a.iter()
                .find(|(k, _)| norm(context).contains(&norm(k)))
                .and_then(|(_, v)| v.as_str())
        });
        let nlabel = norm(label);
        let nhints = norm(&hints);
        let mut matches = Vec::new();
        for f in &self.fields {
            if blank(&f.value) || f.section == "documents" {
                continue;
            }
            if context_section.is_some_and(|s| s != f.section) {
                continue;
            }
            if f.index > 1 {
                continue;
            }
            let synonyms = Self::synonyms(&f.label);
            let score = synonyms
                .iter()
                .map(|s| {
                    if *s == nlabel && !s.is_empty() {
                        100
                    } else if s.len() >= 4 && nhints.contains(s) {
                        50
                    } else {
                        0
                    }
                })
                .max()
                .unwrap_or(0);
            if score > 0 {
                matches.push((score, &f.value));
            }
        }
        matches.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        let (score, value) = matches.first()?;
        if matches.iter().any(|(s, v)| s == score && v != value) {
            return None;
        }
        Some((*value).clone())
    }
    pub fn plan(&self, page: &Value) -> (Vec<Value>, Vec<String>) {
        let mut actions = Vec::new();
        let mut missing = Vec::new();
        for field in page["fields"].as_array().into_iter().flatten() {
            if field["kind"] == "file" {
                continue;
            }
            if let Some(value) = self.value_for(field) {
                if field["value"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty())
                    || field["checked"] == true
                {
                    continue;
                }
                actions.push(json!({"id":field["id"],"kind":field["kind"],"custom_select":field["custom_select"],"value":value}));
            } else if field["required"] == true
                && field["disabled"] != true
                && field["value"].as_str().is_none_or(|s| s.trim().is_empty())
                && field["checked"] != true
            {
                missing.push(field["label"].as_str().unwrap_or("未识别字段").into());
            }
        }
        (actions, missing)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markdown_alias_placeholder_and_context() {
        let p = Profile::parse(
            "## 基本信息\n- 姓名：测试人\n- 手机号：13800000000\n- 现居城市：待补充\n## 教育经历\n### 本科\n- 学校：测试大学",
        );
        assert_eq!(
            p.value_for(&json!({"label":"真实姓名"})),
            Some("测试人".into())
        );
        assert!(p.value_for(&json!({"label":"现居城市"})).is_none());
        assert!(
            p.value_for(&json!({"label":"手机验证码","name":"otp"}))
                .is_none()
        );
        assert_eq!(
            p.value_for(&json!({"label":"毕业院校","section":"教育经历"})),
            Some("测试大学".into())
        );
    }
    #[test]
    fn ambiguous_repeated_field_is_not_guessed() {
        let p = Profile::parse(
            "## 教育经历\n### 学历\n- 开始时间：2020\n## 项目经历\n### 项目\n- 开始时间：2024",
        );
        assert!(p.value_for(&json!({"label":"开始时间"})).is_none());
    }
}
