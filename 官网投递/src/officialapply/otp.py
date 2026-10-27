"""Local OTP receiver for SmsForwarder webhooks."""

from __future__ import annotations

import hmac
import ipaddress
import json
import logging
import os
import re
import secrets
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from collections import deque
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Callable
from urllib.parse import parse_qs, quote, unquote_plus, urlparse

from officialapply.errors import ToolError

LOGGER = logging.getLogger("officialapply.otp")

_KEYWORDS = (
    "验证码",
    "校验码",
    "动态码",
    "动态密码",
    "确认码",
    "验证代码",
    "登录码",
    "verification code",
    "passcode",
)
_KEYWORD_CODE = re.compile(
    r"(?:验证码|校验码|动态码|动态密码|确认码|验证代码|登录码|verification\s*code|passcode)"
    r"[^0-9A-Za-z]{0,24}([0-9]{4,8})",
    re.IGNORECASE,
)
_SIX_DIGITS = re.compile(r"(?<!\d)([0-9]{6})(?!\d)")


def extract_verification_code(content: str) -> str | None:
    text = " ".join(str(content or "").split())
    if not text:
        return None
    match = _KEYWORD_CODE.search(text)
    if match:
        return match.group(1)
    lowered = text.lower()
    if not any(keyword in text or keyword in lowered for keyword in _KEYWORDS):
        return None
    matches = list(dict.fromkeys(_SIX_DIGITS.findall(text)))
    return matches[0] if len(matches) == 1 else None


def mask_sender(value: str) -> str:
    text = " ".join(str(value or "").split())
    if not text:
        return ""
    if text.isdigit() and len(text) >= 7:
        return f"{text[:3]}****{text[-2:]}"
    return text[:48]


@dataclass(frozen=True)
class OtpMessage:
    code: str
    sender: str
    received_at: float
    received_at_text: str


class OtpInbox:
    def __init__(self, ttl_seconds: float = 120.0, *, clock: Callable[[], float] = time.time) -> None:
        self.ttl_seconds = float(ttl_seconds)
        self._clock = clock
        self._lock = threading.Lock()
        self._items: deque[OtpMessage] = deque(maxlen=20)
        self._received = 0
        self._with_code = 0
        self._last_error: str | None = None

    def add(self, sender: str, content: str) -> OtpMessage | None:
        code = extract_verification_code(content)
        now = self._clock()
        with self._lock:
            self._received += 1
            self._prune_locked(now)
            if not code:
                self._last_error = "code_not_found"
                return None
            item = OtpMessage(
                code=code,
                sender=mask_sender(sender),
                received_at=now,
                received_at_text=time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(now)),
            )
            self._items.append(item)
            self._with_code += 1
            self._last_error = None
            return item

    def latest(self, *, since: float | None = None) -> OtpMessage | None:
        with self._lock:
            self._prune_locked(self._clock())
            for item in reversed(self._items):
                if since is None or item.received_at > since:
                    return item
            return None

    def status(self) -> dict[str, Any]:
        with self._lock:
            self._prune_locked(self._clock())
            latest = self._items[-1] if self._items else None
            return {
                "received": self._received,
                "with_code": self._with_code,
                "pending": len(self._items),
                "ttl_seconds": self.ttl_seconds,
                "latest_received_at": latest.received_at_text if latest else None,
                "latest_sender": latest.sender if latest else None,
                "last_error": self._last_error,
            }

    def _prune_locked(self, now: float) -> None:
        while self._items and now - self._items[0].received_at > self.ttl_seconds:
            self._items.popleft()


def wait_for_code(
    inbox: OtpInbox,
    *,
    since: float | None = None,
    timeout: float = 120.0,
    poll_interval: float = 0.5,
) -> OtpMessage | None:
    deadline = time.monotonic() + max(0.0, timeout)
    while True:
        item = inbox.latest(since=since)
        if item is not None:
            return item
        if time.monotonic() >= deadline:
            return None
        time.sleep(max(0.05, poll_interval))


