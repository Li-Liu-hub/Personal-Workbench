"""Command line interface for BossHunter."""

from __future__ import annotations

import json
import time
from pathlib import Path
from typing import Any

import click

from bosshunter import __version__
from bosshunter.api import BossHunter
from bosshunter.boss.search import BOSS_FILTER_PARAMS
from bosshunter.config import ConfigError
from bosshunter.errors import ToolError, error_payload
from bosshunter.queue import STATUSES


def _echo_json(payload: Any) -> None:
    click.echo(json.dumps(payload, ensure_ascii=False, indent=2))


def _handle_error(ctx: click.Context, exc: Exception) -> None:
    payload = error_payload(exc)
    if isinstance(exc, ConfigError):
        payload = {"code": "config_error", "message": str(exc)}
    result = {"success": False, "error": payload}
    if ctx.obj["json"]:
        _echo_json(result)
    else:
        click.secho(f"错误 [{payload['code']}]: {payload['message']}", fg="red", err=True)
        extra = {key: value for key, value in payload.items() if key not in {"code", "message"}}
        if extra:
            click.echo(json.dumps(extra, ensure_ascii=False, indent=2), err=True)
    ctx.exit(1)


def _client(ctx: click.Context) -> BossHunter:
    return BossHunter(ctx.obj["config_path"])


def _parse_filters(values: tuple[str, ...]) -> dict[str, list[str]]:
    filters: dict[str, list[str]] = {}
    for item in values:
        key, separator, value = item.partition("=")
        key = key.strip()
        if not separator or not key or not value.strip():
            raise ToolError("invalid_filter", f"筛选条件格式必须为 键=值，收到: {item}")
        if key not in BOSS_FILTER_PARAMS:
            allowed = "、".join(BOSS_FILTER_PARAMS)
            raise ToolError("invalid_filter", f"未知筛选键 {key}，可用: {allowed}")
        filters.setdefault(key, []).extend(
            part.strip() for part in value.replace("，", ",").split(",") if part.strip()
        )
    return filters


@click.group(context_settings={"help_option_names": ["-h", "--help"]})
@click.option(
    "--config",
    "config_path",
    type=click.Path(dir_okay=False, path_type=str),
    default=None,
    help="配置文件路径，默认读取当前目录 config.yaml",
)
@click.option("--json", "as_json", is_flag=True, help="以 JSON 输出，便于程序或 Agent 解析")
@click.version_option(__version__, prog_name="bosshunter")
@click.pass_context
def cli(ctx: click.Context, config_path: str | None, as_json: bool) -> None:
    """BOSS直聘岗位搜索与投递工具。"""
    ctx.obj = {"config_path": config_path, "json": as_json}


@cli.command("search")
@click.option("--keyword", "-k", "keywords", multiple=True, help="搜索关键词，可重复；默认读取配置")
@click.option("--city", "-c", "cities", multiple=True, help="城市名称，可重复；默认读取配置")
@click.option("--max-pages", type=click.IntRange(1, 50), default=None, help="每个搜索组合最多读取多少页")
@click.option(
    "--filter",
    "filter_values",
    multiple=True,
    help="筛选条件 键=值，可重复，例如 --filter 经验=1-3年 --filter 学历=本科",
)
@click.option("--sort", type=click.Choice(["default", "newest"]), default=None)
@click.option("--details", "include_details", is_flag=True, help="同时抓取岗位详情（JD、HR 等）")
@click.option("--max-details", type=click.IntRange(1, 500), default=60, show_default=True)
@click.pass_context
def search_command(
    ctx: click.Context,
    keywords: tuple[str, ...],
    cities: tuple[str, ...],
    max_pages: int | None,
    filter_values: tuple[str, ...],
    sort: str | None,
    include_details: bool,
    max_details: int,
) -> None:
    """搜索 BOSS 岗位列表。"""
    try:
        client = _client(ctx)
        result = client.search(
            keywords=list(keywords) or None,
            cities=list(cities) or None,
            max_pages=max_pages,
            filters=_parse_filters(filter_values),
            sort=sort,
            include_details=include_details,
            max_details=max_details,
        )
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.echo(f"共读取到 {result['count']} 个岗位")
    for job in result["jobs"]:
        salary = job.get("salary") or ("薪资未解析" if job.get("salary_unreadable") else "")
        header = " | ".join(
            part for part in [
                str(job.get("title") or ""),
                str(job.get("company") or ""),
                salary,
                str(job.get("location") or job.get("city") or ""),
                str(job.get("experience") or ""),
                str(job.get("education") or ""),
            ] if part
        )
        click.echo(f"- {header}")
        click.echo(f"  {job.get('url') or ''}")
        if job.get("job_id") and not job.get("url"):
            click.echo(f"  job_id: {job['job_id']}")
        if job.get("jd"):
            summary = " ".join(str(job["jd"]).split())
            click.echo(f"  JD: {summary[:160]}{'...' if len(summary) > 160 else ''}")
    for warning in result.get("warnings") or []:
        click.secho(f"提示: {warning}", fg="yellow")


