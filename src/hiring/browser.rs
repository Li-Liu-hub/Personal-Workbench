use super::{Result, Settings, err};
use serde_json::{Value, json};
use std::{
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Site {
    Boss,
    Official,
    Job51,
    Zhaopin,
    Shixiseng,
}
impl Site {
    pub fn port(self) -> u16 {
        match self {
            Self::Boss => 19333,
            Self::Official => 19444,
            Self::Job51 => 19551,
            Self::Zhaopin => 19552,
            Self::Shixiseng => 19553,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Boss => "boss",
            Self::Official => "official",
            Self::Job51 => "51job",
            Self::Zhaopin => "zhaopin",
            Self::Shixiseng => "shixiseng",
        }
    }
}
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .into()
}
fn targets_port(port: u16) -> Result<Vec<Value>> {
    agent()
        .get(format!("http://127.0.0.1:{port}/json/list"))
        .call()
        .map_err(|_| "专用浏览器未连接，请先打开此账号的登录页".to_owned())?
        .body_mut()
        .read_json()
        .map_err(err)
}
pub fn targets(site: Site) -> Result<Vec<Value>> {
    targets_port(site.port())
}
fn context(dir: &Path, settings: &Settings, site: Site) -> Result<(PathBuf, u16)> {
    if site == Site::Boss {
        let account = super::accounts::active(dir, settings)?;
        Ok((account.browser_directory(dir), account.port()))
    } else {
        Ok((dir.to_path_buf(), site.port()))
    }
}
pub fn targets_for(dir: &Path, settings: &Settings, site: Site) -> Result<Vec<Value>> {
    let (_, port) = context(dir, settings, site)?;
    if port == site.port() {
        targets(site)
    } else {
        targets_port(port)
    }
}
fn executable(settings: &Settings) -> Result<PathBuf> {
    if !settings.browser_path.is_empty() {
        let path = PathBuf::from(&settings.browser_path);
        return if path.is_file() {
            Ok(path)
        } else {
            Err("浏览器路径不存在".into())
        };
    }
    for key in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(key) {
            for tail in [
                "Google/Chrome/Application/chrome.exe",
                "Microsoft/Edge/Application/msedge.exe",
            ] {
                let path = PathBuf::from(&base).join(tail);
                if path.is_file() {
                    return Ok(path);
                }
            }
        }
    }
    Err("未找到 Chrome / Edge，请在工具设置中选择浏览器".into())
}
pub fn validate_url(raw: &str, site: Site) -> Result<url::Url> {
    let url = url::Url::parse(raw.trim()).map_err(|_| "请输入完整的招聘网页地址".to_owned())?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("只支持 HTTP/HTTPS 招聘网页".into());
    }
    if site == Site::Boss
        && !url
            .host_str()
            .is_some_and(|s| s == "www.zhipin.com" || s == "zhipin.com")
    {
        return Err("BOSS 工具只接受 zhipin.com 岗位地址".into());
    }
    let domain = match site {
        Site::Job51 => Some("51job.com"),
        Site::Zhaopin => Some("zhaopin.com"),
        Site::Shixiseng => Some("shixiseng.com"),
        _ => None,
    };
    if let Some(domain) = domain
        && (url.scheme() != "https"
            || url.port().is_some()
            || !url
                .host_str()
                .is_some_and(|h| h == domain || h.ends_with(&format!(".{domain}"))))
    {
        return Err(format!("此平台只接受 {domain} 的 HTTPS 网页"));
    }
    Ok(url)
}
pub fn open(dir: &Path, settings: &Settings, site: Site, raw: &str) -> Result<String> {
    let url = validate_url(raw, site)?;
    let (runtime, port) = context(dir, settings, site)?;
    let session_path = runtime.join(format!("{}-browser-session.json", site.name()));
    let tab_path = runtime.join(format!("{}-tab.json", site.name()));
    let existing = targets_port(port);
    let mut launched = false;
    if existing.is_err() {
        let profile = runtime.join(format!("{}-browser", site.name()));
        std::fs::create_dir_all(&profile).map_err(err)?;
        let mut command = Command::new(executable(settings)?);
        command.args([
            format!("--remote-debugging-port={port}"),
            "--remote-debugging-address=127.0.0.1".into(),
            format!("--user-data-dir={}", profile.display()),
            "--no-first-run".into(),
            "--no-default-browser-check".into(),
            "about:blank".into(),
        ]);
        command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut child = command.spawn().map_err(err)?;
        let deadline = Instant::now() + Duration::from_secs(20);
        while targets_port(port).is_err() {
            if child.try_wait().map_err(err)?.is_some() || Instant::now() > deadline {
                return Err("浏览器启动失败，请关闭此账号的专用浏览器后重试".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        launched = true;
    }
    let version: Value = agent()
        .get(format!("http://127.0.0.1:{port}/json/version"))
        .call()
        .map_err(err)?
        .body_mut()
        .read_json()
        .map_err(err)?;
    let socket = version["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("无浏览器连接")?;
    if site != Site::Official && !launched {
        let owned = std::fs::read(&session_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<String>(&b).ok())
            .is_some_and(|saved| saved == socket);
        let legacy_tab = std::fs::read(&tab_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<String>(&b).ok());
        let legacy_owned = existing.as_ref().is_ok_and(|list| {
            list.iter()
                .any(|t| t["type"] == "page" && t["id"].as_str() == legacy_tab.as_deref())
        });
        if !owned && !legacy_owned {
            return Err(
                "此账号的浏览器端口被其他会话占用；请关闭冲突的专用浏览器后重新打开".into(),
            );
        }
    }
    if site != Site::Official {
        super::atomic_json(&session_path, &socket)?;
    }
    let mut client = Cdp::connect(socket, port)?;
    let target = client.call(
        "Target.createTarget",
        json!({"url":url.as_str(),"background":false}),
    )?["targetId"]
        .as_str()
        .ok_or("无法打开标签页")?
        .to_owned();
    super::atomic_json(&tab_path, &target)?;
    Ok(target)
}
pub fn selected(dir: &Path, site: Site) -> Result<Cdp> {
    selected_for(dir, &Settings::load(dir)?, site)
}
pub fn selected_for(dir: &Path, settings: &Settings, site: Site) -> Result<Cdp> {
    let (runtime, port) = context(dir, settings, site)?;
    let list = targets_port(port)?;
    let saved = std::fs::read(runtime.join(format!("{}-tab.json", site.name())))
        .ok()
        .and_then(|b| serde_json::from_slice::<String>(&b).ok());
    let target = list
        .iter()
        .find(|t| t["type"] == "page" && saved.as_deref() == t["id"].as_str())
        .ok_or("此账号的工具标签页已关闭，请重新打开目标网页")?;
    let raw = target["url"].as_str().ok_or("网页地址无效")?;
    // Chrome can briefly expose an empty URL while a navigation starts. The tab id
    // is pinned to this account; callers wait for the exact requested URL next.
    if site == Site::Official || (!raw.is_empty() && raw != "about:blank") {
        validate_url(raw, site)?;
    }
    Cdp::connect(
        target["webSocketDebuggerUrl"]
            .as_str()
            .ok_or("无标签页连接")?,
        port,
    )
}
pub fn adopt_new_chat(dir: &Path, settings: &Settings, before: &[Value]) -> Result<()> {
    let (runtime, _) = context(dir, settings, Site::Boss)?;
    let list = targets_for(dir, settings, Site::Boss)?;
    let path = runtime.join("boss-tab.json");
    let source = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<String>(&b).ok());
    let new: Vec<_> = list
        .iter()
        .filter(|t| {
            t["type"] == "page"
                && !before.iter().any(|b| b["id"] == t["id"])
                && t["url"].as_str().is_some_and(|s| {
                    validate_url(s, Site::Boss).is_ok() && s.contains("/web/geek/chat")
                })
                && t["openerId"]
                    .as_str()
                    .is_none_or(|id| Some(id) == source.as_deref())
        })
        .collect();
    if new.len() == 1 {
        super::atomic_json(&path, &new[0]["id"])?;
    } else if new.len() > 1 {
        return Err("出现多个聊天标签页，已停止自动发送".into());
    }
    Ok(())
}
pub struct Cdp {
    ws: WebSocket<TcpStream>,
    next: u64,
}
impl Cdp {
    pub fn connect(raw: &str, port: u16) -> Result<Self> {
        let url = url::Url::parse(raw).map_err(err)?;
        if url.scheme() != "ws" || url.host_str() != Some("127.0.0.1") || url.port() != Some(port) {
            return Err("拒绝非本机浏览器连接".into());
        }
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let socket = TcpStream::connect_timeout(&address, Duration::from_secs(5)).map_err(err)?;
        socket
            .set_read_timeout(Some(Duration::from_secs(12)))
            .map_err(err)?;
        socket
            .set_write_timeout(Some(Duration::from_secs(12)))
            .map_err(err)?;
        let (ws, _) = tungstenite::client(raw, socket).map_err(|_| "浏览器握手失败".to_owned())?;
        Ok(Self { ws, next: 0 })
    }
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next += 1;
        self.ws
            .send(Message::Text(
                json!({"id":self.next,"method":method,"params":params})
                    .to_string()
                    .into(),
            ))
            .map_err(|_| "浏览器连接已断开".to_owned())?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if Instant::now() > deadline {
                return Err("浏览器操作超时".into());
            }
            let message = self
                .ws
                .read()
                .map_err(|_| "浏览器无响应，请重新打开网页".to_owned())?;
            if let Message::Text(text) = message {
                let reply: Value = serde_json::from_str(&text).map_err(err)?;
                if reply["id"].as_u64() != Some(self.next) {
                    continue;
                }
                if reply.get("error").is_some() {
                    return Err("浏览器未能完成操作，请检查页面".into());
                }
                return Ok(reply["result"].clone());
            }
        }
    }
    pub fn eval(&mut self, expression: &str) -> Result<Value> {
        let reply = self.call(
            "Runtime.evaluate",
            json!({"expression":expression,"returnByValue":true,"awaitPromise":true}),
        )?;
        if reply.get("exceptionDetails").is_some() {
            return Err("页面结构已变化，无法完成此操作".into());
        }
        let value = reply["result"]["value"].clone();
        if let Some(text) = value.as_str() {
            Ok(serde_json::from_str(text).unwrap_or(value))
        } else {
            Ok(value)
        }
    }
    pub fn click(&mut self, point: &Value) -> Result<()> {
        let x = point["x"].as_f64().ok_or("未定位到按钮")?;
        let y = point["y"].as_f64().ok_or("未定位到按钮")?;
        for kind in ["mousePressed", "mouseReleased"] {
            self.call(
                "Input.dispatchMouseEvent",
                json!({"type":kind,"x":x,"y":y,"button":"left","clickCount":1}),
            )?;
        }
        Ok(())
    }
    pub fn wait_ready(&mut self) -> Result<()> {
        for _ in 0..150 {
            if self
                .eval("({ready:document.readyState,url:location.href})")
                .is_ok_and(|v| {
                    v["ready"] == "complete"
                        && v["url"]
                            .as_str()
                            .is_some_and(|u| u.starts_with("http://") || u.starts_with("https://"))
                })
            {
                std::thread::sleep(Duration::from_millis(600));
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Err("页面尚未加载完成".into())
    }
    pub fn wait_location(&mut self, expected: &str) -> Result<()> {
        for _ in 0..100 {
            if self.eval("location.href").is_ok_and(|v| {
                v.as_str()
                    .is_some_and(|actual| location_matches(expected, actual))
            }) {
                return self.wait_ready();
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Err("网页尚未进入请求的岗位或搜索条件，已停止读取".into())
    }
    pub fn upload(&mut self, expression: &str, paths: &[String]) -> Result<()> {
        if paths.is_empty() || paths.iter().any(|p| !Path::new(p).is_file()) {
            return Err("附件不存在，请重新选择文件".into());
        }
        let object = self.call("Runtime.evaluate", json!({"expression":expression}))?;
        let id = object["result"]["objectId"]
            .as_str()
            .ok_or("未找到附件输入框")?;
        self.call(
            "DOM.setFileInputFiles",
            json!({"files":paths,"objectId":id}),
        )?;
        Ok(())
    }
}

fn location_matches(expected: &str, actual: &str) -> bool {
    let (Ok(expected), Ok(actual)) = (url::Url::parse(expected), url::Url::parse(actual)) else {
        return false;
    };
    let expected_path = if expected.path() == "/web/geek/job" {
        "/web/geek/jobs"
    } else {
        expected.path()
    };
    let actual_path = if actual.path() == "/web/geek/job" {
        "/web/geek/jobs"
    } else {
        actual.path()
    };
    expected.host_str() == actual.host_str()
        && expected_path == actual_path
        && expected
            .query_pairs()
            .all(|(key, value)| actual.query_pairs().any(|(k, v)| k == key && v == value))
}

#[cfg(test)]
mod navigation_tests {
    use super::*;
    #[test]
    fn redirected_search_must_match_requested_query_and_job_identity() {
        let expected = "https://www.zhipin.com/web/geek/job?query=Java&city=100010000&page=1";
        assert!(location_matches(
            expected,
            "https://www.zhipin.com/web/geek/jobs?page=1&city=100010000&query=Java"
        ));
        assert!(!location_matches(expected, "about:blank"));
        assert!(!location_matches(
            expected,
            "https://www.zhipin.com/web/geek/jobs?query=AI&city=100010000&page=1"
        ));
        assert!(!location_matches(
            expected,
            "https://www.zhipin.com/web/geek/jobs?query=Java&city=101020100&page=1"
        ));
        assert!(!location_matches(
            "https://www.zhipin.com/job_detail/a.html",
            "https://www.zhipin.com/job_detail/b.html"
        ));
        assert!(!location_matches(
            expected,
            "https://example.test/web/geek/jobs?query=Java&city=100010000&page=1"
        ));
    }
}
