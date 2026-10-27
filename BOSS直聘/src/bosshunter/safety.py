"""Local safety state: page budgets, send windows, cooldowns and delivery dedup."""

from __future__ import annotations

import json
import os
import tempfile
import time
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any

from bosshunter.config import parse_send_windows


class SafetyError(RuntimeError):
    """Raised when a local safety guard refuses an action."""

    def __init__(self, code: str, message: str, **details: Any) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.details = details


PAGE_KINDS = ("search", "detail", "chat")
SUCCESS_STATES = {"sent", "ambiguous"}


class SafetyState:
    def __init__(self, path: str | Path, config: dict[str, Any], now: float | None = None) -> None:
        self.path = Path(path)
        self.config = config
        self.now = now if now is not None else time.time()
        self.data = self._load()
        self._rollover()

    def _default_data(self) -> dict[str, Any]:
        return {
            "date": self._today(),
            "pages": {kind: 0 for kind in PAGE_KINDS},
            "sends": 0,
            "last_send_at": 0.0,
            "consecutive_failures": 0,
            "deliveries": {},
            "lock": {},
        }

    def _load(self) -> dict[str, Any]:
        if not self.path.exists():
            return self._default_data()
        try:
            raw = json.loads(self.path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return self._default_data()
        data = self._default_data()
        if isinstance(raw, dict):
            data.update(raw)
        pages = data.get("pages")
        data["pages"] = {kind: int((pages or {}).get(kind, 0) or 0) for kind in PAGE_KINDS}
        deliveries = data.get("deliveries")
        data["deliveries"] = deliveries if isinstance(deliveries, dict) else {}
        lock = data.get("lock")
        data["lock"] = lock if isinstance(lock, dict) else {}
        return data

    def _rollover(self) -> None:
        today = self._today()
        if self.data.get("date") != today:
            deliveries = self.data.get("deliveries") or {}
            lock = self.data.get("lock") or {}
            self.data = self._default_data()
            self.data["deliveries"] = deliveries
            self.data["lock"] = lock
            self.save()

    def reload(self) -> None:
        self.data = self._load()
        self._rollover()

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        temporary_path: Path | None = None
        try:
            with tempfile.NamedTemporaryFile(
                "w",
                encoding="utf-8",
                dir=self.path.parent,
                prefix=f".{self.path.name}.",
                suffix=".tmp",
                delete=False,
            ) as temporary:
                temporary_path = Path(temporary.name)
                json.dump(self.data, temporary, ensure_ascii=False, indent=2)
                temporary.flush()
                os.fsync(temporary.fileno())
            os.replace(temporary_path, self.path)
            temporary_path = None
        finally:
            if temporary_path is not None:
                try:
                    temporary_path.unlink()
                except FileNotFoundError:
                    pass

    # -- risk lock ---------------------------------------------------------
    def ensure_unlocked(self) -> None:
        lock = self.data.get("lock") or {}
        until = float(lock.get("until") or 0)
        if until > time.time():
            remaining = int(until - time.time()) + 1
            raise SafetyError(
                "risk_lock",
                f"平台风险冷却中，请 {remaining} 秒后再试",
                reason=str(lock.get("reason") or ""),
                retry_after_seconds=remaining,
            )
        if lock:
            self.data["lock"] = {}
            self.save()

    def lock(self, reason: str, minutes: int) -> None:
        now = time.time()
        self.data["lock"] = {
            "reason": reason,
            "at": now,
            "until": now + max(1, minutes) * 60,
        }
        self.save()

    # -- page budgets ------------------------------------------------------
    def reserve_page(self, kind: str, *, daily_limit: int | None = None) -> None:
        if kind not in PAGE_KINDS:
            raise ValueError(f"unknown page kind: {kind}")
        self.ensure_unlocked()
        safety = self.config["safety"]
        total = sum(int(self.data["pages"].get(item, 0)) for item in PAGE_KINDS)
        if total >= int(safety["daily_page_limit"]):
            raise SafetyError(
                "daily_page_limit",
                "已达到每日 BOSS 页面访问总上限，请明天再试",
                limit=int(safety["daily_page_limit"]),
            )
        if daily_limit is not None and int(self.data["pages"].get(kind, 0)) >= int(daily_limit):
            raise SafetyError(
                f"daily_{kind}_limit",
                f"已达到每日 {kind} 页面访问上限",
                limit=int(daily_limit),
            )
        self.data["pages"][kind] = int(self.data["pages"].get(kind, 0)) + 1
        self.save()

    # -- sending -----------------------------------------------------------
    def check_send_allowed(self, interval_seconds: float, *, ignore_window: bool = False) -> None:
        self.ensure_unlocked()
        delivery = self.config["delivery"]
        if not ignore_window and not send_window_active(delivery["send_windows"]):
            raise SafetyError(
                "outside_send_window",
                "当前不在配置的发送时间窗口内",
                windows=list(delivery["send_windows"]),
                next_window=next_window_text(delivery["send_windows"]),
            )
        if int(self.data["sends"]) >= int(delivery["daily_limit"]):
            raise SafetyError(
                "daily_send_limit",
                "已达到每日投递上限",
                limit=int(delivery["daily_limit"]),
            )
        last_send_at = float(self.data.get("last_send_at") or 0)
        ready_at = last_send_at + max(0.0, interval_seconds)
        if ready_at > time.time():
            retry_after = int(ready_at - time.time()) + 1
            raise SafetyError(
                "send_cooldown",
                f"距离上次投递不足安全间隔，请 {retry_after} 秒后再试",
                retry_after_seconds=retry_after,
            )

    def delivery_for(self, job_id: str) -> dict[str, Any] | None:
        value = self.data["deliveries"].get(job_id)
        return value if isinstance(value, dict) else None

    def is_delivered(self, job_id: str) -> bool:
        record = self.delivery_for(job_id)
        return bool(record and record.get("status") in SUCCESS_STATES)

    def record_delivery(
        self,
        job_id: str,
        *,
        status: str,
        title: str = "",
        company: str = "",
        url: str = "",
        greeting: str = "",
        images: list[str] | None = None,
        note: str = "",
    ) -> dict[str, Any]:
        now = time.time()
        record = {
            "status": status,
            "at": now,
            "at_text": datetime.fromtimestamp(now).strftime("%Y-%m-%d %H:%M:%S"),
            "title": title,
            "company": company,
            "url": url,
            "greeting": greeting,
            "images": list(images or []),
            "note": note,
        }
        self.data["deliveries"][job_id] = record
        if status in SUCCESS_STATES:
            self.data["sends"] = int(self.data.get("sends") or 0) + 1
            self.data["last_send_at"] = now
        if status == "sent":
            self.data["consecutive_failures"] = 0
        elif status == "failed":
            self.data["consecutive_failures"] = int(self.data.get("consecutive_failures") or 0) + 1
        self.save()
        return record

    def record_failure(self, note: str = "") -> int:
        failures = int(self.data.get("consecutive_failures") or 0) + 1
        self.data["consecutive_failures"] = failures
        self.save()
        return failures

    def reset_failures(self) -> None:
        if self.data.get("consecutive_failures"):
            self.data["consecutive_failures"] = 0
            self.save()

    def snapshot(self) -> dict[str, Any]:
        delivery = self.config["delivery"]
        safety = self.config["safety"]
        lock = self.data.get("lock") or {}
        today = self._today()
        todays = [
            record
            for record in self.data["deliveries"].values()
            if isinstance(record, dict)
            and str(record.get("at_text") or "").startswith(today)
            and record.get("status") in SUCCESS_STATES
        ]
        return {
            "date": today,
            "pages": dict(self.data["pages"]),
            "page_total": sum(int(value) for value in self.data["pages"].values()),
            "page_limit": int(safety["daily_page_limit"]),
            "sends": int(self.data.get("sends") or 0),
            "send_limit": int(delivery["daily_limit"]),
            "send_window_active": send_window_active(delivery["send_windows"]),
            "send_windows": list(delivery["send_windows"]),
            "next_window": next_window_text(delivery["send_windows"]),
            "last_send_at": float(self.data.get("last_send_at") or 0),
            "consecutive_failures": int(self.data.get("consecutive_failures") or 0),
            "delivered_today": len(todays),
            "lock": lock,
            "state_path": str(self.path),
        }

    @staticmethod
    def _today() -> str:
        return datetime.now().strftime("%Y-%m-%d")


def send_window_active(windows: list[str], now: datetime | None = None) -> bool:
    parsed = parse_send_windows(windows)
    if not parsed:
        return True
    current = now or datetime.now()
    minutes = current.hour * 60 + current.minute
    return any(start <= minutes < end for start, end in parsed)


def next_window_text(windows: list[str], now: datetime | None = None) -> str:
    parsed = sorted(parse_send_windows(windows))
    if not parsed:
        return "无窗口限制"
    current = now or datetime.now()
    minutes = current.hour * 60 + current.minute
    for start, _ in parsed:
        if minutes < start:
            tomorrow = False
            start_dt = current.replace(hour=start // 60, minute=start % 60, second=0, microsecond=0)
            if start_dt <= current:
                tomorrow = True
                start_dt += timedelta(days=1)
            return f"明日 {start_dt:%H:%M}" if tomorrow else f"今日 {start_dt:%H:%M}"
    first = parsed[0][0]
    tomorrow_dt = current.replace(hour=first // 60, minute=first % 60, second=0, microsecond=0) + timedelta(days=1)
    return f"明日 {tomorrow_dt:%H:%M}"
