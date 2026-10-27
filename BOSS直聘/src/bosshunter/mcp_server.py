"""Minimal MCP stdio server exposing BossHunter as agent tools."""

from __future__ import annotations

import json
import os
import sys
from typing import Any

from bosshunter import __version__
from bosshunter.api import BossHunter
from bosshunter.errors import ToolError, error_payload
from bosshunter.queue import STATUSES

PROTOCOL_VERSION = "2024-11-05"
SERVER_NAME = "bosshunter"

TOOLS: list[dict[str, Any]] = [
    {
        "name": "boss_search_jobs",
        "description": (
            "搜索 BOSS 直聘岗位并返回列表（可选补全 JD 详情）。使用专用 Chrome 登录态，"
            "受本地每日页面预算与风险冷却限制。默认关键词/城市来自 config.yaml，可覆盖。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "keywords": {"type": "array", "items": {"type": "string"}, "description": "搜索关键词列表"},
                "cities": {"type": "array", "items": {"type": "string"}, "description": "城市名称列表，如 上海、深圳"},
                "max_pages": {"type": "integer", "minimum": 1, "maximum": 50, "description": "每个城市×关键词组合最多读取页数"},
                "filters": {
                    "type": "object",
                    "description": "BOSS 筛选条件",
                    "additionalProperties": True,
                },
                "sort": {"type": "string", "enum": ["default", "newest"], "description": "排序方式"},
                "include_details": {"type": "boolean", "description": "是否逐个打开详情页补全 JD/HR 信息"},
                "max_details": {"type": "integer", "minimum": 1, "maximum": 500, "description": "最多补全多少个详情"},
            },
        },
    },
    {
        "name": "boss_job_detail",
        "description": "读取单个 BOSS 岗位详情页（JD、薪资、公司、招聘者）。",
        "inputSchema": {
            "type": "object",
            "properties": {"url": {"type": "string", "description": "https://www.zhipin.com/job_detail/... 完整链接"}},
            "required": ["url"],
        },
    },
    {
        "name": "boss_apply",
        "description": (
            "向岗位发起沟通并发送文字消息与图片。会点击「立即沟通」，处理平台预设招呼语/首次沟通弹窗，"
            "随后在聊天页发送 greeting 和图片并验证送达。默认会真实发送；建议先传 dry_run=true 检查页面。"
            "同一岗位已有成功/待确认记录时会跳过，除非 force=true。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "job_url": {"type": "string", "description": "岗位详情页链接"},
                "greeting": {"type": "string", "description": "文字消息；不传则使用配置 delivery.greeting"},
                "images": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "要发送的图片路径；不传则使用配置 delivery.images",
                },
                "send_greeting": {"type": "boolean", "description": "是否发送文字消息，默认 true"},
                "send_images": {"type": "boolean", "description": "是否发送图片，默认 true"},
                "force": {"type": "boolean", "description": "忽略已投递记录并重置连续失败计数，默认 false"},
                "dry_run": {"type": "boolean", "description": "只检查岗位和沟通按钮，不点击、不发送，默认 false"},
                "wait": {"type": "boolean", "description": "冷却期内最多等待 10 分钟，默认 false"},
            },
            "required": ["job_url"],
        },
    },
    {
        "name": "boss_send_images",
        "description": "向已有岗位会话补发图片，可选择附带一条文字消息。默认忽略已投递记录。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "job_url": {"type": "string", "description": "岗位详情页链接"},
                "images": {"type": "array", "items": {"type": "string"}, "description": "图片路径；不传则使用配置 delivery.images"},
                "message": {"type": "string", "description": "可选，同时发送的文字消息"},
                "force": {"type": "boolean", "description": "忽略已投递记录，默认 true"},
                "wait": {"type": "boolean", "description": "冷却期内最多等待 10 分钟"},
            },
            "required": ["job_url"],
        },
    },
    {
        "name": "boss_queue_list",
        "description": "查看持久化投递队列，可按状态过滤；返回待投递数量、优先级和岗位元数据。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "status": {"type": "string", "enum": list(STATUSES), "description": "只显示指定状态"},
                "limit": {"type": "integer", "minimum": 0, "maximum": 5000, "description": "最多返回多少条，默认 50"},
            },
        },
    },
    {
        "name": "boss_queue_add",
        "description": (
            "向持久化投递队列添加岗位。jobs 每项至少提供 url 或 job_id；可带 title/company/city 等元数据。"
            "重复岗位会跳过，replace=true 时更新元数据。默认使用配置中的招呼语和简历图片。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "jobs": {
                    "type": "array",
                    "items": {"type": "object", "additionalProperties": True},
                    "description": "岗位数组，字段如 url、job_id、title、company、city、salary、priority、greeting、images",
                },
                "priority": {"type": "integer", "description": "统一优先级，越大越先投，默认 0"},
                "greeting": {"type": "string", "description": "统一覆盖招呼语；不传则使用配置"},
                "images": {"type": "array", "items": {"type": "string"}, "description": "统一覆盖图片；不传则使用配置"},
                "replace": {"type": "boolean", "description": "重复时用导入数据覆盖元数据，默认 false"},
                "source": {"type": "string", "description": "来源标记"},
            },
            "required": ["jobs"],
        },
    },
    {
        "name": "boss_queue_update",
        "description": "修改队列中单个岗位的状态、优先级、专用招呼语、专用图片或备注。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "job_id": {"type": "string", "description": "岗位 ID"},
                "status": {"type": "string", "enum": list(STATUSES)},
                "priority": {"type": "integer"},
                "greeting": {"type": ["string", "null"], "description": "传 null 恢复使用配置招呼语"},
                "images": {
                    "type": ["array", "null"],
                    "items": {"type": "string"},
                    "description": "传 null 恢复使用配置图片",
                },
                "note": {"type": "string"},
            },
            "required": ["job_id"],
        },
    },
    {
        "name": "boss_queue_remove",
        "description": "从投递队列移除一个或多个岗位。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "job_ids": {"type": "array", "items": {"type": "string"}, "description": "要移除的岗位 ID 列表"},
            },
            "required": ["job_ids"],
        },
    },
    {
        "name": "boss_queue_reset",
        "description": "把 failed 等状态的队列岗位重置为 pending，便于重新投递。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "statuses": {
                    "type": "array",
                    "items": {"type": "string", "enum": list(STATUSES)},
                    "description": "要重置的状态，默认 failed",
                },
            },
        },
    },
    {
        "name": "boss_queue_run",
        "description": (
            "批量执行投递队列：按优先级逐个发起沟通并发送招呼语/图片，受每日上限、发送窗口、安全冷却"
            "和连续失败限制。调用会一直阻塞到队列耗尽、达到 limit/max_minutes、窗口关闭或触发风险停止。"
            "长时间全量投递建议用 CLI 或计划任务运行。dry_run=true 只预检第一个岗位。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "limit": {"type": "integer", "minimum": 1, "maximum": 10000, "description": "本次最多尝试多少个岗位"},
                "wait_window": {"type": "boolean", "description": "窗口外最多等待 60 分钟，默认 false"},
                "dry_run": {"type": "boolean", "description": "只预检第一个岗位，不发送，默认 false"},
                "force": {"type": "boolean", "description": "忽略已投递记录与连续失败计数，默认 false"},
                "max_minutes": {"type": "integer", "minimum": 1, "maximum": 1440, "description": "本次运行最长分钟数"},
                "retry_failed": {"type": "boolean", "description": "先把 failed 岗位重置为 pending"},
            },
        },
    },
    {
        "name": "boss_status",
        "description": "查看本地安全状态：今日页面预算、投递次数、发送窗口/冷却、风险锁、投递队列、最近投递记录与配置摘要。",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "boss_check_login",
        "description": "打开 BOSS 聊天页检查登录状态，返回 logged_in 与专用 Chrome 配置目录。",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "boss_open_login",
        "description": "打开专用 Chrome 的 BOSS 登录页，用户需在浏览器中扫码/短信登录。",
        "inputSchema": {"type": "object", "properties": {}},
    },
]


