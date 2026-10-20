use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

pub struct Bridge {
    child: Option<Child>,
    send: Sender<Value>,
    receive: Receiver<Result<Value, String>>,
}
impl Bridge {
    pub fn start(executable: &Path) -> Result<Self, String> {
        let mut command = Command::new(executable);
        command
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("无法启动本机 Codex：{e}"))?;
        let mut stdin = child.stdin.take().ok_or("无法连接 Codex 输入")?;
        let stdout = child.stdout.take().ok_or("无法连接 Codex 输出")?;
        let (send, writer) = mpsc::channel::<Value>();
        let (events, receive) = mpsc::channel();
        let errors = events.clone();
        thread::spawn(move || {
            for value in writer {
                if serde_json::to_writer(&mut stdin, &value).is_err()
                    || stdin.write_all(b"\n").is_err()
                    || stdin.flush().is_err()
                {
                    let _ = errors.send(Err("Codex 连接已断开，请重新连接".into()));
                    break;
                }
            }
        });
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if let Ok(value) = serde_json::from_str(&line)
                            && events.send(Ok(value)).is_err()
                        {
                            return;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = events.send(Err("Codex 进程已结束，请重新连接".into()));
        });
        Ok(Self {
            child: Some(child),
            send,
            receive,
        })
    }
    fn write(&self, value: Value) -> Result<(), String> {
        self.send.send(value).map_err(|_| "Codex 连接已断开".into())
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub item_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub project: PathBuf,
    pub thread_id: Option<String>,
    pub messages: Vec<Message>,
}
#[derive(Clone)]
pub struct ServerRequest {
    pub id: Value,
    pub method: String,
    pub params: Value,
    pub answers: BTreeMap<String, String>,
}
#[derive(Clone)]
pub struct Model {
    pub id: String,
    pub name: String,
}
pub struct CodexState {
    pub settings_open: bool,
    pub executable: PathBuf,
    pub project: PathBuf,
    pub model: String,
    pub models: Vec<Model>,
    pub prompt: String,
    pub status: String,
    pub error: String,
    pub ready: bool,
    pub busy: bool,
    pub turn_id: Option<String>,
    pub current: Conversation,
    pub history: Vec<Conversation>,
    pub requests: VecDeque<ServerRequest>,
    pub login_url: Option<String>,
    bridge: Option<Bridge>,
    next_id: u64,
    pending: BTreeMap<u64, String>,
    queued_prompt: Option<String>,
    directory: PathBuf,
}
fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
pub fn detect_executable() -> PathBuf {
    if let Some(path) = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join("codex.exe"))
            .find(|p| p.is_file())
    }) {
        return path;
    }
    if let Some(base) = std::env::var_os("LOCALAPPDATA")
        && let Ok(entries) = std::fs::read_dir(PathBuf::from(base).join("OpenAI/Codex/bin"))
    {
        let mut paths = entries
            .flatten()
            .map(|e| e.path().join("codex.exe"))
            .filter(|p| p.is_file())
            .collect::<Vec<_>>();
        paths.sort();
        if let Some(path) = paths.pop() {
            return path;
        }
    }
    PathBuf::from("codex.exe")
}
impl CodexState {
    pub fn load(data: &Path, settings: &BTreeMap<String, String>) -> Self {
        let project = settings
            .get("codex.project")
            .map(PathBuf::from)
            .unwrap_or_default();
        let directory = data.join("codex");
        let mut history = std::fs::read_dir(&directory)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| std::fs::read(e.path()).ok())
            .filter_map(|b| serde_json::from_slice::<Conversation>(&b).ok())
            .collect::<Vec<_>>();
        history.sort_by(|a, b| b.id.cmp(&a.id));
        Self {
            settings_open: false,
            executable: settings
                .get("codex.executable")
                .map(PathBuf::from)
                .unwrap_or_else(detect_executable),
            project: project.clone(),
            model: settings.get("codex.model").cloned().unwrap_or_default(),
            models: vec![],
            prompt: String::new(),
            status: "尚未连接本机 Codex".into(),
            error: String::new(),
            ready: false,
            busy: false,
            turn_id: None,
            current: Self::empty(project),
            history,
            requests: VecDeque::new(),
            login_url: None,
            bridge: None,
            next_id: 1,
            pending: BTreeMap::new(),
            queued_prompt: None,
            directory,
        }
    }
    fn empty(project: PathBuf) -> Conversation {
        Conversation {
            id: uuid::Uuid::new_v4().to_string(),
            title: "新对话".into(),
            project,
            thread_id: None,
            messages: vec![],
        }
    }
    pub fn has_request(&self) -> bool {
        !self.requests.is_empty()
    }
    pub fn connect(&mut self) {
        self.bridge = None;
        self.pending.clear();
        self.requests.clear();
        self.ready = false;
        self.busy = false;
        self.turn_id = None;
        self.queued_prompt = None;
        self.error.clear();
        match Bridge::start(&self.executable) {
            Ok(bridge) => {
                self.bridge = Some(bridge);
                self.status = "正在连接…".into();
                self.request("initialize",json!({"clientInfo":{"name":"personal_desk","title":"个人办公台","version":"0.1.0"}}));
            }
            Err(e) => self.error = e,
        }
    }
    fn send(&mut self, value: Value) {
        if let Some(bridge) = &self.bridge {
            if let Err(e) = bridge.write(value) {
                self.error = e;
                self.ready = false;
                self.busy = false;
            }
        } else {
            self.error = "请先连接本机 Codex".into();
        }
    }
    fn request(&mut self, method: &str, params: Value) {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(id, method.into());
        self.send(json!({"id":id,"method":method,"params":params}));
    }
    pub fn login(&mut self) {
        self.request("account/login/start", json!({"type":"chatgpt"}));
    }
    pub fn new_chat(&mut self) {
        if self.busy {
            return;
        }
        self.save();
        self.current = Self::empty(self.project.clone());
        self.error.clear();
    }
    pub fn select_chat(&mut self, conversation: Conversation) {
        if self.busy {
            return;
        }
        self.save();
        self.project = conversation.project.clone();
        self.current = conversation;
        self.error.clear();
        if self.ready
            && let Some(id) = self.current.thread_id.clone()
        {
            self.busy = true;
            self.request(
                "thread/resume",
                json!({"threadId":id,"cwd":self.project,"excludeTurns":true}),
            );
        }
    }
    pub fn submit(&mut self) {
        if !self.ready || self.busy || self.prompt.trim().is_empty() {
            return;
        }
        if !self.project.is_absolute() || !self.project.is_dir() {
            self.error = "请先选择一个项目文件夹".into();
            return;
        }
        self.error.clear();
        self.busy = true;
        self.current.project = self.project.clone();
        self.queued_prompt = Some(self.prompt.clone());
        if let Some(id) = self.current.thread_id.clone() {
            self.request(
                "thread/resume",
                json!({"threadId":id,"cwd":self.project,"excludeTurns":true}),
            );
        } else {
            let mut params = json!({"cwd":self.project});
            if !self.model.is_empty() {
                params["model"] = json!(self.model);
            }
            self.request("thread/start", params);
        }
    }
    fn start_turn(&mut self) {
        let Some(prompt) = self.queued_prompt.take() else {
            self.busy = false;
            return;
        };
        let Some(id) = self.current.thread_id.clone() else {
            return;
        };
        if self.current.messages.is_empty() {
            self.current.title = prompt.chars().take(28).collect();
        }
        self.current.messages.push(Message {
            role: "user".into(),
            text: prompt.clone(),
            item_id: String::new(),
        });
        self.prompt.clear();
        self.save();
        let mut params = json!({"threadId":id,"input":[{"type":"text","text":prompt}]});
        if !self.model.is_empty() {
            params["model"] = json!(self.model);
        }
        self.request("turn/start", params);
        self.status = "Codex 正在处理…".into();
    }
    pub fn stop(&mut self) {
        if let (Some(thread), Some(turn)) = (self.current.thread_id.clone(), self.turn_id.clone()) {
            self.request("turn/interrupt", json!({"threadId":thread,"turnId":turn}));
            self.status = "正在停止…".into();
        }
    }
    pub fn respond(&mut self, result: Value) {
        if let Some(request) = self.requests.pop_front() {
            self.send(json!({"id":request.id,"result":result}));
        }
    }
    pub fn reject_request(&mut self) {
        if let Some(request) = self.requests.pop_front() {
            let value = if request.method.ends_with("requestApproval") {
                json!({"id":request.id,"result":{"decision":"cancel"}})
            } else {
                json!({"id":request.id,"error":{"code":-32601,"message":"此客户端不支持该请求"}})
            };
            self.send(value);
        }
    }
    pub fn poll(&mut self) {
        let events = self
            .bridge
            .as_ref()
            .map(|b| b.receive.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for event in events {
            match event {
                Ok(value) => self.handle(value),
                Err(error) => {
                    self.error = error;
                    self.ready = false;
                    self.busy = false;
                    self.turn_id = None;
                    self.requests.clear();
                    self.save();
                }
            }
        }
    }
    fn handle(&mut self, value: Value) {
        if value.get("method").is_some() {
            let method = text(&value, "method");
            let params = &value["params"];
            if value.get("id").is_some() {
                if matches!(
                    method.as_str(),
                    "item/commandExecution/requestApproval"
                        | "item/fileChange/requestApproval"
                        | "item/tool/requestUserInput"
                ) {
                    self.requests.push_back(ServerRequest {
                        id: value["id"].clone(),
                        method,
                        params: params.clone(),
                        answers: BTreeMap::new(),
                    });
                } else {
                    self.send(json!({"id":value["id"],"error":{"code":-32601,"message":"个人办公台暂不支持此请求"}}));
                    self.error = format!("暂不支持的 Codex 请求：{method}");
                }
                return;
            }
            if method == "account/login/completed" {
                self.login_url = None;
                self.request("account/read", json!({"refreshToken":false}));
                return;
            }
            if let Some(id) = params["threadId"].as_str()
                && self.current.thread_id.as_deref() != Some(id)
            {
                return;
            }
            match method.as_str() {
                "turn/started" => {
                    self.turn_id = Some(text(&params["turn"], "id"));
                    self.busy = true;
                }
                "turn/completed" => {
                    self.busy = false;
                    self.turn_id = None;
                    self.requests.clear();
                    let status = text(&params["turn"], "status");
                    self.status = match status.as_str() {
                        "interrupted" => "任务已停止",
                        "failed" => "任务失败",
                        _ => "回复已完成",
                    }
                    .into();
                    if let Some(e) = params["turn"]["error"]["message"].as_str() {
                        self.error = e.into();
                    }
                    self.save();
                }
                "item/agentMessage/delta" => {
                    let id = text(params, "itemId");
                    let delta = text(params, "delta");
                    if let Some(message) = self
                        .current
                        .messages
                        .iter_mut()
                        .find(|m| m.role == "assistant" && m.item_id == id)
                    {
                        message.text.push_str(&delta);
                    } else {
                        self.current.messages.push(Message {
                            role: "assistant".into(),
                            text: delta,
                            item_id: id,
                        });
                    }
                }
                "item/completed" => {
                    let item = &params["item"];
                    let kind = text(item, "type");
                    if kind == "agentMessage" {
                        let id = text(item, "id");
                        let message = Message {
                            role: "assistant".into(),
                            text: text(item, "text"),
                            item_id: id.clone(),
                        };
                        if let Some(old) = self
                            .current
                            .messages
                            .iter_mut()
                            .find(|m| m.role == "assistant" && m.item_id == id)
                        {
                            *old = message;
                        } else {
                            self.current.messages.push(message);
                        }
                        self.save();
                    } else if matches!(kind.as_str(), "commandExecution" | "fileChange") {
                        let command = text(item, "command");
                        let status = text(item, "status");
                        self.current.messages.push(Message {
                            role: "activity".into(),
                            text: if kind == "commandExecution" {
                                format!("{command}\n{status}\n{}", text(item, "aggregatedOutput"))
                            } else {
                                format!("文件变更：{status}\n{}", item["changes"])
                            },
                            item_id: text(item, "id"),
                        });
                        self.save();
                    }
                }
                "error" => {
                    self.error = text(&params["error"], "message");
                }
                _ => {}
            }
            return;
        }
        let Some(id) = value["id"].as_u64() else {
            return;
        };
        let Some(method) = self.pending.remove(&id) else {
            return;
        };
        if value.get("error").is_some() {
            self.error = format!("{method}：{}", text(&value["error"], "message"));
            if matches!(
                method.as_str(),
                "thread/start" | "thread/resume" | "turn/start"
            ) {
                self.busy = false;
                self.queued_prompt = None;
            }
            return;
        }
        let result = &value["result"];
        match method.as_str() {
            "initialize" => {
                self.send(json!({"method":"initialized"}));
                self.ready = true;
                self.status = "已连接本机 Codex".into();
                self.request("account/read", json!({"refreshToken":false}));
                self.request("model/list", json!({"limit":100,"includeHidden":false}));
            }
            "account/read" => {
                self.status = if result["account"].is_null()
                    && result["requiresOpenaiAuth"].as_bool() == Some(true)
                {
                    "已连接，请登录 Codex"
                } else {
                    "已连接，使用本机 Codex 配置"
                }
                .into();
            }
            "model/list" => {
                self.models = result["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|m| Model {
                        id: text(m, "model"),
                        name: text(m, "displayName"),
                    })
                    .collect();
                if self.model.is_empty() {
                    self.model = result["data"]
                        .as_array()
                        .and_then(|a| a.iter().find(|m| m["isDefault"].as_bool() == Some(true)))
                        .map(|m| text(m, "model"))
                        .unwrap_or_default();
                }
            }
            "thread/start" | "thread/resume" => {
                self.current.thread_id = Some(text(&result["thread"], "id"));
                self.start_turn();
            }
            "turn/start" => {
                self.turn_id = Some(text(&result["turn"], "id"));
            }
            "account/login/start" => {
                self.login_url = result["authUrl"].as_str().map(str::to_owned);
            }
            _ => {}
        }
    }
    pub fn save(&mut self) {
        if self.current.messages.is_empty() {
            return;
        }
        if uuid::Uuid::parse_str(&self.current.id).is_err() {
            self.error = "对话存储标识无效".into();
            return;
        }
        let result = (|| {
            std::fs::create_dir_all(&self.directory)?;
            let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
            serde_json::to_writer_pretty(&mut file, &self.current)
                .map_err(std::io::Error::other)?;
            file.as_file().sync_all()?;
            file.persist(self.directory.join(format!("{}.json", self.current.id)))
                .map_err(|e| e.error)?;
            Ok::<_, std::io::Error>(())
        })();
        if let Err(e) = result {
            self.error = format!("对话保存失败：{e}");
            return;
        }
        if let Some(old) = self.history.iter_mut().find(|c| c.id == self.current.id) {
            *old = self.current.clone();
        } else {
            self.history.insert(0, self.current.clone());
        }
    }
}

impl Drop for CodexState {
    fn drop(&mut self) {
        self.save();
    }
}

#[cfg(test)]
mod tests;