class OtpReceiver:
    def __init__(
        self,
        inbox: OtpInbox,
        token: str,
        *,
        host: str = "0.0.0.0",
        port: int = 8765,
        max_clock_skew_seconds: int = 600,
        max_requests_per_minute: int = 60,
        logger: logging.Logger | None = None,
    ) -> None:
        if not token:
            raise ToolError("otp_token_missing", "OTP 接收端缺少 token")
        self.inbox = inbox
        self.token = token
        self.host = host
        self.port = int(port)
        self.max_clock_skew_seconds = max(0, int(max_clock_skew_seconds))
        self.max_requests_per_minute = max(0, int(max_requests_per_minute))
        self.logger = logger or LOGGER
        self._httpd: ThreadingHTTPServer | None = None
        self._thread: threading.Thread | None = None
        self._rate_lock = threading.Lock()
        self._rate_buckets: dict[str, deque[float]] = {}

    @property
    def actual_port(self) -> int:
        if self._httpd is not None:
            return int(self._httpd.server_address[1])
        return self.port

    def start(self) -> None:
        if self._httpd is not None:
            return
        handler = _make_handler(self)
        try:
            self._httpd = ThreadingHTTPServer((self.host, self.port), handler)
        except OSError as exc:
            raise ToolError(
                "otp_listen_failed",
                f"无法监听 {self.host}:{self.port}：{exc}",
                host=self.host,
                port=self.port,
            ) from exc
        self._httpd.daemon_threads = True
        self._thread = threading.Thread(
            target=self._httpd.serve_forever,
            name="officialapply-otp",
            daemon=True,
        )
        self._thread.start()

    def stop(self) -> None:
        if self._httpd is None:
            return
        self._httpd.shutdown()
        self._httpd.server_close()
        self._httpd = None
        self._thread = None

    def token_matches(self, path: str) -> bool:
        parsed = urlparse(path)
        parts = [unquote_plus(part) for part in parsed.path.strip("/").split("/") if part]
        if len(parts) >= 2 and parts[0] == "otp" and hmac.compare_digest(parts[1], self.token):
            return True
        query_token = (parse_qs(parsed.query).get("token") or [""])[-1]
        return bool(query_token) and hmac.compare_digest(query_token, self.token)

    def is_loopback(self, address: str) -> bool:
        try:
            return ipaddress.ip_address(address).is_loopback
        except ValueError:
            return False

    def is_local_client(self, address: str) -> bool:
        if self.is_loopback(address):
            return True
        return self.host not in ("", "0.0.0.0") and address == self.host

    def body_is_fresh(self, timestamp: Any) -> bool:
        if timestamp in (None, ""):
            return True
        try:
            value = float(timestamp)
        except (TypeError, ValueError):
            return False
        if value > 1e12:
            value /= 1000.0
        return abs(time.time() - value) <= self.max_clock_skew_seconds

    def allow_request(self, address: str) -> bool:
        if self.max_requests_per_minute <= 0:
            return True
        now = time.monotonic()
        with self._rate_lock:
            bucket = self._rate_buckets.setdefault(
                address,
                deque(maxlen=self.max_requests_per_minute),
            )
            while bucket and now - bucket[0] > 60:
                bucket.popleft()
            if len(bucket) >= self.max_requests_per_minute:
                return False
            bucket.append(now)
            return True


def _make_handler(receiver: OtpReceiver) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        server_version = "OfficialApplyOTP/1.0"
        protocol_version = "HTTP/1.1"

        def log_message(self, fmt: str, *args: Any) -> None:
            # Request lines contain the token; never write them to logs.
            receiver.logger.debug("OTP request from %s", self.client_address[0])

        def do_GET(self) -> None:
            if not receiver.token_matches(self.path):
                self._send(403, {"ok": False, "error": "forbidden"})
                return
            path = urlparse(self.path).path.rstrip("/")
            if path.endswith("/status"):
                self._send(200, {"ok": True, **receiver.inbox.status()})
                return
            if path.endswith("/latest"):
                if not receiver.is_local_client(self.client_address[0]):
                    self._send(403, {"ok": False, "error": "local_only"})
                    return
                item = receiver.inbox.latest()
                if item is None:
                    self._send(200, {"ok": True, "code": None})
                    return
                self._send(
                    200,
                    {
                        "ok": True,
                        "code": item.code,
                        "sender": item.sender,
                        "received_at": item.received_at,
                        "received_at_text": item.received_at_text,
                    },
                )
                return
            self._send(200, {"ok": True, "service": "officialapply-otp"})

        def do_POST(self) -> None:
            if not receiver.allow_request(self.client_address[0]):
                receiver.logger.warning("OTP request rate limited from %s", self.client_address[0])
                self._send(429, {"ok": False, "error": "too_many_requests"})
                return
            if not receiver.token_matches(self.path):
                self._send(403, {"ok": False, "error": "forbidden"})
                return
            try:
                length = int(self.headers.get("Content-Length") or 0)
            except ValueError:
                length = 0
            if length <= 0 or length > 64 * 1024:
                self._send(413, {"ok": False, "error": "invalid_body_size"})
                return
            raw = self.rfile.read(length)
            data = _parse_body(raw, self.headers.get("Content-Type") or "")
            sender = str(data.get("from") or data.get("sender") or data.get("phone") or "")
            content = str(data.get("content") or data.get("message") or data.get("body") or "")
            if not receiver.body_is_fresh(data.get("timestamp")):
                receiver.logger.warning("OTP rejected: stale or invalid timestamp")
                self._send(400, {"ok": False, "error": "stale_or_invalid_timestamp"})
                return
            item = receiver.inbox.add(sender, content)
            receiver.logger.info("OTP received sender=%s code_found=%s", mask_sender(sender), bool(item))
            self._send(200, {"ok": True, "code_found": bool(item)})

        def _send(self, status: int, payload: dict[str, Any]) -> None:
            body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(body)
            self.close_connection = True

    return Handler


