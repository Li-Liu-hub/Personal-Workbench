use super::*;
use crate::hiring::{
    self, Settings,
    browser::{self, Site},
    store::{Job, Store},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
mod accounts;
#[cfg(test)]
mod tests;
pub(super) const TOOLS: [(&str, &str, &str); 3] = [
    ("builtin:boss", "BOSS 直聘", "岗位搜索 · 招呼投递 · 队列"),
    (
        "builtin:official-apply",
        "官网投递",
        "个人资料 · 自动填表 · 附件上传",
    ),
    ("builtin:otp", "接码", "手机短信转发 · 本地验证码接收"),
];
enum Message {
    Progress(String),
    Done(hiring::Result<Output>),
}
enum Output {
    Text(String),
    Jobs(Vec<Job>),
    Detail(Box<Job>),
    Imported(String),
}
#[derive(Clone)]
enum Action {
    Open(Site, String),
    LoginStatus,
    Search,
    Scroll,
    Detail(Job),
    DryRun(Job),
    Send(Job),
    Queue,
    Pool(Vec<String>, usize),
    Resolve(i64, bool),
    Inspect,
    Autofill,
    OtpFill,
    Preflight,
    Submit,
    Import(PathBuf),
}
pub(super) struct HiringState {
    pub open: bool,
    tab: usize,
    page: usize,
    loaded: bool,
    settings: Settings,
    windows: String,
    jobs: Vec<Job>,
    queue: Vec<Job>,
    history: Vec<String>,
    receiver: Option<Receiver<Message>>,
    cancel: Arc<AtomicBool>,
    receiver_service: Option<hiring::otp::Receiver>,
    otp_checked: bool,
    otp_external: Option<serde_json::Value>,
    url: String,
    title: String,
    company: String,
    queue_limit: usize,
    profile: String,
    profile_dirty: bool,
    status: String,
    error: String,
    report: String,
    confirmed: bool,
    terms: bool,
    upload: bool,
    advance: bool,
    confirmation: Option<Action>,
    action: Option<Action>,
    accounts: Vec<hiring::accounts::Account>,
    account_label: String,
    label_account: String,
    new_account_label: String,
    pool_accounts: Vec<String>,
    pool_limit: usize,
    pool_plan: Option<hiring::store::PoolPlan>,
    attempts: Vec<hiring::tracking::Attempt>,
    shared_busy: bool,
    shared_checked: Option<std::time::Instant>,
}
impl Default for HiringState {
    fn default() -> Self {
        let settings = Settings::default();
        Self {
            open: false,
            tab: 0,
            page: 0,
            loaded: false,
            windows: settings.send_windows.join(","),
            settings,
            jobs: vec![],
            queue: vec![],
            history: vec![],
            receiver: None,
            cancel: Arc::new(AtomicBool::new(false)),
            receiver_service: None,
            otp_checked: false,
            otp_external: None,
            url: String::new(),
            title: String::new(),
            company: String::new(),
            queue_limit: 1,
            profile: String::new(),
            profile_dirty: false,
            status: String::new(),
            error: String::new(),
            report: String::new(),
            confirmed: false,
            terms: false,
            upload: true,
            advance: false,
            confirmation: None,
            action: None,
            accounts: vec![],
            account_label: String::new(),
            label_account: String::new(),
            new_account_label: "账号2".into(),
            pool_accounts: vec![hiring::accounts::DEFAULT_ACCOUNT.into()],
            pool_limit: hiring::accounts::MAX_POOL,
            pool_plan: None,
            attempts: vec![],
            shared_busy: false,
            shared_checked: None,
        }
    }
}
impl HiringState {
    pub(super) fn jobhunt_open(
        &mut self,
        dir: &std::path::Path,
        boss: bool,
        row: Option<&crate::jobhunt::Entry>,
    ) {
        self.open = true;
        if self.busy() {
            return;
        }
        if !self.loaded {
            self.load(dir);
        }
        self.tab = if boss { 0 } else { 1 };
        self.confirmed = false;
        self.confirmation = None;
        self.terms = false;
        if let Some(row) = row {
            self.url = row.url.clone();
            self.title = row.position.clone();
            self.company = row.company.clone();
            if boss {
                self.jobs = vec![Job {
                    url: row.url.clone(),
                    title: row.position.clone(),
                    company: row.company.clone(),
                    location: row.location.clone(),
                    jd: row.description.clone(),
                    ..Default::default()
                }];
            }
        }
    }

    #[cfg(debug_assertions)]
    pub(super) fn preview_tab(&mut self, tab: usize) {
        self.open = true;
        self.tab = tab;
    }
    pub fn busy(&self) -> bool {
        self.receiver.is_some() || self.shared_busy
    }
    fn load(&mut self, dir: &std::path::Path) {
        match Settings::load(dir) {
            Ok(settings) => {
                self.windows = settings.send_windows.join(",");
                self.settings = settings;
            }
            Err(e) => self.error = e,
        }
        match hiring::tracking::read(dir) {
            Ok(snapshot) => {
                self.history = snapshot
                    .attempts
                    .iter()
                    .map(|r| {
                        format!(
                            "{} · {} · {} · {} · {} · {}",
                            r.day, r.site, r.title, r.company, r.status, r.note
                        )
                    })
                    .collect();
                self.queue = snapshot.queue;
            }
            Err(e) => self.error = e,
        }
        if !self.profile_dirty {
            match fs::read_to_string(dir.join("profile.md")) {
                Ok(text) => self.profile = text,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => self.error = e.to_string(),
            }
        }
        self.loaded = true;
        if self.settings.account_id.is_empty() {
            self.settings.account_id = hiring::accounts::DEFAULT_ACCOUNT.into();
        }
        self.refresh_accounts(dir);
    }
    pub fn poll(&mut self, dir: &std::path::Path) -> Option<String> {
        if self.open
            && self
                .shared_checked
                .is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(1))
        {
            self.shared_checked = Some(std::time::Instant::now());
            self.shared_busy = hiring::lease::Lease::coordinator_busy(dir);
            if self.shared_busy && self.receiver.is_none() {
                self.status = "后台正在执行求职任务，可以停止全部账号队列".into();
            }
            if dir.join("hiring.sqlite").exists() {
                self.refresh_accounts(dir);
            }
        }
        loop {
            let message = self.receiver.as_ref()?.try_recv();
            match message {
                Ok(Message::Progress(p)) => self.status = p,
                Ok(Message::Done(result)) => {
                    self.receiver = None;
                    match result {
                        Ok(Output::Text(text)) => {
                            self.status = text.clone();
                            self.report = text;
                        }
                        Ok(Output::Jobs(jobs)) => {
                            self.status = format!("读取到 {} 个岗位", jobs.len());
                            self.jobs = jobs;
                        }
                        Ok(Output::Detail(job)) => {
                            self.report = format!(
                                "{} · {}\n{} · {} · {}\n\n{}",
                                job.title,
                                job.company,
                                job.salary,
                                job.experience,
                                job.education,
                                job.jd
                            )
                        }
                        Ok(Output::Imported(text)) => {
                            self.status = text;
                            self.load(dir);
                        }
                        Err(e) => self.error = e,
                    }
                    if let Ok(q) = Store::open(dir).and_then(|s| s.jobs()) {
                        self.queue = q;
                    }
                    if let Ok(h) = Store::open(dir).and_then(|s| s.history()) {
                        self.history = h;
                    }
                    return Some(self.status.clone());
                }
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.error = "后台任务结束，发送状态请到网页核验".into();
                    return Some(self.error.clone());
                }
            }
        }
    }
    pub fn flush(&mut self, dir: &std::path::Path) -> bool {
        if !self.profile_dirty {
            return true;
        }
        match hiring::atomic_write(&dir.join("profile.md"), self.profile.as_bytes()) {
            Ok(()) => {
                self.profile_dirty = false;
                true
            }
            Err(e) => {
                self.error = e;
                false
            }
        }
    }
    fn start(&mut self, dir: PathBuf, action: Action) {
        if self.busy() {
            return;
        }
        self.error.clear();
        self.status = "正在处理…".into();
        let settings = self.settings.clone();
        let limit = self.queue_limit;
        let page = self.page as u32 + 1;
        let advance = self.advance;
        let upload = self.upload;
        let title = self.title.clone();
        let company = self.company.clone();
        let terms = self.terms;
        let inbox = self.receiver_service.as_ref().map(|s| s.inbox.clone());
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        thread::spawn(move || {
            let result = (|| -> hiring::Result<Output> {
                let managed = matches!(&action, Action::Queue | Action::Pool(..));
                let _coordinator = if managed {
                    None
                } else {
                    Some(hiring::lease::Lease::coordinator(&dir)?)
                };
                let boss_action = matches!(
                    &action,
                    Action::Open(Site::Boss, _)
                        | Action::LoginStatus
                        | Action::Search
                        | Action::Scroll
                        | Action::Detail(_)
                        | Action::DryRun(_)
                        | Action::Send(_)
                );
                let _lease = if managed {
                    None
                } else if boss_action {
                    Some(hiring::lease::Lease::account(&dir, &settings.account_id)?)
                } else {
                    Some(hiring::lease::Lease::acquire(&dir)?)
                };
                match action {
                    Action::Open(site, url) => {
                        browser::open(&dir, &settings, site, &url)?;
                        Ok(Output::Text(
                            "已打开专用浏览器，请在网页完成登录和验证".into(),
                        ))
                    }
                    Action::LoginStatus => {
                        hiring::boss::login_status(&dir, &settings).map(Output::Text)
                    }
                    Action::Search => hiring::boss::search(&dir, &settings, page).map(Output::Jobs),
                    Action::Scroll => hiring::boss::scroll(&dir, &settings).map(Output::Jobs),
                    Action::Detail(job) => hiring::boss::detail(&dir, &settings, &job.url)
                        .map(|job| Output::Detail(Box::new(job))),
                    Action::DryRun(job) => {
                        hiring::boss::deliver(&dir, &settings, &job, true).map(Output::Text)
                    }
                    Action::Send(job) => {
                        hiring::boss::deliver(&dir, &settings, &job, false).map(Output::Text)
                    }
                    Action::Queue => hiring::boss::run_queue(&dir, &settings, limit, cancel, |p| {
                        let _ = sender.send(Message::Progress(p));
                    })
                    .map(Output::Text),
                    Action::Pool(ids, total) => {
                        hiring::boss::run_pool(&dir, &settings, &ids, total, cancel, |p| {
                            let _ = sender.send(Message::Progress(p));
                        })
                        .map(Output::Text)
                    }
                    Action::Resolve(id, sent) => Store::open(&dir)
                        .and_then(|mut store| store.resolve_attempt(id, sent, true))
                        .map(|_| Output::Text("已保存网页核验结果".into())),
                    Action::Inspect => {
                        let mut tab = browser::selected(&dir, Site::Official)?;
                        Ok(Output::Text(hiring::official::summary(
                            &hiring::official::inspect(&mut tab)?,
                        )))
                    }
                    Action::Autofill => {
                        hiring::official::autofill(&dir, advance, upload).map(Output::Text)
                    }
                    Action::OtpFill => {
                        let inbox = match inbox {
                            Some(inbox) => inbox,
                            None => hiring::otp::service_inbox(&dir)?,
                        };
                        hiring::official::otp_fill(&dir, inbox).map(Output::Text)
                    }
                    Action::Preflight => {
                        hiring::official::submit(&dir, &settings, terms, true, &title, &company)
                            .map(Output::Text)
                    }
                    Action::Submit => {
                        hiring::official::submit(&dir, &settings, terms, false, &title, &company)
                            .map(Output::Text)
                    }
                    Action::Import(path) => {
                        hiring::import::legacy(&path, &dir).map(Output::Imported)
                    }
                }
            })();
            let _ = sender.send(Message::Done(result));
        });
    }
    fn save_settings(&mut self, dir: &std::path::Path) {
        let _coordinator = match hiring::lease::Lease::coordinator(dir) {
            Ok(lease) => lease,
            Err(error) => {
                self.error = error;
                return;
            }
        };
        self.settings.send_windows = self
            .windows
            .split([',', '，', ';'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        match self.settings.save(dir) {
            Ok(()) => {
                self.error.clear();
                self.status = "配置已保存".into();
            }
            Err(e) => self.error = e,
        }
    }
    fn boss(&mut self, ui: &mut egui::Ui, dir: &std::path::Path) {
        self.account_manager(ui, dir);
        ui.horizontal_wrapped(|ui| {
            if ui.button("登录 BOSS").clicked() {
                self.action = Some(Action::Open(
                    Site::Boss,
                    "https://www.zhipin.com/web/geek/chat".into(),
                ));
            }
            if ui.button("检查登录").clicked() {
                self.action = Some(Action::LoginStatus);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("关键词");
            ui.add(egui::TextEdit::singleline(&mut self.settings.keyword).desired_width(180.));
            ui.label("城市");
            ui.add(egui::TextEdit::singleline(&mut self.settings.city).desired_width(75.));
        });
        ui.horizontal_wrapped(|ui| {
            for (id, label, current, values) in [
                (
                    "hiring-exp",
                    "经验",
                    &mut self.settings.experience,
                    vec![
                        "",
                        "经验不限",
                        "应届生",
                        "1年以内",
                        "1-3年",
                        "3-5年",
                        "5-10年",
                        "10年以上",
                        "在校生",
                    ],
                ),
                (
                    "hiring-deg",
                    "学历",
                    &mut self.settings.degree,
                    vec!["", "学历不限", "大专", "本科", "硕士", "博士"],
                ),
                (
                    "hiring-type",
                    "类型",
                    &mut self.settings.job_type,
                    vec!["", "实习", "全职", "兼职"],
                ),
            ] {
                egui::ComboBox::from_id_salt(id)
                    .selected_text(format!(
                        "{label}：{}",
                        if current.is_empty() {
                            "不限"
                        } else {
                            current.as_str()
                        }
                    ))
                    .show_ui(ui, |ui| {
                        for v in values {
                            ui.selectable_value(
                                current,
                                v.into(),
                                if v.is_empty() { "不限" } else { v },
                            );
                        }
                    });
            }
            if ui.button("搜索岗位").clicked() {
                self.page = 0;
                self.save_settings(dir);
                self.action = Some(Action::Search);
            }
            if ui.button("加载更多").clicked() {
                self.action = Some(Action::Scroll);
            }
        });
        ui.separator();
        egui::CollapsingHeader::new("投递配置").show(ui, |ui| {
            ui.label("招呼语");
            ui.add(
                egui::TextEdit::multiline(&mut self.settings.greeting)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                if ui.button("添加简历图片").clicked()
                    && let Some(paths) = rfd::FileDialog::new()
                        .add_filter("图片", &["png", "jpg", "jpeg", "webp", "bmp"])
                        .pick_files()
                {
                    self.settings
                        .images
                        .extend(paths.iter().map(|p| p.display().to_string()));
                }
                if ui.button("清空图片").clicked() {
                    self.settings.images.clear();
                }
            });
            for path in &self.settings.images {
                ui.small(path);
            }
            ui.horizontal_wrapped(|ui| {
                ui.label("每个账号每日上限");
                ui.add(
                    egui::DragValue::new(&mut self.settings.daily_limit)
                        .range(1..=hiring::BOSS_MAX_DAILY),
                );
                ui.label("随机间隔（秒）");
                ui.add(egui::DragValue::new(&mut self.settings.interval_min).range(30..=3600));
                ui.label("至");
                ui.add(egui::DragValue::new(&mut self.settings.interval_max).range(30..=3600));
            });
            ui.horizontal(|ui| {
                ui.label("允许时段");
                ui.add(
                    egui::TextEdit::singleline(&mut self.windows)
                        .hint_text("09:00-18:00,19:00-21:00"),
                );
            });
            if ui.button("保存投递配置").clicked() {
                self.save_settings(dir);
            }
        });
        ui.checkbox(
            &mut self.confirmed,
            "已核对本人 BOSS 账号，且原工具的投递队列已停止",
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("本轮最多");
            ui.add(egui::DragValue::new(&mut self.queue_limit).range(1..=hiring::BOSS_MAX_DAILY));
            ui.label("项");
            if ui
                .add_enabled(self.confirmed, primary_button("执行待投队列"))
                .clicked()
            {
                self.confirmation = Some(Action::Queue);
            }
        });
        egui::CollapsingHeader::new(format!("搜索结果（{}）", self.jobs.len()))
            .default_open(true)
            .show(ui, |ui| {
                for (i, job) in self.jobs.clone().iter().enumerate() {
                    ui.push_id(("result", i), |ui| {
                        ui.label(
                            RichText::new(format!("{} · {}", job.title, job.company)).strong(),
                        );
                        ui.small(format!(
                            "{} · {} · {} · {}",
                            job.salary, job.location, job.experience, job.education
                        ));
                        ui.horizontal_wrapped(|ui| {
                            if ui.small_button("查看详情").clicked() {
                                self.action = Some(Action::Detail(job.clone()));
                            }
                            if ui.small_button("加入队列").clicked() {
                                match Store::open(dir).and_then(|mut s| s.add(job)) {
                                    Ok(added) => {
                                        self.status = if added {
                                            "已加入队列"
                                        } else {
                                            "队列中已存在"
                                        }
                                        .into();
                                        if let Ok(q) = Store::open(dir).and_then(|s| s.jobs()) {
                                            self.queue = q;
                                        }
                                    }
                                    Err(e) => self.error = e,
                                }
                            }
                            if ui.small_button("投递预检").clicked() {
                                self.action = Some(Action::DryRun(job.clone()));
                            }
                            if ui
                                .add_enabled(
                                    self.confirmed,
                                    egui::Button::new("投递此岗位").small(),
                                )
                                .clicked()
                            {
                                self.confirmation = Some(Action::Send(job.clone()));
                            }
                        });
                        ui.separator();
                    });
                }
            });
        egui::CollapsingHeader::new(format!("投递队列（{}）", self.queue.len())).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("岗位链接");
                ui.add(
                    egui::TextEdit::singleline(&mut self.url)
                        .desired_width((ui.available_width() - 70.0).max(100.0)),
                );
                if ui.button("添加").clicked() {
                    let job = Job {
                        url: self.url.clone(),
                        ..Default::default()
                    };
                    match Store::open(dir).and_then(|mut s| s.add(&job)) {
                        Ok(_) => self.load(dir),
                        Err(e) => self.error = e,
                    }
                }
            });
            for (i, job) in self.queue.clone().iter().enumerate() {
                ui.push_id(("queue", i), |ui| {
                    ui.label(format!(
                        "{} · {} · {}",
                        status_label(&job.status),
                        job.title,
                        job.company
                    ));
                    ui.horizontal_wrapped(|ui| {
                        if ui.small_button("查看岗位").clicked() {
                            self.action = Some(Action::Detail(job.clone()));
                        }
                        for (label, status) in [("设为待投", "pending"), ("跳过", "skipped")]
                        {
                            if ui.small_button(label).clicked() {
                                match Store::open(dir)
                                    .and_then(|mut s| s.set_status(&job.url, status))
                                {
                                    Ok(()) => self.load(dir),
                                    Err(e) => self.error = e,
                                }
                            }
                        }
                    });
                });
            }
        });
    }
    fn official(&mut self, ui: &mut egui::Ui, dir: &std::path::Path) {
        ui.label("打开公司招聘页，完成登录后识别和填写。自动填表停在最终提交前。");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.url)
                    .hint_text("招聘网页地址")
                    .desired_width((ui.available_width() - 90.).max(160.)),
            );
            if ui.button("打开招聘页").clicked() {
                self.action = Some(Action::Open(Site::Official, self.url.clone()));
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("识别表单").clicked() {
                self.action = Some(Action::Inspect);
            }
            ui.checkbox(&mut self.upload, "上传已配置材料");
            ui.checkbox(&mut self.advance, "推进下一步");
            if ui.button("自动填写").clicked() && self.flush(dir) {
                self.action = Some(Action::Autofill);
            }
            if ui.button("填入验证码").clicked() {
                self.action = Some(Action::OtpFill);
            }
        });
        ui.horizontal(|ui| {
            let field_width = ((ui.available_width() - 100.0) / 2.0).max(120.0);
            ui.label("岗位");
            ui.add(egui::TextEdit::singleline(&mut self.title).desired_width(field_width));
            ui.label("公司");
            ui.add(egui::TextEdit::singleline(&mut self.company).desired_width(field_width));
        });
        ui.checkbox(
            &mut self.terms,
            "我已在网页检查资料、附件及条款，并同意提交申请",
        );
        ui.horizontal(|ui| {
            if ui.button("提交预检").clicked() {
                self.action = Some(Action::Preflight);
            }
            if ui
                .add_enabled(self.terms, primary_button("确认提交申请"))
                .clicked()
            {
                self.confirmation = Some(Action::Submit);
            }
        });
        egui::CollapsingHeader::new("个人资料（仅保存在本机）").show(ui, |ui| {
            ui.label(
                "支持原工具的 Markdown 资料格式；“待补充”字段会跳过，无法确定的字段需手动填写。",
            );
            self.profile_dirty |= ui
                .add(
                    egui::TextEdit::multiline(&mut self.profile)
                        .desired_width(f32::INFINITY)
                        .desired_rows(16),
                )
                .changed();
            ui.horizontal(|ui| {
                if ui.button("保存资料").clicked() && self.flush(dir) {
                    self.status = "资料已保存".into();
                }
                if ui.button("导入资料文件").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Markdown 资料", &["md"])
                        .pick_file()
                {
                    match fs::read_to_string(path) {
                        Ok(text) => {
                            self.profile = text;
                            self.profile_dirty = true;
                        }
                        Err(e) => self.error = e.to_string(),
                    }
                }
            });
        });
    }
    fn otp(&mut self, ui: &mut egui::Ui, dir: &std::path::Path) {
        ui.label("安卓 SmsForwarder 将验证码短信转发到电脑，官网投递可直接取码填入。验证码仅在内存保留 120 秒。");
        if let Some(service) = self.receiver_service.as_ref() {
            ui.label(format!("接收中 · {}", service.address));
            if service.address.starts_with("127.") {
                ui.label("当前仅本机可访问；手机转发需选择电脑的局域网地址。");
            }
            let status = service.inbox.lock().unwrap().status();
            ui.label(format!(
                "已收 {} 条短信 · {} 条识别到验证码 · 当前有效 {} 条",
                status["received"], status["with_code"], status["pending"]
            ));
            ui.horizontal(|ui| {
                if ui.button("刷新").clicked() {
                    self.status = "接码状态已刷新".into();
                    self.error.clear();
                    ui.ctx().request_repaint();
                }
                if ui.button("复制手机 Webhook 地址").clicked() {
                    match fs::read_to_string(dir.join("otp_url.txt")) {
                        Ok(text) => {
                            ui.ctx().copy_text(text);
                            self.status = "手机转发地址已复制，请粘贴到 SmsForwarder".into();
                        }
                        Err(e) => self.error = e.to_string(),
                    }
                }
                if ui.button("停止接收").clicked() {
                    self.receiver_service = None;
                    self.otp_checked = false;
                    self.otp_external = None;
                    self.status = "接码服务已停止".into();
                }
            });
        } else {
            ui.horizontal(|ui| {
                ui.label("监听地址");
                ui.text_edit_singleline(&mut self.settings.otp_host);
                ui.label("端口");
                ui.add(egui::DragValue::new(&mut self.settings.otp_port).range(1024..=65535));
            });
            ui.horizontal(|ui| {
                if ui.button("刷新服务状态").clicked() {
                    self.otp_checked = true;
                    match hiring::otp::service_status(dir) {
                        Ok(value) => {
                            self.otp_external = Some(value);
                            self.status = "接码服务正在运行（可能由 MCP 启动）".into();
                            self.error.clear();
                        }
                        Err(e) => {
                            self.otp_external = None;
                            self.status = format!("未检测到接码服务：{e}");
                            self.error.clear();
                        }
                    }
                }
                if ui.button("启动接码服务").clicked() {
                    match hiring::otp::Receiver::start(
                        dir,
                        &self.settings.otp_host,
                        self.settings.otp_port,
                    ) {
                        Ok(s) => {
                            self.receiver_service = Some(s);
                            self.otp_checked = false;
                            self.otp_external = None;
                            self.save_settings(dir);
                        }
                        Err(e) => self.error = e,
                    }
                }
            });
            if self.otp_checked {
                match self.otp_external.as_ref() {
                    Some(status) => {
                        ui.label(format!(
                            "其他进程接收中 · 已收 {} 条短信 · {} 条识别到验证码 · 当前有效 {} 条",
                            status["received"], status["with_code"], status["pending"]
                        ));
                    }
                    None => {
                        ui.label("接码服务未运行");
                    }
                }
            }
        }
        ui.separator();
        ui.label("手机和电脑连接同一局域网。在 SmsForwarder 新增 Webhook 通道：POST，地址使用上方复制的内容；消息模板如下：");
        ui.monospace(r#"{"from":"[from]","content":"[org_content]","timestamp":"[timestamp]"}"#);
        ui.label("转发规则匹配短信内容中的“验证码 / 校验码 / 动态码”。收到短信后，在官网投递页点击“填入验证码”。");
    }
}
fn status_label(status: &str) -> &str {
    match status {
        "pending" => "待投递",
        "sent" => "已发送",
        "partial" => "部分发送",
        "ambiguous" => "待核验",
        "failed" => "失败",
        "skipped" => "已跳过",
        _ => status,
    }
}
impl PersonalDeskApp {
    pub(super) fn render_hiring_entries(
        &mut self,
        ui: &mut egui::Ui,
        layout: crate::ui_components::EntryLayout,
        favorites: bool,
    ) {
        for (i, (id, title, description)) in TOOLS.iter().enumerate() {
            if self.is_favorite("tools", id) != favorites
                || !self.record_matches_filter(TagScope::Tools, id)
                || !matches_query(
                    &self.search_query.to_lowercase(),
                    &[title, description, "求职 招聘 内置 Rust"],
                )
            {
                continue;
            }
            let response = entry(ui, id, title, description, TileKind::Setting, layout);
            self.favorite_control(ui, &response, "tools", id);
            if response.clicked() {
                self.hiring.open = true;
                self.hiring.tab = i;
            }
            response.context_menu(|ui| {
                self.record_tag_menu(ui, TagScope::Tools, id);
                if ui.button(format!("打开{title}")).clicked() {
                    self.hiring.open = true;
                    self.hiring.tab = i;
                    ui.close();
                }
            });
        }
    }
    pub(super) fn render_hiring_dialog(&mut self, ctx: &egui::Context) {
        if !self.hiring.open {
            return;
        }
        let dir = self.data_dir.join("hiring");
        if !self.hiring.loaded {
            self.hiring.load(&dir);
        }
        let state = &mut self.hiring;
        let busy = state.busy();
        let dialog = centered_dialog(ctx, "hiring-tools", "求职工具", "", 820., |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, (_, title, _)) in TOOLS.iter().enumerate() {
                    ui.selectable_value(&mut state.tab, i, *title);
                }
                ui.selectable_value(&mut state.tab, 3, "投递记录");
                ui.selectable_value(&mut state.tab, 4, "资料与设置");
            });
            ui.separator();
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(&state.status);
                    if ui.button("停止队列").clicked() {
                        state.cancel.store(true, Ordering::Release);
                        if let Err(error) = Store::open(&dir).and_then(|mut s| s.request_stop()) {
                            state.error = error;
                        }
                    }
                });
            } else if !state.status.is_empty() {
                ui.label(&state.status);
            }
            if !state.error.is_empty() {
                ui.colored_label(Color32::from_rgb(184, 65, 71), &state.error);
            }
            egui::ScrollArea::vertical().id_salt("hiring-scroll").max_height((ctx.content_rect().height()-230.).max(220.)).auto_shrink([false,false]).show(ui,|ui|{
                ui.add_enabled_ui(!busy,|ui|match state.tab{
                    0=>state.boss(ui,&dir),1=>state.official(ui,&dir),2=>state.otp(ui,&dir),3=>state.history_panel(ui,&dir),_=>{
                        ui.label("三个工具使用办公台数据目录，原工具目录作为导入来源保留。");
                        if ui.button("导入原三个工具的资料和队列").clicked()&&let Some(path)=rfd::FileDialog::new().set_title("选择包含三个工具的个人办公台目录").pick_folder(){state.action=Some(Action::Import(path));}
                        ui.label("原待投队列作为快照导入并默认跳过；核验旧任务停止后，可在队列里设为待投。");
                        ui.horizontal(|ui| {
                            ui.label("浏览器路径");
                            ui.add(egui::TextEdit::singleline(&mut state.settings.browser_path)
                                .desired_width((ui.available_width() - 120.0).max(100.0)));
                            if ui.button("选择浏览器").clicked()
                                && let Some(path) = rfd::FileDialog::new().add_filter("浏览器", &["exe"]).pick_file()
                            {
                                state.settings.browser_path = path.display().to_string();
                            }
                        });
                        if ui.button("保存设置").clicked(){state.save_settings(&dir);}
                        if ui.button("打开求职数据目录").clicked(){let _=files::open_in_system(&dir);}
                    }
                });
                if !state.report.is_empty(){ui.separator();ui.label(&state.report);}
            });
            if let Some(action) = state.confirmation.take() {
                ui.separator();
                let label = match &action {
                    Action::Send(job) => format!(
                        "将向 {} · {} 发送招呼语和 {} 张图片",
                        job.company,
                        job.title,
                        state.settings.images.len()
                    ),
                    Action::Queue => format!("将依次向待投队列发送，最多 {} 项", state.queue_limit),
                    Action::Pool(ids, total) => format!(
                        "将使用{}个独立账号投递不同岗位，本轮最多{}项、每账号每日最多150项；各账号保持配置间隔，共享15天去重。",
                        ids.len(),
                        total
                    ),
                    Action::Resolve(id, sent) => format!(
                        "记录#{id}：仅在已于网页核验{}后确认执行。原记录和额度计数都会保留。",
                        if *sent {
                            "文字和图片已完整发送"
                        } else {
                            "确实没有发送任何消息"
                        }
                    ),
                    _ => format!(
                        "将提交 {} · {} 的申请，请核对浏览器中当前页面",
                        state.company, state.title
                    ),
                };
                ui.label(RichText::new(label).strong());
                let mut keep = true;
                ui.horizontal(|ui| {
                    if ui.button("确认执行").clicked() {
                        state.action = Some(action.clone());
                        keep = false;
                    }
                    if ui.button("取消").clicked() {
                        keep = false;
                    }
                });
                // Keep a confirmation separate from a running action.
                if keep && state.action.is_none() {
                    state.confirmation = Some(action);
                }
            }
        });
        if dialog_closed(&dialog) {
            state.open = false;
            state.flush(&dir);
            state.confirmation = None;
        }
        if let Some(action) = state.action.take()
            && state.flush(&dir)
        {
            if !matches!(action, Action::Import(_)) {
                state.save_settings(&dir);
            } else {
                state.error.clear();
            }
            if state.error.is_empty() {
                state.start(dir, action);
            }
        }
    }
}
