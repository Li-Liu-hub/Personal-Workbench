use super::{Result, err};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    io::Read,
    net::{IpAddr, Ipv4Addr, UdpSocket},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;
pub struct Inbox {
    items: VecDeque<(Zeroizing<String>, Instant, i64)>,
    pub received: u64,
    pub with_code: u64,
}
impl Inbox {
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
            received: 0,
            with_code: 0,
        }
    }
    fn prune(&mut self) {
        self.items
            .retain(|(_, at, _)| at.elapsed() < Duration::from_secs(120));
    }
    pub fn latest(&mut self) -> Option<Zeroizing<String>> {
        self.prune();
        self.items.back().map(|(code, _, _)| code.clone())
    }
    fn add(&mut self, content: &str) -> bool {
        self.prune();
        self.received += 1;
        if let Some(code) = extract(content) {
            self.with_code += 1;
            self.items
                .push_back((Zeroizing::new(code), Instant::now(), unix()));
            while self.items.len() > 20 {
                self.items.pop_front();
            }
            true
        } else {
            false
        }
    }
    pub fn status(&mut self) -> Value {
        self.prune();
        json!({"received":self.received,"with_code":self.with_code,"pending":self.items.len(),"ttl_seconds":120,"latest_received_at":self.items.back().map(|(_,_,t)|*t)})
    }
}
pub fn extract(content: &str) -> Option<String> {
    let re=regex::Regex::new(r"(?i)(?:验证码|校验码|动态码|动态密码|确认码|验证代码|登录码|verification\s*code|passcode)[^0-9A-Za-z]{0,24}([0-9]{4,8})(?:[^0-9]|$)").unwrap();
    if let Some(c) = re.captures(content) {
        return Some(c[1].into());
    }
    if ![
        "验证码",
        "校验码",
        "动态码",
        "动态密码",
        "确认码",
        "验证代码",
        "登录码",
        "verification code",
        "passcode",
    ]
    .iter()
    .any(|k| content.to_lowercase().contains(k))
    {
        return None;
    }
    let re = regex::Regex::new(r"[0-9]+").unwrap();
    let mut codes: Vec<_> = re
        .find_iter(content)
        .map(|m| m.as_str())
        .filter(|s| s.len() == 6)
        .collect();
    codes.sort();
    codes.dedup();
    (codes.len() == 1).then(|| codes[0].to_owned())
}
fn unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn host(raw: &str) -> Result<IpAddr> {
    let ip = if raw == "auto" {
        auto_host()
    } else {
        raw.parse()
            .map_err(|_| "监听地址须为 auto 或本机局域网 IP".to_owned())?
    };
    match ip {
        IpAddr::V4(v) if v.is_loopback() || v.is_private() => Ok(ip),
        _ => Err("接码服务只允许绑定本机回环或私有局域网地址".into()),
    }
}
fn auto_host() -> IpAddr {
    #[cfg(windows)]
    if let Some(ip) = lan_ip() {
        return IpAddr::V4(ip);
    }
    match default_route_ip() {
        IpAddr::V4(v) if v.is_private() || v.is_loopback() => IpAddr::V4(v),
        _ => IpAddr::V4(Ipv4Addr::LOCALHOST),
    }
}
fn default_route_ip() -> IpAddr {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("8.8.8.8:80")?;
            socket.local_addr()
        })
        .map(|address| address.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}
