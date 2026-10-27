"""High-level service used by the CLI and MCP server."""

from __future__ import annotations

import re
import time
from datetime import datetime
from pathlib import Path
from typing import Any

from officialapply.browser import Browser, BrowserError, CDPError
from officialapply.config import load_config
from officialapply.errors import ToolError
from officialapply.forms import (
    click_oa,
    click_text,
    fill_fields,
    find_otp_fields,
    inspect_page,
    otp_fill_actions,
    select_custom_option,
    type_otp_fields,
    upload_to_field,
    upload_via_button,
    verify_otp_fill,
)
from officialapply.mapping import (
    SENSITIVE_SEMANTICS,
    detect_document_role,
    plan_mapping,
    public_plan,
)
from officialapply.otp import ensure_otp_client
from officialapply.profile import load_profile, mask_email, mask_id, mask_phone
from officialapply.registry import Registry
from officialapply.templates import detect_template, template_catalog

_SUBMIT_EXACT = (
    "提交申请",
    "提交简历",
    "提交报名",
    "提交投递",
    "确认投递",
    "确认提交",
    "完成投递",
    "投递简历",
    "申请职位",
    "立即投递",
    "提交",
    "投递",
    "申请",
)
_SUBMIT_EXCLUDE = (
    "下一步",
    "继续",
    "保存",
    "暂存",
    "上一步",
    "返回",
    "取消",
    "登录",
    "注册",
    "上传",
    "预览",
    "添加",
    "删除",
    "编辑",
    "修改",
    "关闭",
    "搜索",
    "筛选",
    "查看",
    "下载",
)
_CONFIRM_PATTERN = re.compile(r"确认投递|确认提交|确认申请|确定投递|确定提交|提交申请|确认报名")
_OTP_SEND_TEXTS = (
    "获取短信验证码",
    "发送短信验证码",
    "获取验证码",
    "发送验证码",
    "获取校验码",
    "获取动态码",
    "发送动态码",
    "重新获取验证码",
    "重新发送",
    "重新获取",
)
_OTP_SEND_EXCLUDE = ("语音", "登录", "注册", "密码")
_OTP_LOGIN_TEXTS = ("登录", "立即登录", "确认登录", "安全登录", "验证并登录", "登陆")
_OTP_LOGIN_EXCLUDE = ("注册", "忘记", "获取", "发送", "短信", "验证码", "扫码", "切换", "密码", "账号")


