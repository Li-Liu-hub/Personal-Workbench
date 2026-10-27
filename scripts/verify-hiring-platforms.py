"""Exercise the native MCP against intercepted local HTML in fresh Chrome profiles.

Requires websocket-client. No real application is submitted: every page request in
the three test browsers is fulfilled locally through CDP before reaching a site.
"""
import argparse
import base64
from contextlib import closing
import json
from pathlib import Path
import queue
import socket
import sqlite3
import subprocess
import tempfile
import threading
import time
import urllib.request

import websocket

PLATFORMS = {
    "51job": (19551, "https://jobs.51job.com/shanghai/12345678.html"),
    "zhaopin": (19552, "https://jobs.zhaopin.com/CC123456789J00000000001.htm"),
    "shixiseng": (19553, "https://www.shixiseng.com/intern/inn_fixture"),
}


def get_json(url):
    with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(url, timeout=2) as r:
        return json.load(r)


class Browser:
    def __init__(self, chrome, root, platform):
        self.platform = platform
        port, self.url = PLATFORMS[platform]
        with socket.socket() as s:
            if s.connect_ex(("127.0.0.1", port)) == 0:
                raise RuntimeError(f"Test port {port} is already used; no browser was touched")
        self.process = subprocess.Popen([
            chrome, "--headless=new", "--disable-gpu", "--disable-background-networking",
            "--no-first-run", "--no-default-browser-check", "--remote-debugging-address=127.0.0.1",
            f"--remote-debugging-port={port}", f"--user-data-dir={root / platform}", "about:blank",
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            try:
                version = get_json(f"http://127.0.0.1:{port}/json/version")
                break
            except (OSError, ValueError):
                time.sleep(.1)
        else:
            raise RuntimeError("Test Chrome did not start")
        self.ws = websocket.create_connection(version["webSocketDebuggerUrl"], suppress_origin=True)
        self.ws.settimeout(1)
        self.lock = threading.Lock()
        self.pending = {}
        self.counter = 0
        self.closed = False
        self.mode = "normal"
        self.identity = platform
        self.requests = 0
        self.sessions = []
        self.thread = threading.Thread(target=self.listen, daemon=True)
        self.thread.start()
        self.call("Target.setAutoAttach", {"autoAttach": True, "waitForDebuggerOnStart": True,
                                         "flatten": True, "filter": [{"type": "page"}, {"exclude": True}]})
        hiring = root / "data" / "hiring"
        hiring.mkdir(parents=True, exist_ok=True)
        (hiring / f"{platform}-browser-session.json").write_text(json.dumps(version["webSocketDebuggerUrl"]), encoding="utf-8")
        targets=self.call("Target.getTargets",{})["targetInfos"]
        blank=next(t for t in targets if t["type"]=="page" and t["url"]=="about:blank")
        (hiring / f"{platform}-tab.json").write_text(json.dumps(blank["targetId"]),encoding="utf-8")
        for _ in range(100):
            if self.sessions:
                break
            time.sleep(.05)
        self.call("Fetch.enable",{"patterns":[{"urlPattern":"*"}]},self.sessions[-1])

    def send(self, method, params, session=None, reply=None):
        with self.lock:
            self.counter += 1
            n = self.counter
            if reply is not None:
                self.pending[n] = reply
            msg = {"id": n, "method": method, "params": params}
            if session:
                msg["sessionId"] = session
            self.ws.send(json.dumps(msg))
            return n

    def call(self, method, params, session=None):
        reply = queue.Queue()
        self.send(method, params, session, reply)
        result = reply.get(timeout=15)
        if "error" in result:
            raise RuntimeError(result["error"])
        return result.get("result", {})

    def html(self):
        title = f"Java开发{self.identity}"
        script = "this.textContent='已投递';window.sentCount=(window.sentCount||0)+1"
        if self.mode == "ambiguous":
            script = "this.textContent='处理中';window.sentCount=(window.sentCount||0)+1"
        block = '<div role="dialog">请完成安全验证，拖动滑块</div>' if self.mode == "captcha" else ""
        if self.mode == "edgeone":
            block = '<h1>www.zhaopin.com</h1><p>正在验证连接安全性，请勾选下方复选框。</p>'
        account = '<a href="/profile">我的简历</a>' if self.mode != "logged_out" else '<div class="login-btn">登录/注册</div><a href="/resume">简历中心</a><div class="company-avatar">公司头像</div>'
        card = f'<div class="job-item"><a class="intern-name jobinfo__name jname" href="{self.url}">{title}</a><span class="company-name">测试科技{self.identity}</span><span class="salary">200/天</span></div>'
        if self.platform == "51job":
            data = json.dumps({"jobId":"12345678","jobYear":"在校生","jobDegree":"本科"}).replace('"','&quot;')
            card = f'<div class="joblist-item"><div class="joblist-item-job" sensorsdata="{data}"><span class="jname">{title}</span><span class="area">上海</span></div><a href="https://jobs.51job.com/all/co123.html"><span class="cname">测试科技{self.identity}</span></a></div>'
        buttons = f'<button onclick="{script}">投递简历</button>'
        if self.platform == "shixiseng":
            buttons = f'<div class="btn_warp"><div class="resume_apply com_res" onclick="{script}">投个简历</div></div><div class="resume_content"><div class="resume_apply com_res" onclick="{script}">投个简历</div></div>'
        return f'''<!doctype html><meta charset="utf-8"><title>本地招聘验证</title>
        {account}{block}{card}
        <h1>{title}</h1><div class="company-name">测试科技{self.identity}</div><div class="job-description">负责Java后端开发与自动化测试</div>
        <label>选择简历<select name="resume"><option value="a">简历A</option><option value="b">简历B</option></select></label>
        {buttons}'''

    def listen(self):
        while not self.closed:
            try:
                msg = json.loads(self.ws.recv())
                if "id" in msg:
                    with self.lock:
                        reply = self.pending.pop(msg["id"], None)
                    if reply is not None:
                        reply.put(msg)
                elif msg.get("method") == "Target.attachedToTarget":
                    sid = msg["params"]["sessionId"]
                    self.sessions.append(sid)
                    self.send("Fetch.enable", {"patterns": [{"urlPattern": "*"}]}, sid)
                    self.send("Runtime.runIfWaitingForDebugger", {}, sid)
                elif msg.get("method") == "Fetch.requestPaused":
                    self.requests += 1
                    self.send("Fetch.fulfillRequest", {"requestId": msg["params"]["requestId"], "responseCode": 200,
                        "responseHeaders": [{"name": "Content-Type", "value": "text/html; charset=utf-8"}],
                        "body": base64.b64encode(self.html().encode()).decode()}, msg["sessionId"])
            except websocket.WebSocketTimeoutException:
                continue
            except (OSError, ValueError, websocket.WebSocketException):
                if not self.closed:
                    self.closed = True

    def eval(self, expression):
        return self.call("Runtime.evaluate", {"expression": expression, "returnByValue": True}, self.sessions[-1])["result"].get("value")

    def close(self):
        try:
            self.send("Browser.close", {})
        except (OSError, queue.Empty, websocket.WebSocketException):
            pass
        self.closed = True
        self.ws.close()
        self.thread.join(timeout=2)
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()


class MCP:
    def __init__(self, exe, data):
        self.process = subprocess.Popen([exe, "--mcp", "--data-dir", str(data)], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8")
        self.n = 0
        self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "fixture", "version": "1"}})
        self.process.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
        self.process.stdin.flush()

    def request(self, method, params):
        self.n += 1
        self.process.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.n, "method": method, "params": params}) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError(self.process.stderr.read())
        result = json.loads(line)
        if "error" in result:
            raise RuntimeError(result["error"])
        return result["result"]

    def call(self, name, args, error=False):
        result = self.request("tools/call", {"name": name, "arguments": args})
        assert bool(result.get("isError")) == error, (name, result)
        value=result.get("structuredContent", result.get("content"))
        return value["items"] if isinstance(value,dict) and set(value)=={"items"} else value

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=15)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--executable", required=True)
    parser.add_argument("--browser", default=r"C:\Program Files\Google\Chrome\Application\chrome.exe")
    args = parser.parse_args()
    browsers = []
    mcp = None
    with tempfile.TemporaryDirectory(prefix="personaldesk-platform-fixtures-") as tmp:
        root = Path(tmp)
        try:
            for p in PLATFORMS:
                browsers.append(Browser(args.browser, root, p))
            mcp = MCP(args.executable, root / "data")
            mcp.call("hiring_settings_update", {"settings": {"send_windows": ["00:00-23:59"]}})
            for browser in browsers:
                p = browser.platform
                print(f"Checking intercepted {p}", flush=True)
                jobs = mcp.call("platform_search_jobs", {"platform": p, "keyword": "Java", "city": "上海"})
                assert len(jobs) == 1, jobs
                detail = mcp.call("platform_job_detail", {"platform": p, "url": browser.url})
                assert detail["company"] == f"测试科技{p}" and detail["jd"]
                apply = {"platform": p, "url": browser.url}
                mcp.call("platform_queue_add", {**apply, "title": detail["title"], "company": detail["company"], "jd": detail["jd"]})
                mcp.call("platform_apply", {**apply, "dry_run": False}, error=True)
                browser.mode = "logged_out"
                mcp.call("platform_job_detail", {"platform": p, "url": browser.url})
                mcp.call("platform_apply", apply, error=True)
                browser.mode = "normal"
                mcp.call("platform_job_detail", {"platform": p, "url": browser.url})
                mcp.call("platform_apply", apply)
                assert browser.eval("window.sentCount||0") == 0
                browser.eval("document.querySelector('select').value='b'")
                mcp.call("platform_apply", {**apply, "dry_run": False, "confirmed": True}, error=True)
                mcp.call("platform_apply", apply)
                mcp.call("platform_apply", {**apply, "dry_run": False, "confirmed": True})
                assert browser.eval("window.sentCount") == 1
                mcp.call("platform_apply", {**apply, "dry_run": False, "confirmed": True}, error=True)
                with closing(sqlite3.connect(root / "data/hiring/hiring.sqlite")) as db:
                    db.execute("UPDATE attempts SET at=at-31")
                    db.commit()
            with closing(sqlite3.connect(root / "data/hiring/hiring.sqlite")) as db:
                assert db.execute("SELECT COUNT(*) FROM queue").fetchone()[0] == 3
                assert db.execute("SELECT COUNT(*) FROM attempts WHERE status='sent'").fetchone()[0] == 3
                assert db.execute("SELECT COUNT(*) FROM sqlite_master WHERE name='platform_queue'").fetchone()[0] == 0
            # Same job on a different portal is blocked even if its URL is unrelated.
            other = browsers[1]
            other.identity = "51job"
            mcp.call("platform_job_detail", {"platform": other.platform, "url": other.url})
            mcp.call("platform_apply", {"platform": other.platform, "url": other.url}, error=True)
            # Website verification must fail before any reservation or click.
            other.mode = "captcha"
            mcp.call("platform_search_jobs", {"platform": other.platform, "keyword": "Java", "city": "上海"}, error=True)
            other.mode = "edgeone"
            mcp.call("platform_search_jobs", {"platform": other.platform, "keyword": "Java", "city": "上海"}, error=True)
            assert all(b.requests > 0 for b in browsers)
            print(json.dumps({"passed": True, "platforms": list(PLATFORMS), "confirmed_fixture_sends": 3,
                "checks": ["search", "detail", "single_shared_queue", "confirmation", "changed_resume", "duplicate", "cross_platform_duplicate", "captcha", "51job_clickable_cards", "shixiseng_repeated_apply", "login_required", "edgeone_verification"],
                "live_website_submissions": 0}, ensure_ascii=False))
        except Exception:
            for browser in browsers:
                try:
                    print(json.dumps({"platform":browser.platform,"requests":browser.requests,"page":browser.eval("({url:location.href,body:document.body?.innerText,links:[...document.querySelectorAll('a')].map(a=>a.href)})")},ensure_ascii=False),flush=True)
                except Exception:
                    pass
            raise
        finally:
            if mcp:
                mcp.close()
            for browser in reversed(browsers):
                browser.close()


if __name__ == "__main__":
    main()
