"""Minimal MCP stdio server exposing OfficialApply as agent tools."""

from __future__ import annotations

import json
import os
import sys
from typing import Any

from officialapply import __version__
from officialapply.app import OfficialApply
from officialapply.errors import ToolError, error_payload

PROTOCOL_VERSION = "2024-11-05"
SERVER_NAME = "officialapply"

TOOLS: list[dict[str, Any]] = [
    {
        "name": "official_profile_check",
        "description": (
            "检查官网投递资料 MD 的完整度：缺失必填字段、建议补充字段、材料文件是否可用。"
            "投递前先调用一次，资料不完整时不要把问题带进表单。"
        ),
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "official_profile_show",
        "description": "查看解析后的资料摘要与教育/材料信息；手机号、邮箱、身份证等自动打码。",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "official_status",
        "description": "查看资料完整度、专用 Chrome 连接状态、今日官网提交次数/额度、提交间隔和最近记录。",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "official_open",
        "description": (
            "在公司官网/招聘页打开或切换标签页。传 url 会导航；不传则复用当前标签页。"
            "官网通常需要登录，登录/验证码由本人完成。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "招聘页或岗位页链接"},
                "new": {"type": "boolean", "description": "是否强制打开新标签页，默认 false"},
                "front": {"type": "boolean", "description": "是否切到前台，默认 false"},
            },
        },
    },
    {
        "name": "official_inspect",
        "description": (
            "读取当前页面的表单字段（含 data-oa-id、标签、必填、选项）、按钮、链接、登录/验证码/"
            "已投递/校验错误信号。多步表单每一步都先 inspect，再 fill/click。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "max_text": {"type": "integer", "minimum": 500, "maximum": 30000, "description": "返回页面文字上限，默认 6000"},
                "front": {"type": "boolean", "description": "读取前切到前台，默认 false"},
            },
        },
    },
    {
        "name": "official_plan",
        "description": (
            "只读预览：把页面字段映射到资料 MD，返回将填写的字段、值预览、附件上传计划、"
            "未识别必填字段、待确认条款和警告。不会修改页面。正式填写前建议先 plan。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "overrides": {
                    "type": "object",
                    "additionalProperties": {"type": "string"},
                    "description": "显式赋值，键可以是语义（如 expected_salary）、data-oa-id 或页面标签，值为字符串",
                },
                "job_title": {"type": "string", "description": "当前岗位名称；缺省读取页面结构化数据"},
                "job_company": {"type": "string", "description": "当前公司名称"},
                "accept_terms": {"type": "boolean", "description": "计划中是否包含勾选同意条款，默认 false"},
            },
        },
    },
    {
        "name": "official_fill",
        "description": (
            "按资料自动填写当前页面字段，可同时上传计划中的简历/照片/成绩单。支持原生 input/select/"
            "radio/checkbox、自定义下拉、同源 iframe。多步表单填完当前步后 inspect/click 下一步。"
            "已存在不同内容默认覆盖，可用 overwrite=false 禁止。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "overrides": {
                    "type": "object",
                    "additionalProperties": {"type": "string"},
                    "description": "显式赋值，键为语义/data-oa-id/页面标签",
                },
                "only": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "只填写指定语义、data-oa-id 或标签；不传则填写全部已匹配字段",
                },
                "job_title": {"type": "string"},
                "job_company": {"type": "string"},
                "accept_terms": {"type": "boolean", "description": "是否勾选同意/隐私条款，默认 false；需本人确认后使用"},
                "upload_documents": {"type": "boolean", "description": "是否同时自动上传计划中的附件，默认 false"},
                "skip_correct": {"type": "boolean", "description": "页面值已正确时跳过，默认 true"},
                "overwrite": {"type": "boolean", "description": "覆盖页面已有但不同的内容，默认 true"},
            },
        },
    },
    {
        "name": "official_autofill",
        "description": (
            "一键填写当前网申页面并自动推进多步表单：识别模板、上传简历、填写原生/自定义控件、"
            "处理拆分年月、自动点击下一步，最终停在「预览并提交/提交」按钮前，绝不自动提交。"
            "遇到登录、验证码、未知必填、条款或校验错误会停止并说明原因。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "overrides": {
                    "type": "object",
                    "additionalProperties": {"type": "string"},
                    "description": "显式赋值，键为语义/data-oa-id/页面标签",
                },
                "job_title": {"type": "string", "description": "当前岗位名称"},
                "job_company": {"type": "string", "description": "当前公司名称"},
                "accept_terms": {
                    "type": "boolean",
                    "description": "是否勾选同意/隐私条款，默认 false；需本人确认后使用",
                },
                "upload_documents": {
                    "type": "boolean",
                    "description": "是否自动上传简历/照片等计划附件，默认 true",
                },
                "max_steps": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 20,
                    "description": "最多处理几个页面步骤，缺省读取配置",
                },
                "wait_seconds": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 30,
                    "description": "点击下一步后的等待秒数，缺省读取配置",
                },
            },
        },
    },
    {
        "name": "official_templates",
        "description": "列出内置网申模板（Moka、北森、大易、谷露、国聘、Workday、Greenhouse 等）及识别规则。",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "official_upload",
        "description": (
            "上传附件。可以指定 file 字段的 data-oa-id，或按材料角色（resume/photo/transcript/"
            "enrollment_proof 等）自动找上传框；不指定时按配置自动上传全部计划材料。"
            "身份证等敏感证件只应在明确确认后上传。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "field_id": {"type": "string", "description": "文件输入框或上传按钮的 data-oa-id"},
                "role": {"type": "string", "description": "材料角色，如 resume、photo、transcript"},
                "paths": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "本地文件路径；不传则使用资料 MD 中对应材料",
                },
                "upload_all": {"type": "boolean", "description": "同角色有多个上传框时全部上传，默认 false"},
            },
        },
    },
    {
        "name": "official_click",
        "description": "点击页面按钮/链接，用于「下一步」「继续」「保存」「进入申请表」等。优先使用 inspect 返回的 data-oa-id。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "按钮文字"},
                "field_id": {"type": "string", "description": "按钮的 data-oa-id"},
                "wait": {"type": "number", "minimum": 0, "maximum": 30, "description": "点击后等待秒数，默认 0.6"},
            },
        },
    },
    {
        "name": "official_submit",
        "description": (
            "提交当前官网申请。会先检查登录/验证码、今日上限、提交间隔、相同链接重复投递、"
            "未识别必填字段、必填为空、必填附件未上传、同意/隐私条款；任一项不满足会拒绝提交。"
            "提交后自动截图并写入 applications.jsonl。建议先 dry_run=true 预检。"
            "accept_terms=true 只能在本人确认条款后使用。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "button": {"type": "string", "description": "提交按钮文字，缺省自动识别"},
                "accept_terms": {"type": "boolean", "description": "确认勾选同意/隐私条款，默认 false"},
                "force": {"type": "boolean", "description": "忽略相同链接的已投递记录，默认 false"},
                "job_title": {"type": "string", "description": "岗位名称"},
                "job_company": {"type": "string", "description": "公司名称"},
                "notes": {"type": "string", "description": "备注"},
                "dry_run": {"type": "boolean", "description": "只预检，不点击、不截图、不记录，默认 false"},
            },
        },
    },
    {
        "name": "official_screenshot",
        "description": "保存当前页面截图到 data/screenshots，返回绝对路径。",
        "inputSchema": {
            "type": "object",
            "properties": {"name": {"type": "string", "description": "截图名称，可选"}},
        },
    },
    {
        "name": "official_history",
        "description": "查看官网投递记录（JSONL）和今日统计。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "limit": {"type": "integer", "minimum": 1, "maximum": 500, "description": "返回条数，默认 20"},
                "status": {"type": "string", "enum": ["submitted", "unconfirmed", "failed"], "description": "按状态过滤"},
            },
        },
    },
    {
        "name": "official_login",
        "description": (
            "打开指定官网登录页并切到前台。短信验证码可用 official_otp_fill 自动从本地接收端取码填入；"
            "扫码、滑块、图形验证码必须由本人完成，工具不会绕过验证。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {"url": {"type": "string", "description": "登录页链接；不传则复用当前页面"}},
        },
    },
    {
        "name": "official_check_login",
        "description": "检查当前页面（或指定 URL）是否仍要求登录，返回登录证据和页面标题。",
        "inputSchema": {
            "type": "object",
            "properties": {"url": {"type": "string", "description": "可选，检查前导航到该链接"}},
        },
    },
    {
        "name": "official_otp_fill",
        "description": (
            "短信验证码自动登录：从本机 SmsForwarder 接收端读取最新验证码并直接填入页面，"
            "可自动点击获取验证码和登录按钮。验证码只在内存与页面之间传递，不会返回给模型、"
            "不打印、不写日志。遇到图形验证码、滑块或人机验证会返回 human_handoff。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "send": {
                    "type": "boolean",
                    "description": "本地没有较新验证码时，是否点击获取/发送验证码按钮，默认 true",
                },
                "send_button": {"type": "string", "description": "可选，指定发送验证码按钮文字"},
                "field_id": {"type": "string", "description": "可选，指定验证码输入框的 data-oa-id"},
                "submit": {
                    "type": "boolean",
                    "description": "填入并校验成功后是否点击登录按钮，默认 true",
                },
                "submit_button": {"type": "string", "description": "可选，指定登录按钮文字"},
                "timeout": {
                    "type": "number",
                    "minimum": 5,
                    "maximum": 600,
                    "description": "等待新验证码的秒数，默认 120",
                },
                "reuse_existing": {
                    "type": "boolean",
                    "description": "本地已有近期验证码时直接使用，避免重复发送，默认 true",
                },
            },
        },
    },
]