def _parse_body(raw: bytes, content_type: str) -> dict[str, Any]:
    text = raw.decode("utf-8", errors="replace").strip()
    if not text:
        return {}
    if "application/json" in content_type.lower() or text.startswith("{"):
        try:
            data = json.loads(text)
        except json.JSONDecodeError:
            # SmsForwarder does raw substitutions; malformed JSON still carries the SMS text.
            return {"content": text}
        return data if isinstance(data, dict) else {"content": text}
    parsed = parse_qs(text, keep_blank_values=True)
    return {key: values[-1] for key, values in parsed.items() if values}


def restrict_file_access(path: Path) -> bool:
    """Keep secrets readable only by the current Windows user."""
    path = path.resolve()
    try:
        path.chmod(0o600)
    except OSError:
        pass
    if os.name != "nt":
        return True
    user = os.environ.get("USERNAME") or ""
    if not user:
        return False
    try:
        completed = subprocess.run(
            ["icacls", str(path), "/inheritance:r", "/grant:r", f"{user}:F"],
            capture_output=True,
            text=True,
            errors="replace",
            timeout=10,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return False
    return completed.returncode == 0


def write_private_file(path: Path, text: str) -> Path:
    path = path.resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    if not restrict_file_access(path):
        LOGGER.warning("无法收紧文件权限，请手动检查: %s", path)
    return path


def load_or_create_token(path: Path, *, rotate: bool = False, minimum_length: int = 32) -> str:
    path = path.resolve()
    if path.exists() and not rotate:
        token = path.read_text(encoding="utf-8").strip()
        # Tokens shorter than 32 chars are legacy and may have leaked; rotate them.
        if len(token) >= minimum_length:
            restrict_file_access(path)
            return token
    token = secrets.token_urlsafe(32)
    write_private_file(path, token + "\n")
    return token


def mask_url_token(url: str) -> str:
    parsed = urlparse(url)
    parts = parsed.path.strip("/").split("/")
    if len(parts) >= 2 and parts[0] == "otp":
        token = parts[1]
        parts[1] = f"{token[:4]}...{token[-2:]}" if len(token) > 8 else "***"
    return parsed._replace(path="/" + "/".join(parts)).geturl()


def write_url_file(path: Path, urls: list[tuple[str, str]]) -> Path:
    lines = ["# OTP 接收端完整地址（含 token，仅本机用户可读，勿分享）"]
    lines.extend(f"{name}: {url}" for name, url in urls)
    return write_private_file(path, "\n".join(lines) + "\n")


def detect_bind_address() -> tuple[str, str] | None:
    """Pick the hotspot/LAN adapter and skip VPN and virtual adapters."""
    blocked = ("vmware", "virtualbox", "vethernet", "loopback", "atrust", "vpn", "tap", "bluetooth")
    preferred = ("wlan", "wi-fi", "wifi", "无线", "本地连接*", "local area connection*")
    candidates: list[tuple[int, str, str]] = []
    for name, address in adapter_ipv4_addresses():
        lowered = name.lower()
        if any(word in lowered for word in blocked):
            continue
        score = 0
        if any(word in lowered for word in preferred):
            score += 10
        if address.startswith("172.20.10."):
            score += 5
        elif address.startswith("192.168."):
            score += 4
        elif address.startswith("10."):
            score += 1
        candidates.append((score, name, address))
    if not candidates:
        return None
    _, name, address = max(candidates, key=lambda item: item[0])
    return name, address


def adapter_ipv4_addresses() -> list[tuple[str, str]]:
    if os.name == "nt":
        parsed = _windows_adapter_addresses()
        if parsed:
            return parsed
    return [(address, address) for address in _socket_ipv4_addresses()]


def _windows_adapter_addresses() -> list[tuple[str, str]]:
    try:
        completed = subprocess.run(
            ["ipconfig"],
            capture_output=True,
            text=True,
            encoding="mbcs",
            errors="replace",
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return []
    current = ""
    result: list[tuple[str, str]] = []
    for line in completed.stdout.splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        lowered = stripped.lower()
        if stripped.endswith(":") and ("适配器" in stripped or "adapter" in lowered):
            current = stripped.rstrip(":").strip()
            continue
        match = re.search(r"IPv4[^:]*:\s*([0-9.]+)", stripped, re.IGNORECASE)
        if match and current:
            address = match.group(1)
            if not address.startswith("127.") and not address.startswith("169.254."):
                result.append((current, address))
    return result


def _socket_ipv4_addresses() -> list[str]:
    addresses: set[str] = set()
    try:
        for family, _, _, _, sockaddr in socket.getaddrinfo(socket.gethostname(), None, socket.AF_INET):
            if family == socket.AF_INET:
                addresses.add(str(sockaddr[0]))
    except OSError:
        pass
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.connect(("8.8.8.8", 80))
            addresses.add(str(sock.getsockname()[0]))
    except OSError:
        pass
    return sorted(
        address
        for address in addresses
        if not address.startswith("127.") and not address.startswith("169.254.")
    )


def receiver_urls(token: str, port: int, *, host_ip: str = "") -> list[tuple[str, str]]:
    adapters = adapter_ipv4_addresses()
    if host_ip:
        name = next((name for name, address in adapters if address == host_ip), host_ip)
        return [(name, f"http://{host_ip}:{port}/otp/{token}")]
    urls = []
    for name, address in adapters:
        urls.append((name, f"http://{address}:{port}/otp/{token}"))
    return urls


def write_runtime_file(path: Path, host: str, port: int) -> Path:
    payload = {
        "host": host,
        "port": int(port),
        "pid": os.getpid(),
        "started_at": time.strftime("%Y-%m-%d %H:%M:%S"),
    }
    path = path.resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False) + "\n", encoding="utf-8")
    restrict_file_access(path)
    return path