def _config_path() -> str | None:
    return os.environ.get("BOSSHUNTER_CONFIG") or None


def _client() -> BossHunter:
    return BossHunter(_config_path())


def _string_arg(args: dict[str, Any], key: str, *, required: bool = False) -> str | None:
    value = args.get(key)
    if value is None:
        if required:
            raise ToolError("invalid_arguments", f"缺少参数 {key}")
        return None
    if not isinstance(value, str):
        raise ToolError("invalid_arguments", f"参数 {key} 必须是字符串")
    if required and not value.strip():
        raise ToolError("invalid_arguments", f"参数 {key} 不能为空")
    return value


def _bool_arg(args: dict[str, Any], key: str, default: bool) -> bool:
    value = args.get(key)
    if value is None:
        return default
    if not isinstance(value, bool):
        raise ToolError("invalid_arguments", f"参数 {key} 必须是布尔值")
    return value


def _int_arg(args: dict[str, Any], key: str) -> int | None:
    value = args.get(key)
    if value is None:
        return None
    if not isinstance(value, int) or isinstance(value, bool):
        raise ToolError("invalid_arguments", f"参数 {key} 必须是整数")
    return value


def _string_list_arg(args: dict[str, Any], key: str) -> list[str] | None:
    value = args.get(key)
    if value is None:
        return None
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise ToolError("invalid_arguments", f"参数 {key} 必须是字符串数组")
    return value


