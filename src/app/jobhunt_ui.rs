use super::*;
use crate::{
    campus,
    hiring::tracking,
    jobhunt::{self, Entry, Site},
};
use std::time::Instant;

#[derive(Default)]
pub(super) struct JobHuntState {
    rows: Vec<Entry>,
    attempts: Vec<tracking::Attempt>,
    loaded: Option<Instant>,
    tab: u8,
    source: u8,
    company: String,
    page: usize,
    expanded: Option<String>,
    draft: Option<Site>,
    deleting: Option<Site>,
    error: String,
    demand_query: String,
    demand_selected_keyword: String,
    demand_last_query: String,
    demand_filter: String,
    demand_results: crate::demand::Results,
    demand_receiver: Option<std::sync::mpsc::Receiver<crate::demand::Results>>,
}
#[cfg(test)]
mod tests;

impl JobHuntState {
    pub(super) fn dialog_open(&self) -> bool {
        self.draft.is_some() || self.deleting.is_some()
    }
}
impl PersonalDeskApp {
    #[cfg(debug_assertions)]
    pub(super) fn prepare_jobhunt_preview(&mut self) {
        self.section = Section::JobHunt;
        self.pdf_images.open = false;
        if let Ok(path) = std::env::var("PERSONAL_DESK_JOBHUNT_PREVIEW_DATA")
            && let Ok(bytes) = fs::read(path)
            && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
            && let (Ok(campus), Ok(tracking)) = (
                serde_json::from_value::<Vec<campus::Job>>(value["campus"].clone()),
                serde_json::from_value::<tracking::Snapshot>(value["tracking"].clone()),
            )
        {
            self.jobhunt.rows = jobhunt::entries(&campus, &[], &tracking);
            self.jobhunt.attempts = tracking.attempts;
            self.jobhunt.loaded = Some(Instant::now());
        }
        if std::env::var_os("PERSONAL_DESK_JOBHUNT_PREVIEW_HISTORY").is_some() {
            self.jobhunt.tab = 1;
        }
        if let Some(capture) = self.preview_capture.as_mut() {
            capture.step = 57;
        }
    }