class OfficialApply:
    def __init__(self, config_path: str | Path | None = None) -> None:
        self.config = load_config(config_path)
        self.profile = load_profile(self.config)
        self.browser = Browser(self.config)
        self.registry = Registry(self.config["_registry_path"])

    # ------------------------------------------------------------------ status
    def status(self) -> dict[str, Any]:
        check = self.profile.check()
        today = self.registry.today()
        last = self.registry.last_submit_at()
        apply_cfg = self.config["apply"]
        interval = int(apply_cfg["min_submit_interval_seconds"])
        remaining = 0
        if last:
            elapsed = (datetime.now().astimezone() - last.astimezone()).total_seconds()
            remaining = max(0, int(interval - elapsed))
        return {
            "success": True,
            "config": {
                "path": self.config["_config_path"],
                "profile_path": self.config["_profile_path"],
                "registry_path": self.config["_registry_path"],
                "daily_submit_limit": int(apply_cfg["daily_submit_limit"]),
                "min_submit_interval_seconds": interval,
                "auto_upload_documents": list(apply_cfg["auto_upload_documents"]),
                "fill_sensitive_fields": bool(apply_cfg["fill_sensitive_fields"]),
                "require_no_unknown_required": bool(apply_cfg["require_no_unknown_required"]),
                "accept_terms_default": bool(apply_cfg["accept_terms_default"]),
            },
            "profile": {
                "exists": check["exists"],
                "complete_required": check["complete_required"],
                "summary": check["summary"],
                "missing_required": check["missing_required"],
                "missing_recommended": check["missing_recommended"],
                "missing_documents": check["missing_documents"],
                "warnings": check["warnings"],
            },
            "browser": self.browser.status(),
            "submissions": {
                "today": today,
                "daily_limit": int(apply_cfg["daily_submit_limit"]),
                "daily_remaining": max(0, int(apply_cfg["daily_submit_limit"]) - today["counted"]),
                "min_interval_seconds": interval,
                "interval_remaining_seconds": remaining,
                "last_submit_at": last.isoformat(timespec="seconds") if last else "",
            },
            "recent": self.registry.history(10),
        }

    def profile_check(self) -> dict[str, Any]:
        result = self.profile.check()
        result["success"] = True
        return result

    def profile_show(self) -> dict[str, Any]:
        values = self.profile.flat_values()
        masked = dict(values)
        for key in ("phone", "email", "id_number", "address", "emergency_contact_phone"):
            if key in masked and masked[key]:
                masked[key] = _mask_value(str(masked[key]), key)
        masked["documents"] = [item.as_dict() for item in self.profile.documents]
        masked["path"] = str(self.profile.path)
        masked["success"] = True
        return masked

    # -------------------------------------------------------------------- page
    def open(self, url: str = "", *, new: bool = False, front: bool = False) -> dict[str, Any]:
        try:
            tab = self.browser.open(url, new=new, front=front)
        except BrowserError as exc:
            raise ToolError(exc.code, exc.message, **exc.details) from exc
        title = _safe_title(tab)
        return {
            "success": True,
            "url": tab.current_url() or url,
            "title": title,
            "tab_id": tab.target_id,
            "profile_dir": str(Path(self.config["browser"]["user_data_dir"])),
        }

    def inspect(self, *, max_text: int = 6000, front: bool = False) -> dict[str, Any]:
        tab = self._current(front=front)
        page = self._inspect(tab, max_text=max_text)
        return {
            "success": True,
            "page": page,
            "template": page.get("template") or {},
            "login_required": _login_required(page),
            "captcha": bool((page.get("signals") or {}).get("captcha_evidence")),
        }

    def plan(
        self,
        *,
        overrides: dict[str, str] | None = None,
        job_title: str = "",
        job_company: str = "",
        accept_terms: bool = False,
        max_text: int = 6000,
    ) -> dict[str, Any]:
        tab = self._current()
        page = self._inspect(tab, max_text=max_text)
        self._guard_page(page)
        plan = plan_mapping(
            self.profile,
            page,
            self.config,
            overrides=overrides,
            job_title=job_title,
            job_company=job_company,
            accept_terms=accept_terms,
            template=page.get("template"),
        )
        payload = public_plan(plan)
        payload["success"] = True
        payload["login_required"] = _login_required(page)
        return payload

    def fill(
        self,
        *,
        overrides: dict[str, str] | None = None,
        job_title: str = "",
        job_company: str = "",
        accept_terms: bool = False,
        upload_documents: bool = False,
        skip_correct: bool = True,
        overwrite: bool = True,
        only: list[str] | None = None,
        max_text: int = 6000,
    ) -> dict[str, Any]:
        tab = self._current()
        page = self._inspect(tab, max_text=max_text)
        self._guard_page(page)
        plan = plan_mapping(
            self.profile,
            page,
            self.config,
            overrides=overrides,
            job_title=job_title,
            job_company=job_company,
            accept_terms=accept_terms,
            template=page.get("template"),
        )
        wanted = {normalize_key(item) for item in (only or []) if str(item).strip()}
        results, skipped = self._apply_plan(
            tab,
            plan,
            skip_correct=skip_correct,
            overwrite=overwrite,
            wanted=wanted,
        )
        upload_results: list[dict[str, Any]] = []
        if upload_documents:
            upload_results = self._upload_plan(tab, plan["uploads"])
        filled = [item for item in results if item.get("ok")]
        failed = [item for item in results if not item.get("ok")]
        return {
            "success": not failed,
            "url": page.get("url") or "",
            "title": page.get("title") or "",
            "filled": filled,
            "failed": failed,
            "filled_count": len(filled),
            "failed_count": len(failed),
            "skipped": skipped,
            "uploads": upload_results,
            "upload_plan": plan["uploads"] if not upload_documents else [],
            "unknown_required": plan["unknown_required"],
            "consent_pending": plan["consent_pending"],
            "missing_known": plan["missing_known"],
            "warnings": plan["warnings"],
            "signals": page.get("signals") or {},
        }

    def templates(self) -> dict[str, Any]:
        items = template_catalog()
        return {"success": True, "count": len(items), "templates": items}

    def autofill(
        self,
        *,
        overrides: dict[str, str] | None = None,
        job_title: str = "",
        job_company: str = "",
        accept_terms: bool = False,
        upload_documents: bool = True,
        max_steps: int = 0,
        wait_seconds: float = 0.0,
        front: bool = False,
    ) -> dict[str, Any]:
        tab = self._current(front=front)
        cfg = self.config.get("autofill") or {}
        steps_limit = max(1, int(max_steps or cfg.get("max_steps") or 6))
        wait = float(wait_seconds if wait_seconds else cfg.get("wait_seconds") or 1.2)
        parse_wait = float(cfg.get("resume_parse_wait_seconds") or 4)
        upload_wait = float(cfg.get("upload_wait_seconds") or 0.8)

        steps: list[dict[str, Any]] = []
        warnings: list[str] = []
        attempted_uploads: set[str] = set()
        seen_pages: set[tuple[Any, ...]] = set()
        total_filled = 0
        total_uploads = 0
        status = "max_steps"
        message = f"已达到最大步骤数 {steps_limit}，停止在填写阶段"
        final_button: dict[str, Any] | None = None
        next_button: dict[str, Any] | None = None
        last_plan: dict[str, Any] | None = None
        current_page: dict[str, Any] = {}

        for index in range(1, steps_limit + 1):
            try:
                page = self._inspect_retry(tab, attempts=2) if index > 1 else self._inspect(tab, max_text=10000)
            except ToolError as exc:
                status = "needs_attention"
                message = f"读取第 {index} 步页面失败: {exc.message}"
                break
            current_page = page
            template = page.get("template") or {}
            signals = page.get("signals") or {}
            if signals.get("captcha_evidence"):
                status = "human_handoff"
                message = "页面出现验证码或人机验证，请在浏览器中人工完成后重试"
                break
            if _login_required(page):
                status = "human_handoff"
                message = "页面要求登录，请先在专用 Chrome 中完成登录/短信验证后再运行一键填写"
                break
            if signals.get("already_applied_evidence") or _success_by_text(page):
                status = "already_submitted"
                message = "页面显示该岗位可能已经投递，已停止自动填写"
                break
            signature = _step_signature(page)
            if signature in seen_pages:
                status = "no_progress"
                message = "点击下一步后页面结构没有变化，已停止以避免重复操作"
                break
            seen_pages.add(signature)

            plan = self._plan_page(
                page,
                overrides=overrides,
                job_title=job_title,
                job_company=job_company,
                accept_terms=accept_terms,
            )
            last_plan = plan
            warnings.extend(plan.get("warnings") or [])
            step_uploads: list[dict[str, Any]] = []

            if upload_documents and plan["uploads"]:
                first = sorted(
                    plan["uploads"],
                    key=lambda item: 0 if item.get("role") in {"resume", "resume_image"} else 1,
                )[0]
                outcome = self._upload_plan(tab, [first])
                step_uploads.extend(outcome)
                total_uploads += sum(1 for item in outcome if item.get("ok"))
                if outcome and outcome[0].get("ok"):
                    attempted_uploads.add(_upload_key(first))
                    delay = parse_wait if first.get("role") in {"resume", "resume_image"} else upload_wait
                    if delay > 0:
                        time.sleep(delay)
                    page = self._inspect_retry(tab, attempts=3)
                    current_page = page
                    template = page.get("template") or template
                    plan = self._plan_page(
                        page,
                        overrides=overrides,
                        job_title=job_title,
                        job_company=job_company,
                        accept_terms=accept_terms,
                    )
                    last_plan = plan
                    warnings.extend(plan.get("warnings") or [])

            results, skipped = self._apply_plan(tab, plan)
            total_filled += sum(1 for item in results if item.get("ok"))
            plan_actions = {str(action.get("id")): action for action in plan["actions"]}
            failed_required = [
                item for item in results
                if not item.get("ok")
                and bool((plan_actions.get(str(item.get("id"))) or {}).get("required"))
            ]
            filled_keys = {
                (str(item.get("semantic") or ""), normalize_key(str(item.get("section") or "")))
                for item in results if item.get("ok")
            }

            page_after = (
                self._inspect_retry(tab, attempts=2)
                if (results or step_uploads)
                else page
            )
            current_page = page_after
            template = page_after.get("template") or template
            plan_after = self._plan_page(
                page_after,
                overrides=overrides,
                job_title=job_title,
                job_company=job_company,
                accept_terms=accept_terms,
            )
            last_plan = plan_after
            warnings.extend(plan_after.get("warnings") or [])

            remaining_uploads = [
                upload for upload in plan_after["uploads"]
                if _upload_key(upload) not in attempted_uploads
            ]
            if upload_documents and remaining_uploads:
                outcomes = self._upload_plan(tab, remaining_uploads)
                step_uploads.extend(outcomes)
                total_uploads += sum(1 for item in outcomes if item.get("ok"))
                for upload, outcome in zip(remaining_uploads, outcomes):
                    if outcome.get("ok"):
                        attempted_uploads.add(_upload_key(upload))
                if any(item.get("ok") for item in outcomes):
                    time.sleep(upload_wait)
                    page_after = self._inspect_retry(tab, attempts=2)
                    current_page = page_after
                    template = page_after.get("template") or template
                    plan_after = self._plan_page(
                        page_after,
                        overrides=overrides,
                        job_title=job_title,
                        job_company=job_company,
                        accept_terms=accept_terms,
                    )
                    last_plan = plan_after

            required_empty = [
                action for action in plan_after["actions"]
                if action.get("required")
                and action.get("semantic") != "consent"
                and not action.get("has_current_value")
                and not action.get("already_correct")
                and (
                    str(action.get("semantic") or ""),
                    normalize_key(str(action.get("section") or "")),
                ) not in filled_keys
            ]
            required_uploads = [
                upload for upload in plan_after["uploads"]
                if upload.get("required") and _upload_key(upload) not in attempted_uploads
            ]
            validation_errors = [
                str(item) for item in (page_after.get("signals") or {}).get("validation_errors") or []
            ]
            final_button = _find_final_button(page_after, template)
            next_button = _find_advance_button(page_after, template)
            blocking = bool(
                plan_after["unknown_required"]
                or required_empty
                or required_uploads
                or failed_required
                or validation_errors
            )
            step_status = "filled"
            if blocking:
                step_status = "needs_attention"
            elif plan_after["consent_pending"]:
                step_status = "terms_required"
            elif final_button:
                step_status = "ready_to_submit"
            elif next_button:
                step_status = "next"
            steps.append({
                "index": index,
                "url": page_after.get("url") or "",
                "title": page_after.get("title") or "",
                "template": template.get("id") or "generic",
                "filled_count": sum(1 for item in results if item.get("ok")),
                "failed_count": sum(1 for item in results if not item.get("ok")),
                "uploaded_count": sum(1 for item in step_uploads if item.get("ok")),
                "unknown_required_count": len(plan_after["unknown_required"]),
                "required_empty_count": len(required_empty),
                "consent_pending_count": len(plan_after["consent_pending"]),
                "validation_error_count": len(validation_errors),
                "status": step_status,
            })

            if blocking:
                status = "needs_attention"
                message = _attention_message(
                    plan_after,
                    required_empty=required_empty,
                    required_uploads=required_uploads,
                    failed_required=failed_required,
                    validation_errors=validation_errors,
                )
                break
            if plan_after["consent_pending"]:
                status = "terms_required"
                message = "表单已填写完成，但还有同意/隐私条款需要本人确认；确认后可用 official_submit 提交"
                break
            if final_button:
                status = "ready_to_submit"
                message = "表单已填写完成，已停在最终提交按钮前，未提交"
                break
            if not next_button:
                status = "no_next_step"
                message = "未找到下一步按钮或最终提交按钮，请人工检查页面"
                break

            outcome = click_oa(tab, str(next_button.get("id") or ""))
            if not outcome.get("ok"):
                outcome = click_text(tab, str(next_button.get("text") or ""))
            if not outcome.get("ok"):
                status = "needs_attention"
                message = f"点击“{next_button.get('text')}”失败，请人工检查页面"
                break
            if wait > 0:
                time.sleep(wait)

        return {
            "success": status in {"ready_to_submit", "terms_required", "already_submitted"},
            "status": status,
            "message": message,
            "stopped_before_submit": True,
            "url": current_page.get("url") or "",
            "title": current_page.get("title") or "",
            "template": current_page.get("template") or {},
            "steps": steps,
            "step_count": len(steps),
            "filled_count": total_filled,
            "uploaded_count": total_uploads,
            "final_button": final_button,
            "next_button": next_button,
            "unknown_required": (last_plan or {}).get("unknown_required") or [],
            "consent_pending": (last_plan or {}).get("consent_pending") or [],
            "missing_known": (last_plan or {}).get("missing_known") or [],
            "warnings": _dedupe_strings(warnings),
            "plan": public_plan(last_plan) if last_plan else {},
        }

    def upload(
        self,
        *,
        field_id: str = "",
        role: str = "",
        paths: list[str] | None = None,
        upload_all: bool = False,
        front: bool = False,
    ) -> dict[str, Any]:
        tab = self._current(front=front)
        page = self._inspect(tab)
        self._guard_page(page)
        fields = list(page.get("fields") or [])
        targets: list[dict[str, Any]] = []
        explicit_paths = [str(item) for item in (paths or []) if str(item).strip()]
        if field_id:
            field = next((item for item in fields if str(item.get("id")) == field_id), None)
            if field:
                detected = detect_document_role(field)
                field_paths = explicit_paths or _profile_paths(self.profile, detected)
                targets.append({"field": field, "role": detected or "", "paths": field_paths, "button_id": ""})
            else:
                button = next(
                    (item for item in page.get("buttons") or [] if str(item.get("id")) == field_id),
                    None,
                )
                if not button:
                    raise ToolError("field_not_found", f"页面中找不到字段 {field_id}")
                targets.append({
                    "field": None,
                    "role": "",
                    "paths": explicit_paths,
                    "button_id": field_id,
                    "label": button.get("text") or "",
                })
        elif role:
            candidates = [
                item for item in fields
                if str(item.get("kind")) == "file" and detect_document_role(item) == role
            ]
            if not upload_all:
                candidates = candidates[:1]
            if not candidates:
                raise ToolError("upload_field_not_found", f"页面中找不到上传“{role}”的文件输入框")
            role_paths = explicit_paths or _profile_paths(self.profile, role)
            for field in candidates:
                targets.append({"field": field, "role": role, "paths": role_paths, "button_id": ""})
        else:
            if explicit_paths:
                raise ToolError("invalid_arguments", "指定文件路径时还需提供 field_id 或 role")
            plan = plan_mapping(
                self.profile, page, self.config,
                include_uploaded=False,
                template=page.get("template"),
            )
            for upload in plan["uploads"]:
                targets.append({
                    "field": {"id": upload["id"], "label": upload["label"]},
                    "role": upload["role"],
                    "paths": [upload["path"]],
                    "button_id": "",
                })
        if not targets:
            raise ToolError("no_upload_targets", "页面没有需要自动上传的附件字段")

        results = []
        for target in targets:
            field = target.get("field")
            target_paths = [str(Path(path).expanduser()) for path in target.get("paths") or []]
            if not target_paths:
                results.append({
                    "ok": False,
                    "error": "document_missing",
                    "role": target.get("role") or "",
                    "label": (field or {}).get("label") or target.get("label") or "",
                })
                continue
            if target.get("button_id"):
                outcome = upload_via_button(tab, str(target["button_id"]), target_paths)
            else:
                outcome = upload_to_field(tab, str((field or {}).get("id") or ""), target_paths)
            outcome.update({
                "role": target.get("role") or "",
                "label": (field or {}).get("label") or target.get("label") or "",
                "files": outcome.get("files") or [Path(path).name for path in target_paths],
            })
            results.append(outcome)
        return {
            "success": bool(results) and all(item.get("ok") for item in results),
            "results": results,
            "uploaded_count": sum(1 for item in results if item.get("ok")),
        }

    def click(
        self,
        *,
        text: str = "",
        field_id: str = "",
        wait: float = 0.6,
    ) -> dict[str, Any]:
        tab = self._current()
        if field_id:
            result = click_oa(tab, field_id)
        elif text:
            result = click_text(tab, text)
        else:
            raise ToolError("invalid_arguments", "需要提供 text 或 field_id")
        if not result.get("ok"):
            raise ToolError("click_failed", f"未能点击目标: {result.get('error') or 'not_found'}")
        if wait > 0:
            time.sleep(wait)
        return {
            "success": True,
            "clicked": result.get("text") or text or field_id,
            "url": tab.current_url(),
            "title": _safe_title(tab),
        }

    def submit(
        self,
        *,
        button: str = "",
        accept_terms: bool = False,
        force: bool = False,
        job_title: str = "",
        job_company: str = "",
        notes: str = "",
        dry_run: bool = False,
    ) -> dict[str, Any]:
        tab = self._current()
        page = self._inspect(tab, max_text=8000)
        self._guard_page(page, action="submit")
        signals = page.get("signals") or {}
        if signals.get("already_applied_evidence"):
            raise ToolError(
                "already_applied",
                "页面显示该岗位可能已经投递过",
                evidence=signals.get("already_applied_evidence"),
                url=page.get("url") or "",
            )
        posting = page.get("job_posting") or {}
        title = str(job_title or posting.get("title") or "").strip()
        company = str(job_company or posting.get("company") or "").strip()
        url = str(page.get("url") or tab.current_url() or "")
        apply_cfg = self.config["apply"]
        guard = self.registry.check(
            url,
            company=company,
            title=title,
            daily_limit=int(apply_cfg["daily_submit_limit"]),
            min_interval_seconds=int(apply_cfg["min_submit_interval_seconds"]),
        )
        if guard["duplicate_url"] and not force:
            record = guard["duplicate_url"]
            raise ToolError(
                "duplicate_application",
                "该链接已有成功或待确认的投递记录，使用 force=true 才能再次提交",
                record_id=record.get("id"),
                submitted_at=record.get("at"),
                status=record.get("status"),
                url=url,
            )
        if guard["limit_reached"]:
            raise ToolError(
                "daily_limit_reached",
                f"今天官网投递已达上限 {apply_cfg['daily_submit_limit']} 次",
                today=guard["daily"],
                url=url,
            )
        if guard["interval_remaining_seconds"] > 0:
            raise ToolError(
                "submit_too_soon",
                f"距离上次提交不足 {apply_cfg['min_submit_interval_seconds']} 秒，请等待 "
                f"{guard['interval_remaining_seconds']} 秒",
                wait_seconds=guard["interval_remaining_seconds"],
                url=url,
            )

        plan = plan_mapping(
            self.profile,
            page,
            self.config,
            job_title=title,
            job_company=company,
            accept_terms=False,
            template=page.get("template"),
        )
        if plan["unknown_required"] and apply_cfg["require_no_unknown_required"]:
            raise ToolError(
                "unknown_required_fields",
                f"还有 {len(plan['unknown_required'])} 个必填字段未处理，已拒绝提交",
                fields=plan["unknown_required"][:30],
                url=url,
            )
        required_unfilled = [
            action for action in plan["actions"]
            if action.get("required") and not action.get("has_current_value")
        ]
        if required_unfilled:
            raise ToolError(
                "required_fields_empty",
                f"还有 {len(required_unfilled)} 个必填字段为空，请先调用 official_fill",
                fields=[
                    {
                        "id": item["id"],
                        "semantic": item.get("semantic") or "",
                        "label": item.get("screen_label") or item.get("label") or "",
                    }
                    for item in required_unfilled[:30]
                ],
                url=url,
            )
        field_values = {
            str(field.get("id")): str(field.get("value") or "")
            for field in page.get("fields") or []
            if field.get("kind") == "file"
        }
        pending_uploads = [
            upload for upload in plan["uploads"]
            if upload.get("required") and not field_values.get(str(upload.get("id")))
        ]
        if pending_uploads:
            raise ToolError(
                "required_uploads_pending",
                f"还有 {len(pending_uploads)} 个必填附件未上传，请先调用 official_upload",
                fields=pending_uploads[:20],
                url=url,
            )
        target = _find_submit_button(page.get("buttons") or [], button)
        if not target:
            available = [
                item.get("text") for item in page.get("buttons") or []
                if item.get("text") and not item.get("disabled")
            ][:30]
            raise ToolError(
                "submit_button_not_found",
                "未找到可用的提交按钮，请用 button 参数指定按钮文字",
                buttons=available,
                url=url,
            )
        if dry_run:
            preview = public_plan(plan)
            if accept_terms and plan["consent_pending"]:
                preview["summary"]["cannot_submit"] = bool(plan["unknown_required"])
                preview["would_check_consent"] = plan["consent_pending"]
            return {
                "success": True,
                "dry_run": True,
                "ready": not preview["summary"].get("cannot_submit"),
                "url": url,
                "title": title,
                "company": company,
                "submit_button": target,
                "guard": _public_guard(guard),
                "plan": preview,
                "message": "预检完成，未点击提交、未勾选条款、未写入投递记录",
            }

        if plan["consent_pending"]:
            if not accept_terms:
                raise ToolError(
                    "terms_confirmation_required",
                    "页面要求确认同意/隐私条款；请本人确认后用 accept_terms=true 重新调用",
                    checkboxes=plan["consent_pending"],
                    url=url,
                )
            consent_plan = plan_mapping(
                self.profile,
                page,
                self.config,
                job_title=title,
                job_company=company,
                accept_terms=True,
                template=page.get("template"),
            )
            consent_actions = [
                item for item in consent_plan["actions"] if item.get("semantic") == "consent"
            ]
            if consent_actions:
                checked = fill_fields(tab, consent_actions)
                failed_consent = [item for item in checked if not item.get("ok")]
                if failed_consent:
                    raise ToolError(
                        "terms_check_failed",
                        "无法勾选同意/隐私条款，请人工确认页面",
                        failed=failed_consent,
                        url=url,
                    )

        clicked = click_oa(tab, str(target["id"]))
        if not clicked.get("ok"):
            raise ToolError("submit_click_failed", "提交按钮点击失败", button=target, url=url)
        time.sleep(2.0)
        page_after = self._inspect_retry(tab, attempts=4)
        confirm = _find_confirm_button(page_after)
        if confirm:
            click_oa(tab, str(confirm["id"]))
            time.sleep(1.8)
            page_after = self._inspect_retry(tab, attempts=4)
        signals_after = page_after.get("signals") or {}
        errors = [str(item) for item in signals_after.get("validation_errors") or []]
        success_evidence = list(signals_after.get("success_evidence") or [])
        applied_evidence = list(signals_after.get("already_applied_evidence") or [])
        if success_evidence or applied_evidence or _success_by_text(page_after):
            status = "submitted"
            message = "已提交，页面出现成功提示"
        elif errors:
            status = "failed"
            message = "提交后页面出现校验错误"
        elif _login_required(page_after) or signals_after.get("captcha_evidence"):
            status = "unconfirmed"
            message = "提交后页面要求登录或验证，结果待确认"
        else:
            status = "unconfirmed"
            message = "已点击提交，但未检测到明确成功提示，请在页面确认结果"
        screenshot = self._screenshot(tab, f"{company}-{title}") if not dry_run else ""
        record = self.registry.append({
            "status": status,
            "url": url,
            "url_key": guard["url_key"],
            "company": company,
            "title": title,
            "profile_path": self.config["_profile_path"],
            "submit_button": target.get("text") or "",
            "confirm_button": (confirm or {}).get("text") or "",
            "consent_checked": bool(plan["consent_pending"]),
            "unknown_required": [item.get("label") for item in plan["unknown_required"]],
            "validation_errors": errors,
            "success_evidence": success_evidence or applied_evidence,
            "screenshot": screenshot,
            "notes": notes,
            "message": message,
        })
        warnings = [
            item for item in plan["warnings"]
            if not (plan["consent_pending"] and "accept_terms" in item)
        ]
        if guard.get("similar_title"):
            warnings.append("已投递过相同公司+岗位名称的记录，请确认不是重复岗位")
        if plan["unknown_required"] and not apply_cfg["require_no_unknown_required"]:
            warnings.append("配置允许在存在未识别必填字段时提交，请自行核对页面")
        result = {
            "success": status == "submitted",
            "status": status,
            "message": message,
            "record_id": record.get("id"),
            "url": page_after.get("url") or url,
            "company": company,
            "title": title,
            "submit_button": target.get("text") or "",
            "confirm_button": (confirm or {}).get("text") or "",
            "validation_errors": errors,
            "success_evidence": success_evidence or applied_evidence,
            "screenshot": screenshot,
            "unknown_required": plan["unknown_required"],
            "consent_checked": bool(plan["consent_pending"]),
            "warnings": warnings,
        }
        return result

    # -------------------------------------------------------------------- misc
    def screenshot(self, name: str = "") -> dict[str, Any]:
        tab = self._current()
        label = name.strip() or datetime.now().strftime("shot-%Y%m%d-%H%M%S")
        path = self._screenshot(tab, label)
        if not path:
            raise ToolError("screenshot_failed", "截图失败，请确认页面仍在")
        return {"success": True, "path": path, "url": tab.current_url()}

    def history(self, *, limit: int = 20, status: str = "") -> dict[str, Any]:
        items = self.registry.history(limit, status=status)
        return {
            "success": True,
            "items": items,
            "today": self.registry.today(),
            "last_submit_at": (
                self.registry.last_submit_at().isoformat(timespec="seconds")
                if self.registry.last_submit_at()
                else ""
            ),
        }

    def login(self, url: str = "") -> dict[str, Any]:
        tab = self.browser.open(url, front=True)
        return {
            "success": True,
            "url": tab.current_url() or url,
            "title": _safe_title(tab),
            "profile_dir": str(Path(self.config["browser"]["user_data_dir"])),
            "message": (
                "请在弹出的 Chrome 窗口中完成登录、短信验证或滑块验证；"
                "工具不会代替人工绕过验证。完成后调用 official_check_login 或 official_inspect。"
            ),
        }

    def check_login(self, url: str = "") -> dict[str, Any]:
        tab = self.browser.open(url, front=False) if url else self._current()
        page = self._inspect(tab)
        signals = page.get("signals") or {}
        login_required = _login_required(page)
        return {
            "success": True,
            "logged_in": not login_required,
            "login_required": login_required,
            "signals": signals,
            "url": page.get("url") or "",
            "title": page.get("title") or "",
            "field_count": page.get("field_count") or 0,
        }

    def otp_fill(
        self,
        *,
        send: bool = True,
        send_button: str = "",
        field_id: str = "",
        submit: bool = True,
        submit_button: str = "",
        timeout: float = 120.0,
        reuse_existing: bool = True,
    ) -> dict[str, Any]:
        tab = self._current(front=True)
        page = self._inspect(tab, max_text=8000)
        signals = page.get("signals") or {}
        if signals.get("captcha_evidence"):
            raise ToolError(
                "human_handoff",
                "页面出现图形验证码或人机验证，请人工完成后重试",
                evidence=signals.get("captcha_evidence"),
                url=page.get("url") or "",
            )
        fields = find_otp_fields(page, field_id=field_id)
        if not fields:
            raise ToolError(
                "otp_field_not_found",
                "未找到验证码输入框，请先用 official_inspect 确认页面已进入验证码登录步骤",
                url=page.get("url") or "",
                title=page.get("title") or "",
            )

        client = ensure_otp_client(self.config)
        status = client.status()
        ttl = float(status.get("ttl_seconds") or 120)
        message = None
        used_existing = False
        clicked_send = ""
        warnings: list[str] = []

        latest = client.latest()
        if latest is not None and reuse_existing and time.time() - latest.received_at <= max(30.0, ttl):
            message = latest
            used_existing = True
        else:
            since = time.time() - 2.0
            if send:
                target = _find_text_button(
                    page.get("buttons") or [],
                    send_button,
                    _OTP_SEND_TEXTS,
                    _OTP_SEND_EXCLUDE,
                )
                if target:
                    outcome = click_oa(tab, str(target.get("id") or ""))
                    if not outcome.get("ok"):
                        raise ToolError(
                            "otp_send_failed",
                            f"点击发送验证码按钮失败: {outcome.get('error') or 'not_found'}",
                            url=page.get("url") or "",
                        )
                    clicked_send = str(target.get("text") or "")
                    time.sleep(0.8)
                    page = self._inspect(tab, max_text=8000)
                    refreshed = find_otp_fields(page, field_id=field_id)
                    if refreshed:
                        fields = refreshed
                else:
                    warnings.append("未找到可点击的发送验证码按钮，等待手机端已触发的验证码")
            else:
                since = time.time() - max(30.0, ttl)
            message = client.wait_for_new_code(since=since, timeout=max(5.0, min(600.0, float(timeout))))

        if message is None:
            last_error = ""
            try:
                last_error = str(client.status().get("last_error") or "")
            except ToolError:
                pass
            raise ToolError(
                "otp_timeout",
                f"在 {int(timeout)} 秒内没有收到新的短信验证码",
                last_error=last_error,
                url=page.get("url") or "",
            )

        actions = otp_fill_actions(fields, message.code)
        fill_results = fill_fields(tab, actions)
        verify = verify_otp_fill(tab, actions)
        typed_fallback = False
        if not verify.get("ok"):
            type_otp_fields(tab, actions)
            typed_fallback = True
            verify = verify_otp_fill(tab, actions)
        filled_count = sum(1 for item in verify.get("results") or [] if item.get("match"))
        verified = bool(verify.get("ok"))
        failed_ids = [
            str(item.get("id")) for item in verify.get("results") or []
            if not item.get("match")
        ]
        if not verified:
            warnings.append(f"有 {len(failed_ids)} 个验证码输入框未能确认写入，已跳过自动登录")

        submitted = False
        submit_label = ""
        if submit and verified:
            button = _find_text_button(
                page.get("buttons") or [],
                submit_button,
                _OTP_LOGIN_TEXTS,
                _OTP_LOGIN_EXCLUDE,
            )
            if button:
                outcome = click_oa(tab, str(button.get("id") or ""))
                submitted = bool(outcome.get("ok"))
                if submitted:
                    submit_label = str(button.get("text") or "")
                    time.sleep(1.2)
            else:
                warnings.append("验证码已填入，但未找到登录按钮，请人工点击登录")

        page_after = self._inspect_retry(tab, attempts=2) if submitted else self._inspect(tab, max_text=8000)
        fill_failed = [item for item in fill_results if not item.get("ok")]
        if fill_failed and not verified:
            warnings.append("直接赋值填写未完全成功，已尝试键盘输入")
        return {
            "success": verified,
            "verified": verified,
            "field_count": len(actions),
            "filled_count": filled_count,
            "failed_field_ids": failed_ids,
            "split_boxes": len(actions) > 1,
            "typed_fallback": typed_fallback,
            "send_clicked": clicked_send,
            "used_existing": used_existing,
            "sender": message.sender,
            "received_at": message.received_at_text,
            "submitted": submitted,
            "submit_button": submit_label,
            "login_required": _login_required(page_after),
            "url": page_after.get("url") or "",
            "title": page_after.get("title") or "",
            "warnings": warnings,
        }

    # ---------------------------------------------------------------- internal
    def _plan_page(
        self,
        page: dict[str, Any],
        *,
        overrides: dict[str, str] | None,
        job_title: str,
        job_company: str,
        accept_terms: bool,
    ) -> dict[str, Any]:
        return plan_mapping(
            self.profile,
            page,
            self.config,
            overrides=overrides,
            job_title=job_title,
            job_company=job_company,
            accept_terms=accept_terms,
            template=page.get("template"),
        )

    def _apply_plan(
        self,
        tab,
        plan: dict[str, Any],
        *,
        skip_correct: bool = True,
        overwrite: bool = True,
        wanted: set[str] | None = None,
    ) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
        selected: list[dict[str, Any]] = []
        skipped: list[dict[str, Any]] = []
        for action in plan["actions"]:
            if wanted and not _action_selected(action, wanted):
                continue
            if skip_correct and action.get("already_correct"):
                skipped.append(_skip(action, "already_correct", "页面值已与资料一致"))
                continue
            if action.get("will_overwrite") and not overwrite:
                skipped.append(_skip(action, "overwrite_disabled", "页面已有不同内容，按参数不覆盖"))
                continue
            selected.append(action)

        native = [item for item in selected if not item.get("custom_select")]
        custom = [item for item in selected if item.get("custom_select")]
        results: list[dict[str, Any]] = []
        if native:
            results.extend(fill_fields(tab, native))
        for action in custom:
            outcome = select_custom_option(
                tab,
                action,
                [action["value"], *(action.get("alternatives") or [])],
            )
            outcome.update({
                "semantic": action.get("semantic") or "",
                "label": action.get("screen_label") or "",
                "section": action.get("section") or "",
                "kind": "custom_select",
                "value_preview": (
                    "***" if action.get("semantic") in SENSITIVE_SEMANTICS
                    else action.get("value") or ""
                ),
            })
            results.append(outcome)
        return results, skipped

    def _upload_plan(self, tab, uploads: list[dict[str, Any]]) -> list[dict[str, Any]]:
        results: list[dict[str, Any]] = []
        for upload in uploads:
            outcome = upload_to_field(tab, str(upload.get("id") or ""), [str(upload.get("path") or "")])
            outcome.update({
                "role": upload.get("role") or "",
                "label": upload.get("label") or "",
                "filename": upload.get("filename") or "",
                "required": bool(upload.get("required")),
            })
            results.append(outcome)
        return results

    def _current(self, *, front: bool = False):
        try:
            return self.browser.current(front=front)
        except BrowserError as exc:
            raise ToolError(exc.code, exc.message, **exc.details) from exc

    def _inspect(self, tab, *, max_text: int = 6000) -> dict[str, Any]:
        try:
            page = inspect_page(tab, max_text=max_text)
        except CDPError as exc:
            raise ToolError("inspect_failed", f"读取页面失败: {exc}") from exc
        page["template"] = detect_template(page, self.config)
        return page

    def _inspect_retry(self, tab, *, attempts: int = 3) -> dict[str, Any]:
        last: Exception | None = None
        for _ in range(max(1, attempts)):
            time.sleep(0.7)
            try:
                return self._inspect(tab, max_text=8000)
            except (ToolError, CDPError) as exc:
                last = exc
        if isinstance(last, ToolError):
            raise last
        raise ToolError("inspect_failed", f"提交后读取页面失败: {last}")

    def _guard_page(self, page: dict[str, Any], *, action: str = "fill") -> None:
        signals = page.get("signals") or {}
        if signals.get("captcha_evidence"):
            raise ToolError(
                "human_handoff",
                "页面出现验证码或人机验证，请人工完成后重试",
                evidence=signals.get("captcha_evidence"),
                url=page.get("url") or "",
            )
        if _login_required(page):
            raise ToolError(
                "login_required",
                f"{action}前需要先在该网站登录，请用 official_open_login 打开登录页并人工完成登录",
                evidence=signals.get("login_evidence"),
                url=page.get("url") or "",
            )

    def _screenshot(self, tab, label: str) -> str:
        directory = Path(self.config["_screenshot_dir"])
        directory.mkdir(parents=True, exist_ok=True)
        stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
        safe = re.sub(r'[\\/:*?"<>|\s]+', "-", label).strip("-")[:60] or "page"
        path = directory / f"{stamp}-{safe}.png"
        for attempt in range(2):
            try:
                if tab.screenshot(path):
                    return str(path)
            except (CDPError, OSError):
                pass
            if attempt == 0:
                time.sleep(0.5)
        return ""