def _dict_arg(args: dict[str, Any], key: str) -> dict[str, Any] | None:
    value = args.get(key)
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ToolError("invalid_arguments", f"参数 {key} 必须是对象")
    return value


def _object_list_arg(args: dict[str, Any], key: str, *, required: bool = False) -> list[dict[str, Any]] | None:
    value = args.get(key)
    if value is None:
        if required:
            raise ToolError("invalid_arguments", f"缺少参数 {key}")
        return None
    if not isinstance(value, list) or not all(isinstance(item, dict) for item in value):
        raise ToolError("invalid_arguments", f"参数 {key} 必须是对象数组")
    return value


def _logical_job_url(args: dict[str, Any]) -> str:
    for key in ("job_url", "url"):
        value = args.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    raise ToolError("invalid_arguments", "缺少参数 job_url")


def call_tool(name: str, args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    if name == "boss_search_jobs":
        max_pages = _int_arg(args, "max_pages")
        max_details = _int_arg(args, "max_details")
        if max_pages is not None and max_pages < 1:
            raise ToolError("invalid_arguments", "max_pages 必须大于 0")
        if max_details is not None and max_details < 1:
            raise ToolError("invalid_arguments", "max_details 必须大于 0")
        return client.search(
            keywords=_string_list_arg(args, "keywords"),
            cities=_string_list_arg(args, "cities"),
            max_pages=max_pages,
            filters=_dict_arg(args, "filters"),
            sort=_string_arg(args, "sort"),
            include_details=_bool_arg(args, "include_details", False),
            max_details=max_details or 60,
        )
    if name == "boss_job_detail":
        return client.detail(_string_arg(args, "url", required=True) or "")
    if name == "boss_apply":
        return client.apply(
            _logical_job_url(args),
            greeting=_string_arg(args, "greeting"),
            images=_string_list_arg(args, "images"),
            send_greeting=_bool_arg(args, "send_greeting", True),
            send_images=_bool_arg(args, "send_images", True),
            force=_bool_arg(args, "force", False),
            dry_run=_bool_arg(args, "dry_run", False),
            wait=_bool_arg(args, "wait", False),
        )
    if name == "boss_send_images":
        return client.send_images(
            _logical_job_url(args),
            images=_string_list_arg(args, "images"),
            message=_string_arg(args, "message"),
            force=_bool_arg(args, "force", True),
            wait=_bool_arg(args, "wait", False),
        )
    if name == "boss_queue_list":
        status = _string_arg(args, "status")
        if status is not None and status not in STATUSES:
            raise ToolError("invalid_arguments", f"status 必须是: {', '.join(STATUSES)}")
        limit = _int_arg(args, "limit")
        if limit is not None and limit < 0:
            raise ToolError("invalid_arguments", "limit 不能小于 0")
        return client.queue_list(status=status, limit=limit)
    if name == "boss_queue_add":
        jobs = _object_list_arg(args, "jobs", required=True) or []
        if not jobs:
            raise ToolError("invalid_arguments", "jobs 不能为空")
        return client.queue_add(
            jobs,
            priority=_int_arg(args, "priority") or 0,
            greeting=_string_arg(args, "greeting"),
            images=_string_list_arg(args, "images"),
            replace=_bool_arg(args, "replace", False),
            source=_string_arg(args, "source") or "",
        )
    if name == "boss_queue_update":
        updates = {key: args[key] for key in ("status", "priority", "greeting", "images", "note") if key in args}
        if not updates:
            raise ToolError("invalid_arguments", "至少提供一个要修改的字段")
        return client.queue_update(_string_arg(args, "job_id", required=True) or "", updates)
    if name == "boss_queue_remove":
        job_ids = _string_list_arg(args, "job_ids")
        if not job_ids:
            raise ToolError("invalid_arguments", "job_ids 不能为空")
        return client.queue_remove(job_ids)
    if name == "boss_queue_reset":
        statuses = _string_list_arg(args, "statuses")
        if statuses is not None and not all(status in STATUSES for status in statuses):
            raise ToolError("invalid_arguments", f"statuses 只能包含: {', '.join(STATUSES)}")
        return client.queue_reset(statuses)
    if name == "boss_queue_run":
        return client.queue_run(
            limit=_int_arg(args, "limit"),
            wait_window=_bool_arg(args, "wait_window", False),
            dry_run=_bool_arg(args, "dry_run", False),
            force=_bool_arg(args, "force", False),
            max_minutes=_int_arg(args, "max_minutes"),
            retry_failed=_bool_arg(args, "retry_failed", False),
        )
    if name == "boss_status":
        return client.status()
    if name == "boss_check_login":
        return client.check_login()
    if name == "boss_open_login":
        return client.login()
    raise ToolError("unknown_tool", f"未知工具: {name}")


def _tool_result(payload: dict[str, Any], *, is_error: bool = False) -> dict[str, Any]:
    return {
        "content": [{"type": "text", "text": json.dumps(payload, ensure_ascii=False, indent=2)}],
        "structuredContent": payload,
        "isError": is_error,
    }


def _handle_request(request: dict[str, Any]) -> dict[str, Any] | None:
    method = request.get("method")
    request_id = request.get("id")
    is_notification = "id" not in request
    if method == "initialize":
        requested = request.get("params", {}).get("protocolVersion")
        version = requested if isinstance(requested, str) and requested else PROTOCOL_VERSION
        result = {
            "protocolVersion": version,
            "capabilities": {"tools": {"listChanged": False}},
            "serverInfo": {"name": SERVER_NAME, "version": __version__},
        }
        return None if is_notification else {"jsonrpc": "2.0", "id": request_id, "result": result}
    if method in {"notifications/initialized", "initialized", "notifications/cancelled"}:
        return None
    if method == "ping":
        return None if is_notification else {"jsonrpc": "2.0", "id": request_id, "result": {}}
    if method == "tools/list":
        result = {"tools": TOOLS}
        return None if is_notification else {"jsonrpc": "2.0", "id": request_id, "result": result}
    if method == "tools/call":
        params = request.get("params") or {}
        name = params.get("name") if isinstance(params, dict) else None
        arguments = params.get("arguments") if isinstance(params, dict) else None
        if not isinstance(name, str) or not name:
            return {
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {"code": -32602, "message": "tools/call 缺少工具名称"},
            }
        if arguments is None:
            arguments = {}
        if not isinstance(arguments, dict):
            return {
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {"code": -32602, "message": "arguments 必须是对象"},
            }
        try:
            payload = call_tool(name, arguments)
            tool_result = _tool_result(payload)
        except Exception as exc:
            tool_result = _tool_result({"success": False, "error": error_payload(exc)}, is_error=True)
        return None if is_notification else {"jsonrpc": "2.0", "id": request_id, "result": tool_result}
    return {
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {"code": -32601, "message": f"未知方法: {method}"},
    }


def _write(message: dict[str, Any]) -> bool:
    try:
        sys.stdout.write(json.dumps(message, ensure_ascii=False, separators=(",", ":")) + "\n")
        sys.stdout.flush()
        return True
    except (BrokenPipeError, OSError):
        return False


def serve() -> None:
    if hasattr(sys.stdin, "reconfigure"):
        sys.stdin.reconfigure(encoding="utf-8", errors="replace")
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    while True:
        line = sys.stdin.readline()
        if not line:
            break
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            if not _write({
                "jsonrpc": "2.0",
                "id": None,
                "error": {"code": -32700, "message": "JSON 解析失败"},
            }):
                break
            continue
        if not isinstance(request, dict):
            if not _write({
                "jsonrpc": "2.0",
                "id": None,
                "error": {"code": -32600, "message": "无效请求"},
            }):
                break
            continue
        response = _handle_request(request)
        if response is not None and not _write(response):
            break


if __name__ == "__main__":
    serve()