def read_runtime_endpoint(path: Path) -> tuple[str, int] | None:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        host = str(data.get("host") or "").strip()
        port = int(data.get("port") or 0)
    except (OSError, ValueError, TypeError, json.JSONDecodeError):
        return None
    if not host or not 1 <= port <= 65535:
        return None
    return host, port


class OtpClient:
    def __init__(self, base_url: str, token: str, *, timeout: float = 1.5) -> None:
        self.base_url = base_url.rstrip("/")
        self.token = token
        self.timeout = max(0.2, float(timeout))
        # Local LAN requests must not go through a system HTTP proxy.
        self._opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def _request(self, suffix: str = "") -> dict[str, Any]:
        url = f"{self.base_url}/otp/{quote(self.token, safe='')}{suffix}"
        request = urllib.request.Request(url, headers={"Accept": "application/json"})
        try:
            with self._opener.open(request, timeout=self.timeout) as response:
                raw = response.read().decode("utf-8", errors="replace")
        except urllib.error.HTTPError as exc:
            if exc.code == 403:
                raise ToolError("otp_forbidden", "OTP 接收端拒绝访问：token 不匹配或请求不是本机") from exc
            raise ToolError("otp_receiver_error", f"OTP 接收端返回 HTTP {exc.code}") from exc
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            raise ToolError("otp_receiver_unreachable", "无法连接 OTP 接收端") from exc
        try:
            data = json.loads(raw)
        except json.JSONDecodeError as exc:
            raise ToolError("otp_receiver_error", "OTP 接收端返回了无法解析的响应") from exc
        return data if isinstance(data, dict) else {}

    def status(self) -> dict[str, Any]:
        data = self._request("/status")
        if not data.get("ok"):
            raise ToolError("otp_receiver_error", "OTP 接收端状态异常")
        return data

    def latest(self) -> OtpMessage | None:
        data = self._request("/latest")
        if not data.get("ok"):
            raise ToolError("otp_receiver_error", "OTP 接收端返回错误")
        code = str(data.get("code") or "")
        if not code:
            return None
        if not re.fullmatch(r"[0-9]{4,8}", code):
            raise ToolError("otp_invalid_code", "接收端返回的验证码格式异常")
        try:
            received_at = float(data.get("received_at") or 0)
        except (TypeError, ValueError):
            received_at = 0.0
        return OtpMessage(
            code=code,
            sender=mask_sender(str(data.get("sender") or "")),
            received_at=received_at,
            received_at_text=str(data.get("received_at_text") or ""),
        )

    def wait_for_new_code(
        self,
        *,
        since: float | None = None,
        timeout: float = 120.0,
        poll_interval: float = 1.0,
    ) -> OtpMessage | None:
        deadline = time.monotonic() + max(0.0, timeout)
        while True:
            item = self.latest()
            if item is not None and (since is None or item.received_at > since):
                return item
            if time.monotonic() >= deadline:
                return None
            time.sleep(max(0.2, poll_interval))


