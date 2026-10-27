"""Command line interface for OfficialApply."""

from __future__ import annotations

import json
import logging
import time
from typing import Any

import click

from officialapply import __version__
from officialapply.app import OfficialApply
from officialapply.config import ConfigError, load_config, resolve_path
from officialapply.errors import ToolError, error_payload


def _echo_json(payload: Any) -> None:
    click.echo(json.dumps(payload, ensure_ascii=False, indent=2))


def _handle_error(ctx: click.Context, exc: Exception) -> None:
    payload = error_payload(exc)
    if isinstance(exc, ConfigError):
        payload = {"code": "config_error", "message": str(exc)}
    if ctx.obj["json"]:
        _echo_json({"success": False, "error": payload})
    else:
        click.secho(f"错误 [{payload['code']}]: {payload['message']}", fg="red", err=True)
        extra = {key: value for key, value in payload.items() if key not in {"code", "message"}}
        if extra:
            click.echo(json.dumps(extra, ensure_ascii=False, indent=2), err=True)
    ctx.exit(1)


def _client(ctx: click.Context) -> OfficialApply:
    return OfficialApply(ctx.obj["config_path"])


def _parse_sets(values: tuple[str, ...]) -> dict[str, str]:
    result: dict[str, str] = {}
    for item in values:
        key, separator, value = item.partition("=")
        key = key.strip()
        if not separator or not key:
            raise ToolError("invalid_assignment", f"赋值格式必须为 字段=值，收到: {item}")
        result[key] = value.strip()
    return result


@click.group(context_settings={"help_option_names": ["-h", "--help"]})
@click.option(
    "--config",
    "config_path",
    type=click.Path(dir_okay=False, path_type=str),
    default=None,
    help="配置文件路径，默认读取当前目录 config.yaml",
)
@click.option("--json", "as_json", is_flag=True, help="以 JSON 输出，便于程序或 Agent 解析")
@click.version_option(__version__, prog_name="officialapply")
@click.pass_context
def cli(ctx: click.Context, config_path: str | None, as_json: bool) -> None:
    """公司官网投递工具：打开招聘页、填写表单、上传材料并提交。"""
    ctx.obj = {"config_path": config_path, "json": as_json}


@cli.command("profile-check")
@click.pass_context
def profile_check_command(ctx: click.Context) -> None:
    """检查资料 MD 的完整度、缺失字段和材料文件。"""
    try:
        result = _client(ctx).profile_check()
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    summary = result.get("summary") or {}
    click.echo(f"资料: {result.get('path')}")
    click.echo(
        f"姓名: {summary.get('name') or '（空）'} | 学校: {summary.get('school') or '（空）'} | "
        f"专业: {summary.get('major') or '（空）'} | 学历: {summary.get('degree') or '（空）'}"
    )
    click.echo(
        f"期望岗位: {summary.get('expected_position') or '（空）'} | "
        f"期望城市: {'、'.join(summary.get('expected_cities') or []) or '（空）'}"
    )
    click.echo(
        f"材料: {summary.get('documents_ready', 0)}/{summary.get('documents_total', 0)} 个文件可用"
    )
    if result.get("complete_required"):
        click.secho("必填资料完整，可以开始投递", fg="green")
    else:
        click.secho("必填资料尚未完整", fg="yellow")
    for title, key in (("缺少必填字段", "missing_required"), ("建议补充", "missing_recommended"), ("缺少材料", "missing_documents")):
        items = result.get(key) or []
        if not items:
            continue
        click.echo(f"{title}:")
        for item in items:
            label = item.get("field") or item.get("label") or item.get("role")
            reason = item.get("reason") or ""
            click.echo(f"- {label}（{reason}）")
    for warning in result.get("warnings") or []:
        click.secho(f"提示: {warning}", fg="yellow")