@cli.command("detail")
@click.argument("url")
@click.pass_context
def detail_command(ctx: click.Context, url: str) -> None:
    """读取单个岗位详情。"""
    try:
        result = _client(ctx).detail(url)
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    job = result["job"]
    click.echo(f"{job.get('title') or ''} | {job.get('company') or ''} | {job.get('salary') or ''}")
    click.echo(
        " | ".join(
            part for part in [
                str(job.get("experience") or ""),
                str(job.get("education") or ""),
                str(job.get("company_size") or ""),
                str(job.get("company_industry") or ""),
            ] if part
        )
    )
    click.echo(job.get("url") or url)
    if job.get("hr_name"):
        click.echo(f"招聘者: {job.get('hr_name')} {job.get('hr_title') or ''}")
    click.echo("")
    click.echo(job.get("jd") or "（未读取到 JD）")
    if job.get("salary_unreadable"):
        click.secho("提示: 薪资字体未能解析", fg="yellow")


@cli.command("deliver")
@click.argument("url")
@click.option("--greeting", default=None, help="文字消息；未提供时使用配置 delivery.greeting")
@click.option("--image", "images", multiple=True, help="随投递发送的图片路径，可重复")
@click.option("--no-greeting", is_flag=True, help="不发送文字消息")
@click.option("--no-images", is_flag=True, help="不发送图片")
@click.option("--force", is_flag=True, help="忽略已投递记录与连续失败计数")
@click.option("--dry-run", "dry_run", is_flag=True, help="只读取岗位和沟通按钮，不点击不发送")
@click.option("--wait", is_flag=True, help="若处于发送冷却期，最多等待 10 分钟")
@click.pass_context
def deliver_command(
    ctx: click.Context,
    url: str,
    greeting: str | None,
    images: tuple[str, ...],
    no_greeting: bool,
    no_images: bool,
    force: bool,
    dry_run: bool,
    wait: bool,
) -> None:
    """向岗位发起沟通并发送文字/图片。"""
    try:
        result = _client(ctx).apply(
            url,
            greeting=greeting,
            images=list(images) or None,
            send_greeting=not no_greeting,
            send_images=not no_images,
            force=force,
            dry_run=dry_run,
            wait=wait,
        )
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    if result.get("skipped"):
        click.secho(f"跳过: {result.get('message') or result.get('reason')}", fg="yellow")
        return
    if result.get("dry_run"):
        job = result.get("job") or {}
        click.echo(f"dry-run: {job.get('title') or ''} | {job.get('company') or ''}")
        click.echo(f"沟通按钮: {'可用' if (result.get('chat_button') or {}).get('success') else '未找到'}")
        click.echo(f"将发送文字: {'是' if result.get('would_send_greeting') else '否'}")
        click.echo(f"将发送图片: {result.get('would_send_images') or 0} 张")
        return
    status = "部分发送" if result.get("partial") else "已发送"
    click.secho(
        f"{status}: {result.get('title') or ''} | {result.get('company') or ''}",
        fg="yellow" if result.get("partial") else "green",
    )
    greeting_info = result.get("greeting") or {}
    if greeting_info.get("requested"):
        click.echo(f"文字消息: {greeting_info.get('state')} {greeting_info.get('message') or ''}".rstrip())
    image_info = result.get("images") or {}
    if image_info.get("requested"):
        click.echo(f"图片: 已发送 {image_info.get('sent') or 0}/{image_info.get('requested')} 张")
        if image_info.get("error"):
            click.secho(
                f"图片错误: {image_info['error'].get('message') or image_info['error'].get('error')}",
                fg="yellow",
            )
    for warning in result.get("warnings") or []:
        click.secho(f"提示: {warning}", fg="yellow")


