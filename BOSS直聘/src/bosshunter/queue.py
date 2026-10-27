"""Persistent application queue and batch delivery runner."""

from __future__ import annotations

import json
import os
import tempfile
import time
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any, Callable, Iterable

from bosshunter.boss.deliver import BossDeliverer
from bosshunter.boss.search import job_id_from_url
from bosshunter.config import parse_send_windows
from bosshunter.errors import ToolError, error_payload
from bosshunter.safety import SUCCESS_STATES, SafetyState, send_window_active

QUEUE_VERSION = 1

STATUS_PENDING = "pending"
STATUS_SENT = "sent"
STATUS_PARTIAL = "partial"
STATUS_AMBIGUOUS = "ambiguous"
STATUS_FAILED = "failed"
STATUS_SKIPPED = "skipped"

STATUSES = (
    STATUS_PENDING,
    STATUS_SENT,
    STATUS_PARTIAL,
    STATUS_AMBIGUOUS,
    STATUS_FAILED,
    STATUS_SKIPPED,
)
TERMINAL_STATUSES = {STATUS_SENT, STATUS_PARTIAL, STATUS_AMBIGUOUS, STATUS_SKIPPED}

GLOBAL_STOP_CODES = {
    "risk_lock",
    "platform_risk",
    "login_required",
    "daily_send_limit",
    "daily_page_limit",
    "daily_chat_limit",
    "daily_search_limit",
    "daily_detail_limit",
    "too_many_consecutive_failures",
}

EventCallback = Callable[[dict[str, Any]], None]


def _now_text(at: float | None = None) -> str:
    return datetime.fromtimestamp(at if at is not None else time.time()).strftime("%Y-%m-%d %H:%M:%S")