@cli.command("profile-show")
@click.pass_context
def profile_show_command(ctx: click.Context) -> None:
    """显示解析后的资料（手机号/邮箱/身份证等会打码）。"""
    try:
        result = _client(ctx).profile_show()
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    basic = [
        ("姓名", result.get("name")),
        ("性别", result.get("gender")),
        ("出生日期", result.get("birth_date")),
        ("手机号", result.get("phone")),
        ("邮箱", result.get("email")),
        ("现居城市", result.get("current_city")),
    ]
    for label, value in basic:
        click.echo(f"{label}: {value or '（空）'}")
    click.echo(f"期望岗位: {result.get('expected_position') or '（空）'}")
    click.echo(f"期望城市: {'、'.join(result.get('expected_cities') or []) or '（空）'}")
    education = result.get("education_entries") or []
    if education:
        click.echo("教育经历:")
        for entry in education:
            click.echo(f"- {entry.get('school') or entry.get('title') or ''} | {entry.get('major') or ''} | {entry.get('degree') or ''}")
    click.echo("材料:")
    for item in result.get("documents") or []:
        state = "可用" if item.get("exists") else "缺失"
        click.echo(f"- {item.get('label')}: {item.get('filename')}（{state}）")


@cli.command("status")
@click.pass_context
def status_command(ctx: click.Context) -> None:
    """查看资料、浏览器和今日提交额度。"""
    try:
        result = _client(ctx).status()
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    profile = result["profile"]
    click.echo(f"资料: {result['config']['profile_path']}")
    click.echo(f"必填完整: {'是' if profile['complete_required'] else '否'}")
    missing = profile.get("missing_required") or []
    if missing:
        click.secho("缺少: " + "、".join(str(item.get("field")) for item in missing), fg="yellow")
    browser = result["browser"]
    click.echo(
        f"Chrome: {'已连接' if browser.get('connected') else '未启动'}，端口 {browser.get('port')}，"
        f"标签页 {browser.get('tabs')}"
    )
    if browser.get("session", {}).get("url"):
        click.echo(f"当前页面: {browser['session']['url']}")
    submissions = result["submissions"]
    click.echo(
        f"今日提交: {submissions['today']['counted']}/{submissions['daily_limit']}，"
        f"剩余 {submissions['daily_remaining']}"
    )
    if submissions.get("interval_remaining_seconds"):
        click.secho(f"提交间隔冷却: 还需 {submissions['interval_remaining_seconds']} 秒", fg="yellow")
    if submissions.get("last_submit_at"):
        click.echo(f"上次提交: {submissions['last_submit_at']}")
    if result.get("recent"):
        click.echo("最近记录:")
        for item in result["recent"]:
            click.echo(f"- [{item.get('status')}] {item.get('company') or ''} {item.get('title') or ''} {item.get('message') or ''}".rstrip())


@cli.command("open")
@click.argument("url", required=False, default="")
@click.option("--new", is_flag=True, help="强制打开新标签页")
@click.option("--front", is_flag=True, help="把标签页切到前台")
@click.pass_context
def open_command(ctx: click.Context, url: str, new: bool, front: bool) -> None:
    """打开或切换到招聘页面。"""
    try:
        result = _client(ctx).open(url, new=new, front=front)
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.secho(f"已打开: {result.get('title') or result.get('url')}", fg="green")
    click.echo(result.get("url") or "")


@cli.command("inspect")
@click.option("--max-text", type=click.IntRange(500, 30000), default=6000, show_default=True, help="最多返回多少页面文字")
@click.option("--front", is_flag=True, help="读取前把标签页切到前台")
@click.pass_context
def inspect_command(ctx: click.Context, max_text: int, front: bool) -> None:
    """读取当前页面的字段、按钮、链接和登录/验证信号。"""
    try:
        result = _client(ctx).inspect(max_text=max_text, front=front)
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    page = result["page"]
    click.echo(f"页面: {page.get('title') or ''}")
    click.echo(page.get("url") or "")
    signals = page.get("signals") or {}
    if result.get("login_required"):
        click.secho("需要登录: " + "、".join(signals.get("login_evidence") or []), fg="yellow")
    if result.get("captcha"):
        click.secho("检测到验证码/人机验证，需要人工处理", fg="yellow")
    click.echo(f"字段 {page.get('field_count', 0)} 个，按钮 {page.get('button_count', 0)} 个")
    for field in page.get("fields") or []:
        options = "、".join(
            str(item.get("label") or item.get("value") or "") for item in (field.get("options") or [])[:8]
        )
        state = "必填" if field.get("required") else "可选"
        click.echo(f"- [{field.get('id')}] {field.get('label') or '(无标签)'} | {field.get('kind')} | {state}")
        if options:
            click.echo(f"  选项: {options}")
    if page.get("buttons"):
        click.echo("按钮: " + " | ".join(str(item.get("text")) for item in page["buttons"][:20]))
    for error in signals.get("validation_errors") or []:
        click.secho(f"页面错误: {error}", fg="red")


