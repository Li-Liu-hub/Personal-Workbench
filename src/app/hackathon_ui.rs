use super::*;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{self, Receiver};

const TOOL_ID: &str = "builtin:hackathon-search";
#[derive(Clone, Serialize, Deserialize)]
struct Event {
    title: String,
    details: String,
    url: String,
    #[serde(default)]
    official_url: String,
    source: String,
    collected: String,
}
#[derive(Default)]
pub(super) struct HackathonState {
    open: bool,
    loaded: bool,
    query: String,
    events: Vec<Event>,
    reports: Vec<String>,
    receiver: Option<Receiver<(Vec<Event>, Vec<String>)>>,
    error: String,
}
impl HackathonState {
    pub(super) fn busy(&self) -> bool {
        self.receiver.is_some()
    }
}

fn parse_trip(body: &str) -> Vec<Event> {
    let doc = Html::parse_document(body);
    let cards = Selector::parse("a[href^='/hackathon/ht-']").unwrap();
    let heading = Selector::parse("h2, h3").unwrap();
    let mut events = Vec::new();
    for card in doc.select(&cards) {
        let href = card.value().attr("href").unwrap_or("");
        let title = card
            .select(&heading)
            .next()
            .map(|h| h.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        let details = card
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !title.is_empty() && !events.iter().any(|e: &Event| e.url.ends_with(href)) {
            events.push(Event {
                title,
                details,
                url: format!("https://hackertrip.cn{href}"),
                official_url: String::new(),
                source: "HackerTrip（聚合页面）".into(),
                collected: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
            });
        }
    }
    events
}
fn valid_external_url(value: &str) -> Option<String> {
    let parsed = url::Url::parse(value).ok()?;
    if matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some() {
        Some(parsed.into())
    } else {
        None
    }
}
fn parse_hack123(body: &str) -> Vec<Event> {
    let doc = Html::parse_document(body);
    let cards = Selector::parse("li.card").unwrap();
    let main = Selector::parse("a.main[href^='/h/']").unwrap();
    let heading = Selector::parse(".title").unwrap();
    let link = Selector::parse("a.go[href]").unwrap();
    doc.select(&cards)
        .filter_map(|card| {
            let main = card.select(&main).next()?;
            let href = main.value().attr("href")?;
            let details = main
                .text()
                .collect::<Vec<_>>()
                .join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let title = main
                .select(&heading)
                .next()?
                .text()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_owned();
            if title.is_empty() {
                return None;
            }
            let official_url = card
                .select(&link)
                .next()
                .and_then(|a| a.value().attr("href"))
                .and_then(valid_external_url)
                .unwrap_or_default();
            Some(Event {
                title,
                details,
                url: format!("https://hack123.henryht.sh{href}"),
                official_url,
                source: "黑123（聚合页面）".into(),
                collected: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
            })
        })
        .collect()
}
fn trip_official(body: &str) -> Option<String> {
    let doc = Html::parse_document(body);
    let selector = Selector::parse("a[class*='registerButton'][href]").unwrap();
    doc.select(&selector)
        .next()
        .and_then(|a| a.value().attr("href"))
        .and_then(valid_external_url)
}
fn collect() -> (Vec<Event>, Vec<String>) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .build()
        .into();
    let mut reports = Vec::new();
    let mut events = Vec::new();
    for (name, url) in [
        ("HackerTrip", "https://hackertrip.cn/explore"),
        ("黑123", "https://hack123.henryht.sh/"),
        ("Devpost", "https://devpost.com/hackathons"),
    ] {
        let fetch = || {
            agent
                .get(url)
                .header("User-Agent", "Mozilla/5.0 PersonalDesk/1.0")
                .call()
                .map_err(|e| e.to_string())
                .and_then(|mut r| {
                    r.body_mut()
                        .with_config()
                        .limit(8_000_000)
                        .read_to_string()
                        .map_err(|e| e.to_string())
                })
        };
        let result = fetch().or_else(|_| fetch());
        match result {
            Ok(body) if name == "HackerTrip" || name == "黑123" => {
                let mut found = if name == "HackerTrip" {
                    parse_trip(&body)
                } else {
                    parse_hack123(&body)
                };
                if found.is_empty() {
                    reports.push(format!("{name}：未识别赛事，页面可能已变化"));
                } else {
                    if name == "HackerTrip" {
                        for event in &mut found {
                            if let Ok(mut response) = agent
                                .get(&event.url)
                                .header("User-Agent", "Mozilla/5.0 PersonalDesk/1.0")
                                .call()
                                && let Ok(body) = response
                                    .body_mut()
                                    .with_config()
                                    .limit(4_000_000)
                                    .read_to_string()
                                && let Some(url) = trip_official(&body)
                            {
                                event.official_url = url;
                            }
                        }
                    }
                    let linked = found.iter().filter(|e| !e.official_url.is_empty()).count();
                    reports.push(format!(
                        "{name}：获取 {} 条，其中 {linked} 条提供报名链接",
                        found.len()
                    ));
                    events.extend(found);
                }
            }
            Ok(_) => reports.push(format!("{name}：页面结构暂不支持自动提取，请打开网站查看")),
            Err(e) => reports.push(format!("{name}：访问失败或需网站验证（{e}）")),
        }
    }
    (events, reports)
}
impl PersonalDeskApp {
    pub(super) fn render_hackathon_entry(
        &mut self,
        ui: &mut egui::Ui,
        layout: crate::ui_components::EntryLayout,
    ) {
        if !self.record_matches_filter(TagScope::Tools, TOOL_ID)
            || !matches_query(
                &self.search_query.to_lowercase(),
                &["黑客松比赛", "hackathon 比赛 赛事 活动 搜索 爬虫"],
            )
        {
            return;
        }
        let response = entry(
            ui,
            TOOL_ID,
            "黑客松比赛",
            if self.hackathon.busy() {
                "正在采集 · 点击查看"
            } else {
                "国内外赛事发现 · 来源与原始页面"
            },
            TileKind::Website,
            layout,
        );
        self.favorite_control(ui, &response, "tools", TOOL_ID);
        if response.clicked() {
            self.hackathon.open = true;
        }
    }
    pub(super) fn poll_hackathons(&mut self) {
        let result = match self.hackathon.receiver.as_ref().map(|r| r.try_recv()) {
            Some(Ok(value)) => Some(value),
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.hackathon.receiver = None;
                self.hackathon.error = "采集任务意外结束，请重试".into();
                None
            }
            _ => None,
        };
        if let Some((events, reports)) = result {
            self.hackathon.receiver = None;
            for event in events {
                if let Some(old) = self
                    .hackathon
                    .events
                    .iter_mut()
                    .find(|e| e.url == event.url)
                {
                    let mut event = event;
                    if event.official_url.is_empty() {
                        event.official_url = old.official_url.clone();
                    }
                    *old = event;
                } else {
                    self.hackathon.events.push(event);
                }
            }
            self.hackathon.reports = reports;
            if let Some(db) = self.db.as_mut() {
                match serde_json::to_string(&self.hackathon.events)
                    .map_err(|e| e.to_string())
                    .and_then(|s| {
                        db.set_setting("hackathon.events", &s)
                            .map_err(|e| e.to_string())
                    }) {
                    Ok(()) => {}
                    Err(e) => self.hackathon.error = format!("保存失败：{e}"),
                }
            }
        }
    }
    fn load_hackathons(&mut self) {
        if self.hackathon.loaded {
            return;
        }
        if let Some(db) = self.db.as_ref() {
            match db.setting("hackathon.events") {
                Ok(Some(s)) => match serde_json::from_str(&s) {
                    Ok(events) => self.hackathon.events = events,
                    Err(e) => {
                        self.hackathon.error = format!("读取历史失败：{e}");
                        return;
                    }
                },
                Ok(None) => {}
                Err(e) => {
                    self.hackathon.error = e.to_string();
                    return;
                }
            }
        } else {
            self.hackathon.error = "数据库不可用，无法保存比赛".into();
            return;
        }
        self.hackathon.loaded = true;
    }
    pub(super) fn render_hackathon_page(&mut self, ui: &mut egui::Ui) {
        self.load_hackathons();
        let state = &mut self.hackathon;
        let mut start = false;
        ui.heading("黑客松比赛");
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(state.loaded && !state.busy(), primary_button("更新比赛"))
                .clicked()
            {
                start = true;
            }
            ui.label(if state.busy() {
                "采集中…"
            } else {
                "关键词"
            });
            ui.text_edit_singleline(&mut state.query);
        });
        ui.label(
            "公开网站收集的具体比赛；报名链接由来源提供，时间、资格和真实性请在主办方页面核实。",
        );
        for report in &state.reports {
            ui.label(report);
        }
        if !state.error.is_empty() {
            ui.colored_label(Color32::from_rgb(184, 65, 71), &state.error);
        }
        let query = state.query.to_lowercase();
        let count = state
            .events
            .iter()
            .filter(|e| {
                query.is_empty()
                    || format!("{} {} {}", e.title, e.details, e.source)
                        .to_lowercase()
                        .contains(&query)
            })
            .count();
        ui.label(format!("{count} 场比赛 · 历史信息可能过期"));
        egui::ScrollArea::vertical()
            .id_salt("hackathon-results")
            .show(ui, |ui| {
                for event in state.events.iter().filter(|e| {
                    query.is_empty()
                        || format!("{} {} {}", e.title, e.details, e.source)
                            .to_lowercase()
                            .contains(&query)
                }) {
                    ui.group(|ui| {
                        ui.strong(&event.title);
                        ui.label(&event.details);
                        ui.label(format!("{} · 采集于 {}", event.source, event.collected));
                        ui.horizontal_wrapped(|ui| {
                            ui.hyperlink_to("赛事来源", &event.url);
                            if !event.official_url.is_empty() {
                                ui.hyperlink_to("报名 / 比赛网站", &event.official_url);
                            } else {
                                ui.label("未取得报名网址，请查看赛事来源");
                            }
                        });
                    });
                }
            });
        if start {
            let (tx, rx) = mpsc::channel();
            self.hackathon.receiver = Some(rx);
            self.hackathon.error.clear();
            std::thread::spawn(move || {
                let _ = tx.send(collect());
            });
        }
    }
    pub(super) fn render_hackathon_dialog(&mut self, ctx: &egui::Context) {
        if !self.hackathon.open {
            return;
        }
        let dialog = centered_dialog(ctx, "hackathon-search", "黑客松比赛", "", 1000., |ui| {
            self.render_hackathon_page(ui);
        });
        if dialog_closed(&dialog) {
            self.hackathon.open = false;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires live public sites"]
    fn live_sources() {
        let (events, reports) = collect();
        for report in reports {
            println!("{report}");
        }
        println!("events: {}", events.len());
        assert!(!events.is_empty(), "no source returned recognizable events");
    }
    #[test]
    fn extracts_event_and_its_registration_url() {
        let items = parse_hack123(
            r#"<li class='card'><a class='main' href='/h/sample'><span class='title'>示例比赛</span><span>线上 截止 11-02</span></a><a class='go' href='https://example.org/register'>报名</a></li>"#,
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "示例比赛");
        assert_eq!(items[0].official_url, "https://example.org/register");
        assert!(items[0].details.contains("11-02"));
        assert_eq!(trip_official(r#"<a class='event-detail_registerButton__abc' href='https://example.org/apply'>报名</a>"#).as_deref(), Some("https://example.org/apply"));
        assert!(valid_external_url("javascript:alert(1)").is_none());
        let legacy = r#"[{"title":"旧比赛","details":"","url":"https://example.org","source":"来源","collected":""}]"#;
        assert!(
            serde_json::from_str::<Vec<Event>>(legacy).unwrap()[0]
                .official_url
                .is_empty()
        );
    }
    #[test]
    fn parses_event_cards_and_ignores_navigation() {
        let items = parse_trip(
            r#"<a href='/hackathon/city/shanghai'><h3>城市</h3></a><a href='/hackathon/ht-123'><h3>AI 黑客松</h3><p>线上 报名中</p></a><a href='/hackathon/ht-123'><h3>重复</h3></a>"#,
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "AI 黑客松");
        assert!(items[0].details.contains("报名中"));
        assert_eq!(items[0].url, "https://hackertrip.cn/hackathon/ht-123");
    }
}