@cli.command("send-images")
@click.argument("url")
@click.option("--image", "images", multiple=True, help="要发送的图片路径，可重复")
@click.option("--message", default=None, help="同时发送一条文字消息")
@click.option("--force/--no-force", default=True, show_default=True, help="忽略已投递记录后直接发送图片")
@click.option("--wait", is_flag=True, help="若处于发送冷却期，最多等待 10 分钟")
@click.pass_context
def send_images_command(
    ctx: click.Context,
    url: str,
    images: tuple[str, ...],
    message: str | None,
    force: bool,
    wait: bool,
) -> None:
    """向已有会话补发图片。"""
    try:
        result = _client(ctx).send_images(
            url,
            images=list(images) or None,
            message=message,
            force=force,
            wait=wait,
        )
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    image_info = result.get("images") or {}
    click.echo(f"图片: 已发送 {image_info.get('sent') or 0}/{image_info.get('requested') or 0} 张")
    if image_info.get("error"):
        click.secho(
            f"图片错误: {image_info['error'].get('message') or image_info['error'].get('error')}",
            fg="red" if not image_info.get("sent") else "yellow",
        )
    for warning in result.get("warnings") or []:
        click.secho(f"提示: {warning}", fg="yellow")


def _queue_event_printer(event: dict[str, Any]) -> None:
    kind = event.get("event")
    stamp = event.get("at") or ""
    title = event.get("title") or event.get("job_id") or ""
    if kind == "run_started":
        click.echo(f"[{stamp}] 开始执行投递队列，待投递 {event.get('pending')} 个")
    elif kind == "item_started":
        click.echo(f"[{stamp}] 投递: {title}")
    elif kind == "item_sent":
        click.secho(f"[{stamp}] {'部分发送' if event.get('status') == 'partial' else '已发送'}: {title}", fg="green")
    elif kind == "item_ambiguous":
        click.secho(f"[{stamp}] 待确认: {title} | {event.get('message') or ''}", fg="yellow")
    elif kind == "item_failed":
        click.secho(f"[{stamp}] 失败: {title} | {event.get('message') or ''}", fg="red")
    elif kind == "item_skipped":
        click.echo(f"[{stamp}] 跳过: {title} | {event.get('message') or ''}")
    elif kind == "item_dry_run":
        click.echo(f"[{stamp}] 预检: {title} | 沟通按钮{'可用' if event.get('button') else '未找到'}")
    elif kind == "waiting_window":
        click.echo(f"[{stamp}] 等待发送窗口，约 {event.get('seconds')} 秒")
    elif kind == "waiting_cooldown":
        click.echo(f"[{stamp}] 安全冷却，等待 {event.get('seconds')} 秒")
    elif kind == "run_stopping":
        click.secho(f"[{stamp}] 停止: {event.get('reason')} | {event.get('message') or ''}", fg="yellow")
    elif kind == "run_finished":
        click.echo(
            f"[{stamp}] 队列执行结束: {event.get('reason')} | 尝试 {event.get('attempted')}，"
            f"成功 {event.get('sent')}，部分 {event.get('partial')}，失败 {event.get('failed')}，"
            f"跳过 {event.get('skipped')}，剩余 {event.get('pending')}"
        )


@cli.group("queue")
def queue_group() -> None:
    """投递队列管理：候选岗位增删改、批量投递。"""


