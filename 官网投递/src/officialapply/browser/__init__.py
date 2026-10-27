"""Chrome facade used by the official-site application flow."""

from __future__ import annotations

import time
from pathlib import Path
from typing import Any

from officialapply.browser.cdp import CDPError, CDPTab, ChromeConnection
from officialapply.browser.chrome import BrowserError, ensure_chrome, launch_chrome
from officialapply.browser.session import SessionStore

__all__ = [
    "Browser",
    "BrowserError",
    "CDPError",
    "CDPTab",
    "ChromeConnection",
    "SessionStore",
    "ensure_chrome",
    "launch_chrome",
]


class Browser:
    def __init__(self, config: dict[str, Any]) -> None:
        self.config = config
        self.connection: ChromeConnection | None = None
        self.session = SessionStore(config["_session_path"])

    @property
    def chrome(self) -> ChromeConnection:
        if self.connection is None:
            self.connection = ensure_chrome(self.config)
        return self.connection

    def open(
        self,
        url: str = "",
        *,
        new: bool = False,
        front: bool = False,
        wait: bool = True,
    ) -> CDPTab:
        connection = self.chrome
        tab: CDPTab | None = None
        targets = connection.list_targets()
        if not new:
            stored = self.session.load()
            target_id = str(stored.get("target_id") or "")
            if target_id and any(str(item.get("id")) == target_id for item in targets):
                tab = connection.tab(target_id)
            elif len(targets) == 1:
                tab = connection.tab(str(targets[0]["id"]))
            elif targets:
                regular = [
                    item for item in targets
                    if str(item.get("url") or "").startswith(("http://", "https://"))
                ]
                chosen = regular[0] if regular else targets[0]
                tab = connection.tab(str(chosen["id"]))

        if tab is None:
            target_id = connection.new_tab(url or "about:blank", background=not front)
            tab = connection.tab(target_id)
            if url and wait:
                tab.wait_for_load(timeout=int(self.config["apply"]["page_load_timeout"]))
        elif url:
            current = tab.current_url()
            if not _same_url(current, url):
                if not tab.navigate(url, wait=wait, timeout=int(self.config["apply"]["page_load_timeout"])):
                    raise BrowserError(
                        "navigation_failed",
                        "页面跳转失败，请检查链接或网络",
                        requested=url,
                        current=current,
                    )

        _prepare_tab(tab)
        if front:
            tab.bring_to_front()
        self.session.save(tab.target_id, tab.current_url() or url)
        return tab

    def current(self, url: str = "", *, front: bool = False) -> CDPTab:
        return self.open(url, new=False, front=front)

    def save_url(self, tab: CDPTab) -> None:
        self.session.save(tab.target_id, tab.current_url())

    def close_current(self) -> bool:
        stored = self.session.load()
        target_id = str(stored.get("target_id") or "")
        ok = self.chrome.close_tab(target_id) if target_id else False
        self.session.clear()
        return ok

    def status(self) -> dict[str, Any]:
        version = self.chrome.version() if self.connection is not None or _port_alive(self.config) else None
        stored = self.session.load()
        targets = self.chrome.list_targets() if version else []
        current_id = str(stored.get("target_id") or "")
        current = next((item for item in targets if str(item.get("id")) == current_id), None)
        return {
            "connected": bool(version),
            "port": int(self.config["browser"]["debug_port"]),
            "browser": (version or {}).get("Browser") if version else "",
            "profile_dir": str(Path(self.config["browser"]["user_data_dir"])),
            "session": {
                "target_id": current_id,
                "url": (current or {}).get("url") or stored.get("url") or "",
                "alive": bool(current),
            },
            "tabs": len(targets),
        }


def _prepare_tab(tab: CDPTab) -> None:
    # Hidden tabs get timer throttling and treated as unfocused; both break
    # lazy-loaded form sections and select popups.
    try:
        tab.command("Emulation.setFocusEmulationEnabled", {"enabled": True}, timeout=5)
        tab.command("Page.setWebLifecycleState", {"state": "active"}, timeout=5)
    except CDPError:
        pass


def _same_url(current: str, target: str) -> bool:
    if not current or not target:
        return False
    return current.split("#", 1)[0].rstrip("/") == target.split("#", 1)[0].rstrip("/")


def _port_alive(config: dict[str, Any]) -> bool:
    connection = ChromeConnection(int(config["browser"]["debug_port"]))
    return connection.version() is not None
