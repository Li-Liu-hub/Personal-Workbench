use super::*;
use crate::hiring::platforms::Platform;
impl HiringState {
    pub(super) fn unified_panel(&mut self, ui: &mut egui::Ui, dir: &std::path::Path) {
        ui.heading("自动求职");
        ui.label("四个平台同时搜索 → 带平台、账号入库 → 统一去重筛选 → 投递 → 检查今日额度；未达到上限继续下一轮。");
        ui.label("相同岗位优先保留：BOSS直聘 → 智联招聘 → 前程无忧 → 实习僧。15天去重与待核验保护覆盖所有平台。");
        ui.horizontal_wrapped(|ui| {
            ui.label("关键词");
            ui.add(egui::TextEdit::singleline(&mut self.settings.keyword).desired_width(180.));
            ui.label("城市");
            ui.add(egui::TextEdit::singleline(&mut self.settings.city).desired_width(80.));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("今日全平台总上限");
            ui.add(egui::DragValue::new(&mut self.settings.daily_limit).range(1..=150));
            ui.label("项");
            ui.label("本次最多搜索");
            ui.add(egui::DragValue::new(&mut self.settings.unified_max_pages).range(1..=50));
            ui.label("页/平台");
        });
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let used = self.attempts.iter().filter(|a| a.day == today).count();
        let sent = self
            .attempts
            .iter()
            .filter(|a| a.day == today && a.status == "sent")
            .count();
        ui.label(format!(
            "今日已占用{used}/{}项，确认成功{sent}项。投递间隔{}–{}秒，时段{}。",
            self.settings.daily_limit,
            self.settings.interval_min,
            self.settings.interval_max,
            self.windows
        ));
        ui.checkbox(
            &mut self.confirmed,
            "我已核对四个平台的账号、在线简历及条款，授权按当前条件自动搜索和投递",
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.confirmed, primary_button("开始自动求职"))
                .clicked()
            {
                self.confirmation = Some(Action::Unified(false));
            }
            if ui.button("仅搜索并筛选").clicked() {
                self.action = Some(Action::Unified(true));
            }
            if ui.button("查看统一队列").clicked() {
                self.tab = 7;
            }
        });
        egui::CollapsingHeader::new("平台登录与投递设置").show(ui,|ui|{
            ui.label("BOSS使用当前所选账号，其余平台各使用自己的账号1；来源平台和账号随岗位一同保存。登录、验证码和简历选择在对应网页完成。");
            ui.horizontal_wrapped(|ui|{if ui.button("登录BOSS直聘").clicked(){self.action=Some(Action::Open(Site::Boss,"https://www.zhipin.com/web/geek/chat".into()));}for p in Platform::ALL{if ui.button(format!("登录{}",p.label())).clicked(){self.action=Some(Action::Open(p.site(),p.home().into()));}}});
            self.account_manager(ui,dir);
            ui.label("招呼语（BOSS）");ui.add(egui::TextEdit::multiline(&mut self.settings.greeting).desired_rows(2).desired_width(f32::INFINITY));
            ui.horizontal_wrapped(|ui|{ui.label("投递间隔");ui.add(egui::DragValue::new(&mut self.settings.interval_min).range(30..=3600));ui.label("至");ui.add(egui::DragValue::new(&mut self.settings.interval_max).range(30..=3600));ui.label("秒");});
            ui.horizontal_wrapped(|ui|{ui.label("投递时段");ui.add(egui::TextEdit::singleline(&mut self.windows).desired_width(220.));if ui.button("保存设置").clicked(){self.save_settings(dir);}});
        });
    }
    pub(super) fn platform(&mut self, ui: &mut egui::Ui, dir: &std::path::Path, p: Platform) {
        if self.platform_loaded != Some(p) {
            self.platform_loaded = Some(p);
            self.platform_jobs.clear();
            self.platform_queue.clear();
            self.page = 0;
            self.confirmed = false;
            self.confirmation = None;
            self.report.clear();
        }
        self.platform_queue = self
            .unified_queue
            .iter()
            .filter(|j| hiring::store::site_for_url(&j.url) == p.id())
            .cloned()
            .collect();
        ui.label(format!(
            "{}使用独立浏览器登录。请先在网页完善在线简历；投递使用网页当前选中的简历。",
            p.label()
        ));
        ui.horizontal_wrapped(|ui| {
            if ui.button(format!("登录{}", p.label())).clicked() {
                self.action = Some(Action::Open(p.site(), p.home().into()));
            }
            if ui.button("检查登录").clicked() {
                self.action = Some(Action::PlatformLogin(p));
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("关键词");
            ui.add(egui::TextEdit::singleline(&mut self.settings.keyword).desired_width(180.));
            ui.label("城市");
            ui.add(egui::TextEdit::singleline(&mut self.settings.city).desired_width(80.));
            if ui.button("搜索岗位").clicked() {
                self.page = 0;
                self.action = Some(Action::PlatformSearch(p));
            }
            if ui.button("下一页").clicked() {
                self.page = (self.page + 1).min(49);
                self.action = Some(Action::PlatformSearch(p));
            }
            if ui.button("读取更多").clicked() {
                self.action = Some(Action::PlatformScroll(p));
            }
        });
        ui.small("其他经验、学历、实习/校招等条件请在网页筛选后点击“读取更多”。城市未配置时会提示，请选全国后在网页选择。");
        ui.checkbox(
            &mut self.confirmed,
            "我已核对当前平台账号、在线简历和网页条款，同意申请选定岗位",
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("本轮最多");
            ui.add(egui::DragValue::new(&mut self.queue_limit).range(1..=150));
            ui.label("项");
            if ui
                .add_enabled(self.confirmed, primary_button("执行待投队列"))
                .clicked()
            {
                self.confirmation = Some(Action::PlatformQueue(p));
            }
        });
        ui.small(format!(
            "沿用投递设置：每日最多{}项，间隔{}–{}秒，时段{}；投递前逐项预检，共享15天去重。",
            self.settings.daily_limit,
            self.settings.interval_min,
            self.settings.interval_max,
            self.windows
        ));
        egui::CollapsingHeader::new(format!("搜索结果（{}）", self.platform_jobs.len()))
            .default_open(true)
            .show(ui, |ui| {
                for (i, job) in self.platform_jobs.clone().iter().enumerate() {
                    ui.push_id(("platform-result", p.id(), i), |ui| {
                        ui.label(
                            RichText::new(format!("{} · {}", job.title, job.company)).strong(),
                        );
                        ui.small(format!("{} · {}", job.salary, job.location));
                        self.platform_job_controls(ui, dir, p, job, true);
                        ui.separator();
                    });
                }
            });
        egui::CollapsingHeader::new(format!(
            "{}队列（{}）",
            p.label(),
            self.platform_queue.len()
        ))
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.url)
                        .hint_text("粘贴此平台的职位详情链接")
                        .desired_width(380.),
                );
                if ui.button("加入队列").clicked() {
                    let job = Job {
                        url: self.url.clone(),
                        ..Default::default()
                    };
                    self.platform_add(dir, p, &job);
                }
            });
            for (i, job) in self.platform_queue.clone().iter().enumerate() {
                ui.push_id(("platform-queue", p.id(), i), |ui| {
                    ui.label(format!(
                        "{} · {} · {}",
                        status_label(&job.status),
                        job.title,
                        job.company
                    ));
                    if job.title.is_empty() {
                        ui.small(&job.url);
                    }
                    self.platform_job_controls(ui, dir, p, job, false);
                    ui.horizontal_wrapped(|ui| {
                        for (label, status) in [("设为待投", "pending"), ("跳过", "skipped")]
                        {
                            if ui.small_button(label).clicked() {
                                match hiring::lease::Lease::coordinator(dir).and_then(|_guard| {
                                    Store::open(dir)?.platform_status(p, &job.url, status)
                                }) {
                                    Ok(()) => self.status = "队列已更新".into(),
                                    Err(e) => self.error = e,
                                }
                            }
                        }
                    });
                    ui.separator();
                });
            }
        });
    }
    fn platform_add(&mut self, dir: &std::path::Path, p: Platform, job: &Job) {
        match hiring::lease::Lease::coordinator(dir)
            .and_then(|_guard| Store::open(dir)?.platform_add(p, job))
        {
            Ok(added) => {
                self.status = if added {
                    "已加入队列"
                } else {
                    "队列中已存在"
                }
                .into()
            }
            Err(e) => self.error = e,
        }
        self.refresh_accounts(dir);
    }
    fn platform_job_controls(
        &mut self,
        ui: &mut egui::Ui,
        dir: &std::path::Path,
        p: Platform,
        job: &Job,
        add: bool,
    ) {
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("查看详情").clicked() {
                self.action = Some(Action::PlatformDetail(p, job.clone()));
            }
            if add && ui.small_button("加入队列").clicked() {
                self.platform_add(dir, p, job);
            }
            if ui.small_button("投递预检").clicked() {
                self.action = Some(Action::PlatformApply(p, job.clone(), true));
            }
            if ui
                .add_enabled(self.confirmed, egui::Button::new("投递此岗位").small())
                .clicked()
            {
                self.confirmation = Some(Action::PlatformApply(p, job.clone(), false));
            }
        });
    }
    pub(super) fn unified_queue_panel(&mut self, ui: &mut egui::Ui, dir: &std::path::Path) {
        ui.label("所有平台共用一张岗位队列表及投递记录；已成功投递的同一岗位15天内全平台拦截，待核验记录不会自动过期。");
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.queue_filter, String::new(), "全部平台");
            for site in ["boss", "51job", "zhaopin", "shixiseng"] {
                ui.selectable_value(
                    &mut self.queue_filter,
                    site.into(),
                    hiring::tracking::site_label(site),
                );
            }
            if ui.button("刷新队列").clicked() {
                self.refresh_accounts(dir);
            }
        });
        let jobs: Vec<_> = self
            .unified_queue
            .iter()
            .filter(|j| {
                self.queue_filter.is_empty()
                    || hiring::store::site_for_url(&j.url) == self.queue_filter
            })
            .cloned()
            .collect();
        ui.small(format!(
            "共{}项，其中待投{}项",
            jobs.len(),
            jobs.iter().filter(|j| j.status == "pending").count()
        ));
        egui::Grid::new("unified-hiring-queue")
            .num_columns(6)
            .max_col_width(155.)
            .striped(true)
            .show(ui, |ui| {
                for text in ["平台", "来源账号", "公司", "岗位", "状态", "操作"] {
                    ui.strong(text);
                }
                ui.end_row();
                for job in jobs {
                    let site = hiring::store::site_for_url(&job.url);
                    ui.label(hiring::tracking::site_label(site));
                    let account = self
                        .accounts
                        .iter()
                        .find(|a| a.id == job.account_id)
                        .map(|a| a.label.as_str())
                        .unwrap_or("账号1");
                    ui.label(if site == "boss" { account } else { "账号1" });
                    ui.label(&job.company);
                    ui.label(if job.title.is_empty() {
                        &job.url
                    } else {
                        &job.title
                    });
                    ui.label(status_label(&job.status)).on_hover_text(&job.note);
                    if ui.small_button("打开工具").clicked() {
                        self.confirmed = false;
                        self.confirmation = None;
                        if let Ok(p) = Platform::parse(site) {
                            self.open_platform(p);
                            self.platform_loaded = Some(p);
                            self.platform_jobs = vec![job];
                        } else {
                            self.tab = 0;
                            self.jobs = vec![job];
                        }
                    }
                    ui.end_row();
                }
            });
    }
}