def build_otp_client(config: dict[str, Any]) -> OtpClient:
    token_path = Path(str(config.get("_otp_token_path") or ""))
    if not token_path.is_file():
        raise ToolError("otp_token_missing", "找不到 OTP token 文件，请先启动 officialapply otp-serve")
    token = token_path.read_text(encoding="utf-8").strip()
    if not token:
        raise ToolError("otp_token_missing", "OTP token 文件为空，请重启接收端")

    candidates: list[tuple[str, int]] = []
    runtime_path = Path(str(config.get("_otp_runtime_path") or ""))
    if str(runtime_path):
        runtime = read_runtime_endpoint(runtime_path)
        if runtime:
            candidates.append(runtime)
    otp_config = config.get("otp") or {}
    bind_ip = str(otp_config.get("bind_ip") or "").strip()
    configured_port = int(otp_config.get("port") or 8765)
    if bind_ip and bind_ip.lower() != "auto":
        candidates.append((bind_ip, configured_port))
    detected = detect_bind_address()
    if detected:
        candidates.append((detected[1], configured_port))
    candidates.append(("127.0.0.1", configured_port))

    seen: set[tuple[str, int]] = set()
    forbidden = False
    last_error: ToolError | None = None
    for host, port in candidates:
        if (host, port) in seen:
            continue
        seen.add((host, port))
        client = OtpClient(f"http://{host}:{port}", token)
        try:
            client.status()
            return client
        except ToolError as exc:
            if exc.code == "otp_forbidden":
                forbidden = True
            last_error = exc
    if forbidden:
        raise ToolError(
            "otp_token_mismatch",
            "正在运行的 OTP 接收端 token 与 token 文件不一致，请重启接收端",
        )
    raise ToolError(
        "otp_receiver_unreachable",
        "OTP 接收端未运行或不可达，请先执行 officialapply otp-serve",
    ) from last_error


def ensure_otp_client(config: dict[str, Any]) -> OtpClient:
    try:
        return build_otp_client(config)
    except ToolError as exc:
        if exc.code != "otp_receiver_unreachable" or not bool((config.get("otp") or {}).get("auto_start", True)):
            raise
    _spawn_receiver(config)
    deadline = time.monotonic() + 10.0
    last_error: ToolError | None = None
    while time.monotonic() < deadline:
        time.sleep(0.4)
        try:
            return build_otp_client(config)
        except ToolError as exc:
            last_error = exc
    raise ToolError(
        "otp_receiver_unreachable",
        "OTP 接收端启动失败或超时，请手动执行 officialapply otp-serve",
    ) from last_error


def _spawn_receiver(config: dict[str, Any]) -> None:
    data_dir = Path(str(config.get("_data_dir") or "."))
    data_dir.mkdir(parents=True, exist_ok=True)
    log_path = data_dir / "otp_receiver.log"
    log_path.touch(exist_ok=True)
    restrict_file_access(log_path)
    log_handle = open(log_path, "ab", buffering=0)
    command = [
        sys.executable,
        "-m",
        "officialapply.cli",
        "--config",
        str(config.get("_config_path") or "config.yaml"),
        "otp-serve",
    ]
    kwargs: dict[str, Any] = {}
    if os.name == "nt":
        kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP | subprocess.DETACHED_PROCESS
    else:
        kwargs["start_new_session"] = True
    try:
        subprocess.Popen(
            command,
            cwd=str(config.get("_base_dir") or "."),
            stdin=subprocess.DEVNULL,
            stdout=log_handle,
            stderr=log_handle,
            close_fds=True,
            **kwargs,
        )
    except OSError as exc:
        raise ToolError("otp_start_failed", "无法自动启动 OTP 接收端") from exc
    finally:
        log_handle.close()