def _config_path() -> str | None:
    return os.environ.get("OFFICIALAPPLY_CONFIG") or None


def _client() -> OfficialApply:
    return OfficialApply(_config_path())


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


def _float_arg(args: dict[str, Any], key: str, default: float) -> float:
    value = args.get(key)
    if value is None:
        return default
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise ToolError("invalid_arguments", f"参数 {key} 必须是数字")
    return float(value)


def _string_list_arg(args: dict[str, Any], *keys: str) -> list[str] | None:
    for key in keys:
        value = args.get(key)
        if value is None:
            continue
        if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
            raise ToolError("invalid_arguments", f"参数 {key} 必须是字符串数组")
        return value
    return None


def _string_map_arg(args: dict[str, Any], key: str) -> dict[str, str] | None:
    value = args.get(key)
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ToolError("invalid_arguments", f"参数 {key} 必须是对象")
    result: dict[str, str] = {}
    for item_key, item_value in value.items():
        if not isinstance(item_key, str) or not isinstance(item_value, (str, int, float, bool)):
            raise ToolError("invalid_arguments", f"参数 {key} 的键和值必须是字符串")
        result[item_key] = str(item_value)
    return result


def call_tool(name: str, args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    if name == "official_profile_check":
        return client.profile_check()
    if name == "official_profile_show":
        return client.profile_show()
    if name == "official_status":
        return client.status()
    if name == "official_open":
        return client.open(
            _string_arg(args, "url") or "",
            new=_bool_arg(args, "new", False),
            front=_bool_arg(args, "front", False),
        )
    if name == "official_inspect":
        return client.inspect(
            max_text=_int_arg(args, "max_text") or 6000,
            front=_bool_arg(args, "front", False),
        )
    if name == "official_plan":
        return client.plan(
            overrides=_string_map_arg(args, "overrides"),
            job_title=_string_arg(args, "job_title") or "",
            job_company=_string_arg(args, "job_company") or "",
            accept_terms=_bool_arg(args, "accept_terms", False),
        )
    if name == "official_fill":
        return client.fill(
            overrides=_string_map_arg(args, "overrides"),
            job_title=_string_arg(args, "job_title") or "",
            job_company=_string_arg(args, "job_company") or "",
            accept_terms=_bool_arg(args, "accept_terms", False),
            upload_documents=_bool_arg(args, "upload_documents", False),
            skip_correct=_bool_arg(args, "skip_correct", True),
            overwrite=_bool_arg(args, "overwrite", True),
            only=_string_list_arg(args, "only"),
        )
    if name == "official_autofill":
        return client.autofill(
            overrides=_string_map_arg(args, "overrides"),
            job_title=_string_arg(args, "job_title") or "",
            job_company=_string_arg(args, "job_company") or "",
            accept_terms=_bool_arg(args, "accept_terms", False),
            upload_documents=_bool_arg(args, "upload_documents", True),
            max_steps=_int_arg(args, "max_steps") or 0,
            wait_seconds=_float_arg(args, "wait_seconds", 0.0),
        )
    if name == "official_templates":
        return client.templates()
    if name == "official_upload":
        return client.upload(
            field_id=_string_arg(args, "field_id") or _string_arg(args, "id") or "",
            role=_string_arg(args, "role") or "",
            paths=_string_list_arg(args, "paths", "files"),
            upload_all=_bool_arg(args, "upload_all", False),
        )
    if name == "official_click":
        return client.click(
            text=_string_arg(args, "text") or "",
            field_id=_string_arg(args, "field_id") or _string_arg(args, "id") or "",
            wait=_float_arg(args, "wait", 0.6),
        )
    if name == "official_submit":
        return client.submit(
            button=_string_arg(args, "button") or "",
            accept_terms=_bool_arg(args, "accept_terms", False),
            force=_bool_arg(args, "force", False),
            job_title=_string_arg(args, "job_title") or "",
            job_company=_string_arg(args, "job_company") or "",
            notes=_string_arg(args, "notes") or "",
            dry_run=_bool_arg(args, "dry_run", False),
        )
    if name == "official_screenshot":
        return client.screenshot(_string_arg(args, "name") or "")
    if name == "official_history":
        return client.history(
            limit=_int_arg(args, "limit") or 20,
            status=_string_arg(args, "status") or "",
        )
    if name == "official_login":
        return client.login(_string_arg(args, "url") or "")
    if name == "official_check_login":
        return client.check_login(_string_arg(args, "url") or "")
    if name == "official_otp_fill":
        return client.otp_fill(
            send=_bool_arg(args, "send", True),
            send_button=_string_arg(args, "send_button") or "",
            field_id=_string_arg(args, "field_id") or _string_arg(args, "id") or "",
            submit=_bool_arg(args, "submit", True),
            submit_button=_string_arg(args, "submit_button") or "",
            timeout=_float_arg(args, "timeout", 120.0),
            reuse_existing=_bool_arg(args, "reuse_existing", True),
        )
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
