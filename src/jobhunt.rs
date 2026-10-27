//! Company/job website records and a projection over recruiting data, without copying attempts.
use crate::{
    campus,
    hiring::{store, tracking},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Site {
    pub id: String,
    pub company: String,
    pub position: String,
    pub url: String,
    pub location: String,
    pub notes: String,
    pub revision: i64,
}
impl Site {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            ..Default::default()
        }
    }
    pub fn validate(&mut self) -> Result<(), String> {
        self.company = self.company.trim().into();
        if self.company.is_empty() {
            return Err("请填写公司名称".into());
        }
        if self.company.chars().count() > 200
            || self.position.chars().count() > 300
            || self.notes.chars().count() > 10000
        {
            return Err("公司、岗位或备注过长".into());
        }
        self.url = campus::canonical_url(&self.url).ok_or("请填写完整的 http / https 网址")?;
        Ok(())
    }
}
#[derive(Clone, Default, Serialize)]
pub struct Entry {
    pub id: String,
    pub company: String,
    pub position: String,
    pub url: String,
    pub source_url: String,
    pub location: String,
    pub description: String,
    pub source: String,
    pub status: String,
    pub manual: Option<Site>,
}
impl Entry {
    pub fn matches(&self, query: &str) -> bool {
        matches(
            query,
            &[
                &self.company,
                &self.position,
                &self.location,
                &self.description,
                &self.url,
                &self.source,
                &self.status,
            ],
        )
    }
    pub fn boss(&self) -> bool {
        url::Url::parse(&self.url).ok().is_some_and(|u| {
            u.host_str()
                .is_some_and(|h| h == "www.zhipin.com" || h == "zhipin.com")
        })
    }
}
pub fn matches(query: &str, fields: &[&str]) -> bool {
    let text = fields.join(" ").to_lowercase();
    query
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}
pub fn identity(url: &str) -> String {
    if url::Url::parse(url).ok().is_some_and(|u| {
        u.host_str()
            .is_some_and(|h| h.ends_with(".zhipin.com") || h == "zhipin.com")
    }) {
        store::canonical(url, true).unwrap_or_default()
    } else {
        campus::canonical_url(url).unwrap_or_default()
    }
}
pub fn entries(
    campus: &[campus::Job],
    manual: &[Site],
    snapshot: &tracking::Snapshot,
) -> Vec<Entry> {
    let mut latest = BTreeMap::new();
    for attempt in &snapshot.attempts {
        let key = identity(&attempt.url);
        if key.is_empty() {
            continue;
        }
        let replace = latest
            .get(&key)
            .is_none_or(|old: &&tracking::Attempt| (attempt.at, attempt.id) > (old.at, old.id));
        if replace {
            latest.insert(key, attempt);
        }
    }
    let mut rows = Vec::new();
    for site in manual {
        rows.push(Entry {
            id: format!("manual:{}", site.id),
            company: site.company.clone(),
            position: site.position.clone(),
            url: site.url.clone(),
            location: site.location.clone(),
            description: site.notes.clone(),
            source: "手动记录".into(),
            manual: Some(site.clone()),
            ..Default::default()
        });
    }
    for job in campus.iter().filter(|job| !job.ignored) {
        rows.push(Entry {
            id: format!("campus:{}", job.id),
            company: job.company.clone(),
            position: job.title.clone(),
            url: if job.apply_url.is_empty() {
                job.source_url.clone()
            } else {
                job.apply_url.clone()
            },
            source_url: job.source_url.clone(),
            location: job.location.clone(),
            description: job.summary.clone(),
            source: job.source.clone(),
            status: if job.applied {
                "手动已标记".into()
            } else {
                String::new()
            },
            ..Default::default()
        });
    }
    for job in &snapshot.queue {
        rows.push(Entry {
            id: format!("boss:{}", job.url),
            company: job.company.clone(),
            position: job.title.clone(),
            url: job.url.clone(),
            location: job.location.clone(),
            description: job.jd.clone(),
            source: "BOSS 队列".into(),
            status: format!("队列：{}", tracking::status("boss", &job.status)),
            ..Default::default()
        });
    }
    // An attempt without a queue item (e.g. official delivery) still appears as a website record.
    let known: BTreeSet<_> = rows
        .iter()
        .map(|r| identity(&r.url))
        .filter(|v| !v.is_empty())
        .collect();
    for (url, attempt) in &latest {
        if !known.contains(url) {
            rows.push(Entry {
                id: format!("attempt:{url}"),
                company: attempt.company.clone(),
                position: attempt.title.clone(),
                url: attempt.url.clone(),
                source: if attempt.site == "boss" {
                    "BOSS 投递"
                } else {
                    "官网投递"
                }
                .into(),
                ..Default::default()
            });
        }
    }
    for row in &mut rows {
        if let Some(attempt) = latest.get(&identity(&row.url)) {
            row.status = tracking::status(&attempt.site, &attempt.status).into();
        }
    }
    rows.sort_by(|a, b| a.company.cmp(&b.company).then(a.position.cmp(&b.position)));
    rows
}

#[cfg(test)]
mod tests;