#[derive(Clone, Copy)]
struct LanCandidate {
    ip: Ipv4Addr,
    gateway: bool,
    wireless: bool,
}
// Proxy/VPN adapters can own the default route (for example 198.18.x.x), so auto
// picks a real LAN adapter with a gateway instead of probing 8.8.8.8.
fn pick_lan(candidates: &[LanCandidate]) -> Option<Ipv4Addr> {
    candidates
        .iter()
        .filter(|candidate| {
            candidate.ip.is_private()
                && !candidate.ip.is_loopback()
                && !candidate.ip.is_link_local()
        })
        .max_by_key(|candidate| (candidate.gateway, candidate.wireless))
        .map(|candidate| candidate.ip)
}
#[cfg(windows)]
fn lan_ip() -> Option<Ipv4Addr> {
    use windows_sys::Win32::{
        Foundation::ERROR_BUFFER_OVERFLOW,
        NetworkManagement::{
            IpHelper::{
                GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
                GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses, IF_TYPE_IEEE80211,
                IP_ADAPTER_ADDRESSES_LH,
            },
            Ndis::IfOperStatusUp,
        },
        Networking::WinSock::AF_INET,
    };
    let mut size = 15_000u32;
    let mut buffer = Vec::new();
    let mut attempts = 0;
    loop {
        buffer.resize(size as usize, 0u8);
        let mut used = size;
        let result = unsafe {
            GetAdaptersAddresses(
                AF_INET as u32,
                GAA_FLAG_INCLUDE_GATEWAYS
                    | GAA_FLAG_SKIP_ANYCAST
                    | GAA_FLAG_SKIP_MULTICAST
                    | GAA_FLAG_SKIP_DNS_SERVER,
                std::ptr::null(),
                buffer.as_mut_ptr().cast(),
                &mut used,
            )
        };
        if result == ERROR_BUFFER_OVERFLOW {
            attempts += 1;
            if attempts > 4 {
                return None;
            }
            size = used.max(size.saturating_mul(2)).saturating_add(4096);
            continue;
        }
        if result != 0 {
            return None;
        }
        break;
    }
    let mut candidates = Vec::new();
    let mut adapter = buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    while !adapter.is_null() {
        let info = unsafe { &*adapter };
        if info.OperStatus == IfOperStatusUp {
            let name = format!(
                "{} {}",
                wide_string(info.FriendlyName),
                wide_string(info.Description)
            );
            if !virtual_adapter(&name) {
                let gateway = has_ipv4_gateway(info.FirstGatewayAddress);
                let wireless = info.IfType == IF_TYPE_IEEE80211;
                let mut unicast = info.FirstUnicastAddress;
                while !unicast.is_null() {
                    let entry = unsafe { &*unicast };
                    if let Some(ip) = socket_ipv4(entry.Address) {
                        candidates.push(LanCandidate {
                            ip,
                            gateway,
                            wireless,
                        });
                    }
                    unicast = entry.Next;
                }
            }
        }
        adapter = info.Next;
    }
    pick_lan(&candidates)
}
#[cfg(windows)]
fn virtual_adapter(name: &str) -> bool {
    let name = name.to_lowercase();
    [
        "vmware",
        "virtualbox",
        "hyper-v",
        "vethernet",
        "loopback",
        "tailscale",
        "zerotier",
        "wintun",
        "tap-windows",
        "openvpn",
        "clash",
        "meta",
        "tunnel",
    ]
    .iter()
    .any(|needle| name.contains(needle))
}
#[cfg(windows)]
fn wide_string(pointer: *const u16) -> String {
    if pointer.is_null() {
        return String::new();
    }
    let mut length = 0usize;
    unsafe {
        while *pointer.add(length) != 0 {
            length += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length))
    }
}
#[cfg(windows)]
fn socket_ipv4(
    address: windows_sys::Win32::Networking::WinSock::SOCKET_ADDRESS,
) -> Option<Ipv4Addr> {
    use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};
    if address.lpSockaddr.is_null()
        || (address.iSockaddrLength as usize) < std::mem::size_of::<SOCKADDR_IN>()
    {
        return None;
    }
    unsafe {
        let socket = &*address.lpSockaddr;
        if socket.sa_family != AF_INET {
            return None;
        }
        let ipv4 = &*address.lpSockaddr.cast::<SOCKADDR_IN>();
        Some(Ipv4Addr::from(ipv4.sin_addr.S_un.S_addr.to_ne_bytes()))
    }
}
#[cfg(windows)]
fn has_ipv4_gateway(
    mut entry: *mut windows_sys::Win32::NetworkManagement::IpHelper::IP_ADAPTER_GATEWAY_ADDRESS_LH,
) -> bool {
    while !entry.is_null() {
        let gateway = unsafe { &*entry };
        if socket_ipv4(gateway.Address).is_some() {
            return true;
        }
        entry = gateway.Next;
    }
    false
}
pub fn private_file(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let user = std::env::var("USERNAME").map_err(err)?;
        let result = std::process::Command::new("icacls.exe")
            .arg(path)
            .args(["/inheritance:r", "/grant:r", &format!("{user}:(F)")])
            .creation_flags(0x08000000)
            .output()
            .map_err(err)?;
        if !result.status.success() {
            return Err("无法为接码密钥设置当前用户访问权限".into());
        }
    }
    Ok(())
}
fn token(dir: &Path) -> Result<Zeroizing<String>> {
    let path = dir.join("otp_token.txt");
    if path.exists() {
        private_file(&path)?;
        let raw = Zeroizing::new(
            std::fs::read_to_string(path)
                .map_err(err)?
                .trim()
                .to_owned(),
        );
        if raw.len() < 32
            || !raw
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err("现有接码密钥无效，请重新导入或更换密钥文件".into());
        }
        return Ok(raw);
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(err)?;
    let raw = Zeroizing::new(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
    super::atomic_write(&path, raw.as_bytes())?;
    private_file(&path)?;
    Ok(raw)
}
pub struct Receiver {
    pub inbox: Arc<Mutex<Inbox>>,
    pub address: String,
    server: Arc<tiny_http::Server>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    runtime: PathBuf,
}
impl Receiver {
    pub fn start(dir: &Path, raw_host: &str, port: u16) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(err)?;
        let host = host(raw_host)?;
        let server = Arc::new(
            tiny_http::Server::http((host, port))
                .map_err(|_| format!("无法监听 {host}:{port}，请检查旧接码服务是否仍在运行"))?,
        );
        let actual = server.server_addr().to_ip().ok_or("监听地址无效")?;
        let token = token(dir)?;
        let url = Zeroizing::new(format!("http://{actual}/otp/{}", token.as_str()));
        let url_path = dir.join("otp_url.txt");
        super::atomic_write(&url_path, url.as_bytes())?;
        private_file(&url_path)?;
        let runtime = dir.join("otp_runtime.json");
        super::atomic_json(
            &runtime,
            &json!({"host":host.to_string(),"port":actual.port(),"pid":std::process::id()}),
        )?;
        private_file(&runtime)?;
        let inbox = Arc::new(Mutex::new(Inbox::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (s, i, quit) = (server.clone(), inbox.clone(), stop.clone());
        let handle = std::thread::spawn(move || {
            let mut buckets: HashMap<IpAddr, VecDeque<Instant>> = HashMap::new();
            while !quit.load(Ordering::Acquire) {
                let mut request = match s.recv_timeout(Duration::from_millis(250)) {
                    Ok(Some(r)) => r,
                    _ => continue,
                };
                let (status, body) = handle(&mut request, &token, &host, &i, &mut buckets);
                let response = tiny_http::Response::from_string(body.to_string())
                    .with_status_code(status)
                    .with_header(
                        tiny_http::Header::from_bytes(
                            "Content-Type",
                            "application/json; charset=utf-8",
                        )
                        .unwrap(),
                    )
                    .with_header(
                        tiny_http::Header::from_bytes("Cache-Control", "no-store").unwrap(),
                    );
                let _ = request.respond(response);
            }
        });
        Ok(Self {
            inbox,
            address: actual.to_string(),
            server,
            stop,
            thread: Some(handle),
            runtime,
        })
    }
}
// Replies never contain the SMS body or a secret URL. Latest is restricted to this machine.
fn handle(
    request: &mut tiny_http::Request,
    token: &str,
    host: &IpAddr,
    inbox: &Arc<Mutex<Inbox>>,
    buckets: &mut HashMap<IpAddr, VecDeque<Instant>>,
) -> (u16, Value) {
    (|| -> (u16, Value) {
        let Some(peer) = request.remote_addr().map(|a| a.ip()) else {
            return (403, json!({"ok":false}));
        };
        let path = request.url().split('?').next().unwrap_or("");
        let parts: Vec<_> = path.split('/').collect();
        let candidate = parts.get(2).copied().unwrap_or("");
        let mut mismatch = token.len() ^ candidate.len();
        for (a, b) in token.bytes().zip(candidate.bytes()) {
            mismatch |= usize::from(a ^ b);
        }
        if parts.get(1) != Some(&"otp") || mismatch != 0 {
            return (403, json!({"ok":false}));
        }
        let suffix = match parts.as_slice() {
            ["", "otp", _] => Some(""),
            ["", "otp", _, "status"] => Some("/status"),
            ["", "otp", _, "latest"] => Some("/latest"),
            _ => None,
        };
        if !matches!(suffix, Some("" | "/status" | "/latest")) {
            return (403, json!({"ok":false}));
        }
        if request.method() == &tiny_http::Method::Get && suffix == Some("/status") {
            return (200, inbox.lock().unwrap().status());
        }
        if request.method() == &tiny_http::Method::Get && suffix == Some("/latest") {
            if !peer.is_loopback() && peer != *host {
                return (403, json!({"ok":false}));
            }
            return match inbox.lock().unwrap().latest() {
                Some(code) => (200, json!({"code":code.as_str()})),
                None => (404, json!({"ok":false})),
            };
        }
        if request.method() != &tiny_http::Method::Post || suffix != Some("") {
            return (405, json!({"ok":false}));
        }
        buckets.retain(|_, v| {
            v.retain(|at| at.elapsed() < Duration::from_secs(60));
            !v.is_empty()
        });
        if buckets.len() >= 256 && !buckets.contains_key(&peer) {
            return (429, json!({"ok":false}));
        }
        let bucket = buckets.entry(peer).or_default();
        if bucket.len() >= 60 {
            return (429, json!({"ok":false}));
        }
        bucket.push_back(Instant::now());
        if request.body_length().is_none_or(|n| n > 65536) {
            return (413, json!({"ok":false}));
        }
        let mut body = Zeroizing::new(String::new());
        if request
            .as_reader()
            .take(65537)
            .read_to_string(&mut body)
            .is_err()
            || body.len() > 65536
        {
            return (400, json!({"ok":false}));
        }
        let value: Value = serde_json::from_str(&body).unwrap_or_else(|_| {
            Value::Object(
                url::form_urlencoded::parse(body.as_bytes())
                    .map(|(k, v)| (k.into_owned(), Value::String(v.into_owned())))
                    .collect(),
            )
        });
        if let Some(t) = value.get("timestamp").filter(|v| !v.is_null() && *v != "") {
            let number = t
                .as_f64()
                .or_else(|| t.as_str().and_then(|s| s.parse::<f64>().ok()));
            let Some(mut t) = number else {
                return (
                    400,
                    json!({"ok":false,"error":"stale_or_invalid_timestamp"}),
                );
            };
            if t > 1e12 {
                t /= 1000.;
            }
            if !t.is_finite() || (t - unix() as f64).abs() > 600. {
                return (
                    400,
                    json!({"ok":false,"error":"stale_or_invalid_timestamp"}),
                );
            }
        }
        let content = ["content", "message", "body"]
            .iter()
            .find_map(|k| value[*k].as_str())
            .unwrap_or("");
        let found = inbox.lock().unwrap().add(content);
        (200, json!({"ok":true,"code_found":found}))
    })()
}
impl Drop for Receiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.server.unblock();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.inbox.lock().unwrap().items.clear();
        let _ = std::fs::remove_file(&self.runtime);
    }
}
// Bridge to a receiver owned by the desktop or another MCP session. The secret
// URL and code remain inside the native process and are never MCP return values.
fn service_request(dir: &Path, suffix: &str) -> Result<Value> {
    let runtime: Value = serde_json::from_slice(
        &std::fs::read(dir.join("otp_runtime.json")).map_err(|_| "接码服务未运行".to_owned())?,
    )
    .map_err(|_| "接码服务状态无效".to_owned())?;
    let ip: IpAddr = runtime["host"]
        .as_str()
        .ok_or("接码地址无效")?
        .parse()
        .map_err(|_| "接码地址无效".to_owned())?;
    host(&ip.to_string())?;
    // Only addresses bound to this machine may receive the local secret.
    let _local = UdpSocket::bind((ip, 0)).map_err(|_| "接码地址不属于本机".to_owned())?;
    let port = runtime["port"]
        .as_u64()
        .filter(|p| *p > 0 && *p <= 65535)
        .ok_or("接码端口无效")?;
    let token = Zeroizing::new(
        std::fs::read_to_string(dir.join("otp_token.txt"))
            .map_err(|_| "接码密钥不可用".to_owned())?,
    );
    let token = token.trim();
    if token.len() < 32
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("接码密钥格式无效".into());
    }
    let url = Zeroizing::new(format!("http://{ip}:{port}/otp/{token}/{suffix}"));
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .into();
    let body = Zeroizing::new(
        agent
            .get(url.as_str())
            .call()
            .map_err(|_| "接码服务未响应或暂无有效验证码".to_owned())?
            .body_mut()
            .read_to_string()
            .map_err(|_| "接码响应无效".to_owned())?,
    );
    serde_json::from_str(&body).map_err(|_| "接码响应无效".to_owned())
}
pub fn service_status(dir: &Path) -> Result<Value> {
    let value = service_request(dir, "status")?;
    Ok(
        json!({"received":value["received"],"with_code":value["with_code"],"pending":value["pending"],"ttl_seconds":120}),
    )
}
pub fn service_inbox(dir: &Path) -> Result<Arc<Mutex<Inbox>>> {
    let mut value = service_request(dir, "latest")?;
    let Value::String(raw) = value["code"].take() else {
        return Err("暂无有效验证码".into());
    };
    let code = Zeroizing::new(raw);
    if !(4..=8).contains(&code.len()) || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err("验证码格式无效".into());
    }
    let mut inbox = Inbox::new();
    inbox.items.push_back((code, Instant::now(), unix()));
    Ok(Arc::new(Mutex::new(inbox)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extraction_avoids_orders_and_ambiguous_numbers() {
        assert_eq!(
            extract("【平台】验证码为 123456，5 分钟内有效"),
            Some("123456".into())
        );
        assert_eq!(extract("校验码：4321"), Some("4321".into()));
        assert_eq!(
            extract("Your verification code is 987654."),
            Some("987654".into())
        );
        assert!(extract("订单123456已发货").is_none());
        assert!(extract("验证码 123456 和 234567").is_some());
        assert!(extract("验证码已发送，请查收").is_none());
    }
    #[test]
    fn expiry_and_status_do_not_expose_code() {
        let mut i = Inbox::new();
        i.add("验证码123456");
        assert!(!i.status().to_string().contains("123456"));
        i.items.back_mut().unwrap().1 = Instant::now() - Duration::from_secs(121);
        assert!(i.latest().is_none());
    }
    #[test]
    fn wildcard_and_public_bind_are_rejected() {
        assert!(host("0.0.0.0").is_err());
        assert!(host("8.8.8.8").is_err());
    }
    #[test]
    fn auto_lan_prefers_real_adapter_over_proxy_and_virtual_ones() {
        let candidate = |ip: &str, gateway, wireless| LanCandidate {
            ip: ip.parse().unwrap(),
            gateway,
            wireless,
        };
        let candidates = [
            candidate("198.18.0.1", true, false),
            candidate("192.168.150.1", false, false),
            candidate("172.20.10.5", true, true),
        ];
        assert_eq!(pick_lan(&candidates), Some("172.20.10.5".parse().unwrap()));
        assert_eq!(pick_lan(&[]), None);
        assert_eq!(pick_lan(&[candidate("8.8.8.8", true, false)]), None);
    }
    #[cfg(windows)]
    #[test]
    fn auto_ignores_known_virtual_adapter_names() {
        assert!(virtual_adapter("VMware Network Adapter VMnet8"));
        assert!(virtual_adapter("Meta"));
        assert!(!virtual_adapter("WLAN"));
        assert!(!virtual_adapter("以太网"));
    }
    #[test]
    fn webhook_http_auth_freshness_status_and_limits() {
        let d = tempfile::tempdir().unwrap();
        let receiver = Receiver::start(d.path(), "127.0.0.1", 0).unwrap();
        let url = std::fs::read_to_string(d.path().join("otp_url.txt")).unwrap();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .proxy(None)
            .http_status_as_error(false)
            .build()
            .into();
        let wrong = format!("http://{}/otp/wrong/status", receiver.address);
        assert_eq!(agent.get(wrong).call().unwrap().status().as_u16(), 403);
        let response: Value = agent
            .post(&url)
            .send_json(json!({"content":"验证码123456"}))
            .unwrap()
            .body_mut()
            .read_json()
            .unwrap();
        assert_eq!(response, json!({"ok":true,"code_found":true}));
        let status = agent
            .get(format!("{url}/status"))
            .call()
            .unwrap()
            .body_mut()
            .read_to_string()
            .unwrap();
        assert!(!status.contains("123456"));
        let latest: Value = agent
            .get(format!("{url}/latest"))
            .call()
            .unwrap()
            .body_mut()
            .read_json()
            .unwrap();
        assert_eq!(latest["code"], "123456");
        assert_eq!(
            agent
                .post(&url)
                .send_json(json!({"content":"验证码123456","timestamp":unix()-3600}))
                .unwrap()
                .status()
                .as_u16(),
            400
        );
        for _ in 0..58 {
            agent
                .post(&url)
                .send_json(json!({"content":"无验证码"}))
                .unwrap();
        }
        assert_eq!(
            agent
                .post(&url)
                .send_json(json!({"content":"验证码123456"}))
                .unwrap()
                .status()
                .as_u16(),
            429
        );
        drop(receiver);
        assert!(!d.path().join("otp_runtime.json").exists());
    }
}