def _profile_paths(profile, role: str | None) -> list[str]:
    if not role:
        return []
    path = profile.document_for_role(role)
    if not path and role == "resume":
        path = profile.resume_path()
    if not path and role == "photo":
        path = profile.photo_path()
    return [str(path)] if path else []


def _login_required(page: dict[str, Any]) -> bool:
    signals = page.get("signals") or {}
    if signals.get("login_modal_visible"):
        return True
    evidence = signals.get("login_evidence") or []
    if not evidence:
        return False
    if signals.get("logged_in_hint"):
        return False
    return int(page.get("field_count") or 0) < 6


def _success_by_text(page: dict[str, Any]) -> bool:
    text = str(page.get("page_text") or "")
    title = str(page.get("title") or "")
    return bool(re.search(r"提交成功|投递成功|申请成功|报名成功|简历已投递|申请已提交", text + " " + title))


def _find_text_button(
    buttons: list[dict[str, Any]],
    requested: str,
    defaults: tuple[str, ...],
    exclude: tuple[str, ...],
) -> dict[str, Any] | None:
    candidates = [
        item for item in buttons
        if isinstance(item, dict) and not item.get("disabled") and str(item.get("text") or "").strip()
    ]
    if requested.strip():
        key = normalize_key(requested)
        exact = [item for item in candidates if normalize_key(str(item.get("text"))) == key]
        if exact:
            return max(exact, key=lambda item: (not item.get("in_dialog"), -len(str(item.get("text")))))
        partial = [item for item in candidates if key and key in normalize_key(str(item.get("text")))]
        return partial[0] if partial else None
    for phrase in defaults:
        key = normalize_key(phrase)
        for item in candidates:
            label = normalize_key(str(item.get("text")))
            if not key or key not in label:
                continue
            if any(normalize_key(word) in label for word in exclude):
                continue
            return item
    return None