    pub(super) fn new_jobhunt_site(&mut self) {
        self.jobhunt.draft = Some(Site::new());
        self.jobhunt.error.clear();
    }
    fn refresh_jobhunt(&mut self) {
        let result = (|| -> Result<_, String> {
            let db = self.db.as_ref().ok_or("数据库不可用")?;
            let sites = db.jobhunt_sites().map_err(|e| e.to_string())?;
            let campus = db.campus_jobs().map_err(|e| e.to_string())?;
            let tracking = tracking::read(&self.data_dir.join("hiring"))?;
            Ok((
                jobhunt::entries(&campus, &sites, &tracking),
                tracking.attempts,
            ))
        })();
        match result {
            Ok((rows, attempts)) => {
                self.jobhunt.rows = rows;
                self.jobhunt.attempts = attempts;
                self.jobhunt.error.clear();
            }
            Err(e) => {
                self.jobhunt.error = format!("读取求职数据失败：{e}；当前保留上次读取的列表。")
            }
        }
        self.jobhunt.loaded = Some(Instant::now());
    }
    pub(super) fn render_jobhunt(&mut self, ui: &mut egui::Ui) {
        let preview =
            cfg!(debug_assertions) && std::env::var_os("PERSONAL_DESK_PREVIEW_JOBHUNT").is_some();
        if !preview
            && self
                .jobhunt
                .loaded
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(3))
        {
            self.refresh_jobhunt();
        }
        ui.ctx().request_repaint_after(Duration::from_secs(3));
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_value(&mut self.jobhunt.tab, 0, "网站")
                .changed()
            {
                self.jobhunt.page = 0;
            }
            if ui
                .selectable_value(&mut self.jobhunt.tab, 1, "投递记录")
                .changed()
            {
                self.jobhunt.page = 0;
            }
            if ui
                .selectable_value(&mut self.jobhunt.tab, 2, "需求搜集")
                .changed()
            {
                self.jobhunt.page = 0;
            }
            if ui
                .selectable_value(&mut self.jobhunt.tab, 3, "黑客松比赛")
                .changed()
            {
                self.jobhunt.page = 0;
            }
            ui.separator();
            if ui.button("开始自动求职").clicked() {
                self.hiring.open = true;
                self.hiring.open_unified();
            }
            if ui.button("校招搜索").clicked() {
                self.open_campus_search();
            }
            ui.menu_button("平台工具", |ui| {
                if ui.button("BOSS 直聘").clicked() {
                    self.open_jobhunt_tool(true, None);
                    ui.close();
                }
                for p in crate::hiring::platforms::Platform::ALL {
                    if ui.button(p.label()).clicked() {
                        self.open_jobhunt_tool(false, None);
                        self.hiring.open_platform(p);
                        ui.close();
                    }
                }
                if ui.button("官网投递").clicked() {
                    self.open_jobhunt_tool(false, None);
                    ui.close();
                }
            });
            if ui.button("刷新数据").clicked() {
                self.refresh_jobhunt();
            }
        });
        if !self.jobhunt.error.is_empty() {
            ui.colored_label(Color32::from_rgb(184, 65, 71), &self.jobhunt.error);
        }
        ui.add_space(6.);
        if self.jobhunt.tab == 0 {
            self.render_jobhunt_sites(ui);
        } else if self.jobhunt.tab == 1 {
            self.render_jobhunt_attempts(ui);
        } else if self.jobhunt.tab == 3 {
            self.render_hackathon_page(ui);
        } else {
            self.render_demand(ui);
        }
    }
    fn render_demand(&mut self, ui: &mut egui::Ui) {
        if let Some(rx) = &self.jobhunt.demand_receiver {
            match rx.try_recv() {
                Ok(result) => {
                    self.jobhunt.demand_results = result;
                    self.jobhunt.demand_receiver = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.jobhunt.demand_receiver = None;
                    self.jobhunt.error = "需求搜索意外中断".into();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ui.ctx().request_repaint_after(Duration::from_millis(200));
                }
            }
        }
        ui.heading("岗位关键词与公开需求线索");
        ui.label("从招聘队列已有的岗位说明统计技能词，每份 JD 同一词只计一次；需要更多岗位时，请在“开始自动求职”窗口选择“仅搜索并筛选”。词表覆盖常见技术，不能代表全部关键词。");
        let words = crate::demand::keywords(&self.jobhunt.rows);
        let jd_count = self
            .jobhunt
            .rows
            .iter()
            .filter(|r| r.source.ends_with("队列") && !r.description.trim().is_empty())
            .count();
        ui.small(format!(
            "当前有 {jd_count} 份带说明的队列岗位 · 点击关键词填入搜索"
        ));
        ui.horizontal_wrapped(|ui| {
            for word in words.iter().take(40) {
                if ui
                    .button(format!("{} ({})", word.word, word.jobs))
                    .clicked()
                {
                    self.jobhunt.demand_query = word.word.clone();
                    self.jobhunt.demand_selected_keyword = word.word.clone();
                    self.jobhunt.demand_filter = word.word.clone();
                }
            }
        });
        if !self.jobhunt.demand_selected_keyword.is_empty() {
            let keyword = self.jobhunt.demand_selected_keyword.to_lowercase();
            let matched: Vec<_> = self
                .jobhunt
                .rows
                .iter()
                .filter(|r| {
                    r.source.ends_with("队列")
                        && crate::demand::keyword_match(&r.description.to_lowercase(), &keyword)
                })
                .collect();
            egui::CollapsingHeader::new(format!(
                "包含 {} 的岗位（{}）",
                self.jobhunt.demand_selected_keyword,
                matched.len()
            ))
            .show(ui, |ui| {
                for row in matched.iter().take(50) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!(
                            "{} · {} · {}",
                            row.company, row.position, row.source
                        ));
                        ui.hyperlink_to("查看岗位", &row.url);
                    });
                }
                if matched.len() > 50 {
                    ui.small("仅展示前 50 条，请在求职网站页查看全部岗位。");
                }
            });
            if ui.button("清除关键词关联筛选").clicked() {
                self.jobhunt.demand_selected_keyword.clear();
                self.jobhunt.demand_filter.clear();
            }
        }
        ui.separator();
        ui.label("搜索 GitHub 开放问题、Hacker News 公开评论及 V2EX 最新主题，并用抱怨词初筛；结果是候选线索，需打开原文判断是否属于真实抱怨或需求。");
        ui.horizontal_wrapped(|ui| {
            ui.label("搜索主题");
            ui.text_edit_singleline(&mut self.jobhunt.demand_query);
            if ui
                .add_enabled(
                    self.jobhunt.demand_receiver.is_none()
                        && !self.jobhunt.demand_query.trim().is_empty(),
                    egui::Button::new("搜索公开社区"),
                )
                .clicked()
            {
                let query = self.jobhunt.demand_query.clone();
                self.jobhunt.demand_last_query = query.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                self.jobhunt.demand_receiver = Some(rx);
                std::thread::spawn(move || {
                    let _ = tx.send(crate::demand::search(&query));
                });
            }
        });
        if self.jobhunt.demand_receiver.is_some() {
            ui.label("正在搜索公开社区…");
        }
        if !self.jobhunt.demand_last_query.is_empty() {
            ui.small(format!("社区搜索词：{}", self.jobhunt.demand_last_query));
        }
        for status in &self.jobhunt.demand_results.statuses {
            ui.small(status);
        }
        ui.horizontal(|ui| {
            ui.label("线索内筛选");
            ui.text_edit_singleline(&mut self.jobhunt.demand_filter);
        });
        let filter = self.jobhunt.demand_filter.to_lowercase();
        let clues: Vec<_> = self
            .jobhunt
            .demand_results
            .clues
            .iter()
            .filter(|c| {
                filter.is_empty()
                    || format!("{} {} {}", c.title, c.excerpt, c.source)
                        .to_lowercase()
                        .contains(&filter)
            })
            .collect();
        ui.label(format!("{} 条候选线索", clues.len()));
        for clue in clues {
            egui::Frame::new()
                .fill(surface())
                .corner_radius(8)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.strong(if clue.title.is_empty() {
                        "社区评论"
                    } else {
                        &clue.title
                    });
                    ui.small(format!("{} · {}", clue.source, clue.date));
                    ui.label(&clue.excerpt);
                    ui.hyperlink_to("查看原文", &clue.url);
                });
            ui.add_space(6.);
        }
    }
    fn render_jobhunt_sites(&mut self, ui: &mut egui::Ui) {
        let companies: std::collections::BTreeSet<_> = self
            .jobhunt
            .rows
            .iter()
            .map(|r| r.company.clone())
            .filter(|v| !v.trim().is_empty())
            .collect();
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "{} 家已注明公司 · {} 条岗位 / 网站",
                companies.len(),
                self.jobhunt.rows.len()
            ));
            egui::ComboBox::from_id_salt("jobhunt-company")
                .selected_text(if self.jobhunt.company.is_empty() {
                    "全部公司"
                } else {
                    &self.jobhunt.company
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(&mut self.jobhunt.company, String::new(), "全部公司")
                        .changed()
                    {
                        self.jobhunt.page = 0;
                    }
                    for company in &companies {
                        if ui
                            .selectable_value(&mut self.jobhunt.company, company.clone(), company)
                            .changed()
                        {
                            self.jobhunt.page = 0;
                        }
                    }
                });
            let sources = [
                "全部来源",
                "校招 / 官网",
                "BOSS",
                "手动记录",
                "前程无忧",
                "智联招聘",
                "实习僧",
            ];
            egui::ComboBox::from_id_salt("jobhunt-source")
                .selected_text(
                    sources
                        .get(usize::from(self.jobhunt.source))
                        .copied()
                        .unwrap_or(sources[0]),
                )
                .show_ui(ui, |ui| {
                    for (value, label) in sources.into_iter().enumerate() {
                        if ui
                            .selectable_value(&mut self.jobhunt.source, value as u8, label)
                            .changed()
                        {
                            self.jobhunt.page = 0;
                        }
                    }
                });
        });
        ui.small(
            "搜索结果自动进入此页；投递状态与求职套件共用记录，列表每3秒刷新。点击岗位展开说明。",
        );
        let rows: Vec<_> = self
            .jobhunt
            .rows
            .iter()
            .filter(|r| {
                r.matches(&self.search_query)
                    && (self.jobhunt.company.is_empty() || r.company == self.jobhunt.company)
                    && match self.jobhunt.source {
                        1 => {
                            !r.boss()
                                && crate::hiring::platforms::Platform::detect(&r.url).is_none()
                                && r.manual.is_none()
                        }
                        2 => r.boss(),
                        3 => r.manual.is_some(),
                        4..=6 => {
                            crate::hiring::platforms::Platform::detect(&r.url)
                                == Some(
                                    crate::hiring::platforms::Platform::ALL
                                        [usize::from(self.jobhunt.source - 4)],
                                )
                        }
                        _ => true,
                    }
            })
            .cloned()
            .collect();
        let total = rows.len();
        pagination(ui, &mut self.jobhunt.page, total);
        let grid = self.view_mode() == ViewMode::Grid;
        let width = if grid {
            ((ui.available_width() - 14.) / 2.).max(280.)
        } else {
            ui.available_width()
        };
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
            |ui| {
                for row in rows.into_iter().skip(self.jobhunt.page * 30).take(30) {
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 0.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.push_id(&row.id, |ui| {
                                egui::Frame::new()
                                    .fill(surface())
                                    .corner_radius(10)
                                    .inner_margin(14)
                                    .show(ui, |ui| {
                                        ui.set_width((width - 28.).max(240.));
                                        ui.label(
                                            RichText::new(if row.company.is_empty() {
                                                "公司未注明"
                                            } else {
                                                &row.company
                                            })
                                            .strong(),
                                        );
                                        if ui
                                            .selectable_label(
                                                self.jobhunt.expanded.as_ref() == Some(&row.id),
                                                if row.position.is_empty() {
                                                    "公司招聘网站"
                                                } else {
                                                    &row.position
                                                },
                                            )
                                            .clicked()
                                        {
                                            self.jobhunt.expanded = if self
                                                .jobhunt
                                                .expanded
                                                .as_ref()
                                                == Some(&row.id)
                                            {
                                                None
                                            } else {
                                                Some(row.id.clone())
                                            };
                                        }
                                        ui.horizontal_wrapped(|ui| {
                                            ui.small(&row.location);
                                            ui.small(&row.source);
                                            if !row.status.is_empty() {
                                                ui.label(&row.status);
                                            }
                                        });
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&row.url).small().color(muted()),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&row.url);
                                        ui.horizontal_wrapped(|ui| {
                                            ui.hyperlink_to("打开网站", &row.url);
                                            if !row.source_url.is_empty()
                                                && row.source_url != row.url
                                            {
                                                ui.hyperlink_to("公告原文", &row.source_url);
                                            }
                                            if ui.small_button(row.tool_label()).clicked() {
                                                self.open_jobhunt_tool(row.boss(), Some(&row));
                                            }
                                            if let Some(site) = &row.manual {
                                                if ui.small_button("编辑").clicked() {
                                                    self.jobhunt.draft = Some(site.clone());
                                                }
                                                if ui.small_button("删除").clicked() {
                                                    self.jobhunt.deleting = Some(site.clone());
                                                }
                                            }
                                        });
                                        if self.jobhunt.expanded.as_ref() == Some(&row.id) {
                                            ui.label(&row.description);
                                        }
                                    });
                            });
                        },
                    );
                }
            },
        );
        if total == 0 {
            ui.label("暂无匹配网站。可开始校招搜索，或添加公司、岗位与网址。");
        }
    }
    fn render_jobhunt_attempts(&mut self, ui: &mut egui::Ui) {
        ui.small("与四个招聘平台及官网投递共用记录；部分发送和待核验分别保留。");
        let rows: Vec<_> = self
            .jobhunt
            .attempts
            .iter()
            .filter(|r| {
                jobhunt::matches(
                    &self.search_query,
                    &[
                        &r.company,
                        &r.title,
                        &r.url,
                        &r.note,
                        &r.day,
                        tracking::status(&r.site, &r.status),
                        tracking::site_label(&r.site),
                    ],
                )
            })
            .cloned()
            .collect();
        pagination(ui, &mut self.jobhunt.page, rows.len());
        if rows.is_empty() {
            ui.label("暂无匹配投递记录。使用求职套件投递后，记录会自动出现在这里。");
        }
        for attempt in rows.into_iter().skip(self.jobhunt.page * 30).take(30) {
            ui.push_id(attempt.id, |ui| {
                egui::Frame::new()
                    .fill(surface())
                    .corner_radius(8)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_width((ui.available_width() - 24.).max(200.));
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(if attempt.company.is_empty() {
                                    "公司未注明"
                                } else {
                                    &attempt.company
                                })
                                .strong(),
                            );
                            ui.label(if attempt.title.is_empty() {
                                "岗位未注明"
                            } else {
                                &attempt.title
                            });
                            ui.label(tracking::status(&attempt.site, &attempt.status));
                        });
                        let time = chrono::DateTime::from_timestamp(attempt.at, 0)
                            .filter(|_| attempt.at > 0)
                            .map(|t| {
                                t.with_timezone(&chrono::Local)
                                    .format("%Y-%m-%d %H:%M")
                                    .to_string()
                            })
                            .unwrap_or(attempt.day);
                        ui.small(format!(
                            "{} · {} · {}",
                            time,
                            tracking::site_label(&attempt.site),
                            attempt.note
                        ));
                        if campus::canonical_url(&attempt.url).is_some() {
                            ui.hyperlink_to("查看岗位 / 投递页面", &attempt.url);
                        }
                    });
            });
        }
    }
    fn open_jobhunt_tool(&mut self, boss: bool, row: Option<&Entry>) {
        self.hiring
            .jobhunt_open(&self.data_dir.join("hiring"), boss, row);
    }
    pub(super) fn render_jobhunt_dialogs(&mut self, ctx: &egui::Context) {
        if self.jobhunt.draft.is_some() {
            let mut save = false;
            let draft = self.jobhunt.draft.as_mut().unwrap();
            let dialog = centered_dialog(
                ctx,
                "jobhunt-editor",
                "公司与岗位网站",
                "",
                600.,
                |ui| {
                    for (label, value) in [
                        ("公司", &mut draft.company),
                        ("岗位", &mut draft.position),
                        ("网址", &mut draft.url),
                        ("城市", &mut draft.location),
                    ] {
                        ui.label(label);
                        ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
                    }
                    ui.label("岗位说明 / 备注");
                    ui.add(
                        egui::TextEdit::multiline(&mut draft.notes)
                            .desired_width(f32::INFINITY)
                            .desired_rows(4),
                    );
                    if !self.jobhunt.error.is_empty() {
                        ui.label(&self.jobhunt.error);
                    }
                    save = ui.add(primary_button("保存网站记录")).clicked();
                },
            );
            if save {
                let result = self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.save_jobhunt_site(self.jobhunt.draft.as_mut().unwrap()));
                match result {
                    Ok(()) => {
                        self.jobhunt.draft = None;
                        self.refresh_jobhunt();
                    }
                    Err(e) => self.jobhunt.error = e.to_string(),
                }
            } else if dialog_closed(&dialog) {
                self.jobhunt.draft = None;
            }
        }
        if let Some(site) = self.jobhunt.deleting.clone() {
            let mut confirm = false;
            let dialog =
                centered_dialog(ctx, "jobhunt-delete", "删除网站记录", "", 480., |ui| {
                    ui.label(format!(
                        "确认删除 {} 的 {} 网站记录？",
                        site.company, site.position
                    ));
                    ui.label("投递工具中的历史记录保留。");
                    confirm = ui.button("确认删除记录").clicked();
                    if !self.jobhunt.error.is_empty() {
                        ui.label(&self.jobhunt.error);
                    }
                });
            if confirm {
                match self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.delete_jobhunt_site(&site))
                {
                    Ok(()) => {
                        self.jobhunt.deleting = None;
                        self.refresh_jobhunt();
                    }
                    Err(e) => self.jobhunt.error = e.to_string(),
                }
            } else if dialog_closed(&dialog) {
                self.jobhunt.deleting = None;
            }
        }
    }
}
fn pagination(ui: &mut egui::Ui, page: &mut usize, total: usize) {
    let count = total.div_ceil(30).max(1);
    *page = (*page).min(count - 1);
    ui.horizontal_wrapped(|ui| {
        ui.small(format!("共 {total} 条 · 第 {}/{count} 页", *page + 1));
        if ui
            .add_enabled(*page > 0, egui::Button::new("上一页"))
            .clicked()
        {
            *page -= 1;
        }
        if ui
            .add_enabled(*page + 1 < count, egui::Button::new("下一页"))
            .clicked()
        {
            *page += 1;
        }
    });
}
