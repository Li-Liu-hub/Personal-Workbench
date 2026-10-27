use super::*;
use crate::campus::{self, Job, Options, Progress, SourceReport};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::TryRecvError,
    },
};

pub(super) const TOOL_ID: &str = "builtin:campus-search";
#[cfg(test)]
mod tests;

enum Message {
    Progress(Progress),
    Finished(Result<campus::Outcome, String>),
}
#[derive(Default)]
pub(super) struct CampusState {
    pub open: bool,
    loaded: bool,
    options: Options,
    jobs: Vec<Job>,
    receiver: Option<Receiver<Message>>,
    cancel: Arc<AtomicBool>,
    current: BTreeSet<String>,
    new: BTreeSet<String>,
    reports: Vec<SourceReport>,
    status: String,
    error: String,
    filter: String,
    view: u8,
    only_current: bool,
    only_new: bool,
    hide_expired: bool,
    selected: Option<String>,
    result_page: usize,
}
impl CampusState {
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn cancel_on_exit(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    fn visible(&self, job: &Job) -> bool {
        (match self.view {
            1 => job.favorite && !job.ignored,
            2 => job.applied && !job.ignored,
            3 => job.ignored,
            _ => !job.ignored,
        }) && (!self.only_current || self.current.contains(&job.id))
            && (!self.only_new || self.new.contains(&job.id))
            && (!self.hide_expired || !job.expired())
            && job.matches(&self.filter)
    }
}
impl Drop for CampusState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl PersonalDeskApp {
    #[cfg(debug_assertions)]
    pub(super) fn prepare_campus_preview(&mut self) {
        self.section = Section::Tools;
        self.pdf_images.open = false;
        if let Ok(path) = std::env::var("PERSONAL_DESK_CAMPUS_PREVIEW_RESULTS")
            && let Ok(bytes) = fs::read(path)
            && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
            && let Ok(jobs) = serde_json::from_value::<Vec<Job>>(value["jobs"].clone())
            && let Some(db) = self.db.as_mut()
        {
            let _ = db.save_campus_jobs(&jobs);
        }
        self.load_campus();
        self.campus.open = true;
        self.campus.selected = self.campus.jobs.first().map(|v| v.id.clone());
        if let Some(capture) = self.preview_capture.as_mut() {
            capture.step = 57;
        }
    }
    pub(super) fn open_campus_search(&mut self) {
        self.load_campus();
        if let Some(db) = self.db.as_ref()
            && let Ok(jobs) = db.campus_jobs()
        {
            self.campus.jobs = jobs;
        }
        self.campus.open = true;
    }
    fn load_campus(&mut self) {
        if self.campus.loaded {
            return;
        }
        let Some(db) = self.db.as_ref() else {
            self.campus.error = "数据库不可用，暂时无法保存校招搜索结果".into();
            return;
        };
        match db.campus_jobs() {
            Ok(jobs) => {
                self.campus.jobs = jobs;
                self.campus.loaded = true;
            }
            Err(error) => {
                self.campus.error = error.to_string();
                return;
            }
        }
        match db.setting("campus.options") {
            Ok(Some(value)) => match serde_json::from_str(&value) {
                Ok(options) => self.campus.options = options,
                Err(_) => {
                    self.campus.error =
                        "上次搜索条件无法读取，已使用默认条件；已保存岗位保留".into()
                }
            },
            Ok(None) => {}
            Err(e) => self.campus.error = e.to_string(),
        }
    }
    pub(super) fn render_campus_entry(
        &mut self,
        ui: &mut egui::Ui,
        layout: crate::ui_components::EntryLayout,
    ) {
        if !self.record_matches_filter(TagScope::Tools, TOOL_ID)
            || !matches_query(
                &self.search_query.to_lowercase(),
                &[
                    "校招搜索",
                    "校园招聘 秋招 春招 岗位 公告 求职 Java 官网 内置工具",
                ],
            )
        {
            return;
        }
        let response = entry(
            ui,
            TOOL_ID,
            "校招搜索",
            if self.campus.busy() {
                "正在搜索 · 点击查看结果"
            } else {
                "岗位与招聘公告 · 官网入口 · 收藏与投递记录"
            },
            TileKind::Website,
            layout,
        );
        self.favorite_control(ui, &response, "tools", TOOL_ID);
        if response.clicked() {
            self.load_campus();
            self.campus.open = true;
        }
    }
    fn start_campus(&mut self) {
        if self.campus.busy() {
            return;
        }
        if let Err(error) = self.campus.options.validate() {
            self.campus.error = error;
            return;
        }
        let Some(db) = self.db.as_mut() else {
            self.campus.error = "数据库不可用".into();
            return;
        };
        let saved = serde_json::to_string(&self.campus.options)
            .map_err(DeskError::from)
            .and_then(|v| db.set_setting("campus.options", &v));
        if let Err(error) = saved {
            self.campus.error = error.to_string();
            return;
        }
        let options = self.campus.options.clone();
        let (sender, receiver) = mpsc::channel();
        self.campus.receiver = Some(receiver);
        self.campus.cancel = Arc::new(AtomicBool::new(false));
        self.campus.current.clear();
        self.campus.result_page = 0;
        self.campus.new.clear();
        self.campus.reports.clear();
        self.campus.error.clear();
        self.campus.status = "开始搜索，可关闭面板，结果会继续保存…".into();
        let cancel = self.campus.cancel.clone();
        let directory = self.data_dir.clone();
        thread::spawn(move || {
            let result = campus::collect(&options, &directory, &cancel, |p| {
                let _ = sender.send(Message::Progress(p));
            });
            let _ = sender.send(Message::Finished(result));
        });
    }
    pub(super) fn poll_campus(&mut self) {
        loop {
            let event = match self.campus.receiver.as_ref().map(|r| r.try_recv()) {
                Some(Ok(event)) => event,
                Some(Err(TryRecvError::Disconnected)) => {
                    self.campus.receiver = None;
                    self.campus.error = "搜索任务意外结束，已保存结果保留".into();
                    break;
                }
                _ => break,
            };
            match event {
                Message::Progress(Progress::Status(status)) => self.campus.status = status,
                Message::Progress(Progress::Source(report)) => self.campus.reports.push(report),
                Message::Progress(Progress::Jobs(jobs)) => {
                    let saved = self
                        .db
                        .as_mut()
                        .ok_or(DeskError::UnsupportedFormat)
                        .and_then(|db| db.save_campus_jobs(&jobs));
                    if let Err(e) = saved {
                        self.campus.error = format!("搜索结果保存失败：{e}");
                        self.campus.cancel.store(true, Ordering::Relaxed);
                        continue;
                    }
                    for job in jobs {
                        self.campus.current.insert(job.id.clone());
                        if let Some(old) = self.campus.jobs.iter_mut().find(|old| old.id == job.id)
                        {
                            let flags = (
                                old.favorite,
                                old.applied,
                                old.ignored,
                                old.first_seen.clone(),
                            );
                            *old = job;
                            old.favorite = flags.0;
                            old.applied = flags.1;
                            old.ignored = flags.2;
                            old.first_seen = flags.3;
                        } else {
                            self.campus.new.insert(job.id.clone());
                            self.campus.jobs.push(job);
                        }
                    }
                }
                Message::Finished(result) => {
                    self.campus.receiver = None;
                    match result {
                        Ok(outcome) => {
                            let failures = outcome
                                .reports
                                .iter()
                                .filter(|v| !v.error.is_empty())
                                .count();
                            self.campus.status = format!(
                                "{} · 本次 {} 条，其中新增 {} 条 · {} 个来源提示",
                                if outcome.cancelled {
                                    "已停止，已保存结果保留"
                                } else {
                                    "搜索结束"
                                },
                                self.campus.current.len(),
                                self.campus.new.len(),
                                failures
                            );
                            self.campus
                                .jobs
                                .sort_by(|a, b| b.checked_at.cmp(&a.checked_at));
                        }
                        Err(error) => self.campus.error = error,
                    }
                    break;
                }
            }
        }
    }
    pub(super) fn render_campus_dialog(&mut self, ctx: &egui::Context) {
        if !self.campus.open {
            return;
        }
        self.load_campus();
        let state = &mut self.campus;
        let busy = state.busy();
        let mut start = false;
        let mut export = false;
        let mut changed = None;
        let dialog = centered_dialog(ctx, "campus-search", "校招搜索", "", 1000., |ui| {
            ui.small("搜索岗位与招聘公告，保留原文。信息源持续扩展；搜索线索、届别和投递链接请核对原文。");
            ui.add_enabled_ui(!busy,|ui|{
                ui.horizontal_wrapped(|ui|{
                    ui.label("岗位关键词");
                    ui.add(egui::TextEdit::singleline(&mut state.options.keywords).desired_width(230.).hint_text("Java 后端 测试，多词分别搜索"));
                    ui.label("毕业届别");ui.add(egui::TextEdit::singleline(&mut state.options.year).desired_width(55.).hint_text("不限"));
                    ui.label("城市");ui.add(egui::TextEdit::singleline(&mut state.options.city).desired_width(100.).hint_text("留空全国"));
                });
                ui.horizontal_wrapped(|ui|{
                    ui.checkbox(&mut state.options.ncss,"国家大学生就业服务平台");
                    ui.checkbox(&mut state.options.wonder,"校招公告");
                    ui.checkbox(&mut state.options.web,if state.options.crawl {"公开目录与高校网站"} else {"全网发现"});
                });
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut state.options.crawl,"网站直采（自动翻页与断点续采）");
                    if state.options.crawl {ui.checkbox(&mut state.options.crawl_all,"采集全部方向，入库后筛选");}
                });
                ui.collapsing("补充网址与更多搜索选项",|ui|{
                    if state.options.crawl {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("本轮页面上限");ui.add(egui::DragValue::new(&mut state.options.crawl_pages).range(10..=1000));
                            ui.label("详情页上限");ui.add(egui::DragValue::new(&mut state.options.crawl_details).range(0..=500));
                        });
                        ui.small("直采公开目录、高校就业网及企业详情；每站限速，24 小时内复用已读取页面。整批公告保留为一条，需登录的投递入口请在原网站完成。");
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut state.options.include_unmarked,"含未注明校招的岗位");
                        ui.label("每站列表页数");ui.add(egui::DragValue::new(&mut state.options.pages).range(1..=100));
                    });
                    ui.small("补充公开公告 / 列表网址，每行一个，最多 5 个。暂不支持需要登录或执行脚本的页面。");
                    ui.add(egui::TextEdit::multiline(&mut state.options.extra_urls).desired_width(f32::INFINITY).desired_rows(2).hint_text("https://…"));
                    ui.collapsing("增强全网搜索（可选）",|ui|{
                        ui.small("免费搜索可能受验证限制。填写 Tavily 密钥后，开始搜索将使用该服务，按你的账户额度计费；密钥仅在本次运行保留，不写入数据库或导出文件。");
                        ui.add(egui::TextEdit::singleline(&mut state.options.tavily_key).password(true).hint_text("Tavily API Key，留空使用免费搜索").desired_width(360.));
                    });
                });
            });
            ui.horizontal_wrapped(|ui| {
                start = ui
                    .add_enabled(
                        !busy && state.loaded,
                        primary_button(if state.options.crawl {
                            "开始采集 / 续采"
                        } else {
                            "开始搜索"
                        }),
                    )
                    .clicked();
                if busy {
                    ui.spinner();
                    let stopping = state.cancel.load(Ordering::Relaxed);
                    if ui
                        .add_enabled(!stopping, egui::Button::new("停止搜索"))
                        .clicked()
                    {
                        state.cancel.store(true, Ordering::Relaxed);
                    }
                    if stopping {
                        ui.small("正在停止，当前请求最多等待 10 秒…");
                    }
                }
                export = ui
                    .add_enabled(
                        !state.jobs.is_empty(),
                        egui::Button::new("导出当前列表 Excel"),
                    )
                    .clicked();
                ui.small(format!("已保存 {} 条", state.jobs.len()));
            });
            if !state.status.is_empty() {
                ui.label(&state.status);
            }
            if !state.error.is_empty() {
                ui.colored_label(Color32::from_rgb(184, 65, 71), &state.error);
            }
            if !state.reports.is_empty() {
                ui.collapsing("信息源状态", |ui| {
                    for report in &state.reports {
                        ui.label(format!(
                            "{} · {} 条{}",
                            report.source,
                            report.count,
                            if report.error.is_empty() {
                                String::new()
                            } else {
                                format!(" · {}", report.error)
                            }
                        ));
                    }
                });
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for (value, label) in [(0, "全部"), (1, "收藏"), (2, "已投递"), (3, "已忽略")]
                {
                    ui.selectable_value(&mut state.view, value, label);
                }
                ui.checkbox(&mut state.only_current, "本次结果");
                ui.checkbox(&mut state.only_new, "本次新增");
                ui.checkbox(&mut state.hide_expired, "隐藏已截止");
                ui.add(
                    egui::TextEdit::singleline(&mut state.filter)
                        .desired_width(170.)
                        .hint_text("筛选已保存结果"),
                );
            });
            let visible: Vec<_> = state.jobs.iter().filter(|job| state.visible(job)).collect();
            let pages = visible.len().div_ceil(50).max(1);
            state.result_page = state.result_page.min(pages - 1);
            ui.horizontal_wrapped(|ui| {
                ui.small(format!(
                    "显示 {} 条 · 第 {}/{} 页；未注明届别的岗位请核对原文。",
                    visible.len(),
                    state.result_page + 1,
                    pages
                ));
                if ui
                    .add_enabled(state.result_page > 0, egui::Button::new("上一页"))
                    .clicked()
                {
                    state.result_page -= 1;
                }
                if ui
                    .add_enabled(state.result_page + 1 < pages, egui::Button::new("下一页"))
                    .clicked()
                {
                    state.result_page += 1;
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("campus-results")
                .max_height(330.)
                .min_scrolled_height(120.)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if visible.is_empty() {
                        ui.label(if busy {
                            "正在读取来源，请稍候…"
                        } else {
                            "暂无匹配结果。可以调整条件、信息源或列表筛选。"
                        });
                    }
                    for job in visible.into_iter().skip(state.result_page * 50).take(50) {
                        ui.push_id(&job.id, |ui| {
                            egui::Frame::new()
                                .fill(canvas())
                                .corner_radius(8)
                                .inner_margin(12)
                                .show(ui, |ui| {
                                    if ui
                                        .selectable_label(
                                            state.selected.as_ref() == Some(&job.id),
                                            RichText::new(&job.title).strong(),
                                        )
                                        .clicked()
                                    {
                                        state.selected = Some(job.id.clone());
                                    }
                                    ui.horizontal_wrapped(|ui| {
                                        for value in [&job.company, &job.location, &job.education] {
                                            if !value.is_empty() {
                                                ui.label(value);
                                            }
                                        }
                                        ui.small(if job.cohort.is_empty() {
                                            "届别未注明".into()
                                        } else {
                                            format!("{} 届", job.cohort)
                                        });
                                        if job.expired() {
                                            ui.colored_label(
                                                Color32::from_rgb(184, 65, 71),
                                                "已过注明截止日期",
                                            );
                                        }
                                    });
                                    ui.small(format!(
                                        "{} · 发布 {} · 截止 {}",
                                        job.source,
                                        if job.published.is_empty() {
                                            "未注明"
                                        } else {
                                            &job.published
                                        },
                                        if job.deadline.is_empty() {
                                            "未注明"
                                        } else {
                                            &job.deadline
                                        }
                                    ));
                                    ui.horizontal_wrapped(|ui| {
                                        ui.hyperlink_to("公告原文", &job.source_url);
                                        if !job.apply_url.is_empty() {
                                            ui.hyperlink_to("投递链接（来源提供）", &job.apply_url);
                                        }
                                        let mut query =
                                            url::Url::parse("https://www.bing.com/search").unwrap();
                                        query.query_pairs_mut().append_pair(
                                            "q",
                                            &format!(
                                                "{} {} 校园招聘 官网",
                                                job.company, state.options.year
                                            ),
                                        );
                                        ui.hyperlink_to("查找企业官网", query.as_str());
                                        if ui
                                            .small_button(if job.favorite {
                                                "★ 已收藏"
                                            } else {
                                                "☆ 收藏"
                                            })
                                            .clicked()
                                        {
                                            let mut item = job.clone();
                                            item.favorite = !item.favorite;
                                            changed = Some(item);
                                        }
                                        if ui
                                            .small_button(if job.applied {
                                                "✓ 已投递"
                                            } else {
                                                "标记已投递"
                                            })
                                            .clicked()
                                        {
                                            let mut item = job.clone();
                                            item.applied = !item.applied;
                                            changed = Some(item);
                                        }
                                        if ui
                                            .small_button(if job.ignored {
                                                "恢复"
                                            } else {
                                                "忽略"
                                            })
                                            .clicked()
                                        {
                                            let mut item = job.clone();
                                            item.ignored = !item.ignored;
                                            changed = Some(item);
                                        }
                                    });
                                    if state.selected.as_ref() == Some(&job.id) {
                                        ui.label(&job.summary);
                                        ui.small(format!(
                                            "最后读取：{}；投递前请核验企业与岗位。",
                                            job.checked_at
                                        ));
                                    }
                                });
                            ui.add_space(6.);
                        });
                    }
                });
        });
        if dialog_closed(&dialog) {
            self.campus.open = false;
        }
        if let Some(job) = changed {
            match self
                .db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| db.set_campus_flags(&job))
            {
                Ok(()) => {
                    if let Some(old) = self.campus.jobs.iter_mut().find(|old| old.id == job.id) {
                        *old = job;
                    }
                }
                Err(error) => self.campus.error = error.to_string(),
            }
        }
        if export
            && let Some(path) = rfd::FileDialog::new()
                .set_file_name("校招搜索.xlsx")
                .add_filter("Excel", &["xlsx"])
                .save_file()
        {
            let jobs: Vec<_> = self
                .campus
                .jobs
                .iter()
                .filter(|job| self.campus.visible(job))
                .cloned()
                .collect();
            match campus::export_xlsx(&path, &jobs) {
                Ok(()) => self.notice = format!("已导出 {} 条校招信息", jobs.len()),
                Err(error) => self.campus.error = error,
            }
        }
        if start {
            self.start_campus();
        }
    }
}
