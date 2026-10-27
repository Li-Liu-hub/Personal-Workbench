use super::*;
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;

fn select(value: &str) -> Selector {
    Selector::parse(value).expect("constant selector")
}
fn clean(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn text(element: ElementRef<'_>, selector: &str) -> String {
    element
        .select(&select(selector))
        .next()
        .map(|v| clean(&v.text().collect::<String>()))
        .unwrap_or_default()
}
pub(super) fn html_text(value: &str) -> String {
    let document = Html::parse_fragment(value);
    clean(&document.root_element().text().collect::<String>())
}
fn field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(html_text)
        .unwrap_or_default()
}
fn date(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_i64)
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|v| v.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}
pub(super) fn cohort(value: &str) -> String {
    let value: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    (2020..=2100)
        .filter(|year| {
            [
                format!("{year}届"),
                format!("{year}年应届"),
                format!("{year}年校园招聘"),
            ]
            .iter()
            .any(|p| value.contains(p))
        })
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("、")
}
pub(super) fn ncss(body: &str) -> Result<Vec<Job>, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|_| "返回内容不是职位数据，可能需要浏览器验证".to_owned())?;
    let list = value
        .pointer("/data/list")
        .and_then(Value::as_array)
        .ok_or("职位列表结构已变化，请稍后重试")?;
    Ok(list
        .iter()
        .filter_map(|v| {
            let id = field(v, "jobId");
            let title = field(v, "jobName");
            if id.is_empty() || title.is_empty() {
                return None;
            }
            let mut job = Job {
                title,
                company: field(v, "recName"),
                location: field(v, "areaCodeName"),
                education: field(v, "degreeName"),
                published: date(v, "publishDate"),
                source: "国家大学生就业服务平台".into(),
                source_url: format!("https://www.ncss.cn/student/jobs/{id}/detail.html"),
                summary: format!(
                    "专业：{}；企业性质：{}；信息由 {} 提供。届别和截止日期请查看原文。",
                    field(v, "major"),
                    field(v, "recProperty"),
                    field(v, "sourcesNameCh")
                ),
                ..Default::default()
            };
            job.cohort = cohort(&job.title);
            job.identify();
            Some(job)
        })
        .collect())
}
pub(super) fn wonder(body: &str, base: &str) -> Vec<Job> {
    let doc = Html::parse_document(body);
    doc.select(&select("a.campus-job-card"))
        .filter_map(|el| {
            let source_url = url::Url::parse(base)
                .ok()?
                .join(el.value().attr("href")?)
                .ok()?
                .to_string();
            let company = text(el, ".company");
            let summary = text(el, ".summary");
            let tags = text(el, ".info-tags");
            let mut job = Job {
                title: company.clone() + " · 校招公告",
                company,
                summary: summary.clone(),
                location: tags,
                source: "超级简历 · 校招公告".into(),
                source_url,
                published: text(el, ".card-date")
                    .replace("收录", "")
                    .trim()
                    .replace('.', "-"),
                cohort: cohort(&summary),
                ..Default::default()
            };
            if job.company.is_empty() {
                return None;
            }
            job.identify();
            Some(job)
        })
        .collect()
}
fn json_jobs(value: &Value, jobs: &mut Vec<Job>, page_url: &str, source: &str) {
    if let Some(items) = value.as_array() {
        for item in items {
            json_jobs(item, jobs, page_url, source);
        }
        return;
    }
    if let Some(graph) = value.get("@graph") {
        json_jobs(graph, jobs, page_url, source);
    }
    if value.get("@type").and_then(Value::as_str) != Some("JobPosting") {
        return;
    }
    let mut job = Job {
        title: field(value, "title"),
        company: value
            .get("hiringOrganization")
            .map(|v| field(v, "name"))
            .unwrap_or_default(),
        summary: field(value, "description"),
        published: field(value, "datePosted").chars().take(10).collect(),
        deadline: field(value, "validThrough").chars().take(10).collect(),
        source_url: canonical_url(&field(value, "url")).unwrap_or_else(|| page_url.into()),
        source: source.into(),
        ..Default::default()
    };
    if let Some(location) = value.get("jobLocation") {
        let locations: Vec<&Value> = location
            .as_array()
            .map(|v| v.iter().collect())
            .unwrap_or_else(|| vec![location]);
        job.location = locations
            .iter()
            .filter_map(|v| v.get("address"))
            .map(|v| {
                [field(v, "addressRegion"), field(v, "addressLocality")]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("、");
    }
    job.cohort = cohort(&format!("{} {}", job.title, job.summary));
    job.identify();
    if !job.title.is_empty() {
        jobs.push(job);
    }
}
pub(super) fn details(body: &str, page_url: &str, source: &str) -> Vec<Job> {
    let doc = Html::parse_document(body);
    let mut jobs = Vec::new();
    for el in doc.select(&select("script[type='application/ld+json']")) {
        if let Ok(value) = serde_json::from_str::<Value>(&el.inner_html()) {
            json_jobs(&value, &mut jobs, page_url, source);
        }
    }
    if jobs.is_empty() {
        let root = doc.root_element();
        let mut title = text(root, "#jobName, h1, .article-title, h4.tit");
        if title.is_empty() {
            title = text(root, ".title h4, h4");
        }
        let summary = text(
            root,
            ".mainContent, .job-description, .job-detail, .v_news_content, .wp_articlecontent, .Article_Content, #vsb_content, [id^='vsb_content'], .TRS_Editor, article, .content-html",
        );
        if !title.is_empty() && !summary.is_empty() {
            let mut job = Job {
                title,
                summary,
                source: source.into(),
                source_url: page_url.into(),
                ..Default::default()
            };
            job.cohort = cohort(&format!("{} {}", job.title, job.summary));
            job.identify();
            jobs.push(job);
        }
    }
    // Only explicit application links from the main content, never navigation/footer links.
    let mut apply = String::new();
    for el in doc.select(&select(
        "main a[href], article a[href], .details a[href], .jobdetail-box a[href], .content a[href], .v_news_content a[href], .wp_articlecontent a[href], .Article_Content a[href], #vsb_content a[href], [id^='vsb_content'] a[href]",
    )) {
        let label = clean(&el.text().collect::<String>());
        let contextual = el.parent().and_then(ElementRef::wrap).is_some_and(|parent| {
            let value = clean(&parent.text().collect::<String>());
            value.chars().count() <= 350 && ["网申", "投递", "报名网址"].iter().any(|v|value.contains(v))
                && (label.starts_with("http") || label.starts_with("www."))
        });
        if (label.chars().count() <= 35 || contextual)
            && !el.value().classes().any(|c| c == "campus-job-card")
            && (contextual || [
                "投递简历",
                "立即投递",
                "简历投递",
                "官网投递",
                "点击投递",
                "前往网申",
                "网申入口",
                "网申地址",
                "报名入口",
                "报名地址",
                "招聘官网",
                "申请职位",
                "立即申请",
                "立即报名",
            ]
            .iter()
            .any(|v| label.contains(v)))
            && let Some(href) = el.value().attr("href")
            && !href.starts_with('#')
            && let Ok(base) = url::Url::parse(page_url)
            && let Ok(url) = base.join(href)
            && let Some(url) = canonical_url(url.as_str())
            && url != page_url
            && super::crawl_parse::direct_entry(&url)
        {
            apply = url;
            break;
        }
    }
    for job in &mut jobs {
        job.apply_url = apply.clone();
        if job.cohort.is_empty() {
            job.cohort = cohort(&text(
                doc.root_element(),
                ".careers-jobhero, .job-header, .job-basic-info",
            ));
        }
        for highlight in doc.select(&select(".highlight")) {
            let label = text(highlight, "span");
            let value = text(highlight, "strong");
            if label == "学历要求" {
                job.education = value;
            } else if label == "招聘对象" && job.cohort.is_empty() {
                job.cohort = cohort(&value);
            }
        }
    }
    jobs
}

pub(super) fn links(body: &str, base: &str) -> Vec<(String, String)> {
    let doc = Html::parse_document(body);
    let Ok(base) = url::Url::parse(base) else {
        return vec![];
    };
    doc.select(&select("a[href]"))
        .filter_map(|el| {
            let label = clean(&el.text().collect::<String>());
            if label.chars().count() < 10
                || label.chars().count() > 180
                || !["校园招聘", "校招", "应届", "招聘公告", "招聘简章"]
                    .iter()
                    .any(|v| label.contains(v))
            {
                return None;
            }
            let value = base.join(el.value().attr("href")?).ok()?;
            let value = canonical_url(value.as_str())?;
            Some((label, value))
        })
        .take(40)
        .collect()
}

pub(super) fn search_api(body: &str) -> Result<Vec<(String, String, String)>, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|_| "增强搜索返回内容无法解析".to_owned())?;
    let results = value
        .get("results")
        .and_then(Value::as_array)
        .ok_or("增强搜索未返回结果列表")?;
    Ok(results
        .iter()
        .filter_map(|item| {
            Some((
                field(item, "title"),
                canonical_url(&field(item, "url"))?,
                field(item, "content"),
            ))
        })
        .collect())
}