@cli.command("plan")
@click.option("--set", "assignments", multiple=True, help="字段赋值 语义或字段ID=值，可重复")
@click.option("--job-title", default="", help="当前应聘岗位名称")
@click.option("--job-company", default="", help="当前应聘公司")
@click.option("--accept-terms", is_flag=True, help="计划中勾选同意/隐私条款")
@click.pass_context
def plan_command(
    ctx: click.Context,
    assignments: tuple[str, ...],
    job_title: str,
    job_company: str,
    accept_terms: bool,
) -> None:
    """预览资料与页面字段的匹配结果，不修改页面。"""
    try:
        result = _client(ctx).plan(
            overrides=_parse_sets(assignments),
            job_title=job_title,
            job_company=job_company,
            accept_terms=accept_terms,
        )
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    summary = result.get("summary") or {}
    click.echo(
        f"匹配: 填写 {summary.get('action_count', 0)} 个，上传 {summary.get('upload_count', 0)} 个，"
        f"未识别必填 {summary.get('unknown_required_count', 0)} 个"
    )
    for action in result.get("actions") or []:
        click.echo(f"- {action.get('label')} ← {action.get('value_preview') or ''}（{action.get('source')}）")
    for upload in result.get("uploads") or []:
        click.echo(f"- 上传 {upload.get('label')}: {upload.get('filename')}")
    if result.get("consent_pending"):
        click.secho("待确认条款: " + "；".join(str(item.get("label")) for item in result["consent_pending"]), fg="yellow")
    for item in result.get("unknown_required") or []:
        click.secho(f"未处理必填: {item.get('label')}（{item.get('reason')}）", fg="yellow")
    for warning in result.get("warnings") or []:
        click.secho(f"提示: {warning}", fg="yellow")


@cli.command("fill")
@click.option("--set", "assignments", multiple=True, help="字段赋值 语义或字段ID=值，可重复")
@click.option("--only", multiple=True, help="只填写指定语义/字段ID/标签，可重复")
@click.option("--job-title", default="", help="当前应聘岗位名称")
@click.option("--job-company", default="", help="当前应聘公司")
@click.option("--accept-terms", is_flag=True, help="勾选同意/隐私条款")
@click.option("--upload", "upload_documents", is_flag=True, help="同时自动上传计划中的附件")
@click.option("--no-overwrite", is_flag=True, help="不覆盖页面已有且不同的内容")
@click.option("--include-correct", is_flag=True, help="页面值已正确时也重新填写")
@click.pass_context
def fill_command(
    ctx: click.Context,
    assignments: tuple[str, ...],
    only: tuple[str, ...],
    job_title: str,
    job_company: str,
    accept_terms: bool,
    upload_documents: bool,
    no_overwrite: bool,
    include_correct: bool,
) -> None:
    """按资料自动填写当前页面字段。"""
    try:
        result = _client(ctx).fill(
            overrides=_parse_sets(assignments),
            job_title=job_title,
            job_company=job_company,
            accept_terms=accept_terms,
            upload_documents=upload_documents,
            overwrite=not no_overwrite,
            skip_correct=not include_correct,
            only=list(only) or None,
        )
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.echo(f"已填写 {result.get('filled_count', 0)} 个，失败 {result.get('failed_count', 0)} 个")
    for item in result.get("filled") or []:
        click.echo(f"- {item.get('label') or item.get('semantic')}: {item.get('value_preview') or item.get('selected') or '完成'}")
    for item in result.get("failed") or []:
        click.secho(f"- 失败 {item.get('label') or item.get('semantic')}: {item.get('error')}", fg="red")
    for item in result.get("uploads") or []:
        click.echo(f"- 上传 {item.get('label')}: {'成功' if item.get('ok') else item.get('error')}")
    for item in result.get("unknown_required") or []:
        click.secho(f"未处理必填: {item.get('label')}（{item.get('reason')}）", fg="yellow")
    for warning in result.get("warnings") or []:
        click.secho(f"提示: {warning}", fg="yellow")


