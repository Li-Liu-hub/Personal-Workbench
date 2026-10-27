"""Shared operations used by the CLI and the MCP server."""

from __future__ import annotations

from typing import Any

from bosshunter.boss.deliver import BossDeliverer
from bosshunter.boss.search import BossSearcher, check_login
from bosshunter.browser.chrome import open_login_page
from bosshunter.config import load_config, resolve_path
from bosshunter.errors import ToolError
from bosshunter.queue import ApplyQueue, QueueRunner
from bosshunter.safety import SafetyError, SafetyState


class BossHunter:
    def __init__(self, config_path: str | None = None) -> None:
        self.config = load_config(config_path)
        self.state = SafetyState(self.config["_state_path"], self.config)

    def search(
        self,
        *,
        keywords: list[str] | None = None,
        cities: list[str] | None = None,
        max_pages: int | None = None,
        filters: dict[str, Any] | None = None,
        sort: str | None = None,
        include_details: bool = False,
        max_details: int = 60,
    ) -> dict[str, Any]:
        self.state.reload()
        try:
            result = BossSearcher(self.config, self.state).search(
                keywords=keywords,
                cities=cities,
                max_pages=max_pages,
                filters=filters,
                sort=sort,
                include_details=include_details,
                max_details=max_details,
            )
        except SafetyError as exc:
            raise ToolError(exc.code, exc.message, **exc.details) from exc
        return {"success": True, **result}

    def detail(self, url: str) -> dict[str, Any]:
        self.state.reload()
        return {"success": True, **BossSearcher(self.config, self.state).detail(url)}

    def apply(
        self,
        job_url: str,
        *,
        greeting: str | None = None,
        images: list[str] | None = None,
        send_greeting: bool = True,
        send_images: bool = True,
        force: bool = False,
        dry_run: bool = False,
        wait: bool = False,
    ) -> dict[str, Any]:
        self.state.reload()
        result = BossDeliverer(self.config, self.state).deliver(
            job_url,
            greeting=greeting,
            images=images,
            send_greeting=send_greeting,
            send_images=send_images,
            force=force,
            dry_run=dry_run,
            wait=wait,
        )
        return {"success": True, **result}

    def send_images(
        self,
        job_url: str,
        *,
        images: list[str] | None = None,
        message: str | None = None,
        force: bool = True,
        wait: bool = False,
    ) -> dict[str, Any]:
        self.state.reload()
        result = BossDeliverer(self.config, self.state).send_images(
            job_url,
            images=images,
            message=message,
            force=force,
            wait=wait,
        )
        return {"success": True, **result}

    def queue_list(self, *, status: str | None = None, limit: int | None = 50) -> dict[str, Any]:
        self.state.reload()
        queue = ApplyQueue(self.config["_queue_path"], self.config)
        items = queue.items(status)
        order = {
            "pending": 0,
            "failed": 1,
            "ambiguous": 2,
            "partial": 3,
            "sent": 4,
            "skipped": 5,
        }
        items = sorted(
            enumerate(items),
            key=lambda pair: (order.get(pair[1]["status"], 9), -pair[1]["priority"], pair[0]),
        )
        total = len(items)
        if limit is not None:
            items = items[: max(0, limit)]
        enriched = []
        for _index, item in items:
            delivery = self.state.delivery_for(item["job_id"])
            enriched.append(
                {
                    **item,
                    "delivery_status": delivery.get("status") if delivery else "",
                }
            )
        return {
            "success": True,
            "queue": queue.summary(),
            "count": total,
            "shown": len(enriched),
            "items": enriched,
        }

    def queue_add(
        self,
        jobs: list[dict[str, Any]],
        *,
        priority: int = 0,
        greeting: str | None = None,
        images: list[str] | None = None,
        replace: bool = False,
        source: str = "",
    ) -> dict[str, Any]:
        if not isinstance(jobs, list):
            raise ToolError("invalid_arguments", "jobs 必须是岗位对象数组")
        result = ApplyQueue(self.config["_queue_path"], self.config).add(
            jobs,
            priority=priority,
            greeting=greeting,
            images=images,
            replace=replace,
            source=source,
        )
        return {"success": True, **result}

    def queue_update(self, job_id: str, updates: dict[str, Any]) -> dict[str, Any]:
        if not isinstance(updates, dict) or not updates:
            raise ToolError("invalid_arguments", "updates 必须是非空对象")
        queue = ApplyQueue(self.config["_queue_path"], self.config)
        item = queue.update(job_id, updates)
        return {"success": True, "item": item, "queue": queue.summary()}

    def queue_remove(self, job_ids: list[str]) -> dict[str, Any]:
        if not isinstance(job_ids, list) or not job_ids:
            raise ToolError("invalid_arguments", "job_ids 必须是非空字符串数组")
        result = ApplyQueue(self.config["_queue_path"], self.config).remove(job_ids)
        return {"success": True, **result}

    def queue_reset(self, statuses: list[str] | None = None) -> dict[str, Any]:
        result = ApplyQueue(self.config["_queue_path"], self.config).reset(statuses)
        return {"success": True, **result}

    def queue_run(
        self,
        *,
        limit: int | None = None,
        wait_window: bool = False,
        dry_run: bool = False,
        force: bool = False,
        max_minutes: int | None = None,
        retry_failed: bool = False,
        on_event: Any | None = None,
    ) -> dict[str, Any]:
        self.state.reload()
        runner = QueueRunner(self.config, self.state, on_event=on_event)
        result = runner.run(
            limit=limit,
            wait_window=wait_window,
            dry_run=dry_run,
            force=force,
            max_minutes=max_minutes,
            retry_failed=retry_failed,
        )
        return {"success": True, **result}

    def status(self) -> dict[str, Any]:
        self.state.reload()
        snapshot = self.state.snapshot()
        try:
            queue_summary = ApplyQueue(self.config["_queue_path"], self.config).summary()
        except ToolError as exc:
            queue_summary = {"error": exc.as_dict()}
        deliveries = [
            {"job_id": job_id, **record}
            for job_id, record in sorted(
                (
                    (job_id, record)
                    for job_id, record in self.state.data["deliveries"].items()
                    if isinstance(record, dict)
                ),
                key=lambda item: float(item[1].get("at") or 0),
                reverse=True,
            )[:10]
        ]
        delivery_cfg = self.config["delivery"]
        browser_cfg = self.config["browser"]
        return {
            "success": True,
            "state": snapshot,
            "queue": queue_summary,
            "recent_deliveries": deliveries,
            "config": {
                "path": self.config["_config_path"],
                "search": {
                    "keywords": list(self.config["search"]["keywords"]),
                    "cities": list(self.config["search"]["cities"]),
                    "max_pages": int(self.config["search"]["max_pages"]),
                    "sort": self.config["search"]["sort"],
                    "filters": self.config["search"].get("filters") or {},
                },
                "delivery": {
                    "greeting": delivery_cfg["greeting"],
                    "images": [str(resolve_path(item, self.config)) for item in delivery_cfg["images"]],
                    "daily_limit": int(delivery_cfg["daily_limit"]),
                    "interval_min": int(delivery_cfg["interval_min"]),
                    "interval_max": int(delivery_cfg["interval_max"]),
                    "send_windows": list(delivery_cfg["send_windows"]),
                },
                "resume_path": (
                    str(resolve_path(self.config["profile"]["resume_path"], self.config))
                    if self.config["profile"]["resume_path"] else ""
                ),
                "browser": {
                    "debug_port": int(browser_cfg["debug_port"]),
                    "user_data_dir": str(resolve_path(str(browser_cfg["user_data_dir"]), self.config)),
                    "auto_launch": bool(browser_cfg["auto_launch"]),
                },
            },
        }

    def login(self) -> dict[str, Any]:
        return {"success": True, **open_login_page(self.config)}

    def check_login(self) -> dict[str, Any]:
        self.state.reload()
        return {"success": True, **check_login(self.config, self.state)}