def _find_submit_button(buttons: list[dict[str, Any]], requested: str = "") -> dict[str, Any] | None:
    candidates = [item for item in buttons if not item.get("disabled") and item.get("text")]
    if requested.strip():
        key = normalize_key(requested)
        exact = [item for item in candidates if normalize_key(str(item.get("text"))) == key]
        if exact:
            return max(exact, key=lambda item: (not item.get("in_dialog"), -len(str(item.get("text")))))
        partial = [item for item in candidates if key and key in normalize_key(str(item.get("text")))]
        if partial:
            return partial[0]
        return None
    best: tuple[int, dict[str, Any]] | None = None
    for item in candidates:
        text = str(item.get("text") or "").strip()
        normalized = normalize_key(text)
        if not normalized:
            continue
        if any(word in normalized for word in _SUBMIT_EXCLUDE):
            continue
        score = 0
        for index, word in enumerate(_SUBMIT_EXACT):
            word_key = normalize_key(word)
            if normalized == word_key:
                score = max(score, 100 - index)
            elif word_key and word_key in normalized:
                score = max(score, 60 - index)
        if not score:
            continue
        if item.get("in_dialog"):
            score -= 30
        if best is None or score > best[0]:
            best = (score, item)
    return best[1] if best and best[0] >= 30 else None