@queue_group.command("list")
@click.option("--status", type=click.Choice(list(STATUSES)), default=None, help="只显示指定状态")
@click.option("--limit", type=click.IntRange(0, 5000), default=30, show_default=True)
@click.pass_context
def queue_list_command(ctx: click.Context, status: str | None, limit: int) -> None:
    """查看投递队列。"""
    try:
        result = _client(ctx).queue_list(status=status, limit=limit)
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    summary = result["queue"]
    counts = summary.get("by_status") or {}
    click.echo(
        f"队列: 总计 {summary['total']} | 待投递 {summary['pending']} | 已发送 {counts.get('sent', 0)} | "
        f"失败 {counts.get('failed', 0)} | 待确认 {counts.get('ambiguous', 0)}"
    )
    click.echo(f"文件: {summary['path']}")
    for item in result["items"]:
        header = " | ".join(
            part
            for part in [
                item.get("title") or "",
                item.get("company") or "",
                item.get("city") or "",
                item.get("salary") or "",
                f"P{item.get('priority')}" if item.get("priority") else "",
            ]
            if part
        )
        click.echo(f"- [{item['status']}] {header}")
        if item.get("note"):
            click.echo(f"  {item['note']}")


@queue_group.command("add")
@click.argument("url")
@click.option("--title", default="", help="岗位名称")
@click.option("--company", default="", help="公司名称")
@click.option("--city", default="", help="城市")
@click.option("--salary", default="", help="薪资")
@click.option("--experience", default="", help="经验要求")
@click.option("--education", default="", help="学历要求")
@click.option("--priority", type=int, default=0, show_default=True, help="优先级，越大越先投")
@click.option("--greeting", default=None, help="该岗位专用招呼语；默认使用配置")
@click.option("--image", "images", multiple=True, help="该岗位专用图片；默认使用配置")
@click.option("--note", default="", help="备注")
@click.pass_context
def queue_add_command(
    ctx: click.Context,
    url: str,
    title: str,
    company: str,
    city: str,
    salary: str,
    experience: str,
    education: str,
    priority: int,
    greeting: str | None,
    images: tuple[str, ...],
    note: str,
) -> None:
    """向队列添加一个岗位。"""
    job = {
        "url": url,
        "title": title,
        "company": company,
        "city": city,
        "salary": salary,
        "experience": experience,
        "education": education,
        "note": note,
    }
    try:
        result = _client(ctx).queue_add(
            [job],
            priority=priority,
            greeting=greeting,
            images=list(images) or None,
        )
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    if result["added"]:
        click.secho(f"已加入队列: {url}", fg="green")
    elif result["duplicates"]:
        click.secho("岗位已在队列中", fg="yellow")
    else:
        click.secho("未能加入队列，请检查链接", fg="red")
    click.echo(f"待投递: {result['summary']['pending']} / 总计 {result['summary']['total']}")


@queue_group.command("import")
@click.argument("path", type=click.Path(dir_okay=False, exists=True, path_type=str))
@click.option("--priority", type=int, default=0, show_default=True)
@click.option("--greeting", default=None, help="为导入岗位统一设置招呼语")
@click.option("--image", "images", multiple=True, help="为导入岗位统一设置图片")
@click.option("--replace", is_flag=True, help="已存在时用导入数据覆盖元数据")
@click.option("--source", default="", help="来源标记")
@click.pass_context
def queue_import_command(
    ctx: click.Context,
    path: str,
    priority: int,
    greeting: str | None,
    images: tuple[str, ...],
    replace: bool,
    source: str,
) -> None:
    """从 JSON 文件导入岗位数组（支持 [..] 或 {"jobs": [..]}）。"""
    try:
        raw = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        _handle_error(ctx, ToolError("invalid_queue_file", f"无法读取岗位文件 {path}: {exc}"))
        return
    if isinstance(raw, dict):
        raw = raw.get("jobs")
    if not isinstance(raw, list):
        _handle_error(ctx, ToolError("invalid_queue_file", "岗位文件必须是数组或包含 jobs 数组的对象"))
        return
    try:
        result = _client(ctx).queue_add(
            raw,
            priority=priority,
            greeting=greeting,
            images=list(images) or None,
            replace=replace,
            source=source,
        )
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.echo(
        f"导入完成: 新增 {result['added']}，更新 {result['updated']}，"
        f"重复 {result['duplicates']}，无效 {result['invalid']}"
    )
    click.echo(f"待投递: {result['summary']['pending']} / 总计 {result['summary']['total']}")
    for item in result["invalid_items"][:10]:
        click.secho(f"无效岗位: {item.get('url') or ''} {item.get('error') or ''}", fg="yellow")


