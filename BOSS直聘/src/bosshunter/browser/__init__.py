"""Chrome automation facade used by BOSS search and delivery."""

from __future__ import annotations

from typing import Any

from bosshunter.browser.cdp import CDPError, CDPTab, ChromeConnection
from bosshunter.browser.chrome import BrowserError, ensure_chrome, launch_chrome, open_login_page

__all__ = [
    "Browser",
    "BrowserError",
    "CDPError",
    "CDPTab",
    "ChromeConnection",
    "ensure_chrome",
    "launch_chrome",
    "open_login_page",
]


class Browser:
    def __init__(self, config: dict[str, Any]) -> None:
        self.config = config
        self.connection: ChromeConnection | None = None

    @property
    def chrome(self) -> ChromeConnection:
        if self.connection is None:
            self.connection = ensure_chrome(self.config)
        return self.connection

    def open(self, url: str, *, background: bool = True) -> CDPTab:
        target_id = self.chrome.new_tab(url, background=background)
        tab = self.chrome.tab(target_id)
        # Hidden tabs get timer throttling and treated as unfocused; both break
        # lazy-loaded lists and chat actions that require DOM events.
        try:
            tab.command("Emulation.setFocusEmulationEnabled", {"enabled": True}, timeout=5)
            tab.command("Page.setWebLifecycleState", {"state": "active"}, timeout=5)
        except CDPError:
            pass
        return tab

    def close(self, tab: CDPTab | None) -> None:
        if tab is None:
            return
        self.chrome.close_tab(tab.target_id)
        tab.close()