@cli.command("autofill")
@click.option("--set", "assignments", multiple=True, help="字段赋值 语义或字段ID=值，可重复")
@click.option("--job-title", default="", help="当前应聘岗位名称")
@click.option("--job-company", default="", help="当前应聘公司")
@click.option("--accept-terms", is_flag=True, help="确认勾选同意/隐私条款；必须由本人确认后使用")
@click.option("--no-upload", "no_upload", is_flag=True, help="不自动上传简历/照片等附件")
@click.option("--max-steps", type=click.IntRange(1, 20), default=0, help="最多处理几个步骤，默认读取配置")
@click.option("--wait", "wait_seconds", type=float, default=0.0, help="每步等待秒数，默认读取配置")
@click.option("--front", is_flag=True, help="操作前把标签页切到前台")
@click.pass_context
def autofill_command(
    ctx: click.Context,
    assignments: tuple[str, ...],
    job_title: str,
    job_company: str,
    accept_terms: bool,
    no_upload: bool,
    max_steps: int,
    wait_seconds: float,
    front: bool,
) -> None:
    """一键填写当前页面并自动推进多步表单，停在最终提交前。"""
    try:
        result = _client(ctx).autofill(
            overrides=_parse_sets(assignments),
            job_title=job_title,
            job_company=job_company,
            accept_terms=accept_terms,
            upload_documents=not no_upload,
            max_steps=max_steps,
            wait_seconds=wait_seconds,
            front=front,
        )
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    color = "green" if result.get("status") == "ready_to_submit" else (
        "yellow" if result.get("status") in {"terms_required", "already_submitted"} else "red"
    )
    click.secho(f"[{result.get('status')}] {result.get('message') or ''}", fg=color)
    template = result.get("template") or {}
    if template:
        click.echo(f"模板: {template.get('name') or template.get('id')}（{template.get('confidence')}）")
    click.echo(
        f"已填写 {result.get('filled_count', 0)} 个字段，上传 {result.get('uploaded_count', 0)} 个附件，"
        f"处理 {result.get('step_count', 0)} 步"
    )
    for step in result.get("steps") or []:
        click.echo(
            f"- 第 {step.get('index')} 步 [{step.get('status')}] 填写 {step.get('filled_count')}，"
            f"必填未处理 {step.get('unknown_required_count')}，校验错误 {step.get('validation_error_count')}"
        )
    if result.get("final_button"):
        click.echo(f"最终按钮: {result['final_button'].get('text')}（未点击）")
    for item in result.get("unknown_required") or []:
        click.secho(
            f"未处理必填: {item.get('section') or ''} {item.get('label')}（{item.get('reason')}）",
            fg="yellow",
        )
    for item in result.get("consent_pending") or []:
        click.secho(f"待确认条款: {item.get('label')}", fg="yellow")
    for warning in result.get("warnings") or []:
        click.secho(f"提示: {warning}", fg="yellow")


@cli.command("templates")
@click.pass_context
def templates_command(ctx: click.Context) -> None:
    """列出内置网申模板与检测规则。"""
    try:
        result = _client(ctx).templates()
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.echo(f"共 {result.get('count', 0)} 个模板")
    for item in result.get("templates") or []:
        click.echo(f"- {item.get('id')}: {item.get('name')}（{item.get('vendor') or '通用'}）")
        if item.get("notes"):
            click.echo(f"  {item['notes']}")


@cli.command("upload")
@click.option("--field", "field_id", default="", help="文件输入框或上传按钮的 data-oa-id")
@click.option("--role", default="", help="材料角色：resume/photo/transcript 等")
@click.option("--file", "files", multiple=True, type=click.Path(dir_okay=False, exists=True, path_type=str))
@click.option("--all", "upload_all", is_flag=True, help="同角色有多个上传框时全部上传")
@click.pass_context
def upload_command(ctx: click.Context, field_id: str, role: str, files: tuple[str, ...], upload_all: bool) -> None:
    """上传简历/照片/成绩单等材料。"""
    try:
        result = _client(ctx).upload(
            field_id=field_id,
            role=role,
            paths=list(files) or None,
            upload_all=upload_all,
        )
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.echo(f"上传成功 {result.get('uploaded_count', 0)} 个")
    for item in result.get("results") or []:
        click.echo(f"- {item.get('label') or item.get('role') or ''}: {'成功' if item.get('ok') else item.get('error')}")


