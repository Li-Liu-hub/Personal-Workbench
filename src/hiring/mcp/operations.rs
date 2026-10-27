use super::*;
use crate::hiring::{
    self, Settings,
    browser::{self, Site},
    lease::Lease,
    otp,
    profile::Profile,
    store::{Job, Store},
};
use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

struct Background {
    id: String,
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<VecDeque<String>>>,
    result: Arc<Mutex<Option<Result<String>>>>,
    thread: Option<JoinHandle<()>>,
}
impl Background {
    fn status(&self) -> Value {
        json!({"task_id":self.id,"running":self.thread.as_ref().is_some_and(|t|!t.is_finished()),
            "stop_requested":self.cancel.load(Ordering::Acquire),"progress":self.progress.lock().unwrap().clone(),
            "result":self.result.lock().unwrap().as_ref().map(|r|match r {Ok(v)=>json!({"ok":true,"message":v}),Err(e)=>json!({"ok":false,"error":e})})})
    }
}
impl Drop for Background {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
pub(super) struct Server {
    directory: PathBuf,
    receiver: Option<otp::Receiver>,
    background: Option<Background>,
}
impl Server {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            receiver: None,
            background: None,
        }
    }
    fn busy(&self) -> bool {
        self.background
            .as_ref()
            .is_some_and(|b| b.thread.as_ref().is_some_and(|t| !t.is_finished()))
    }
    fn queue(&self) -> Result<Vec<Job>> {
        if !self.directory.join("hiring.sqlite").exists() {
            return Ok(vec![]);
        }
        Store::open(&self.directory)?.jobs()
    }
    pub fn call(&mut self, name: &str, args: &Value) -> Result<Value> {
        let directory = self.directory.clone();
        let dir = directory.as_path();
        let mut settings = Settings::load(dir)?;
        if name.starts_with("boss_")
            && let Some(id) = args["account_id"].as_str()
        {
            let account = Store::open(dir)?.account(id, settings.daily_limit)?;
            settings.account_id = account.id;
        }
        let browser_action = matches!(
            name,
            "platform_open_login"
                | "platform_check_login"
                | "platform_search_jobs"
                | "platform_scroll_jobs"
                | "platform_job_detail"
                | "platform_apply"
        ) || matches!(
            name,
            "boss_open_login"
                | "boss_check_login"
                | "boss_search_jobs"
                | "boss_scroll_jobs"
                | "boss_job_detail"
                | "boss_apply"
                | "official_open"
                | "official_inspect"
                | "official_plan"
                | "official_autofill"
                | "official_submit"
                | "official_otp_fill"
        );
        let exclusive = browser_action
            || matches!(
                name,
                "hiring_import"
                    | "hiring_settings_update"
                    | "boss_queue_update"
                    | "hiring_account_create"
                    | "hiring_account_update"
                    | "hiring_account_select"
                    | "hiring_attempt_resolve"
            )
            || matches!(name, "platform_queue_update" | "platform_queue_add")
            || (name == "boss_pool_plan" && args["dry_run"] == false);
        if exclusive && self.busy() {
            return Err("后台队列运行中，请先调用 boss_queue_stop 并等待任务停止".into());
        }
        let _coordinator = if exclusive {
            Some(Lease::coordinator(dir)?)
        } else {
            None
        };
        let _lease = if browser_action {
            Some(if name.starts_with("boss_") {
                Lease::account(dir, &settings.account_id)?
            } else {
                Lease::acquire(dir)?
            })
        } else {
            None
        };
        match name {
            "hiring_queue_list" => Ok(json!(
                hiring::tracking::read(dir)?
                    .queue
                    .into_iter()
                    .filter(|j| (text(args, "platform").is_empty()
                        || hiring::store::site_for_url(&j.url) == text(args, "platform"))
                        && (text(args, "status").is_empty() || j.status == text(args, "status")))
                    .map(|j| json!({"platform":hiring::store::site_for_url(&j.url),"job":j}))
                    .collect::<Vec<_>>()
            )),
            "hiring_platforms" => Ok(hiring::platforms::catalog()),
            "platform_open_login"
            | "platform_check_login"
            | "platform_search_jobs"
            | "platform_scroll_jobs"
            | "platform_job_detail"
            | "platform_apply"
            | "platform_queue_list"
            | "platform_queue_add"
            | "platform_queue_update" => {
                let p = hiring::platforms::Platform::parse(text(args, "platform"))?;
                match name {
                    "platform_open_login" => browser::open(dir, &settings, p.site(), p.home())
                        .map(|_| message("已打开平台专用浏览器，请登录并核对在线简历")),
                    "platform_check_login" => hiring::platforms::login_status(dir, p).map(message),
                    "platform_search_jobs" => {
                        if let Some(keyword) = args["keyword"].as_str() {
                            settings.keyword = keyword.into();
                        }
                        if let Some(city) = args["city"].as_str() {
                            settings.city = city.into();
                        }
                        serde_json::to_value(hiring::platforms::search(
                            dir,
                            &settings,
                            p,
                            args["page"].as_u64().unwrap_or(1) as u32,
                        )?)
                        .map_err(err)
                    }
                    "platform_scroll_jobs" => {
                        serde_json::to_value(hiring::platforms::scroll(dir, p)?).map_err(err)
                    }
                    "platform_job_detail" => serde_json::to_value(hiring::platforms::detail(
                        dir,
                        &settings,
                        p,
                        text(args, "url"),
                    )?)
                    .map_err(err),
                    "platform_apply" => {
                        let dry = args["dry_run"].as_bool().unwrap_or(true);
                        if !dry {
                            confirmed(args)?;
                        }
                        let job: Job = serde_json::from_value(args.clone()).map_err(err)?;
                        hiring::platforms::apply(dir, &settings, p, &job, dry).map(message)
                    }
                    "platform_queue_list" => {
                        let filter = text(args, "status");
                        serde_json::to_value(
                            Store::open(dir)?
                                .platform_jobs(p)?
                                .into_iter()
                                .filter(|j| filter.is_empty() || j.status == filter)
                                .collect::<Vec<_>>(),
                        )
                        .map_err(err)
                    }
                    "platform_queue_add" => {
                        let job: Job = serde_json::from_value(args.clone()).map_err(err)?;
                        Ok(json!({"added":Store::open(dir)?.platform_add(p,&job)?}))
                    }
                    _ => {
                        Store::open(dir)?.platform_status(
                            p,
                            text(args, "url"),
                            text(args, "status"),
                        )?;
                        Ok(message("平台队列状态已更新"))
                    }
                }
            }
            "hiring_status" => {
                let jobs = hiring::tracking::read(dir)?.queue;
                let accounts = if dir.join("hiring.sqlite").is_file() {
                    Store::open(dir)?.accounts(settings.daily_limit)?
                } else {
                    vec![]
                };
                Ok(
                    json!({"version":env!("CARGO_PKG_VERSION"),"data_directory":dir.display().to_string(),
                    "profile_configured":dir.join("profile.md").is_file(),"queue_count":jobs.len(),"pending_count":jobs.iter().filter(|j|j.status=="pending").count(),
                    "task":self.background.as_ref().map(Background::status),"otp":self.otp_status(),"accounts":accounts,
                    "active_account_id":hiring::accounts::id(&settings.account_id)?,"shared_dedup_days":hiring::dedupe::DAYS,"shared_across_platforms":true,"queue_table":"queue","attempt_table":"attempts","platforms":hiring::platforms::catalog(),"max_accounts":hiring::accounts::MAX_ACCOUNTS,"shared_task_running":Lease::coordinator_busy(dir)}),
                )
            }
            "hiring_accounts_list" => {
                serde_json::to_value(Store::open(dir)?.accounts(settings.daily_limit)?).map_err(err)
            }
            "hiring_account_create" => {
                serde_json::to_value(Store::open(dir)?.create_account(text(args, "label"))?)
                    .map_err(err)
            }
            "hiring_account_update" => serde_json::to_value(Store::open(dir)?.update_account(
                text(args, "account_id"),
                args["label"].as_str(),
                args["enabled"].as_bool(),
                args["daily_limit"].as_u64().map(|n| n as u32),
            )?)
            .map_err(err),
            "hiring_account_select" => {
                let account =
                    Store::open(dir)?.account(text(args, "account_id"), settings.daily_limit)?;
                settings.account_id = account.id.clone();
                settings.save(dir)?;
                Ok(
                    json!({"active_account":account,"message":"已切换账号；请在该账号的专用窗口登录并检查，历史记录和额度计数保留"}),
                )
            }
            "hiring_dedupe_check" => {
                let mut job: Job = serde_json::from_value(args.clone()).map_err(err)?;
                let site = hiring::store::site_for_url(&job.url);
                job.url = Store::canonical_site(&job.url, site)?;
                let status = Store::open(dir)?.eligibility(site, &job)?;
                Ok(
                    json!({"fingerprint":hiring::dedupe::fingerprint(&job),"eligibility":status,"shared_across_accounts":true,"shared_across_platforms":true,"window_days":hiring::dedupe::DAYS}),
                )
            }
            "hiring_attempt_resolve" => {
                confirmed(args)?;
                Store::open(dir)?.resolve_attempt(
                    args["attempt_id"].as_i64().ok_or("记录编号无效")?,
                    args["sent"].as_bool().unwrap_or(false),
                    args["verified_on_site"] == true,
                )?;
                Ok(message("已保存人工核验结果；每日额度计数保留"))
            }
            "boss_pool_plan" => serde_json::to_value(Store::open(dir)?.plan_accounts(
                &account_ids(args)?,
                args["limit"].as_u64().unwrap_or(450) as usize,
                settings.daily_limit,
                args["dry_run"] == false,
            )?)
            .map_err(err),
            "hiring_import" => {
                let source = Path::new(text(args, "source_directory"));
                if !source.is_absolute() || !source.is_dir() {
                    return Err("导入来源必须是存在的绝对目录".into());
                }
                hiring::import::legacy(source, dir).map(message)
            }
            "hiring_settings_get" => serde_json::to_value(settings).map_err(err),
            "hiring_settings_update" => {
                let mut value = serde_json::to_value(settings).map_err(err)?;
                for (key, v) in args["settings"].as_object().unwrap() {
                    value[key] = v.clone();
                }
                let next: Settings = serde_json::from_value(value).map_err(err)?;
                next.save(dir)?;
                Ok(message("配置已保存"))
            }
            "hiring_history" => {
                let items = if dir.join("hiring.sqlite").exists() {
                    Store::open(dir)?.history()?
                } else {
                    vec![]
                };
                Ok(json!({"items":items}))
            }
            "boss_open_login" => browser::open(
                dir,
                &settings,
                Site::Boss,
                "https://www.zhipin.com/web/user/?ka=header-login",
            )
            .map(|_| message("已打开 BOSS 专用浏览器，请在网页完成登录")),
            "boss_check_login" => hiring::boss::login_status(dir, &settings).map(message),
            "boss_search_jobs" => {
                let mut value = serde_json::to_value(&settings).map_err(err)?;
                for key in ["keyword", "city", "experience", "degree", "job_type"] {
                    if let Some(v) = args.get(key) {
                        value[key] = v.clone();
                    }
                }
                let settings: Settings = serde_json::from_value(value).map_err(err)?;
                serde_json::to_value(hiring::boss::search(
                    dir,
                    &settings,
                    args["page"].as_u64().unwrap_or(1) as u32,
                )?)
                .map_err(err)
            }
            "boss_scroll_jobs" => {
                serde_json::to_value(hiring::boss::scroll(dir, &settings)?).map_err(err)
            }
            "boss_job_detail" => {
                serde_json::to_value(hiring::boss::detail(dir, &settings, text(args, "url"))?)
                    .map_err(err)
            }
            "boss_apply" => {
                let dry = args["dry_run"].as_bool().unwrap_or(true);
                if !dry {
                    confirmed(args)?;
                }
                let job = job(args)?;
                hiring::boss::deliver(dir, &settings, &job, dry).map(message)
            }
            "boss_queue_list" => {
                let filter = text(args, "status");
                let jobs: Vec<_> = self
                    .queue()?
                    .into_iter()
                    .filter(|j| {
                        (filter.is_empty() || j.status == filter)
                            && args["account_id"]
                                .as_str()
                                .is_none_or(|id| j.account_id == id)
                    })
                    .collect();
                serde_json::to_value(jobs).map_err(err)
            }
            "boss_queue_add" => {
                let inserted = Store::open(dir)?.add(&job(args)?)?;
                Ok(json!({"added":inserted}))
            }
            "boss_queue_update" => {
                let url = hiring::store::canonical(text(args, "url"), true)?;
                Store::open(dir)?.set_status(&url, text(args, "status"))?;
                Ok(message("队列状态已更新"))
            }
            "boss_queue_run" | "boss_pool_run" | "platform_queue_run" | "hiring_run" => {
                let unified = name == "hiring_run";
                let dry = args["dry_run"].as_bool().unwrap_or(true);
                if !unified || !dry {
                    confirmed(args)?;
                }
                if unified {
                    if let Some(limit) = args["daily_limit"].as_u64() {
                        settings.daily_limit = limit as u32;
                    }
                    if let Some(pages) = args["max_pages"].as_u64() {
                        settings.unified_max_pages = pages as u32;
                    }
                }
                if self.busy() {
                    return Err("已有后台队列正在执行".into());
                }
                let dir = directory;
                let pool = name == "boss_pool_run";
                let platform = if name == "platform_queue_run" {
                    Some(hiring::platforms::Platform::parse(text(args, "platform"))?)
                } else {
                    None
                };
                let ids = if pool { account_ids(args)? } else { vec![] };
                let limit = args["limit"].as_u64().unwrap_or(if pool { 450 } else { 1 }) as usize;
                let cancel = Arc::new(AtomicBool::new(false));
                let progress = Arc::new(Mutex::new(VecDeque::new()));
                let result = Arc::new(Mutex::new(None));
                let (c, p, r) = (cancel.clone(), progress.clone(), result.clone());
                let thread = std::thread::spawn(move || {
                    let progress_callback = |item| {
                        let mut items = p.lock().unwrap();
                        items.push_back(item);
                        while items.len() > 50 {
                            items.pop_front();
                        }
                    };
                    let outcome = if unified {
                        hiring::unified::run(&dir, &settings, dry, c, progress_callback)
                    } else if let Some(platform) = platform {
                        hiring::platforms::run_queue(
                            &dir,
                            &settings,
                            platform,
                            limit,
                            c,
                            progress_callback,
                        )
                    } else if pool {
                        hiring::boss::run_pool(&dir, &settings, &ids, limit, c, progress_callback)
                    } else {
                        hiring::boss::run_queue(&dir, &settings, limit, c, progress_callback)
                    };
                    *r.lock().unwrap() = Some(outcome);
                });
                self.background = Some(Background {
                    id: uuid::Uuid::new_v4().to_string(),
                    cancel,
                    progress,
                    result,
                    thread: Some(thread),
                });
                Ok(self.background.as_ref().unwrap().status())
            }
            "boss_queue_stop" | "platform_queue_stop" | "hiring_stop" => {
                let generation = Store::open(dir)?.request_stop()?;
                if let Some(background) = self.background.as_ref() {
                    background.cancel.store(true, Ordering::Release);
                }
                Ok(
                    json!({"task":self.background.as_ref().map(Background::status),"stop_generation":generation,"message":"已通知所有账号队列停止；当前项核验完成后结束"}),
                )
            }
            "official_open" => browser::open(dir, &settings, Site::Official, text(args, "url"))
                .map(|_| message("已打开官网专用浏览器，请完成登录后识别表单")),
            "official_inspect" => {
                let page = inspect(dir)?;
                Ok(redacted_page(&page))
            }
            "official_profile_check" => {
                let profile = Profile::read(&dir.join("profile.md"))?;
                Ok(
                    json!({"fields":profile.fields.iter().map(|f|json!({"section":f.section,"label":f.label,"index":f.index,"configured":!hiring::profile::blank(&f.value)})).collect::<Vec<_>>()}),
                )
            }
            "official_plan" => {
                let page = inspect(dir)?;
                let (actions, missing) = Profile::read(&dir.join("profile.md"))?.plan(&page);
                Ok(
                    json!({"page":redacted_page(&page),"mapped_fields":actions.iter().map(|a|json!({"id":a["id"],"kind":a["kind"],"custom_select":a["custom_select"]})).collect::<Vec<_>>(),"missing_required":missing}),
                )
            }
            "official_autofill" => hiring::official::autofill(
                dir,
                args["advance"].as_bool().unwrap_or(false),
                args["upload_documents"].as_bool().unwrap_or(false),
            )
            .map(message),
            "official_submit" => {
                let dry = args["dry_run"].as_bool().unwrap_or(true);
                if !dry {
                    confirmed(args)?;
                }
                hiring::official::submit(
                    dir,
                    &settings,
                    !dry,
                    dry,
                    text(args, "job_title"),
                    text(args, "job_company"),
                )
                .map(message)
            }
            "official_otp_fill" => {
                let inbox = match self.receiver.as_ref() {
                    Some(s) => s.inbox.clone(),
                    None => otp::service_inbox(dir)?,
                };
                hiring::official::otp_fill(dir, inbox).map(message)
            }
            "otp_start" => {
                if self.receiver.is_none() && otp::service_status(dir).is_err() {
                    let host = args["host"].as_str().unwrap_or(&settings.otp_host);
                    let port = args["port"].as_u64().unwrap_or(settings.otp_port as u64) as u16;
                    self.receiver = Some(otp::Receiver::start(dir, host, port)?);
                }
                Ok(
                    json!({"status":self.otp_status(),"webhook_file":dir.join("otp_url.txt").display().to_string(),"instructions":"在手机 SmsForwarder 配置中使用本机文件里的 Webhook 地址，或在办公台接码页复制。不要把 token 发给模型。"}),
                )
            }
            "otp_status" => Ok(self.otp_status()),
            "otp_stop" => {
                let owned = self.receiver.take().is_some();
                Ok(
                    json!({"stopped":owned,"message":if owned {"本 MCP 接码服务已停止"} else {"本会话没有接码服务；其他进程拥有的服务未停止"}}),
                )
            }
            _ => Err("未知工具".into()),
        }
    }
    fn otp_status(&self) -> Value {
        match self.receiver.as_ref() {
            Some(s) => {
                json!({"running":true,"owner":"mcp","address":s.address,"counts":s.inbox.lock().unwrap().status()})
            }
            None => match otp::service_status(&self.directory) {
                Ok(status) => json!({"running":true,"owner":"other_process","counts":status}),
                Err(_) => json!({"running":false}),
            },
        }
    }
}
fn inspect(dir: &Path) -> Result<Value> {
    let mut tab = browser::selected(dir, Site::Official)?;
    hiring::official::inspect(&mut tab)
}
pub(super) fn redacted_page(page: &Value) -> Value {
    json!({"title":page["title"],"url":page["url"].as_str().and_then(|u|url::Url::parse(u).ok()).map(|mut u|{u.set_query(None);u.set_fragment(None);u.to_string()}),
        "fields":page["fields"].as_array().into_iter().flatten().map(|f|json!({"id":f["id"],"label":f["label"],"kind":f["kind"],
            "required":f["required"],"disabled":f["disabled"],"filled":f["checked"]==true||f["value"].as_str().is_some_and(|s|!s.is_empty()),
            "custom_select":f["custom_select"],"options":f["options"]})).collect::<Vec<_>>(),
        "missing_required":hiring::official::missing_required(page),"signals":{
            "needs_login":page["signals"]["login_modal_visible"]==true||page["signals"]["login_evidence"].as_array().is_some_and(|a|a.iter().any(|v|v!="已登录提示")),
            "needs_captcha":page["signals"]["captcha_evidence"].as_array().is_some_and(|a|!a.is_empty()),
            "has_validation_errors":page["signals"]["validation_errors"].as_array().is_some_and(|a|!a.is_empty())}})
}
fn text<'a>(args: &'a Value, key: &str) -> &'a str {
    args[key].as_str().unwrap_or("")
}
fn message(text: impl Into<String>) -> Value {
    json!({"message":text.into()})
}
fn confirmed(args: &Value) -> Result<()> {
    if args["confirmed"] == true {
        Ok(())
    } else {
        Err("正式投递需要用户明确授权，再传 confirmed=true；可先 dry_run=true 预检".into())
    }
}
fn job(args: &Value) -> Result<Job> {
    let mut job: Job = serde_json::from_value(args.clone()).map_err(err)?;
    job.url = hiring::store::canonical(&job.url, true)?;
    Ok(job)
}
fn account_ids(args: &Value) -> Result<Vec<String>> {
    args["account_ids"]
        .as_array()
        .ok_or("请选择账号列表")?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or("账号标识无效".into()))
        .collect()
}
