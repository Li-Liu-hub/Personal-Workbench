use crate::study::{Card, Grade};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub model: PathBuf,
    pub runtime: PathBuf,
    pub context: u32,
    pub idle_seconds: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            model: "D:/models/llm/spark-x2.6/4b/Spark-X2.6-4B-Q4_K_M.gguf".into(),
            runtime: "D:/models/llm/runtime/llama-b11669-cuda12.4/llama-server.exe".into(),
            context: 8192,
            idle_seconds: 120,
        }
    }
}
struct Runtime {
    child: Option<Child>,
    #[cfg(windows)]
    job: Option<ProcessJob>,
    port: u16,
    config: Option<Config>,
    last_used: Instant,
}
pub struct LocalAi {
    runtime: Mutex<Runtime>,
    status: Mutex<String>,
    pub busy: AtomicBool,
    closed: AtomicBool,
    cancelled: AtomicBool,
    telemetry: Mutex<conversation::Telemetry>,
}
impl Default for LocalAi {
    fn default() -> Self {
        Self {
            runtime: Mutex::new(Runtime {
                child: None,
                #[cfg(windows)]
                job: None,
                port: 0,
                config: None,
                last_used: Instant::now(),
            }),
            status: Mutex::new("未加载".into()),
            busy: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            telemetry: Mutex::new(Default::default()),
        }
    }
}
impl LocalAi {
    pub fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        self.stop();
    }
    pub fn status(&self) -> String {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| "模型状态不可用".into())
    }
    fn set_status(&self, s: impl Into<String>) {
        if let Ok(mut status) = self.status.lock() {
            *status = s.into();
        }
    }
    pub fn is_loaded(&self) -> bool {
        self.runtime.lock().is_ok_and(|mut r| {
            r.child
                .as_mut()
                .is_some_and(|child| child.try_wait().ok() == Some(None))
        })
    }
    pub fn stop(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Ok(mut r) = self.runtime.lock() {
            if let Some(mut child) = r.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            r.config = None;
            #[cfg(windows)]
            {
                r.job = None;
            }
        }
        self.set_status("未加载");
    }
    pub fn cancel_generation(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.set_status("正在停止生成 · 保留模型权重");
    }
    /// Reading/thinking time on a learning page is active use, even without inference.
    pub fn unload_if_idle(&self, learning_active: bool) {
        if self.busy.load(Ordering::Acquire) {
            return;
        }
        let idle = self.runtime.lock().is_ok_and(|mut r| {
            if learning_active {
                r.last_used = Instant::now();
                return false;
            }
            let seconds = r.config.as_ref().map(|c| c.idle_seconds).unwrap_or(120);
            r.child.is_some() && seconds > 0 && r.last_used.elapsed().as_secs() >= seconds
        });
        if idle {
            self.stop();
        }
    }
    fn ensure_started(&self, config: &Config, log: &Path) -> Result<u16, String> {
        if self.cancelled.load(Ordering::Acquire) || self.closed.load(Ordering::Acquire) {
            return Err("本次操作已停止".into());
        }
        if !config.model.is_file() {
            return Err("未找到 GGUF 模型，请在练习设置中选择模型文件".into());
        }
        if !config.runtime.is_file() {
            return Err("未找到 llama-server.exe，请在练习设置中选择运行程序".into());
        }
        if !(2048..=16384).contains(&config.context) {
            return Err("模型上下文需要在 2048 到 16384 之间".into());
        }
        let (port, spawned) = {
            let mut r = self.runtime.lock().map_err(|_| "模型运行状态不可用")?;
            if self.closed.load(Ordering::Acquire) || self.cancelled.load(Ordering::Acquire) {
                return Err("办公台已退出，答案保留在待完成记录中".into());
            }
            if r.child
                .as_mut()
                .is_some_and(|p| p.try_wait().ok().flatten().is_some())
            {
                r.child = None;
            }
            if r.config.as_ref().is_some_and(|c| {
                c.model != config.model
                    || c.runtime != config.runtime
                    || c.context != config.context
            }) && let Some(mut c) = r.child.take()
            {
                let _ = c.kill();
                let _ = c.wait();
            }
            let spawned = r.child.is_none();
            if spawned {
                self.set_status("正在加载模型权重…");
                r.port = std::net::TcpListener::bind(("127.0.0.1", 0))
                    .map_err(|e| e.to_string())?
                    .local_addr()
                    .map_err(|e| e.to_string())?
                    .port();
                let output = std::fs::File::create(log).map_err(|e| e.to_string())?;
                let mut command = Command::new(&config.runtime);
                command
                    .current_dir(config.runtime.parent().unwrap_or(Path::new(".")))
                    .args(["-m"])
                    .arg(&config.model)
                    .args([
                        "--alias",
                        "personaldesk-grader",
                        "--host",
                        "127.0.0.1",
                        "--port",
                        &r.port.to_string(),
                        "-ngl",
                        "99",
                        "-c",
                        &config.context.to_string(),
                        "--parallel",
                        "1",
                        "--batch-size",
                        "256",
                        "--ubatch-size",
                        "128",
                        "--jinja",
                        "--reasoning-budget",
                        "0",
                        "--no-webui",
                    ]);
                command
                    .stdout(Stdio::from(output.try_clone().map_err(|e| e.to_string())?))
                    .stderr(Stdio::from(output));
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000);
                }
                let mut child = command.spawn().map_err(|e| format!("模型启动失败：{e}"))?;
                #[cfg(windows)]
                {
                    match ProcessJob::attach(&child) {
                        Ok(job) => r.job = Some(job),
                        Err(error) => {
                            let _ = child.kill();
                            let _ = child.wait();
                            return Err(format!("模型进程管理失败：{error}"));
                        }
                    }
                }
                r.child = Some(child);
            }
            r.config = Some(config.clone());
            r.last_used = Instant::now();
            (r.port, spawned)
        };
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(2)))
            .build()
            .into();
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(90) {
            if self.closed.load(Ordering::Acquire) || self.cancelled.load(Ordering::Acquire) {
                return Err("本次批改已停止，答案保留在待完成记录中".into());
            }
            if agent
                .get(format!("http://127.0.0.1:{port}/health"))
                .call()
                .is_ok()
            {
                self.set_status(if spawned {
                    "权重加载完成"
                } else {
                    "复用已加载模型"
                });
                return Ok(port);
            }
            if self
                .runtime
                .lock()
                .map_err(|_| "模型状态不可用")?
                .child
                .as_mut()
                .is_none_or(|c| c.try_wait().ok().flatten().is_some())
            {
                return Err(format!("模型启动未完成，请查看 {}", log.display()));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Err("本地模型加载超时，可重试或检查运行日志".into())
    }
    pub fn grade(
        &self,
        config: &Config,
        log: &Path,
        card: &Card,
        answer: &str,
    ) -> Result<Grade, String> {
        self.begin("正在准备批改")?;
        let result = (|| {
            if answer.trim().is_empty() {
                return Err("请先填写答案".into());
            }
            let limit = if card.project.is_some() { 14000 } else { 6000 };
            if answer.chars().count() > limit {
                return Err(format!(
                    "本次提交超过 {limit} 字，原答案或源码仍保留，可复制给其他模型分析"
                ));
            }
            let execution = card
                .sql
                .as_ref()
                .filter(|lab| lab.runnable)
                .map(|lab| crate::sql_practice::run(lab, answer));
            let port = self.ensure_started(config, log)?;
            self.set_status("正在按评分点批改…");
            let mut request = grade_request(card, answer, execution.as_ref());
            let mut reviewed = false;
            let mut repairs = 0;
            loop {
                let content = self.complete(port, request.clone())?;
                match checked_grade(&content, card, answer) {
                    Ok(mut grade) => {
                        if grade.completion(card, answer).map_err(|e| e.to_string())? == 0
                            && !reviewed
                        {
                            reviewed = true;
                            self.set_status("正在独立复核零分结果…");
                            request = grade_request(card, answer, execution.as_ref());
                            request["messages"][0]["content"] = serde_json::json!(format!(
                                "{}\n你现在独立复核一份初评零分的答案。不要默认原评分正确，重新寻找每个评分点已答对的部分；区分错误、遗漏和表述不够完整。若确实全部错误或无关，仍应给0分。不要仅为提高分数而给分。",
                                request["messages"][0]["content"].as_str().unwrap()
                            ));
                            continue;
                        }
                        grade.assessment = Some(crate::study::Assessment {
                            version: 2,
                            zero_reviewed: reviewed,
                        });
                        grade.sql = execution;
                        return Ok(grade);
                    }
                    Err(error) if repairs == 0 => {
                        repairs += 1;
                        self.set_status("正在核对评分依据");
                        request["messages"].as_array_mut().unwrap().extend([
                            serde_json::json!({"role":"assistant","content":content}),
                            serde_json::json!({"role":"user","content":format!(
                                "上一份评分未通过程序校验：{error}。请修正并完整重新返回评分 JSON。c_matched只写本项已答对的具体事实，没有则为空；d_missing写本项尚未回答或答错的明确要求，完整正确时为空。b_evidence 必须逐字复制 student_answer 中一小段真实原文，保留原来的空格、标点和代码，不得改写或摘抄参考答案。没有对应证据则c_matched必须为空。不要改变学生答案或题目。")}),
                        ]);
                    }
                    Err(error) => return Err(error),
                }
            }
        })();
        self.finish(result.is_ok());
        result
    }
}

mod conversation;
mod grading;
pub use conversation::Dialogue;
use grading::checked_grade;
pub use grading::grade_request;
impl Drop for LocalAi {
    fn drop(&mut self) {
        self.stop();
    }
}

// The job owns only the inference child. Windows also releases it if the client crashes.
#[cfg(windows)]
pub(crate) struct ProcessJob(isize);
#[cfg(windows)]
impl ProcessJob {
    pub(crate) fn attach(child: &Child) -> std::io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            let job = Self(handle as isize);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle()) == 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(job)
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0 as _);
        }
    }
}
