"""Local JSONL registry and submission guards for official-site applications."""

from __future__ import annotations

import json
import secrets
from datetime import date, datetime
from pathlib import Path
from typing import Any
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

SUBMITTED_STATUSES = {"submitted", "unconfirmed"}
TRACKING_KEYS = {
    "from",
    "source",
    "src",
    "spm",
    "share",
    "shareid",
    "share_id",
    "ref",
    "referer",
    "referrer",
    "channel",
    "track",
    "tracking",
    "trace",
    "traceid",
    "clickid",
}


class Registry:
    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)

    def records(self) -> list[dict[str, Any]]:
        if not self.path.exists():
            return []
        result: list[dict[str, Any]] = []
        try:
            lines = self.path.read_text(encoding="utf-8-sig").splitlines()
        except OSError:
            return []
        for line in lines:
            line = line.strip()
            if not line:
                continue
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(item, dict):
                result.append(item)
        return result

    def append(self, record: dict[str, Any]) -> dict[str, Any]:
        now = datetime.now().astimezone()
        item = dict(record)
        item.setdefault("id", f"{now.strftime('%Y%m%d-%H%M%S')}-{secrets.token_hex(3)}")
        item.setdefault("at", now.isoformat(timespec="seconds"))
        item.setdefault("date", now.date().isoformat())
        item["url_key"] = normalize_url(str(item.get("url") or ""))
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self.path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(item, ensure_ascii=False, sort_keys=False) + "\n")
            handle.flush()
        return item

    def history(self, limit: int = 20, *, status: str = "") -> list[dict[str, Any]]:
        items = [
            item for item in self.records()
            if not status or str(item.get("status") or "") == status
        ]
        return items[-max(0, limit):][::-1]

    def today(self, today: str = "") -> dict[str, Any]:
        day = today or date.today().isoformat()
        records = [item for item in self.records() if str(item.get("date") or "") == day]
        submitted = [item for item in records if item.get("status") == "submitted"]
        unconfirmed = [item for item in records if item.get("status") == "unconfirmed"]
        failed = [item for item in records if item.get("status") == "failed"]
        return {
            "date": day,
            "submitted": len(submitted),
            "unconfirmed": len(unconfirmed),
            "counted": len(submitted) + len(unconfirmed),
            "failed": len(failed),
            "attempts": len(records),
        }

    def last_submit_at(self) -> datetime | None:
        latest: datetime | None = None
        for item in self.records():
            if item.get("status") not in SUBMITTED_STATUSES:
                continue
            stamp = _parse_time(item.get("at"))
            if stamp and (latest is None or stamp > latest):
                latest = stamp
        return latest

    def check(
        self,
        url: str,
        *,
        company: str = "",
        title: str = "",
        daily_limit: int = 0,
        min_interval_seconds: int = 0,
        now: datetime | None = None,
    ) -> dict[str, Any]:
        moment = now or datetime.now().astimezone()
        key = normalize_url(url)
        same_url = [
            item for item in self.records()
            if item.get("url_key") == key and item.get("status") in SUBMITTED_STATUSES
        ]
        same_title = []
        if company.strip() and title.strip():
            same_title = [
                item for item in self.records()
                if str(item.get("company") or "").strip().lower() == company.strip().lower()
                and str(item.get("title") or "").strip().lower() == title.strip().lower()
                and item.get("status") in SUBMITTED_STATUSES
                and item.get("url_key") != key
            ]
        day = self.today(moment.date().isoformat())
        last = self.last_submit_at()
        wait_seconds = 0
        if last and min_interval_seconds > 0:
            elapsed = (moment - last.astimezone(moment.tzinfo)).total_seconds()
            wait_seconds = max(0, int(min_interval_seconds - elapsed))
        limit_reached = daily_limit > 0 and day["counted"] >= daily_limit
        return {
            "url_key": key,
            "daily": day,
            "daily_limit": daily_limit,
            "daily_remaining": max(0, daily_limit - day["counted"]) if daily_limit > 0 else None,
            "limit_reached": limit_reached,
            "min_interval_seconds": min_interval_seconds,
            "interval_remaining_seconds": wait_seconds,
            "duplicate_url": same_url[-1] if same_url else None,
            "similar_title": same_title[-1] if same_title else None,
            "can_submit": not same_url and not limit_reached and wait_seconds <= 0,
        }

    def update(self, record_id: str, updates: dict[str, Any]) -> dict[str, Any] | None:
        items = self.records()
        changed = None
        for item in items:
            if str(item.get("id")) == record_id:
                item.update(updates)
                changed = item
                break
        if changed is None:
            return None
        tmp = self.path.with_suffix(self.path.suffix + ".tmp")
        with tmp.open("w", encoding="utf-8") as handle:
            for item in items:
                handle.write(json.dumps(item, ensure_ascii=False, sort_keys=False) + "\n")
        tmp.replace(self.path)
        return changed


def normalize_url(url: str) -> str:
    text = str(url or "").strip()
    if not text:
        return ""
    parts = urlsplit(text)
    scheme = (parts.scheme or "https").lower()
    host = (parts.hostname or "").lower()
    if not host:
        return text.split("#", 1)[0].rstrip("/")
    port = parts.port
    netloc = host
    if port and not ((scheme == "http" and port == 80) or (scheme == "https" and port == 443)):
        netloc = f"{host}:{port}"
    path = parts.path or "/"
    if path != "/":
        path = path.rstrip("/")
    query_pairs = []
    for key, value in parse_qsl(parts.query, keep_blank_values=True):
        lowered = key.lower()
        if lowered in TRACKING_KEYS or lowered.startswith("utm_"):
            continue
        if lowered.startswith(("spm", "share", "track", "trace")):
            continue
        query_pairs.append((key, value))
    query = urlencode(query_pairs, doseq=True)
    return urlunsplit((scheme, netloc, path, query, ""))


def _parse_time(value: Any) -> datetime | None:
    if not isinstance(value, str) or not value:
        return None
    try:
        return datetime.fromisoformat(value)
    except ValueError:
        return None

