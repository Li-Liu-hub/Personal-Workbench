"""Minimal Chrome DevTools Protocol client (no Node runtime, no Playwright)."""

from __future__ import annotations

import base64
import json
import random
import time
from collections import deque
from pathlib import Path
from typing import Any

import httpx
import websocket


class CDPError(RuntimeError):
    def __init__(self, message: str, *, code: int | None = None) -> None:
        super().__init__(message)
        self.code = code


class ChromeConnection:
    """Talks to Chrome's HTTP discovery endpoint and browser-level CDP socket."""

    def __init__(self, port: int = 9333, host: str = "127.0.0.1", timeout: float = 10) -> None:
        self.host = host
        self.port = int(port)
        self.timeout = timeout
        self.http_base = f"http://{host}:{self.port}"
        self._client = httpx.Client(trust_env=False, timeout=timeout)

    def version(self) -> dict[str, Any] | None:
        try:
            response = self._client.get(f"{self.http_base}/json/version")
            if response.status_code != 200:
                return None
            data = response.json()
            return data if isinstance(data, dict) and data.get("webSocketDebuggerUrl") else None
        except (httpx.HTTPError, ValueError):
            return None

    def list_targets(self) -> list[dict[str, Any]]:
        try:
            response = self._client.get(f"{self.http_base}/json/list")
            if response.status_code != 200:
                return []
            data = response.json()
        except (httpx.HTTPError, ValueError):
            return []
        return [
            item for item in data
            if isinstance(item, dict) and item.get("type") == "page" and item.get("id")
        ] if isinstance(data, list) else []

    def new_tab(self, url: str = "about:blank", *, background: bool = True) -> str:
        version = self.version()
        if not version:
            raise CDPError("Chrome 调试端口不可用")
        ws_url = str(version["webSocketDebuggerUrl"])
        try:
            target_id = self._browser_command(
                ws_url, "Target.createTarget", {"url": url, "background": background}
            ).get("targetId")
        except CDPError:
            target_id = None
        if not target_id:
            try:
                response = self._client.put(f"{self.http_base}/json/new?{httpx.QueryParams({'url': url})}")
                data = response.json() if response.status_code == 200 else {}
                target_id = data.get("id")
            except (httpx.HTTPError, ValueError) as exc:
                raise CDPError(f"无法打开新标签页: {exc}") from exc
        if not target_id:
            raise CDPError("无法打开新标签页")
        return str(target_id)

    def close_tab(self, target_id: str) -> bool:
        version = self.version()
        if version:
            try:
                self._browser_command(
                    str(version["webSocketDebuggerUrl"]),
                    "Target.closeTarget",
                    {"targetId": target_id},
                )
                return True
            except CDPError:
                pass
        try:
            response = self._client.get(f"{self.http_base}/json/close/{target_id}")
            return response.status_code == 200
        except httpx.HTTPError:
            return False

    def tab(self, target_id: str) -> "CDPTab":
        return CDPTab(self.host, self.port, target_id)

    def find_target(self, url_part: str) -> dict[str, Any] | None:
        for target in self.list_targets():
            if url_part in str(target.get("url") or ""):
                return target
        return None

    def _browser_command(self, ws_url: str, method: str, params: dict[str, Any]) -> dict[str, Any]:
        try:
            connection = websocket.create_connection(
                ws_url,
                timeout=self.timeout,
                suppress_origin=True,
            )
        except Exception as exc:
            raise CDPError(f"无法连接 Chrome: {exc}") from exc
        try:
            connection.send(json.dumps({"id": 1, "method": method, "params": params}))
            deadline = time.time() + self.timeout
            while time.time() < deadline:
                connection.settimeout(max(0.1, deadline - time.time()))
                try:
                    message = json.loads(connection.recv())
                except Exception as exc:
                    raise CDPError(f"Chrome 无响应: {exc}") from exc
                if message.get("id") != 1:
                    continue
                if message.get("error"):
                    error = message["error"]
                    raise CDPError(str(error.get("message") or "CDP 命令失败"), code=error.get("code"))
                return message.get("result") or {}
        finally:
            try:
                connection.close()
            except Exception:
                pass
        raise CDPError("Chrome 命令超时")