def _find_confirm_button(page: dict[str, Any]) -> dict[str, Any] | None:
    buttons = page.get("buttons") or []
    page_text = str(page.get("page_text") or "")
    for item in buttons:
        if item.get("disabled") or not item.get("in_dialog"):
            continue
        text = str(item.get("text") or "")
        normalized = normalize_key(text)
        if not normalized or any(word in normalized for word in ("取消", "关闭", "返回", "上一步")):
            continue
        if _CONFIRM_PATTERN.search(text):
            return item
        if normalized in {"确定", "确认", "是"} and re.search(
            r"确认.{0,12}(投递|提交|申请|报名)|(投递|提交|申请|报名).{0,12}确认",
            page_text,
        ):
            return item
    return None


def _find_final_button(
    page: dict[str, Any],
    template: dict[str, Any] | None = None,
) -> dict[str, Any] | None:
    buttons = [
        item for item in page.get("buttons") or []
        if isinstance(item, dict) and item.get("text") and not item.get("disabled") and not item.get("in_dialog")
    ]
    wanted = [
        normalize_key(str(item))
        for item in (template or {}).get("final_buttons") or _SUBMIT_EXACT
        if str(item).strip()
    ]
    for key in wanted:
        for item in buttons:
            if normalize_key(str(item.get("text"))) == key:
                return item
    for item in buttons:
        if normalize_key(str(item.get("text"))) in {"提交", "submit"}:
            return item
    for item in buttons:
        text = normalize_key(str(item.get("text")))
        if any(len(key) >= 4 and key in text for key in wanted):
            return item
    return None