@queue_group.command("update")
@click.argument("job_id")
@click.option("--status", type=click.Choice(list(STATUSES)), default=None)
@click.option("--priority", type=int, default=None)
@click.option("--greeting", default=None)
@click.option("--image", "images", multiple=True)
@click.option("--note", default=None)
@click.pass_context
def queue_update_command(
    ctx: click.Context,
    job_id: str,
    status: str | None,
    priority: int | None,
    greeting: str | None,
    images: tuple[str, ...],
    note: str | None,
) -> None:
    """修改队列中的岗位。"""
    updates: dict[str, Any] = {}
    if status is not None:
        updates["status"] = status
    if priority is not None:
        updates["priority"] = priority
    if greeting is not None:
        updates["greeting"] = greeting
    if images:
        updates["images"] = list(images)
    if note is not None:
        updates["note"] = note
    if not updates:
        _handle_error(ctx, ToolError("invalid_arguments", "至少提供一个要修改的字段"))
        return
    try:
        result = _client(ctx).queue_update(job_id, updates)
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    item = result["item"]
    click.secho(f"已更新 [{item['status']}] {item.get('title') or item['job_id']}", fg="green")
    click.echo(f"待投递: {result['queue']['pending']} / 总计 {result['queue']['total']}")


@queue_group.command("remove")
@click.argument("job_ids", nargs=-1, required=True)
@click.pass_context
def queue_remove_command(ctx: click.Context, job_ids: tuple[str, ...]) -> None:
    """从队列移除岗位。"""
    try:
        result = _client(ctx).queue_remove(list(job_ids))
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.echo(f"已移除 {result['removed']} 个岗位")
    click.echo(f"待投递: {result['summary']['pending']} / 总计 {result['summary']['total']}")


@queue_group.command("reset")
@click.option(
    "--status",
    "statuses",
    type=click.Choice(list(STATUSES)),
    multiple=True,
    default=("failed",),
    show_default=True,
    help="把这些状态的岗位重置为待投递",
)
@click.pass_context
def queue_reset_command(ctx: click.Context, statuses: tuple[str, ...]) -> None:
    """重置失败或其他状态的岗位，使其重新进入待投递。"""
    try:
        result = _client(ctx).queue_reset(list(statuses))
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.echo(f"已重置 {result['reset']} 个岗位")
    click.echo(f"待投递: {result['summary']['pending']} / 总计 {result['summary']['total']}")


@queue_group.command("run")
@click.option("--limit", type=click.IntRange(1, 10000), default=None, help="本次最多尝试多少个岗位")
@click.option("--wait-window", is_flag=True, help="在发送窗口外时最多等待 60 分钟")
@click.option("--dry-run", "dry_run", is_flag=True, help="只预检第一个岗位，不发送")
@click.option("--force", is_flag=True, help="忽略已投递记录与连续失败计数")
@click.option("--max-minutes", type=click.IntRange(1, 1440), default=None, help="本次运行最多持续多少分钟")
@click.option("--retry-failed", is_flag=True, help="先把 failed 岗位重置为待投递")
@click.pass_context
def queue_run_command(
    ctx: click.Context,
    limit: int | None,
    wait_window: bool,
    dry_run: bool,
    force: bool,
    max_minutes: int | None,
    retry_failed: bool,
) -> None:
    """批量投递队列中的岗位。"""
    printer = None if ctx.obj["json"] else _queue_event_printer
    try:
        result = _client(ctx).queue_run(
            limit=limit,
            wait_window=wait_window,
            dry_run=dry_run,
            force=force,
            max_minutes=max_minutes,
            retry_failed=retry_failed,
            on_event=printer,
        )
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.echo(
        f"执行结束: {result['reason']} | 尝试 {result['attempted']}，成功 {result['sent']}，"
        f"部分 {result['partial']}，失败 {result['failed']}，待确认 {result['ambiguous']}，"
        f"跳过 {result['skipped']}"
    )
    click.echo(f"剩余待投递: {result['queue']['pending']} / 总计 {result['queue']['total']}")


