use crate::jobhunt::Entry;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Keyword {
    pub word: String,
    pub jobs: usize,
}
#[derive(Clone, Debug)]
pub struct Clue {
    pub source: String,
    pub title: String,
    pub excerpt: String,
    pub url: String,
    pub date: String,
}
#[derive(Default)]
pub struct Results {
    pub clues: Vec<Clue>,
    pub statuses: Vec<String>,
}

// Count once per JD; the curated vocabulary makes the meaning of Chinese terms explicit.
pub fn keywords(rows: &[Entry]) -> Vec<Keyword> {
    let vocabulary = [
        "Java",
        "Python",
        "Go",
        "Rust",
        "JavaScript",
        "TypeScript",
        "SQL",
        "MySQL",
        "Redis",
        "Spring",
        "Spring Boot",
        "Spring Cloud",
        "微服务",
        "分布式",
        "高并发",
        "数据库",
        "消息队列",
        "Kafka",
        "Docker",
        "Kubernetes",
        "Linux",
        "算法",
        "数据结构",
        "人工智能",
        "机器学习",
        "大模型",
        "深度学习",
        "自然语言处理",
        "计算机网络",
        "操作系统",
        "测试",
        "自动化",
        "前端",
        "后端",
        "React",
        "Vue",
        "Git",
        "云计算",
    ];
    let mut counts = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for row in rows
        .iter()
        .filter(|r| r.source.ends_with("队列") && !r.description.trim().is_empty())
    {
        if !seen.insert(&row.id) {
            continue;
        }
        let text = row.description.to_lowercase();
        for word in vocabulary {
            if keyword_match(&text, &word.to_lowercase()) {
                *counts.entry(word).or_insert(0usize) += 1;
            }
        }
    }
    let mut result: Vec<_> = counts
        .into_iter()
        .map(|(word, jobs)| Keyword {
            word: word.into(),
            jobs,
        })
        .collect();
    result.sort_by(|a, b| b.jobs.cmp(&a.jobs).then(a.word.cmp(&b.word)));
    result
}
pub fn keyword_match(text: &str, word: &str) -> bool {
    if !word.is_ascii() {
        return text.contains(word);
    }
    text.match_indices(word).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + word.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        boundary(before) && boundary(after)
    })
}
fn complaint(text: &str) -> bool {
    let text = text.to_lowercase();
    [
        "bug",
        "broken",
        "problem",
        "issue",
        "error",
        "fail",
        "slow",
        "frustrat",
        "annoy",
        "difficult",
        "can't",
        "cannot",
        "doesn't",
        "does not",
        "wish",
        "need",
        "缺陷",
        "问题",
        "失败",
        "报错",
        "太慢",
        "难用",
        "希望",
        "需要",
        "无法",
        "不能",
        "卡顿",
        "崩溃",
    ]
    .iter()
    .any(|w| text.contains(w))
}
fn read_json(agent: &ureq::Agent, url: &str) -> Result<serde_json::Value, String> {
    let body = agent
        .get(url)
        .header("User-Agent", "PersonalDeskDemandResearch/1.0")
        .header("Accept", "application/json")
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(2_000_000)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| format!("数据格式异常：{e}"))
}
fn s<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("")
}
fn truncate(s: &str) -> String {
    s.chars().take(450).collect()
}
fn url_for(base: &str, pairs: &[(&str, &str)]) -> String {
    let mut u = url::Url::parse(base).expect("fixed URL");
    u.query_pairs_mut().extend_pairs(pairs.iter().copied());
    u.into()
}
pub fn search(query: &str) -> Results {
    let mut out = Results::default();
    let query = query.trim();
    if query.is_empty() {
        out.statuses.push("请输入搜索关键词".into());
        return out;
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(12)))
        .build()
        .into();
    let gh_query = format!("{query} is:issue is:open in:title,body");
    let gh = url_for(
        "https://api.github.com/search/issues",
        &[("q", &gh_query), ("per_page", "30"), ("sort", "updated")],
    );
    match read_json(&agent, &gh) {
        Ok(value) => {
            if let Some(items) = value.get("items").and_then(|v| v.as_array()) {
                for item in items {
                    let url = s(item, "html_url");
                    if url.starts_with("https://github.com/")
                        && complaint(&format!("{} {}", s(item, "title"), s(item, "body")))
                    {
                        out.clues.push(Clue {
                            source: "GitHub Issues".into(),
                            title: s(item, "title").into(),
                            excerpt: truncate(s(item, "body")),
                            url: url.into(),
                            date: s(item, "updated_at").into(),
                        });
                    }
                }
                out.statuses.push(format!(
                    "GitHub Issues：扫描 {} 条开放问题，匹配 {} 条候选线索",
                    items.len(),
                    out.clues.len()
                ));
            } else {
                out.statuses
                    .push("GitHub Issues：返回数据缺少结果列表".into());
            }
        }
        Err(e) => out.statuses.push(format!(
            "GitHub Issues：读取失败（可能限流或网络不可达）：{e}"
        )),
    }
    let hn = url_for(
        "https://hn.algolia.com/api/v1/search",
        &[("query", query), ("tags", "comment"), ("hitsPerPage", "30")],
    );
    match read_json(&agent, &hn) {
        Ok(value) => {
            if let Some(items) = value.get("hits").and_then(|v| v.as_array()) {
                let mut n = 0;
                for item in items {
                    let id = s(item, "objectID");
                    if !id.chars().all(|c| c.is_ascii_digit()) || id.is_empty() {
                        continue;
                    }
                    let text = s(item, "comment_text");
                    if text.is_empty() {
                        continue;
                    }
                    let plain = scraper::Html::parse_fragment(text)
                        .root_element()
                        .text()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !complaint(&plain) {
                        continue;
                    }
                    out.clues.push(Clue {
                        source: "Hacker News 评论".into(),
                        title: truncate(s(item, "story_title")),
                        excerpt: truncate(&plain),
                        url: format!("https://news.ycombinator.com/item?id={id}"),
                        date: s(item, "created_at").into(),
                    });
                    n += 1;
                }
                out.statuses.push(format!(
                    "Hacker News：扫描 {} 条评论，匹配 {n} 条候选线索",
                    items.len()
                ));
            } else {
                out.statuses
                    .push("Hacker News：返回数据缺少结果列表".into());
            }
        }
        Err(e) => out.statuses.push(format!("Hacker News：读取失败：{e}")),
    }
    match read_json(&agent, "https://www.v2ex.com/api/topics/latest.json") {
        Ok(value) => {
            if let Some(items) = value.as_array() {
                let mut n = 0;
                for item in items {
                    let title = s(item, "title");
                    let body = s(item, "content");
                    let full = format!("{title} {body}");
                    let link = s(item, "url");
                    if full.to_lowercase().contains(&query.to_lowercase())
                        && complaint(&full)
                        && link.starts_with("https://www.v2ex.com/t/")
                    {
                        out.clues.push(Clue {
                            source: "V2EX 最新主题".into(),
                            title: title.into(),
                            excerpt: truncate(body),
                            url: link.into(),
                            date: item
                                .get("created")
                                .and_then(|v| v.as_i64())
                                .and_then(|v| chrono::DateTime::from_timestamp(v, 0))
                                .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
                                .unwrap_or_default(),
                        });
                        n += 1;
                    }
                }
                out.statuses.push(format!(
                    "V2EX：仅扫描最近 {} 条主题，匹配 {n} 条候选线索",
                    items.len()
                ));
            } else {
                out.statuses.push("V2EX：返回数据缺少结果列表".into());
            }
        }
        Err(e) => out.statuses.push(format!("V2EX：读取失败：{e}")),
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_each_jd_once_and_excludes_other_sources() {
        let rows = vec![
            Entry {
                id: "1".into(),
                source: "BOSS 队列".into(),
                description: "Java java Redis".into(),
                ..Default::default()
            },
            Entry {
                id: "2".into(),
                source: "BOSS 队列".into(),
                description: "Java SQL".into(),
                ..Default::default()
            },
            Entry {
                source: "手动记录".into(),
                description: "Java".into(),
                ..Default::default()
            },
        ];
        let k = keywords(&rows);
        assert_eq!(k.iter().find(|x| x.word == "Java").unwrap().jobs, 2);
        assert_eq!(k.iter().find(|x| x.word == "Redis").unwrap().jobs, 1);
    }
    #[test]
    fn boundaries_and_complaint_markers() {
        assert!(!keyword_match("django mongodb", "go"));
        assert!(keyword_match("go语言", "go"));
        assert!(complaint("This is broken"));
        assert!(!complaint("This is a tutorial"));
    }
    #[test]
    fn query_encoding_is_safe() {
        let u = url_for("https://example.com/", &[("q", "a&b 中文")]);
        assert_eq!(
            url::Url::parse(&u).unwrap().query_pairs().next().unwrap().1,
            "a&b 中文"
        );
    }
}