def _find_advance_button(
    page: dict[str, Any],
    template: dict[str, Any] | None = None,
) -> dict[str, Any] | None:
    buttons = [
        item for item in page.get("buttons") or []
        if isinstance(item, dict) and item.get("text") and not item.get("disabled") and not item.get("in_dialog")
    ]
    wanted = [
        normalize_key(str(item))
        for item in (template or {}).get("next_buttons") or ()
        if str(item).strip()
    ]
    if not wanted:
        return None
    for key in wanted:
        for item in buttons:
            if normalize_key(str(item.get("text"))) == key:
                return item
    for item in buttons:
        text = normalize_key(str(item.get("text")))
        if re.search(r"提交|投递|申请|预览|submit", text):
            continue
        if any(key and key in text for key in wanted):
            return item
    return None


def _step_signature(page: dict[str, Any]) -> tuple[Any, ...]:
    fields = tuple(
        (
            normalize_key(str(item.get("section") or "")),
            normalize_key(str(item.get("label") or "")),
            str(item.get("kind") or ""),
        )
        for item in page.get("fields") or []
        if isinstance(item, dict) and not item.get("hidden")
    )
    buttons = tuple(
        normalize_key(str(item.get("text") or ""))
        for item in page.get("buttons") or []
        if isinstance(item, dict)
    )
    return (str(page.get("url") or ""), fields, buttons)