@cli.command("click")
@click.option("--text", default="", help="按钮文字，如 下一步")
@click.option("--id", "field_id", default="", help="按钮的 data-oa-id")
@click.option("--wait", type=float, default=0.6, show_default=True, help="点击后等待秒数")
@click.pass_context
def click_command(ctx: click.Context, text: str, field_id: str, wait: float) -> None:
    """点击页面按钮或链接（用于下一步/保存/进入表单）。"""
    try:
        result = _client(ctx).click(text=text, field_id=field_id, wait=wait)
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.secho(f"已点击: {result.get('clicked')}", fg="green")
    click.echo(result.get("url") or "")


@cli.command("submit")
@click.option("--button", default="", help="指定提交按钮文字")
@click.option("--accept-terms", is_flag=True, help="确认勾选同意/隐私条款；必须由本人确认后使用")
@click.option("--force", is_flag=True, help="忽略相同链接的已投递记录")
@click.option("--title", "job_title", default="", help="岗位名称，缺省读取页面结构化数据")
@click.option("--company", "job_company", default="", help="公司名称")
@click.option("--note", "notes", default="", help="备注")
@click.option("--dry-run", "dry_run", is_flag=True, help="只预检提交条件，不点击、不记录")
@click.pass_context
def submit_command(
    ctx: click.Context,
    button: str,
    accept_terms: bool,
    force: bool,
    job_title: str,
    job_company: str,
    notes: str,
    dry_run: bool,
) -> None:
    """提交当前已经填写完成的官网申请。"""
    try:
        result = _client(ctx).submit(
            button=button,
            accept_terms=accept_terms,
            force=force,
            job_title=job_title,
            job_company=job_company,
            notes=notes,
            dry_run=dry_run,
        )
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    if result.get("dry_run"):
        click.secho("预检通过，未提交", fg="green")
        click.echo(f"提交按钮: {(result.get('submit_button') or {}).get('text') or ''}")
        return
    status = result.get("status")
    color = "green" if status == "submitted" else ("red" if status == "failed" else "yellow")
    click.secho(f"[{status}] {result.get('message') or ''}", fg=color)
    click.echo(f"{result.get('company') or ''} | {result.get('title') or ''}")
    if result.get("screenshot"):
        click.echo(f"截图: {result['screenshot']}")
    for error in result.get("validation_errors") or []:
        click.secho(f"页面错误: {error}", fg="red")
    for warning in result.get("warnings") or []:
        click.secho(f"提示: {warning}", fg="yellow")


@cli.command("screenshot")
@click.argument("name", required=False, default="")
@click.pass_context
def screenshot_command(ctx: click.Context, name: str) -> None:
    """保存当前页面截图。"""
    try:
        result = _client(ctx).screenshot(name)
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.echo(result.get("path") or "")


@cli.command("history")
@click.option("--limit", type=click.IntRange(1, 500), default=20, show_default=True)
@click.option("--status", type=click.Choice(["submitted", "unconfirmed", "failed"]), default="")
@click.pass_context
def history_command(ctx: click.Context, limit: int, status: str) -> None:
    """查看官网投递记录。"""
    try:
        result = _client(ctx).history(limit=limit, status=status)
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.echo(f"今日: 成功 {result['today']['submitted']}，待确认 {result['today']['unconfirmed']}，失败 {result['today']['failed']}")
    for item in result.get("items") or []:
        click.echo(f"- [{item.get('status')}] {item.get('at')} {item.get('company') or ''} {item.get('title') or ''}")
        click.echo(f"  {item.get('url') or ''}")


@cli.command("login")
@click.argument("url", required=False, default="")
@click.pass_context
def login_command(ctx: click.Context, url: str) -> None:
    """打开登录页，由本人在 Chrome 中完成登录/验证码。"""
    try:
        result = _client(ctx).login(url)
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.echo(result.get("message") or "")
    click.echo(f"页面: {result.get('url')}")
    click.echo(f"专用配置目录: {result.get('profile_dir')}")