@cli.command("status")
@click.pass_context
def status_command(ctx: click.Context) -> None:
    """查看本地安全状态、投递记录与配置摘要。"""
    try:
        result = _client(ctx).status()
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    state = result["state"]
    click.echo(f"日期: {state['date']}")
    click.echo(
        f"页面: 搜索 {state['pages']['search']} / 详情 {state['pages']['detail']} / 聊天 {state['pages']['chat']}"
        f"（总计 {state['page_total']}/{state['page_limit']}）"
    )
    click.echo(f"投递: {state['sends']}/{state['send_limit']}，今日成功 {state['delivered_today']}")
    window = "开放" if state["send_window_active"] else f"关闭（下次 {state['next_window']}）"
    click.echo(f"发送窗口: {'、'.join(state['send_windows'])}，当前{window}")
    click.echo(f"连续失败: {state['consecutive_failures']}")
    lock = state.get("lock") or {}
    if lock:
        remaining = max(0, int(float(lock.get("until") or 0) - time.time()))
        click.secho(f"风险锁定: {lock.get('reason') or ''}，剩余 {remaining} 秒", fg="yellow")
    if state.get("last_send_at"):
        click.echo(f"上次投递: {time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(state['last_send_at']))}")
    queue = result.get("queue") or {}
    if queue and "error" not in queue:
        counts = queue.get("by_status") or {}
        click.echo(
            f"投递队列: 待投递 {queue.get('pending', 0)} / 总计 {queue.get('total', 0)}"
            f"（已发送 {counts.get('sent', 0)}，失败 {counts.get('failed', 0)}）"
        )
    elif queue.get("error"):
        click.secho(f"投递队列不可读: {queue['error'].get('message') or ''}", fg="yellow")
    config = result["config"]
    click.echo(f"配置: {config['path']}")
    click.echo(f"关键词: {'、'.join(config['search']['keywords']) or '（未配置）'}")
    click.echo(f"城市: {'、'.join(config['search']['cities']) or '（未配置）'}")
    if config["delivery"]["images"]:
        click.echo(f"默认图片: {len(config['delivery']['images'])} 张")
    if result["recent_deliveries"]:
        click.echo("最近投递:")
        for record in result["recent_deliveries"]:
            label = f"{record.get('title') or ''} | {record.get('company') or ''}".strip(" |")
            click.echo(f"- [{record.get('status')}] {label} {record.get('note') or ''}".rstrip())


@cli.command("login")
@click.pass_context
def login_command(ctx: click.Context) -> None:
    """打开专用 Chrome 的 BOSS 登录页。"""
    try:
        result = _client(ctx).login()
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    click.echo(result.get("message") or "请在弹出的 Chrome 窗口中登录")
    click.echo(f"登录页: {result.get('url')}")
    click.echo(f"专用配置目录: {result.get('profile_dir') or ''}")


@cli.command("check-login")
@click.pass_context
def check_login_command(ctx: click.Context) -> None:
    """检查 BOSS 登录状态。"""
    try:
        result = _client(ctx).check_login()
    except Exception as exc:
        _handle_error(ctx, exc)
        return
    if ctx.obj["json"]:
        _echo_json(result)
        return
    if result.get("logged_in"):
        click.secho("BOSS 登录状态有效", fg="green")
    else:
        click.secho("BOSS 未登录或登录已失效，请运行 bosshunter login", fg="red")
    if result.get("profile_dir"):
        click.echo(f"专用配置目录: {result['profile_dir']}")


@cli.command("mcp")
def mcp_command() -> None:
    """以 MCP stdio 服务运行，供 Agent 直接调用。"""
    from bosshunter.mcp_server import serve

    serve()


if __name__ == "__main__":
    cli()