def _upload_key(upload: dict[str, Any]) -> str:
    role = str(upload.get("role") or "")
    label = normalize_key(str(upload.get("label") or ""))
    return f"{role}|{label}" if role or label else str(upload.get("filename") or "")


def _attention_message(
    plan: dict[str, Any],
    *,
    required_empty: list[dict[str, Any]],
    required_uploads: list[dict[str, Any]],
    failed_required: list[dict[str, Any]],
    validation_errors: list[str],
) -> str:
    parts = []
    if plan.get("unknown_required"):
        parts.append(f"{len(plan['unknown_required'])} 个必填字段未匹配到资料")
    if required_empty:
        parts.append(f"{len(required_empty)} 个必填字段为空")
    if required_uploads:
        parts.append(f"{len(required_uploads)} 个必填附件未上传")
    if failed_required:
        parts.append(f"{len(failed_required)} 个必填字段填写失败")
    if validation_errors:
        parts.append(f"页面有 {len(validation_errors)} 条校验错误")
    if plan.get("consent_pending"):
        parts.append("同意/隐私条款待本人确认")
    return "填写未完成：" + "、".join(parts) + "；请人工补齐后重试"


def _dedupe_strings(items: list[str]) -> list[str]:
    seen: set[str] = set()
    result = []
    for item in items:
        text = str(item)
        if text and text not in seen:
            seen.add(text)
            result.append(text)
    return result