class CDPTab:
    """One page target. The WebSocket is opened lazily and re-opened after navigation drops."""

    def __init__(self, host: str, port: int, target_id: str, timeout: float = 30) -> None:
        self.host = host
        self.port = int(port)
        self.target_id = target_id
        self.timeout = timeout
        self.ws_url = f"ws://{host}:{self.port}/devtools/page/{target_id}"
        self._ws: websocket.WebSocket | None = None
        self._command_id = 0
        self._events: deque[dict[str, Any]] = deque(maxlen=200)
        self._closed = False

    def connect(self) -> None:
        if self._ws is not None:
            return
        try:
            self._ws = websocket.create_connection(
                self.ws_url,
                timeout=self.timeout,
                suppress_origin=True,
            )
        except Exception as exc:
            raise CDPError(f"无法连接标签页 {self.target_id}: {exc}") from exc

    def close(self) -> None:
        self._closed = True
        if self._ws is not None:
            try:
                self._ws.close()
            except Exception:
                pass
            self._ws = None

    def command(self, method: str, params: dict[str, Any] | None = None, *, timeout: float | None = None) -> dict[str, Any]:
        if self._closed:
            raise CDPError("标签页已关闭")
        self.connect()
        assert self._ws is not None
        self._command_id += 1
        command_id = self._command_id
        payload = {"id": command_id, "method": method, "params": params or {}}
        try:
            self._ws.send(json.dumps(payload))
        except Exception as exc:
            self._reset()
            raise CDPError(f"发送 CDP 命令失败: {exc}") from exc
        deadline = time.time() + (timeout if timeout is not None else self.timeout)
        while time.time() < deadline:
            self._ws.settimeout(max(0.1, deadline - time.time()))
            try:
                raw = self._ws.recv()
            except websocket.WebSocketTimeoutException as exc:
                raise CDPError(f"CDP 命令超时: {method}") from exc
            except Exception as exc:
                self._reset()
                raise CDPError(f"CDP 连接中断: {exc}") from exc
            if not raw:
                continue
            try:
                message = json.loads(raw)
            except (TypeError, ValueError):
                continue
            if message.get("id") == command_id:
                if message.get("error"):
                    error = message["error"]
                    raise CDPError(
                        str(error.get("message") or f"CDP 命令失败: {method}"),
                        code=error.get("code"),
                    )
                return message.get("result") or {}
            if message.get("method"):
                self._events.append(message)
        raise CDPError(f"CDP 命令超时: {method}")

    def evaluate(
        self,
        expression: str,
        *,
        await_promise: bool = False,
        return_by_value: bool = True,
        timeout: float | None = None,
    ) -> Any:
        try:
            result = self.command(
                "Runtime.evaluate",
                {
                    "expression": expression,
                    "returnByValue": return_by_value,
                    "awaitPromise": await_promise,
                    "userGesture": True,
                },
                timeout=timeout,
            )
        except CDPError as exc:
            # During a cross-origin navigation the previous execution context
            # disappears; callers that poll get another chance.
            if "context" in str(exc).lower() or "navigat" in str(exc).lower():
                return None
            raise
        if result.get("exceptionDetails"):
            details = result["exceptionDetails"]
            description = (
                details.get("exception", {}).get("description")
                if isinstance(details.get("exception"), dict)
                else None
            )
            raise CDPError(str(description or details.get("text") or "页面脚本执行失败"))
        value = result.get("result") or {}
        if value.get("subtype") == "error":
            raise CDPError(str(value.get("description") or "页面脚本执行失败"))
        return value.get("value")

    def evaluate_json(self, expression: str, **kwargs: Any) -> Any:
        raw = self.evaluate(expression, **kwargs)
        if isinstance(raw, str):
            try:
                return json.loads(raw)
            except json.JSONDecodeError:
                return raw
        return raw

    def navigate(self, url: str, *, wait: bool = True, timeout: float = 20) -> bool:
        try:
            result = self.command("Page.navigate", {"url": url}, timeout=timeout)
        except CDPError:
            return False
        if result.get("errorText"):
            return False
        if wait:
            return self.wait_for_load(timeout=timeout)
        return True

    def wait_for_load(self, timeout: float = 20) -> bool:
        deadline = time.time() + timeout
        stable = 0
        while time.time() < deadline:
            try:
                state = self.evaluate(
                    "({ready: document.readyState, href: location.href, "
                    "body: !!document.body})"
                )
            except CDPError:
                state = None
            if isinstance(state, dict) and state.get("ready") == "complete" and state.get("body"):
                stable += 1
                if stable >= 2:
                    return True
            else:
                stable = 0
            time.sleep(0.25)
        return False

    def current_url(self) -> str:
        try:
            value = self.evaluate("location.href")
            return str(value or "")
        except CDPError:
            return ""

    def click(self, selector: str, *, index: int = 0) -> bool:
        selector_json = json.dumps(selector)
        prepared = self.evaluate_json(
            f"""
            (() => {{
                const nodes = document.querySelectorAll({selector_json});
                const el = nodes[{max(0, index)}];
                if (!el) return JSON.stringify({{ok: false, error: 'not_found'}});
                const style = getComputedStyle(el);
                if (style.display === 'none' || style.visibility === 'hidden') {{
                    return JSON.stringify({{ok: false, error: 'not_visible'}});
                }}
                el.scrollIntoView({{block: 'center', inline: 'center'}});
                return JSON.stringify({{ok: true}});
            }})()
            """
        )
        if not isinstance(prepared, dict) or not prepared.get("ok"):
            return False
        time.sleep(0.15)
        rect = self.evaluate_json(
            f"""
            (() => {{
                const el = document.querySelectorAll({selector_json})[{max(0, index)}];
                if (!el) return JSON.stringify({{ok: false}});
                const r = el.getBoundingClientRect();
                const x = r.left + r.width / 2;
                const y = r.top + r.height / 2;
                const top = document.elementFromPoint(x, y);
                const hit = !!top && (top === el || el.contains(top) || top.contains(el));
                return JSON.stringify({{ok: true, x, y, hit}});
            }})()
            """
        )
        if not isinstance(rect, dict) or not rect.get("ok"):
            return False
        return self.click_at(float(rect["x"]), float(rect["y"]))

    def click_at(self, x: float, y: float) -> bool:
        try:
            for method, extra in (
                ("mouseMoved", {}),
                ("mousePressed", {"button": "left", "buttons": 1, "clickCount": 1}),
                ("mouseReleased", {"button": "left", "buttons": 0, "clickCount": 1}),
            ):
                params = {"type": method, "x": x, "y": y, **extra}
                self.command("Input.dispatchMouseEvent", params)
            return True
        except CDPError:
            return False

    def click_js(self, selector: str, *, index: int = 0) -> bool:
        selector_json = json.dumps(selector)
        result = self.evaluate_json(
            f"""
            (() => {{
                const el = document.querySelectorAll({selector_json})[{max(0, index)}];
                if (!el) return JSON.stringify({{ok: false}});
                el.click();
                return JSON.stringify({{ok: true}});
            }})()
            """
        )
        return bool(isinstance(result, dict) and result.get("ok"))

    def press_key(self, name: str) -> bool:
        keys = {
            "SelectAll": (65, "a", "KeyA", 2, ["selectAll"]),
            "Backspace": (8, "Backspace", "Backspace", 0, None),
            "Enter": (13, "Enter", "Enter", 0, None),
            "Tab": (9, "Tab", "Tab", 0, None),
            "Escape": (27, "Escape", "Escape", 0, None),
            "ArrowDown": (40, "ArrowDown", "ArrowDown", 0, None),
        }
        if name not in keys:
            return False
        code, key, dom_code, modifiers, commands = keys[name]
        base = {
            "key": key,
            "code": dom_code,
            "windowsVirtualKeyCode": code,
            "nativeVirtualKeyCode": code,
            "modifiers": modifiers,
        }
        try:
            down = {"type": "rawKeyDown", **base}
            if commands:
                down["commands"] = commands
            self.command("Input.dispatchKeyEvent", down)
            self.command("Input.dispatchKeyEvent", {"type": "keyUp", **base})
            return True
        except CDPError:
            return False

    def type_text(self, text: str, *, human: bool = True) -> bool:
        if not text:
            return True
        try:
            for character in text:
                self.command("Input.insertText", {"text": character})
                if human:
                    time.sleep(random.SystemRandom().uniform(0.025, 0.07))
            return True
        except CDPError:
            return False

    def set_files(self, selector: str, paths: list[str]) -> bool:
        files = [str(Path(path).resolve()) for path in paths]
        if not files:
            return False
        try:
            self.command("DOM.enable")
            document = self.command("DOM.getDocument", {"depth": 0})
            node_id = self.command(
                "DOM.querySelector",
                {"nodeId": document.get("root", {}).get("nodeId"), "selector": selector},
            ).get("nodeId")
            if not node_id:
                return False
            self.command("DOM.setFileInputFiles", {"files": files, "nodeId": node_id})
            return True
        except CDPError:
            return False

    def click_and_choose_files(self, selector: str, paths: list[str], timeout: float = 4) -> bool:
        files = [str(Path(path).resolve()) for path in paths]
        if not files:
            return False
        try:
            self.command("Page.enable")
            self.command("Page.setInterceptFileChooserDialog", {"enabled": True})
            if not self.click(selector):
                return False
            event = self.wait_event("Page.fileChooserOpened", timeout=timeout)
            if not event:
                return False
            backend_node_id = event.get("backendNodeId")
            if not backend_node_id:
                return False
            self.command(
                "DOM.setFileInputFiles",
                {"files": files, "backendNodeId": backend_node_id},
            )
            return True
        except CDPError:
            return False
        finally:
            try:
                self.command("Page.setInterceptFileChooserDialog", {"enabled": False})
            except CDPError:
                pass

    def wait_event(self, method: str, timeout: float = 5) -> dict[str, Any] | None:
        for queued in list(self._events):
            if queued.get("method") == method:
                self._events.remove(queued)
                return queued.get("params") or {}
        self.connect()
        assert self._ws is not None
        deadline = time.time() + timeout
        while time.time() < deadline:
            self._ws.settimeout(max(0.1, deadline - time.time()))
            try:
                raw = self._ws.recv()
            except websocket.WebSocketTimeoutException:
                return None
            except Exception:
                self._reset()
                return None
            if not raw:
                continue
            try:
                message = json.loads(raw)
            except (TypeError, ValueError):
                continue
            if message.get("method") == method:
                return message.get("params") or {}
            if message.get("method"):
                self._events.append(message)
        return None

    def screenshot(self, path: str | Path) -> bool:
        try:
            result = self.command("Page.captureScreenshot", {"format": "png"}, timeout=15)
            data = result.get("data")
            if not data:
                return False
            target = Path(path)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(base64.b64decode(data))
            return True
        except (CDPError, OSError, ValueError):
            return False

    def bring_to_front(self) -> bool:
        try:
            self.command("Page.bringToFront", timeout=5)
            return True
        except CDPError:
            return False

    def _reset(self) -> None:
        if self._ws is not None:
            try:
                self._ws.close()
            except Exception:
                pass
            self._ws = None