@cli.command("check-login")
@click.argument("url", required=False, default="")
@click.pass_context
def check_login_command(ctx: click.Context, url: str) -> None:
    """检查当前招聘页面是否仍需要登录。"""
    try:
        result = _client(ctx).check_login(url)
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    if result.get("login_required"):
        click.secho("当前页面需要登录或登录已失效", fg="yellow")
        evidence = (result.get("signals") or {}).get("login_evidence") or []
        if evidence:
            click.echo("依据: " + "、".join(str(item) for item in evidence))
    else:
        click.secho("当前页面未检测到登录要求", fg="green")
    if result.get("title"):
        click.echo(f"页面: {result['title']} {result.get('url') or ''}")


@cli.command("otp-serve")
@click.option("--bind-ip", default="", help="监听绑定的电脑 IP；不填使用配置（auto 只绑定热点/局域网网卡）")
@click.option("--host-ip", default="", help="生成手机访问地址时使用的电脑 IP；不填则跟随绑定地址")
@click.option("--port", type=click.IntRange(0, 65535), default=0, help="监听端口；0 表示读取配置")
@click.option("--token", default="", help="自定义 token；默认读取/生成配置中的 token_file")
@click.option("--rotate-token", is_flag=True, help="立即生成新 token，旧 token 作废")
@click.option("--ttl", type=click.IntRange(0, 600), default=0, help="验证码内存有效期；0 表示读取配置")
@click.pass_context
def otp_serve_command(
    ctx: click.Context,
    bind_ip: str,
    host_ip: str,
    port: int,
    token: str,
    rotate_token: bool,
    ttl: int,
) -> None:
    """启动本地短信验证码接收端，供 SmsForwarder 的 Webhook 调用。"""
    import ipaddress

    from officialapply.otp import (
        OtpInbox,
        OtpReceiver,
        detect_bind_address,
        load_or_create_token,
        mask_url_token,
        receiver_urls,
        write_runtime_file,
        write_url_file,
    )

    try:
        config = load_config(ctx.obj["config_path"])
        otp_config = config["otp"]
        token_path = resolve_path(str(otp_config["token_file"]), config)
        url_path = resolve_path(str(otp_config["url_file"]), config)
        runtime_path = resolve_path(str(otp_config["runtime_file"]), config)

        requested_bind = bind_ip.strip() or str(otp_config["bind_ip"]).strip()
        detected_name = ""
        if requested_bind.lower() == "auto":
            detected = detect_bind_address()
            if detected:
                detected_name, requested_bind = detected
            else:
                requested_bind = "127.0.0.1"
        try:
            bind_address = ipaddress.ip_address(requested_bind)
        except ValueError as exc:
            raise ToolError("otp_bind_invalid", f"无效的绑定 IP: {requested_bind}") from exc
        if bind_address.is_unspecified:
            click.secho("警告: 绑定 0.0.0.0 会监听所有网卡，建议使用 auto 或热点 IP。", fg="yellow", err=True)

        token = token.strip() or load_or_create_token(token_path, rotate=rotate_token)
        inbox = OtpInbox(ttl_seconds=ttl or float(otp_config["ttl_seconds"]))
        receiver = OtpReceiver(
            inbox,
            token,
            host=str(bind_address),
            port=port or int(otp_config["port"]),
            max_clock_skew_seconds=int(otp_config["max_clock_skew_seconds"]),
            max_requests_per_minute=int(otp_config["max_requests_per_minute"]),
        )
        receiver.start()
        write_runtime_file(runtime_path, receiver.host, receiver.actual_port)
    except Exception as exc:
        _handle_error(ctx, exc)
        return

    logging.basicConfig(level=logging.INFO, format="[%(asctime)s] %(message)s", datefmt="%H:%M:%S")
    display_ip = host_ip.strip() or str(bind_address)
    if display_ip in ("0.0.0.0", "::"):
        display_ip = ""
    urls = receiver_urls(token, receiver.actual_port, host_ip=display_ip)
    if bind_address.is_loopback:
        click.secho(
            "警告: 接收端只监听本机回环地址，手机无法访问；请检查热点连接或使用 --bind-ip 指定网卡。",
            fg="yellow",
            err=True,
        )
    try:
        write_url_file(url_path, urls)
    except OSError as exc:
        click.secho(f"警告: 无法写入完整地址文件 {url_path}: {exc}", fg="yellow", err=True)
    masked_urls = [{"adapter": name, "url": mask_url_token(url)} for name, url in urls]
    adapter_label = detected_name or (urls[0][0] if urls else display_ip or "未知")
    payload = {
        "success": True,
        "listen": f"{receiver.host}:{receiver.actual_port}",
        "adapter": adapter_label,
        "token_file": str(token_path),
        "url_file": str(url_path),
        "runtime_file": str(runtime_path),
        "ttl_seconds": inbox.ttl_seconds,
        "max_requests_per_minute": receiver.max_requests_per_minute,
        "urls": masked_urls,
    }
    if ctx.obj["json"]:
        _echo_json(payload)
    else:
        click.secho("OTP 接收端已启动", fg="green")
        click.echo(f"监听: {receiver.host}:{receiver.actual_port}（仅绑定该网卡）")
        click.echo("手机访问地址（已隐藏 token）:")
        for item in masked_urls:
            click.echo(f"- {item['adapter']}: {item['url']}")
        click.echo(f"完整地址文件: {url_path}（仅当前 Windows 用户可读，勿分享）")
        click.echo(f"token 文件: {token_path}")
        click.echo('先用手机浏览器打开完整地址，看到 {"ok": true} 说明网络通了。')
        click.echo("按 Ctrl+C 停止。")

    seen = 0.0
    try:
        while True:
            time.sleep(0.5)
            item = inbox.latest(since=seen)
            if item is None:
                continue
            seen = item.received_at
            click.secho(
                f"[OTP] 收到验证码 来源={item.sender or '未知'} 时间={item.received_at_text}",
                fg="green",
            )
            click.echo("  验证码已存入内存，不会打印到日志或聊天。")
    except KeyboardInterrupt:
        click.echo("正在停止 OTP 接收端...")
    finally:
        receiver.stop()


