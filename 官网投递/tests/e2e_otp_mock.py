"""Local mock login page for OTP autofill E2E tests.

Reads the OTP receiver endpoint/token from data/, serves a login page on
127.0.0.1:8877, and sends a fake SMS through the receiver when the page's
send-code button is clicked. The fake code is never printed or logged.
"""

from __future__ import annotations

import json
import secrets
import threading
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
TOKEN = (ROOT / "data" / "otp_token.txt").read_text(encoding="utf-8").strip()
RUNTIME = json.loads((ROOT / "data" / "otp_runtime.json").read_text(encoding="utf-8"))
RECEIVER = f"http://{RUNTIME['host']}:{RUNTIME['port']}/otp/{TOKEN}"
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))
CODES: dict[str, str] = {}
LOCK = threading.Lock()

PAGE = """<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>Mock 官网登录</title></head>
<body>
<h1>账号登录</h1>
<form id="loginForm">
  <p><label for="phone">手机号</label>
  <input id="phone" name="phone" type="tel" maxlength="11" value="13800138000"></p>
  <p><label for="code">短信验证码</label>
  <input id="code" name="code" type="text" maxlength="6" autocomplete="one-time-code">
  <button type="button" id="sendBtn">获取验证码</button></p>
  <button type="submit" id="loginBtn">登录</button>
</form>
<p id="status"></p>
<script>
const statusEl = document.getElementById('status');
const sendBtn = document.getElementById('sendBtn');
sendBtn.addEventListener('click', async () => {
  sendBtn.disabled = true;
  statusEl.textContent = '正在发送...';
  try {
    const response = await fetch('/send-code');
    const data = await response.json();
    statusEl.textContent = data.ok ? '验证码已发送' : '发送失败';
  } catch (error) {
    statusEl.textContent = '发送失败';
  }
  setTimeout(() => { sendBtn.disabled = false; }, 3000);
});
document.getElementById('loginForm').addEventListener('submit', async (event) => {
  event.preventDefault();
  const response = await fetch('/login', {
    method: 'POST',
    headers: {'Content-Type': 'application/json'},
    body: JSON.stringify({
      phone: document.getElementById('phone').value,
      code: document.getElementById('code').value,
    }),
  });
  const data = await response.json();
  if (data.ok) {
    document.body.innerHTML = '<h1>登录成功</h1><button id="logout">退出登录</button>';
  } else {
    statusEl.textContent = data.message || '验证码错误';
  }
});
</script>
</body></html>
"""


SPLIT_PAGE = """<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>Mock 分格验证码登录</title></head>
<body>
<h1>账号登录</h1>
<form id="loginForm">
  <p><label for="phone">手机号</label>
  <input id="phone" name="phone" type="tel" maxlength="11" value="13800138000"></p>
  <p>短信验证码
  <span id="boxes">
    <input aria-label="验证码第1位" name="code1" type="text" maxlength="1">
    <input aria-label="验证码第2位" name="code2" type="text" maxlength="1">
    <input aria-label="验证码第3位" name="code3" type="text" maxlength="1">
    <input aria-label="验证码第4位" name="code4" type="text" maxlength="1">
    <input aria-label="验证码第5位" name="code5" type="text" maxlength="1">
    <input aria-label="验证码第6位" name="code6" type="text" maxlength="1">
  </span>
  <button type="button" id="sendBtn">获取验证码</button></p>
  <button type="submit" id="loginBtn">登录</button>
</form>
<p id="status"></p>
<script>
const statusEl = document.getElementById('status');
const sendBtn = document.getElementById('sendBtn');
const boxes = Array.from(document.querySelectorAll('#boxes input'));
boxes.forEach((box, index) => {
  box.addEventListener('input', () => {
    if (box.value && boxes[index + 1]) boxes[index + 1].focus();
  });
});
sendBtn.addEventListener('click', async () => {
  sendBtn.disabled = true;
  statusEl.textContent = '正在发送...';
  try {
    const response = await fetch('/send-code');
    const data = await response.json();
    statusEl.textContent = data.ok ? '验证码已发送' : '发送失败';
  } catch (error) {
    statusEl.textContent = '发送失败';
  }
  setTimeout(() => { sendBtn.disabled = false; }, 3000);
});
document.getElementById('loginForm').addEventListener('submit', async (event) => {
  event.preventDefault();
  const response = await fetch('/login', {
    method: 'POST',
    headers: {'Content-Type': 'application/json'},
    body: JSON.stringify({
      phone: document.getElementById('phone').value,
      code: boxes.map((box) => box.value).join(''),
    }),
  });
  const data = await response.json();
  if (data.ok) {
    document.body.innerHTML = '<h1>登录成功</h1><button id="logout">退出登录</button>';
  } else {
    statusEl.textContent = data.message || '验证码错误';
  }
});
</script>
</body></html>
"""


class Handler(BaseHTTPRequestHandler):
    server_version = "MockLogin/1.0"

    def log_message(self, fmt: str, *args) -> None:
        # Keep request lines clean; never log submitted codes.
        pass

    def do_GET(self) -> None:
        path = urlparse(self.path).path
        if path in ("/", "/login"):
            self._send(200, PAGE.encode("utf-8"), "text/html; charset=utf-8")
            return
        if path == "/login-split":
            self._send(200, SPLIT_PAGE.encode("utf-8"), "text/html; charset=utf-8")
            return
        if path == "/send-code":
            code = f"{secrets.randbelow(900000) + 100000}"
            with LOCK:
                CODES["latest"] = code
            if not self._send_fake_sms(code):
                self._json(502, {"ok": False, "message": "receiver rejected fake sms"})
                return
            self._json(200, {"ok": True})
            return
        self._json(404, {"ok": False, "message": "not_found"})

    def do_POST(self) -> None:
        path = urlparse(self.path).path
        if path != "/login":
            self._json(404, {"ok": False, "message": "not_found"})
            return
        try:
            length = int(self.headers.get("Content-Length") or 0)
            payload = json.loads(self.rfile.read(length).decode("utf-8"))
        except (ValueError, json.JSONDecodeError):
            self._json(400, {"ok": False, "message": "invalid_body"})
            return
        with LOCK:
            expected = CODES.get("latest")
        ok = bool(expected) and str(payload.get("code") or "") == expected
        self._json(200, {"ok": ok, "message": "" if ok else "验证码错误"})

    def _send_fake_sms(self, code: str) -> bool:
        body = json.dumps({
            "from": "13800138000",
            "content": f"【Mock】您的验证码是 {code}，5分钟内有效",
            "timestamp": int(time.time() * 1000),
        }).encode("utf-8")
        request = urllib.request.Request(
            RECEIVER,
            data=body,
            headers={"Content-Type": "application/json"},
        )
        try:
            with OPENER.open(request, timeout=5) as response:
                return bool(json.loads(response.read().decode("utf-8")).get("ok"))
        except OSError:
            return False

    def _send(self, status: int, body: bytes, content_type: str) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)
        self.close_connection = True

    def _json(self, status: int, payload: dict) -> None:
        self._send(status, json.dumps(payload, ensure_ascii=False).encode("utf-8"), "application/json; charset=utf-8")


def main() -> None:
    server = ThreadingHTTPServer(("127.0.0.1", 8877), Handler)
    print("mock login: http://127.0.0.1:8877/login", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
