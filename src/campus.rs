//! Campus recruiting discovery. Every record retains its source; missing fields stay empty.
pub mod cli;
mod crawl_parse;
mod crawler;
mod fetch;
pub use crawler::collect;
mod parse;
#[cfg(test)]
mod tests;

use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub use fetch::search;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Options {
    pub keywords: String,
    pub year: String,
    pub city: String,
    pub ncss: bool,
    pub wonder: bool,
    pub web: bool,
    pub include_unmarked: bool,
    pub pages: u32,
    pub crawl: bool,
    pub crawl_all: bool,
    pub crawl_pages: u32,
    pub crawl_details: u32,
    pub extra_urls: String,
    #[serde(skip)]
    pub tavily_key: String,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            keywords: "Java".into(),
            year: "2027".into(),
            city: String::new(),
            ncss: true,
            wonder: true,
            web: true,
            include_unmarked: false,
            pages: 10,
            crawl: true,
            crawl_all: true,
            crawl_pages: 200,
            crawl_details: 80,
            extra_urls: String::new(),
            tavily_key: String::new(),
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<(), String> {
        if !self.ncss && !self.wonder && !self.web && self.extra_urls.trim().is_empty() {
            return Err("请至少选择一个信息源".into());
        }
        if self.keywords.chars().count() > 100 || self.city.chars().count() > 40 {
            return Err("关键词或城市过长，请缩短搜索条件".into());
        }
        if !self.year.is_empty()
            && (self.year.len() != 4
                || self
                    .year
                    .parse::<u32>()
                    .map_or(true, |v| !(2020..=2100).contains(&v)))
        {
            return Err("毕业届别请输入四位年份，或留空不限".into());
        }
        if !(1..=100).contains(&self.pages) {
            return Err("搜索深度需在 1–100 页之间".into());
        }
        if !(10..=1000).contains(&self.crawl_pages) || self.crawl_details > 500 {
            return Err("采集页数需在 10–1000 之间，详情页最多 500 页".into());
        }
        let urls: Vec<_> = self
            .extra_urls
            .lines()
            .filter(|v| !v.trim().is_empty())
            .collect();
        if urls.len() > 5 {
            return Err("每次最多补充 5 个公告或列表网址".into());
        }
        for value in urls {
            canonical_url(value.trim()).ok_or("补充网址必须是完整的 http 或 https 地址")?;
        }
        Ok(())
    }
    pub fn terms(&self) -> Vec<String> {
        let mut terms: Vec<String> = self
            .keywords
            .split(|c: char| c.is_whitespace() || "，,、;；".contains(c))
            .filter(|v| !v.is_empty())
            .take(4)
            .map(str::to_owned)
            .collect();
        if terms.is_empty() {
            terms.push(String::new());
        }
        terms
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Job {
    pub id: String,
    pub title: String,
    pub company: String,
    pub location: String,
    pub education: String,
    pub cohort: String,
    pub published: String,
    pub deadline: String,
    pub summary: String,
    pub source: String,
    pub source_url: String,
    pub apply_url: String,
    pub checked_at: String,
    pub favorite: bool,
    pub applied: bool,
    pub ignored: bool,
    pub first_seen: String,
}
impl Job {
    pub fn is_campus(&self) -> bool {
        !self.cohort.is_empty()
            || ["校招", "校园招聘", "应届", "毕业生", "实习"]
                .iter()
                .any(|word| format!("{} {}", self.title, self.summary).contains(word))
    }

    pub fn identify(&mut self) {
        self.cohort = self.cohort.replace("届", "");
        self.source_url = canonical_url(&self.source_url).unwrap_or_default();
        self.apply_url = canonical_url(&self.apply_url).unwrap_or_default();
        self.id = format!("{:x}", Sha256::digest(self.source_url.as_bytes()));
        if self.checked_at.is_empty() {
            self.checked_at = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        }
        if self.first_seen.is_empty() {
            self.first_seen = self.checked_at.clone();
        }
    }
    pub fn expired(&self) -> bool {
        ["%Y-%m-%d", "%Y/%m/%d", "%Y.%m.%d"].iter().any(|format| {
            NaiveDate::parse_from_str(&self.deadline, format)
                .is_ok_and(|date| date < Local::now().date_naive())
        })
    }
    pub fn matches(&self, query: &str) -> bool {
        let text = format!(
            "{} {} {} {} {} {} {}",
            self.title,
            self.company,
            self.location,
            self.education,
            self.cohort,
            self.summary,
            self.source
        )
        .to_lowercase();
        query
            .split_whitespace()
            .all(|word| text.contains(&word.to_lowercase()))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceReport {
    pub source: String,
    pub count: usize,
    pub error: String,
}
#[derive(Default)]
pub struct Outcome {
    pub jobs: Vec<Job>,
    pub reports: Vec<SourceReport>,
    pub cancelled: bool,
}
pub enum Progress {
    Status(String),
    Jobs(Vec<Job>),
    Source(SourceReport),
}

pub fn canonical_url(value: &str) -> Option<String> {
    let mut url = url::Url::parse(value.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| {
            !k.starts_with("utm_") && !["spm", "from", "ref", "gclid"].contains(&k.as_ref())
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    // Hash routes are significant on recruiting systems; never strip them.
    Some(url.into())
}

pub fn cancelled(flag: &AtomicBool) -> bool {
    flag.load(Ordering::Relaxed)
}

pub fn export_xlsx(path: &Path, jobs: &[Job]) -> Result<(), String> {
    let parent = path.parent().ok_or("导出目录不可用")?;
    let temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("校招搜索").map_err(|e| e.to_string())?;
    let header = rust_xlsxwriter::Format::new()
        .set_bold()
        .set_background_color(0xEAF0F8);
    for (col, title) in [
        "公司",
        "岗位 / 公告",
        "地点",
        "学历",
        "招聘对象",
        "发布日期",
        "截止日期",
        "信息来源",
        "公告原文",
        "投递链接（来源提供，待核验）",
        "最后读取时间",
        "收藏",
        "已投递",
        "摘要",
    ]
    .iter()
    .enumerate()
    {
        sheet
            .write_string_with_format(0, col as u16, *title, &header)
            .map_err(|e| e.to_string())?;
    }
    for (row, job) in jobs.iter().enumerate() {
        let values: [&str; 14] = [
            &job.company,
            &job.title,
            &job.location,
            &job.education,
            &job.cohort,
            &job.published,
            &job.deadline,
            &job.source,
            &job.source_url,
            &job.apply_url,
            &job.checked_at,
            if job.favorite { "是" } else { "否" },
            if job.applied { "是" } else { "否" },
            &job.summary,
        ];
        for (col, value) in values.iter().enumerate() {
            sheet
                .write_string(row as u32 + 1, col as u16, *value)
                .map_err(|e| e.to_string())?;
        }
    }
    sheet.set_freeze_panes(1, 0).map_err(|e| e.to_string())?;
    sheet
        .autofilter(0, 0, jobs.len() as u32, 13)
        .map_err(|e| e.to_string())?;
    sheet.set_column_width(0, 28).map_err(|e| e.to_string())?;
    sheet.set_column_width(1, 48).map_err(|e| e.to_string())?;
    sheet
        .set_column_range_width(2, 12, 22)
        .map_err(|e| e.to_string())?;
    sheet.set_column_width(13, 75).map_err(|e| e.to_string())?;
    workbook.save(temp.path()).map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
