"""Find, launch and connect to a dedicated Chrome debug instance."""

from __future__ import annotations

import os
import platform
import shutil
import subprocess
import time
from pathlib import Path
from typing import Any

from bosshunter.browser.cdp import CDPError, ChromeConnection
from bosshunter.config import resolve_path


class BrowserError(RuntimeError):
    def __init__(self, code: str, message: str, **details: Any) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.details = details


def ensure_chrome(config: dict[str, Any]) -> ChromeConnection:
    browser = config["browser"]
    connection = ChromeConnection(int(browser["debug_port"]))
    version = connection.version()
    if version:
        return connection
    if not browser.get("auto_launch", True):
        raise BrowserError(
            "chrome_unavailable",
            f"Chrome 调试端口 {browser['debug_port']} 不可用，请先启动 Chrome 或开启 browser.auto_launch",
        )
    launch_chrome(config)
    deadline = time.time() + int(browser.get("launch_timeout_seconds", 25))
    while time.time() < deadline:
        if connection.version():
            return connection
        time.sleep(0.4)
    raise BrowserError(
        "chrome_launch_failed",
        "Chrome 已启动但调试端口未就绪；请检查端口占用或手动启动",
        port=int(browser["debug_port"]),
    )


def launch_chrome(config: dict[str, Any]) -> subprocess.Popen:
    browser = config["browser"]
    executable = find_chrome_path(config)
    if not executable:
        raise BrowserError(
            "chrome_not_found",
            "未找到 Chrome，请在 browser.chrome_path 中填写 chrome.exe 的完整路径",
        )
    user_data_dir = resolve_path(str(browser["user_data_dir"]), config)
    user_data_dir.mkdir(parents=True, exist_ok=True)
    args = [
        executable,
        f"--remote-debugging-port={int(browser['debug_port'])}",
        f"--user-data-dir={user_data_dir}",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-blink-features=AutomationControlled",
        "--disable-session-crashed-bubble",
        "--hide-crash-restore-bubble",
        "--disable-background-timer-throttling",
        "--disable-backgrounding-occluded-windows",
        "--disable-renderer-backgrounding",
    ]
    if browser.get("headless"):
        args.append("--headless=new")
    args.append("about:blank")

    creationflags = 0
    start_new_session = False
    if platform.system() == "Windows":
        creationflags = subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP
    else:
        start_new_session = True
    try:
        return subprocess.Popen(
            args,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            close_fds=True,
            creationflags=creationflags,
            start_new_session=start_new_session,
        )
    except OSError as exc:
        raise BrowserError("chrome_launch_failed", f"启动 Chrome 失败: {exc}") from exc


def find_chrome_path(config: dict[str, Any]) -> str | None:
    configured = str(config["browser"].get("chrome_path") or "").strip()
    if configured:
        resolved = resolve_path(configured, config)
        return str(resolved) if resolved.exists() else configured
    env_path = str(os.environ.get("BOSSHUNTER_CHROME") or "").strip()
    if env_path and Path(env_path).exists():
        return env_path
    system = platform.system()
    candidates: list[str] = []
    if system == "Windows":
        for env_name in ("PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"):
            base = os.environ.get(env_name)
            if base:
                candidates.append(str(Path(base) / "Google" / "Chrome" / "Application" / "chrome.exe"))
                candidates.append(str(Path(base) / "Microsoft" / "Edge" / "Application" / "msedge.exe"))
    elif system == "Darwin":
        candidates.extend([
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ])
    else:
        for name in ("google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "microsoft-edge"):
            found = shutil.which(name)
            if found:
                candidates.append(found)
    for candidate in candidates:
        if candidate and Path(candidate).exists():
            return candidate
    for name in ("chrome", "google-chrome", "chromium", "msedge"):
        found = shutil.which(name)
        if found:
            return found
    return None


def open_login_page(config: dict[str, Any]) -> dict[str, Any]:
    connection = ensure_chrome(config)
    try:
        target_id = connection.new_tab("https://www.zhipin.com/web/user/", background=False)
    except CDPError as exc:
        raise BrowserError("open_page_failed", f"打开登录页失败: {exc}") from exc
    tab = connection.tab(target_id)
    try:
        tab.bring_to_front()
    finally:
        tab.close()
    return {
        "target_id": target_id,
        "url": "https://www.zhipin.com/web/user/",
        "message": "请在弹出的 Chrome 窗口中扫码或短信登录；登录状态会保存在专用浏览器配置目录中。",
    }