pub(super) fn rss(body: &str) -> Result<Vec<(String, String, String)>, String> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_str(body);
    let mut inside = false;
    let mut key = String::new();
    let mut title = String::new();
    let mut link = String::new();
    let mut description = String::new();
    let mut result = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "item" {
                    inside = true;
                    title.clear();
                    link.clear();
                    description.clear();
                }
                key = name;
            }
            Ok(Event::Text(e)) if inside => {
                let decoded = e.decode().map_err(|e| e.to_string())?;
                let decoded = quick_xml::escape::unescape(&decoded).map_err(|e| e.to_string())?;
                match key.as_str() {
                    "title" => title.push_str(&decoded),
                    "link" => link.push_str(&decoded),
                    "description" => description.push_str(&decoded),
                    _ => {}
                }
            }
            Ok(Event::GeneralRef(e)) if inside => {
                let entity = format!("&{};", e.decode().map_err(|e| e.to_string())?);
                let decoded = quick_xml::escape::unescape(&entity).map_err(|e| e.to_string())?;
                match key.as_str() {
                    "title" => title.push_str(&decoded),
                    "link" => link.push_str(&decoded),
                    "description" => description.push_str(&decoded),
                    _ => {}
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"item" => {
                inside = false;
                if let Some(url) = canonical_url(&link) {
                    result.push((html_text(&title), url, html_text(&description)));
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("搜索服务返回了验证页或暂不可用，请使用其他信息源".into()),
            _ => {}
        }
    }
    if !body.contains("<rss") {
        return Err("搜索服务返回了验证页或暂不可用".into());
    }
    Ok(result)
}