@cli.command("otp-fill")
@click.option("--send/--no-send", default=True, help="本地无近期验证码时是否点击获取验证码，默认是")
@click.option("--send-button", default="", help="指定发送验证码按钮文字")
@click.option("--field-id", default="", help="指定验证码输入框的 data-oa-id")
@click.option("--submit/--no-submit", default=True, help="填入成功后是否点击登录按钮，默认是")
@click.option("--submit-button", default="", help="指定登录按钮文字")
@click.option("--timeout", type=click.FloatRange(5, 600), default=120, show_default=True, help="等待新验证码的秒数")
@click.option("--reuse-existing/--no-reuse-existing", default=True, help="复用本地近期验证码，避免重复发送，默认是")
@click.pass_context
def otp_fill_command(
    ctx: click.Context,
    send: bool,
    send_button: str,
    field_id: str,
    submit: bool,
    submit_button: str,
    timeout: float,
    reuse_existing: bool,
) -> None:
    """从本地短信接收端取验证码并填入当前页面；验证码不会输出到终端。"""
    try:
        result = _client(ctx).otp_fill(
            send=send,
            send_button=send_button,
            field_id=field_id,
            submit=submit,
            submit_button=submit_button,
            timeout=timeout,
            reuse_existing=reuse_existing,
        )
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    if result.get("verified"):
        click.secho("验证码已填入并校验通过", fg="green")
    else:
        click.secho("验证码填写未完全确认，请检查输入框", fg="yellow")
    click.echo(f"输入框: {result.get('filled_count')}/{result.get('field_count')}")
    if result.get("send_clicked"):
        click.echo(f"已点击: {result.get('send_clicked')}")
    if result.get("used_existing"):
        click.echo("使用本地已接收的近期验证码，未重复发送短信")
    if result.get("submitted"):
        click.echo(f"已点击登录: {result.get('submit_button')}")
    click.echo(f"页面: {result.get('title') or ''} {result.get('url') or ''}")
    if result.get("login_required"):
        click.echo("页面仍要求登录，请人工检查验证码或完成其他验证。")
    for warning in result.get("warnings") or []:
        click.secho(f"提示: {warning}", fg="yellow")


@cli.command("mcp")
def mcp_command() -> None:
    """以 MCP stdio 服务运行，供 Agent 直接调用。"""
    from officialapply.mcp_server import serve

    serve()


if __name__ == "__main__":
    cli()
