use super::{Job, canonical_url, parse};
use scraper::{Html, Selector};
use serde_json::Value;

fn selector(s: &str) -> Selector {
    Selector::parse(s).unwrap()
}
fn field(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().trim().into()
}
fn join(base: &str, href: &str) -> Option<String> {
    canonical_url(url::Url::parse(base).ok()?.join(href).ok()?.as_str())
}

/// Parse public data, never execute the site's scripts.
pub(super) fn catalog(body: &str, base: &str) -> Result<(Vec<Job>, Vec<String>), String> {
    if base.contains("campus.sma-wiki.cn/campus/data/") {
        let rows: Vec<Value> = serde_json::from_str(body).map_err(|e| e.to_string())?;
        let mut jobs = Vec::new();
        for row in rows {
            if row["isCommercial"].as_bool() == Some(true) {
                continue;
            }
            let company = field(&row, "company");
            let batch = field(&row, "batch");
            let positions = field(&row, "positions");
            let source_url = canonical_url(&field(&row, "sourceLink"))
                .or_else(|| canonical_url(&field(&row, "appLink")));
            let Some(source_url) = source_url else {
                continue;
            };
            let mut cohort = parse::cohort(&company);
            if cohort.is_empty() {
                for year in 2020..=2100 {
                    if batch.contains(&format!("{}届", year % 100)) {
                        cohort = format!("{year}届");
                        break;
                    }
                }
            }
            // appLink may point back to an announcement; only label direct web entries.
            let apply = canonical_url(&field(&row, "appLink")).unwrap_or_default();
            let apply_url = if direct_entry(&apply) {
                apply
            } else {
                String::new()
            };
            let mut job = Job {
                title: format!("{company} · {batch}（招聘公告）"),
                company,
                location: field(&row, "location"),
                cohort,
                published: field(&row, "fullDate"),
                deadline: field(&row, "deadline"),
                summary: format!(
                    "招聘方向：{positions}\n批次：{batch}\n公开目录：{base}\n投递参考：{}",
                    field(&row, "appLink")
                ),
                source: "SMA 校招公开目录".into(),
                source_url,
                apply_url,
                ..Default::default()
            };
            job.identify();
            jobs.push(job);
        }
        return Ok((jobs, vec![]));
    }
    let doc = Html::parse_document(body);
    let mut jobs = Vec::new();
    let mut links = Vec::new();
    if base.contains("campus.sma-wiki.cn") {
        let raw = body
            .split("const META = ")
            .nth(1)
            .ok_or("公开目录结构已变化")?;
        let meta: Value = serde_json::Deserializer::from_str(raw)
            .into_iter::<Value>()
            .next()
            .ok_or("公开目录元信息为空")?
            .map_err(|_| "公开目录元信息无法解析")?;
        for file in meta["files"].as_array().ok_or("公开目录缺少分批索引")? {
            if let Some(file) = file.as_str()
                && file.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
                && let Some(link) = join(base, &format!("data/{file}.json"))
            {
                links.push(link);
            }
        }
    } else if base.contains("qiuzhaowang.org") {
        for row in doc.select(&selector("article.job-row")) {
            let attr = |k| row.value().attr(k).unwrap_or_default().to_owned();
            let Some(source_url) = join(base, &attr("data-job-href")) else {
                continue;
            };
            let mut job = Job {
                title: attr("data-job-title"),
                company: attr("data-job-company"),
                location: attr("data-job-city"),
                cohort: attr("data-job-cohort"),
                summary: format!(
                    "招聘方向：{}\n批次：{}\n公开目录：{base}",
                    attr("data-job-title-details"),
                    attr("data-job-batch")
                ),
                source: "秋招网公开目录".into(),
                source_url,
                ..Default::default()
            };
            // The public catalog's membership/login buttons are not application URLs.
            job.identify();
            jobs.push(job);
        }
    } else if base.contains("wondercv.com/xiaozhao/") {
        jobs = parse::wonder(body, base);
    } else {
        jobs = parse::details(body, base, "招聘网站直采");
        links.extend(parse::links(body, base).into_iter().map(|(_, u)| u));
    }
    for anchor in doc.select(&selector("a[href], link[rel=next]")) {
        let text = anchor.text().collect::<String>();
        let rel = anchor.value().attr("rel").unwrap_or_default();
        if (rel.split_whitespace().any(|v| v == "next")
            || ["下一页", "下页"].iter().any(|v| text.trim() == *v))
            && let Some(href) = anchor.value().attr("href")
            && let Some(link) = join(base, href)
            && url::Url::parse(&link)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned))
                == url::Url::parse(base)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_owned))
        {
            links.push(link);
        }
    }
    Ok((jobs, links))
}

pub(super) fn direct_entry(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default();
    if [
        "mp.weixin.qq.com",
        "www.nowcoder.com",
        "qiuzhaowang.org",
        "campus.sma-wiki.cn",
    ]
    .contains(&host)
    {
        return false;
    }
    !["/login", "/membership", "/account"]
        .iter()
        .any(|v| url.path().starts_with(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_preserves_announcements_and_rejects_fake_apply_links() {
        let (jobs, _) = catalog(r#"[{"company":"某科技2027届校园招聘","batch":"27届秋招","positions":"Java,测试","sourceLink":"https://mp.weixin.qq.com/s/abc","appLink":"https://mp.weixin.qq.com/s/abc"},{"company":"另一公司","batch":"27届秋招","sourceLink":"https://example.org/notice/1","appLink":"https://example.org/campus"}]"#, "https://campus.sma-wiki.cn/campus/data/p1.json").unwrap();
        assert_eq!(jobs.len(), 2);
        assert!(jobs[0].apply_url.is_empty());
        assert_eq!(jobs[1].cohort, "2027");
        assert!(jobs[1].apply_url.ends_with("/campus"));
    }
    #[test]
    fn paging_stays_on_site_and_does_not_use_membership() {
        let (jobs,links)=catalog(r#"<article class="job-row" data-job-title="校招" data-job-company="公司" data-job-cohort="2027届" data-job-href="/jobs/1"><a href="/membership">投递</a></article><a rel="next" href="/?page=2">下一页</a><a rel="next" href="https://evil.example/">下一页</a>"#, "https://qiuzhaowang.org/").unwrap();
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0].apply_url.is_empty());
        assert_eq!(links, vec!["https://qiuzhaowang.org/?page=2"]);
    }
}