def _public_guard(guard: dict[str, Any]) -> dict[str, Any]:
    return {
        key: value for key, value in guard.items()
        if key not in {"duplicate_url", "similar_title"}
    } | {
        "duplicate_submitted": bool(guard.get("duplicate_url")),
        "similar_title_submitted": bool(guard.get("similar_title")),
    }


def _action_selected(action: dict[str, Any], wanted: set[str]) -> bool:
    values = {
        normalize_key(str(action.get("id") or "")),
        normalize_key(str(action.get("semantic") or "")),
        normalize_key(str(action.get("screen_label") or "")),
        normalize_key(str(action.get("label") or "")),
    }
    return bool(values & wanted)


def _skip(action: dict[str, Any], reason: str, message: str) -> dict[str, Any]:
    return {
        "id": action.get("id") or "",
        "semantic": action.get("semantic") or "",
        "label": action.get("screen_label") or action.get("label") or "",
        "reason": reason,
        "message": message,
    }


def normalize_key(value: str) -> str:
    return re.sub(r"[\s_\-*:：·•/\\（）()【】\[\]<>]+", "", str(value or "").lower())


def _safe_title(tab) -> str:
    try:
        return str(tab.evaluate("document.title") or "")
    except CDPError:
        return ""


def _mask_value(value: str, key: str) -> str:
    if key == "phone":
        return mask_phone(value)
    if key == "email":
        return mask_email(value)
    if key == "id_number":
        return mask_id(value)
    if len(value) <= 4:
        return "***"
    return f"{value[:2]}***{value[-2:]}"