class QueueLock:
    """Cross-process exclusive lock released automatically when the process exits."""

    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)
        self._handle: Any = None

    def __enter__(self) -> "QueueLock":
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._handle = open(self.path, "a+", encoding="utf-8")
        try:
            if os.name == "nt":
                import msvcrt

                self._handle.seek(0)
                msvcrt.locking(self._handle.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl

                fcntl.flock(self._handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as exc:
            self._handle.close()
            self._handle = None
            raise ToolError("queue_locked", "已有投递队列正在运行，请等待其结束") from exc
        self._handle.seek(0)
        self._handle.truncate()
        self._handle.write(json.dumps({"pid": os.getpid(), "started_at": _now_text()}, ensure_ascii=False))
        self._handle.flush()
        return self

    def __exit__(self, exc_type: Any, exc: Any, traceback: Any) -> None:
        if self._handle is None:
            return
        try:
            if os.name == "nt":
                import msvcrt

                self._handle.seek(0)
                msvcrt.locking(self._handle.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(self._handle.fileno(), fcntl.LOCK_UN)
        finally:
            self._handle.close()
            self._handle = None


class ApplyQueue:
    def __init__(self, path: str | Path, config: dict[str, Any]) -> None:
        self.path = Path(path)
        self.config = config
        self.data = self._load()

    def _default_data(self) -> dict[str, Any]:
        return {"version": QUEUE_VERSION, "updated_at": "", "items": []}

    def _load(self) -> dict[str, Any]:
        if not self.path.exists():
            return self._default_data()
        try:
            raw = json.loads(self.path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            raise ToolError("queue_corrupt", f"投递队列文件无法读取: {self.path}") from exc
        if not isinstance(raw, dict):
            raise ToolError("queue_corrupt", f"投递队列文件格式错误: {self.path}")
        items = raw.get("items")
        if not isinstance(items, list):
            raise ToolError("queue_corrupt", f"投递队列缺少 items 列表: {self.path}")
        normalized = [
            item
            for item in (self._normalize_item(value) for value in items if isinstance(value, dict))
            if item
        ]
        return {"version": QUEUE_VERSION, "updated_at": str(raw.get("updated_at") or ""), "items": normalized}

    def _normalize_item(self, item: dict[str, Any]) -> dict[str, Any] | None:
        url = str(item.get("url") or "").strip()
        job_id = str(item.get("job_id") or "").strip() or job_id_from_url(url)
        if not job_id:
            return None
        if not url:
            url = f"https://www.zhipin.com/job_detail/{job_id}.html"
        status = str(item.get("status") or STATUS_PENDING)
        if status not in STATUSES:
            status = STATUS_PENDING
        greeting = item.get("greeting")
        images = item.get("images")
        return {
            "job_id": job_id,
            "url": url,
            "title": str(item.get("title") or ""),
            "company": str(item.get("company") or ""),
            "city": str(item.get("city") or ""),
            "location": str(item.get("location") or ""),
            "salary": str(item.get("salary") or ""),
            "experience": str(item.get("experience") or ""),
            "education": str(item.get("education") or ""),
            "keyword": str(item.get("keyword") or ""),
            "source": str(item.get("source") or ""),
            "priority": _to_int(item.get("priority"), 0),
            "status": status,
            "greeting": str(greeting) if isinstance(greeting, str) and greeting.strip() else None,
            "images": [str(value) for value in images if str(value).strip()]
            if isinstance(images, list) and images
            else None,
            "note": str(item.get("note") or ""),
            "attempts": max(0, _to_int(item.get("attempts"), 0)),
            "added_at": str(item.get("added_at") or ""),
            "updated_at": str(item.get("updated_at") or ""),
            "last_attempt_at": str(item.get("last_attempt_at") or ""),
            "last_error": item.get("last_error") if isinstance(item.get("last_error"), dict) else None,
            "result": item.get("result") if isinstance(item.get("result"), dict) else None,
        }

    def reload(self) -> None:
        self.data = self._load()

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.data["version"] = QUEUE_VERSION
        self.data["updated_at"] = _now_text()
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

    # -- queries -----------------------------------------------------------
    def items(self, status: str | None = None) -> list[dict[str, Any]]:
        if status is None:
            return list(self.data["items"])
        return [item for item in self.data["items"] if item["status"] == status]

    def find(self, job_id: str) -> dict[str, Any] | None:
        job_id = str(job_id or "").strip() or job_id_from_url(job_id)
        for item in self.data["items"]:
            if item["job_id"] == job_id:
                return item
        return None

    def next_pending(self) -> dict[str, Any] | None:
        pending = self.items(STATUS_PENDING)
        if not pending:
            return None
        return max(pending, key=lambda item: (item["priority"], -self._order_of(item["job_id"])))

    def _order_of(self, job_id: str) -> int:
        for index, item in enumerate(self.data["items"]):
            if item["job_id"] == job_id:
                return index
        return 0

    def summary(self) -> dict[str, Any]:
        counts = {status: 0 for status in STATUSES}
        for item in self.data["items"]:
            counts[item["status"]] = counts.get(item["status"], 0) + 1
        return {
            "total": len(self.data["items"]),
            "pending": counts[STATUS_PENDING],
            "by_status": counts,
            "path": str(self.path),
            "updated_at": self.data.get("updated_at") or "",
        }

    # -- mutations ---------------------------------------------------------
    def add(
        self,
        jobs: Iterable[dict[str, Any]],
        *,
        priority: int = 0,
        greeting: str | None = None,
        images: list[str] | None = None,
        replace: bool = False,
        source: str = "",
    ) -> dict[str, Any]:
        added: list[str] = []
        duplicates: list[str] = []
        invalid: list[dict[str, str]] = []
        updated: list[str] = []
        now = _now_text()
        for raw in jobs:
            if not isinstance(raw, dict):
                invalid.append({"url": "", "error": "岗位必须是对象"})
                continue
            item = self._item_from_job(raw, priority=priority, greeting=greeting, images=images, source=source)
            if item is None:
                invalid.append({"url": str(raw.get("url") or raw.get("job_id") or ""), "error": "缺少有效的岗位链接或 job_id"})
                continue
            existing = self.find(item["job_id"])
            if existing is not None:
                duplicates.append(item["job_id"])
                if replace:
                    for key in (
                        "url",
                        "title",
                        "company",
                        "city",
                        "location",
                        "salary",
                        "experience",
                        "education",
                        "keyword",
                        "priority",
                        "greeting",
                        "images",
                    ):
                        value = item.get(key)
                        if value not in (None, ""):
                            existing[key] = value
                    if raw.get("priority") is not None:
                        existing["priority"] = _to_int(raw.get("priority"), existing["priority"])
                    existing["updated_at"] = now
                    updated.append(item["job_id"])
                continue
            item["added_at"] = now
            item["updated_at"] = now
            self.data["items"].append(item)
            added.append(item["job_id"])
        if added or updated:
            self.save()
        return {
            "added": len(added),
            "updated": len(updated),
            "duplicates": len(duplicates),
            "invalid": len(invalid),
            "added_ids": added,
            "updated_ids": updated,
            "duplicate_ids": duplicates,
            "invalid_items": invalid,
            "summary": self.summary(),
        }

    def _item_from_job(
        self,
        raw: dict[str, Any],
        *,
        priority: int,
        greeting: str | None,
        images: list[str] | None,
        source: str,
    ) -> dict[str, Any] | None:
        url = str(raw.get("url") or "").strip()
        job_id = str(raw.get("job_id") or "").strip() or job_id_from_url(url)
        if not job_id:
            return None
        if not url:
            url = f"https://www.zhipin.com/job_detail/{job_id}.html"
        item_greeting = raw.get("greeting")
        if not isinstance(item_greeting, str) or not item_greeting.strip():
            item_greeting = greeting
        item_images = raw.get("images")
        if not isinstance(item_images, list):
            item_images = images
        return {
            "job_id": job_id,
            "url": url,
            "title": str(raw.get("title") or ""),
            "company": str(raw.get("company") or ""),
            "city": str(raw.get("city") or ""),
            "location": str(raw.get("location") or ""),
            "salary": str(raw.get("salary") or ""),
            "experience": str(raw.get("experience") or ""),
            "education": str(raw.get("education") or ""),
            "keyword": str(raw.get("keyword") or ""),
            "source": str(raw.get("source") or source),
            "priority": _to_int(raw.get("priority"), priority),
            "status": STATUS_PENDING,
            "greeting": str(item_greeting) if isinstance(item_greeting, str) and item_greeting.strip() else None,
            "images": [str(value) for value in item_images if str(value).strip()]
            if isinstance(item_images, list) and item_images
            else None,
            "note": str(raw.get("note") or ""),
            "attempts": 0,
            "added_at": "",
            "updated_at": "",
            "last_attempt_at": "",
            "last_error": None,
            "result": None,
        }

    def update(self, job_id: str, updates: dict[str, Any]) -> dict[str, Any]:
        item = self.find(job_id)
        if item is None:
            raise ToolError("queue_item_not_found", f"队列中不存在岗位: {job_id}")
        allowed = {"status", "priority", "greeting", "images", "note"}
        unknown = sorted(set(updates) - allowed)
        if unknown:
            raise ToolError("invalid_arguments", f"不支持的队列字段: {', '.join(unknown)}")
        if "status" in updates:
            status = str(updates["status"])
            if status not in STATUSES:
                raise ToolError("invalid_arguments", f"status 必须是: {', '.join(STATUSES)}")
            item["status"] = status
        if "priority" in updates:
            item["priority"] = _to_int(updates["priority"], item["priority"])
        if "greeting" in updates:
            value = updates["greeting"]
            if value is not None and not isinstance(value, str):
                raise ToolError("invalid_arguments", "greeting 必须是字符串或 null")
            item["greeting"] = value.strip() if isinstance(value, str) and value.strip() else None
        if "images" in updates:
            value = updates["images"]
            if value is not None and (
                not isinstance(value, list) or not all(isinstance(entry, str) for entry in value)
            ):
                raise ToolError("invalid_arguments", "images 必须是字符串数组或 null")
            item["images"] = [entry for entry in value if entry.strip()] if value else None
        if "note" in updates:
            item["note"] = str(updates["note"] or "")
        item["updated_at"] = _now_text()
        self.save()
        return item

    def remove(self, job_ids: Iterable[str]) -> dict[str, Any]:
        wanted = {str(value or "").strip() or job_id_from_url(value) for value in job_ids}
        wanted.discard("")
        if not wanted:
            raise ToolError("invalid_arguments", "请提供要移除的 job_id")
        kept = [item for item in self.data["items"] if item["job_id"] not in wanted]
        removed = [item["job_id"] for item in self.data["items"] if item["job_id"] in wanted]
        if removed:
            self.data["items"] = kept
            self.save()
        return {"removed": len(removed), "removed_ids": removed, "summary": self.summary()}

    def reset(self, statuses: Iterable[str] | None = None) -> dict[str, Any]:
        wanted = {str(value) for value in (statuses or [STATUS_FAILED])}
        unknown = wanted - set(STATUSES)
        if unknown:
            raise ToolError("invalid_arguments", f"未知状态: {', '.join(sorted(unknown))}")
        reset_ids: list[str] = []
        now = _now_text()
        for item in self.data["items"]:
            if item["status"] in wanted:
                item["status"] = STATUS_PENDING
                item["last_error"] = None
                item["result"] = None
                item["updated_at"] = now
                reset_ids.append(item["job_id"])
        if reset_ids:
            self.save()
        return {"reset": len(reset_ids), "reset_ids": reset_ids, "summary": self.summary()}

    def mark(
        self,
        job_id: str,
        *,
        status: str,
        note: str | None = None,
        error: dict[str, Any] | None = None,
        result: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        item = self.find(job_id)
        if item is None:
            raise ToolError("queue_item_not_found", f"队列中不存在岗位: {job_id}")
        item["status"] = status
        item["attempts"] = int(item.get("attempts") or 0) + 1
        item["last_attempt_at"] = _now_text()
        item["updated_at"] = item["last_attempt_at"]
        if note is not None:
            item["note"] = note
        item["last_error"] = error
        item["result"] = result
        self.save()
        return item


class QueueRunner:
    def __init__(
        self,
        config: dict[str, Any],
        state: SafetyState,
        *,
        deliverer: Any | None = None,
        sleep: Callable[[float], None] | None = None,
        on_event: EventCallback | None = None,
        log_path: str | Path | None = None,
    ) -> None:
        self.config = config
        self.state = state
        self.deliverer = deliverer
        self.sleep = sleep or time.sleep
        self.on_event = on_event
        self.log_path = Path(
            log_path
            or config.get("_queue_log_path")
            or Path(config["_base_dir"]) / "data" / "apply_queue_log.jsonl"
        )

    def run(
        self,
        *,
        limit: int | None = None,
        wait_window: bool = False,
        dry_run: bool = False,
        force: bool = False,
        max_minutes: int | None = None,
        retry_failed: bool = False,
        max_wait_window_minutes: int = 60,
    ) -> dict[str, Any]:
        if limit is not None and limit < 1:
            raise ToolError("invalid_arguments", "limit 必须大于 0")
        if max_minutes is not None and max_minutes < 1:
            raise ToolError("invalid_arguments", "max_minutes 必须大于 0")
        if dry_run and limit is None:
            limit = 1
        queue = ApplyQueue(self.config["_queue_path"], self.config)
        if retry_failed:
            queue.reset([STATUS_FAILED])
        started_at = _now_text()
        started = time.time()
        results: list[dict[str, Any]] = []
        attempted = sent = failed = ambiguous = partial = skipped = 0
        reason = "queue_empty"
        self._event(
            {
                "event": "run_started",
                "at": started_at,
                "pending": queue.summary()["pending"],
                "dry_run": dry_run,
            }
        )
        with QueueLock(str(Path(self.config["_queue_path"]).with_suffix(".run.lock"))):
            deliverer = self.deliverer or BossDeliverer(self.config, self.state)
            self._run_failures = 0
            while True:
                self.state.reload()
                queue.reload()
                item = queue.next_pending()
                if item is None:
                    reason = "queue_empty"
                    break
                if limit is not None and attempted >= limit:
                    reason = "limit_reached"
                    break
                snapshot = self.state.snapshot()
                if int(snapshot["sends"]) >= int(snapshot["send_limit"]) and not dry_run:
                    reason = "daily_send_limit"
                    break
                max_failures = int(self.config["safety"]["max_consecutive_failures"])
                if int(snapshot["consecutive_failures"]) >= max_failures and not force:
                    reason = "too_many_consecutive_failures"
                    break
                lock = snapshot.get("lock") or {}
                if float(lock.get("until") or 0) > time.time():
                    reason = "risk_lock"
                    break
                if max_minutes is not None and time.time() - started >= max_minutes * 60:
                    reason = "max_minutes"
                    break
                if not send_window_active(self.config["delivery"]["send_windows"]):
                    if not wait_window:
                        reason = "outside_send_window"
                        break
                    seconds = seconds_until_next_window(self.config["delivery"]["send_windows"])
                    if seconds > max_wait_window_minutes * 60:
                        reason = "outside_send_window"
                        break
                    if seconds > 0:
                        self._event(
                            {
                                "event": "waiting_window",
                                "seconds": int(seconds),
                                "until": _now_text(time.time() + seconds),
                            }
                        )
                        self._sleep_seconds(seconds)
                        continue
                existing = self.state.delivery_for(item["job_id"])
                if existing and existing.get("status") in SUCCESS_STATES and not force and not dry_run:
                    status = STATUS_AMBIGUOUS if existing.get("status") == "ambiguous" else STATUS_SENT
                    queue.mark(
                        item["job_id"],
                        status=status,
                        note="已有投递记录，未重复发送",
                        result={"already_delivered": True, "delivery_status": existing.get("status")},
                    )
                    skipped += 1
                    self._event(
                        {
                            "event": "item_skipped",
                            "job_id": item["job_id"],
                            "title": item["title"],
                            "status": status,
                            "message": "已有投递记录",
                        }
                    )
                    continue
                attempted += 1
                self._event(
                    {
                        "event": "item_started",
                        "job_id": item["job_id"],
                        "title": item["title"],
                        "company": item["company"],
                        "attempt": attempted,
                    }
                )
                try:
                    outcome = deliverer.deliver(
                        item["url"],
                        greeting=item.get("greeting"),
                        images=item.get("images"),
                        force=force,
                        dry_run=dry_run,
                        wait=True,
                    )
                except ToolError as exc:
                    if exc.code == "send_cooldown":
                        attempted -= 1
                        retry_after = int(exc.details.get("retry_after_seconds") or 0)
                        if 0 < retry_after <= 600:
                            self._event(
                                {
                                    "event": "waiting_cooldown",
                                    "seconds": retry_after,
                                    "until": _now_text(time.time() + retry_after),
                                }
                            )
                            self._sleep_seconds(retry_after)
                            continue
                        reason = exc.code
                        break
                    if exc.code == "outside_send_window":
                        attempted -= 1
                        seconds = seconds_until_next_window(self.config["delivery"]["send_windows"])
                        if wait_window and seconds <= max_wait_window_minutes * 60:
                            self._event(
                                {
                                    "event": "waiting_window",
                                    "seconds": int(seconds),
                                    "until": _now_text(time.time() + seconds),
                                }
                            )
                            self._sleep_seconds(seconds)
                            continue
                        reason = exc.code
                        break
                    if exc.code in GLOBAL_STOP_CODES:
                        attempted -= 1
                        reason = exc.code
                        self._event(
                            {
                                "event": "run_stopping",
                                "reason": exc.code,
                                "message": exc.message,
                            }
                        )
                        break
                    item = queue.find(item["job_id"]) or item
                    if exc.code in {"greeting_unverified", "first_contact_unverified"}:
                        queue.mark(
                            item["job_id"],
                            status=STATUS_AMBIGUOUS,
                            note=exc.message,
                            error=exc.as_dict(),
                            result={"url": item["url"]},
                        )
                        ambiguous += 1
                        self._note_run_failure()
                        self._event(
                            {
                                "event": "item_ambiguous",
                                "job_id": item["job_id"],
                                "title": item["title"],
                                "status": STATUS_AMBIGUOUS,
                                "message": exc.message,
                                "code": exc.code,
                            }
                        )
                    else:
                        queue.mark(
                            item["job_id"],
                            status=STATUS_FAILED,
                            note=exc.message,
                            error=exc.as_dict(),
                            result={"url": item["url"]},
                        )
                        failed += 1
                        self._note_run_failure()
                        self._event(
                            {
                                "event": "item_failed",
                                "job_id": item["job_id"],
                                "title": item["title"],
                                "status": STATUS_FAILED,
                                "message": exc.message,
                                "code": exc.code,
                            }
                        )
                    results.append(
                        {
                            "job_id": item["job_id"],
                            "title": item["title"],
                            "company": item["company"],
                            "status": STATUS_AMBIGUOUS
                            if exc.code in {"greeting_unverified", "first_contact_unverified"}
                            else STATUS_FAILED,
                            "error": exc.as_dict(),
                        }
                    )
                    if self._consecutive_failures() >= max_failures and not force:
                        reason = "consecutive_failures"
                        break
                    continue
                except KeyboardInterrupt:
                    reason = "interrupted"
                    break
                except Exception as exc:
                    payload = error_payload(exc)
                    item = queue.find(item["job_id"]) or item
                    queue.mark(
                        item["job_id"],
                        status=STATUS_FAILED,
                        note=str(payload.get("message") or exc),
                        error=payload,
                        result={"url": item["url"]},
                    )
                    failed += 1
                    self._note_run_failure()
                    results.append(
                        {
                            "job_id": item["job_id"],
                            "title": item["title"],
                            "company": item["company"],
                            "status": STATUS_FAILED,
                            "error": payload,
                        }
                    )
                    self._event(
                        {
                            "event": "item_failed",
                            "job_id": item["job_id"],
                            "title": item["title"],
                            "status": STATUS_FAILED,
                            "message": payload.get("message"),
                            "code": payload.get("code"),
                        }
                    )
                    if self._consecutive_failures() >= max_failures and not force:
                        reason = "consecutive_failures"
                        break
                    continue
                item = queue.find(item["job_id"]) or item
                if dry_run:
                    results.append(
                        {
                            "job_id": item["job_id"],
                            "title": item["title"],
                            "company": item["company"],
                            "dry_run": True,
                            "job": outcome.get("job") or {},
                            "chat_button": outcome.get("chat_button") or {},
                            "would_send_greeting": outcome.get("would_send_greeting"),
                            "would_send_images": outcome.get("would_send_images"),
                        }
                    )
                    self._reset_run_failures()
                    self._event(
                        {
                            "event": "item_dry_run",
                            "job_id": item["job_id"],
                            "title": item["title"],
                            "button": bool((outcome.get("chat_button") or {}).get("success")),
                        }
                    )
                    continue
                if outcome.get("skipped"):
                    queue.mark(
                        item["job_id"],
                        status=STATUS_SENT,
                        note=str(outcome.get("reason") or "already_delivered"),
                        result={"skipped": True, "reason": outcome.get("reason")},
                    )
                    skipped += 1
                    self._event(
                        {
                            "event": "item_skipped",
                            "job_id": item["job_id"],
                            "title": item["title"],
                            "status": STATUS_SENT,
                            "message": str(outcome.get("message") or "已跳过"),
                        }
                    )
                    continue
                is_partial = bool(outcome.get("partial"))
                status = STATUS_PARTIAL if is_partial else STATUS_SENT
                queue.mark(
                    item["job_id"],
                    status=status,
                    note="；".join(outcome.get("warnings") or []) or "招呼语与图片已送达",
                    result={
                        "partial": is_partial,
                        "greeting": outcome.get("greeting") or {},
                        "images": outcome.get("images") or {},
                        "delivery": outcome.get("delivery") or {},
                        "warnings": outcome.get("warnings") or [],
                    },
                )
                if is_partial:
                    partial += 1
                else:
                    sent += 1
                self._reset_run_failures()
                delivery = outcome.get("delivery") or {}
                results.append(
                    {
                        "job_id": item["job_id"],
                        "title": delivery.get("title") or item["title"],
                        "company": delivery.get("company") or item["company"],
                        "status": status,
                        "partial": is_partial,
                        "warnings": outcome.get("warnings") or [],
                    }
                )
                self._event(
                    {
                        "event": "item_sent",
                        "job_id": item["job_id"],
                        "title": item["title"],
                        "company": item["company"],
                        "status": status,
                        "attempt": attempted,
                    }
                )
                if limit is not None and attempted >= limit:
                    reason = "limit_reached"
                    break
        remaining = ApplyQueue(self.config["_queue_path"], self.config).summary()
        finished_at = _now_text()
        result = {
            "reason": reason,
            "dry_run": dry_run,
            "started_at": started_at,
            "finished_at": finished_at,
            "attempted": attempted,
            "sent": sent,
            "partial": partial,
            "failed": failed,
            "ambiguous": ambiguous,
            "skipped": skipped,
            "results": results,
            "queue": remaining,
            "state": self.state.snapshot(),
        }
        self._event(
            {
                "event": "run_finished",
                "reason": reason,
                "attempted": attempted,
                "sent": sent,
                "partial": partial,
                "failed": failed,
                "skipped": skipped,
                "pending": remaining["pending"],
            }
        )
        self._write_log(result)
        return result

    def _consecutive_failures(self) -> int:
        return int(getattr(self, "_run_failures", 0))

    def _note_run_failure(self) -> None:
        self._run_failures = self._consecutive_failures() + 1

    def _reset_run_failures(self) -> None:
        self._run_failures = 0

    def _sleep_seconds(self, seconds: float) -> None:
        remaining = max(0.0, float(seconds))
        while remaining > 0:
            step = min(remaining, 60.0)
            self.sleep(step)
            remaining -= step

    def _event(self, event: dict[str, Any]) -> None:
        event.setdefault("at", _now_text())
        if self.on_event is not None:
            self.on_event(event)

    def _write_log(self, result: dict[str, Any]) -> None:
        try:
            self.log_path.parent.mkdir(parents=True, exist_ok=True)
            with self.log_path.open("a", encoding="utf-8") as handle:
                handle.write(json.dumps(result, ensure_ascii=False) + "\n")
        except OSError:
            pass


def seconds_until_next_window(windows: list[str], now: datetime | None = None) -> float:
    parsed = sorted(parse_send_windows(windows))
    if not parsed:
        return 0.0
    current = now or datetime.now()
    minutes = current.hour * 60 + current.minute + current.second / 60
    for start, end in parsed:
        if start <= minutes < end:
            return 0.0
        if minutes < start:
            target = current.replace(hour=start // 60, minute=start % 60, second=0, microsecond=0)
            return max(0.0, (target - current).total_seconds())
    target = current.replace(
        hour=parsed[0][0] // 60,
        minute=parsed[0][0] % 60,
        second=0,
        microsecond=0,
    ) + timedelta(days=1)
    return max(0.0, (target - current).total_seconds())


def _to_int(value: Any, default: int) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return default
