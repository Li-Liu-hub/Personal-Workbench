"""Detection and hints for the many official-site application templates."""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path
from typing import Any

import yaml

CATALOG_PATH = Path(__file__).with_name("templates.yaml")

DEFAULT_IGNORE_SECTIONS = (
    r"家庭(成员|状况|信息)?",
    r"亲属|亲友|社会关系",
    r"推荐人|内推人|证明人",
    r"登录|注册|验证码|密码",
)
DEFAULT_NEXT_BUTTONS = (
    "下一步",
    "继续",
    "下一步骤",
    "保存并下一步",
    "保存并继续",
    "确认并继续",
    "Next",
    "Continue",
    "Save and Continue",
)
DEFAULT_FINAL_BUTTONS = (
    "预览并提交",
    "提交申请",
    "提交简历",
    "确认提交",
    "确认投递",
    "完成投递",
    "提交报名",
    "Submit Application",
    "Submit application",
    "Submit",
)


@dataclass(frozen=True)
class TemplateRule:
    id: str
    name: str
    vendor: str = ""
    priority: int = 0
    url_patterns: tuple[re.Pattern[str], ...] = ()
    markers: tuple[re.Pattern[str], ...] = ()
    ignore_sections: tuple[str, ...] = ()
    entry_buttons: tuple[str, ...] = ()
    next_buttons: tuple[str, ...] = DEFAULT_NEXT_BUTTONS
    final_buttons: tuple[str, ...] = DEFAULT_FINAL_BUTTONS
    hints: dict[str, Any] = field(default_factory=dict)
    notes: str = ""


@dataclass(frozen=True)
class TemplateCatalog:
    defaults: dict[str, Any]
    rules: tuple[TemplateRule, ...]


def _compile(value: str) -> re.Pattern[str] | None:
    try:
        return re.compile(str(value), re.IGNORECASE)
    except re.error:
        return None


@lru_cache(maxsize=1)
def load_catalog() -> TemplateCatalog:
    raw: dict[str, Any] = {}
    try:
        loaded = yaml.safe_load(CATALOG_PATH.read_text(encoding="utf-8")) or {}
        if isinstance(loaded, dict):
            raw = loaded
    except (OSError, yaml.YAMLError):
        raw = {}
    defaults = raw.get("defaults") if isinstance(raw.get("defaults"), dict) else {}
    default_ignore = tuple(str(item) for item in defaults.get("ignore_sections") or DEFAULT_IGNORE_SECTIONS)
    default_next = tuple(str(item) for item in defaults.get("next_buttons") or DEFAULT_NEXT_BUTTONS)
    default_final = tuple(str(item) for item in defaults.get("final_buttons") or DEFAULT_FINAL_BUTTONS)
    rules: list[TemplateRule] = []
    for item in raw.get("templates") or []:
        if not isinstance(item, dict) or not item.get("id"):
            continue
        url_patterns = tuple(
            pattern for pattern in (_compile(value) for value in item.get("url_patterns") or []) if pattern
        )
        markers = tuple(
            pattern for pattern in (_compile(value) for value in item.get("markers") or []) if pattern
        )
        ignore = tuple(str(value) for value in item.get("ignore_sections") or [])
        hints = item.get("hints") if isinstance(item.get("hints"), dict) else {}
        rules.append(TemplateRule(
            id=str(item["id"]),
            name=str(item.get("name") or item["id"]),
            vendor=str(item.get("vendor") or ""),
            priority=int(item.get("priority") or 0),
            url_patterns=url_patterns,
            markers=markers,
            ignore_sections=default_ignore + ignore,
            entry_buttons=tuple(str(value) for value in item.get("entry_buttons") or []),
            next_buttons=tuple(str(value) for value in item.get("next_buttons") or default_next),
            final_buttons=tuple(str(value) for value in item.get("final_buttons") or default_final),
            hints={str(key): value for key, value in hints.items()},
            notes=str(item.get("notes") or ""),
        ))
    if not rules:
        rules.append(TemplateRule(
            id="generic",
            name="通用网申",
            vendor="通用",
            url_patterns=(re.compile(".*"),),
            ignore_sections=default_ignore,
            next_buttons=default_next,
            final_buttons=default_final,
            hints={"generic": True},
        ))
    return TemplateCatalog(
        defaults={"ignore_sections": default_ignore, "next_buttons": default_next, "final_buttons": default_final},
        rules=tuple(rules),
    )


def template_catalog() -> list[dict[str, Any]]:
    catalog = load_catalog()
    return [
        {
            "id": rule.id,
            "name": rule.name,
            "vendor": rule.vendor,
            "priority": rule.priority,
            "url_patterns": [pattern.pattern for pattern in rule.url_patterns],
            "markers": [pattern.pattern for pattern in rule.markers],
            "entry_buttons": list(rule.entry_buttons),
            "next_buttons": list(rule.next_buttons),
            "final_buttons": list(rule.final_buttons),
            "ignore_sections": list(rule.ignore_sections),
            "hints": dict(rule.hints),
            "notes": rule.notes,
        }
        for rule in catalog.rules
    ]


def detect_template(page: dict[str, Any], config: dict[str, Any] | None = None) -> dict[str, Any]:
    catalog = load_catalog()
    signature = _page_signature(page)
    url = str(page.get("url") or "")
    best: tuple[int, TemplateRule, list[str], bool] | None = None
    for rule in catalog.rules:
        if rule.id == "generic":
            continue
        evidence: list[str] = []
        url_hits = [pattern.pattern for pattern in rule.url_patterns if pattern.search(url)]
        marker_hits = [pattern.pattern for pattern in rule.markers if pattern.search(signature)]
        if url_hits:
            evidence.extend(f"URL:{item}" for item in url_hits[:3])
        if marker_hits:
            evidence.extend(f"标记:{item}" for item in marker_hits[:4])
        if not url_hits and len(marker_hits) < 2:
            continue
        if not rule.url_patterns and not marker_hits:
            continue
        score = (100 if url_hits else 0) + len(marker_hits) * 12 + min(rule.priority, 99)
        if url_hits and _buttons_present(page, rule.final_buttons):
            score += 4
        if best is None or score > best[0]:
            best = (score, rule, evidence, bool(url_hits))

    if best:
        score, rule, evidence, url_hit = best
        confidence = "high" if url_hit and len(evidence) > len(rule.url_patterns[:3]) else "medium"
        if url_hit and any(item.startswith("标记:") for item in evidence):
            confidence = "high"
        payload = _rule_payload(
            rule,
            confidence=confidence,
            evidence=evidence,
            generic=False,
        )
    else:
        payload = _generic_payload(catalog)

    _apply_site_overrides(payload, url, config)
    return payload


def _rule_payload(
    rule: TemplateRule,
    *,
    confidence: str,
    evidence: list[str],
    generic: bool,
) -> dict[str, Any]:
    return {
        "id": rule.id,
        "name": rule.name,
        "vendor": rule.vendor,
        "confidence": confidence,
        "evidence": evidence,
        "generic": generic,
        "ignore_sections": list(rule.ignore_sections),
        "entry_buttons": list(rule.entry_buttons),
        "next_buttons": list(rule.next_buttons),
        "final_buttons": list(rule.final_buttons),
        "hints": dict(rule.hints),
        "notes": rule.notes,
    }


def _generic_payload(catalog: TemplateCatalog) -> dict[str, Any]:
    rule = next((item for item in catalog.rules if item.id == "generic"), None)
    if rule is None:
        rule = TemplateRule(
            id="generic",
            name="通用网申",
            vendor="通用",
            url_patterns=(re.compile(".*"),),
            ignore_sections=tuple(DEFAULT_IGNORE_SECTIONS),
            hints={"generic": True},
        )
    return _rule_payload(
        rule,
        confidence="low",
        evidence=["未命中已知模板，使用通用引擎"],
        generic=True,
    )


def _page_signature(page: dict[str, Any]) -> str:
    tech = page.get("tech") if isinstance(page.get("tech"), dict) else {}
    buttons = " ".join(
        str(item.get("text") or "")
        for item in page.get("buttons") or []
        if isinstance(item, dict)
    )
    headings = " ".join(str(item) for item in page.get("headings") or [])[:1500]
    scripts = " ".join(str(item) for item in tech.get("scripts") or [])
    parts = [
        str(page.get("url") or ""),
        str(page.get("title") or ""),
        str(page.get("page_text") or "")[:2500],
        headings,
        scripts,
        str(tech.get("class_hint") or ""),
        buttons,
    ]
    return "\n".join(part for part in parts if part)


def _buttons_present(page: dict[str, Any], wanted: tuple[str, ...]) -> bool:
    texts = [
        _normalize(item.get("text") or "")
        for item in page.get("buttons") or []
        if isinstance(item, dict)
    ]
    return any(_normalize(button) in text for text in texts for button in wanted if text)


def _normalize(value: Any) -> str:
    return re.sub(r"[\s_\-*:：·•/\\（）()【】\[\]<>]+", "", str(value or "").lower())


def _apply_site_overrides(payload: dict[str, Any], url: str, config: dict[str, Any] | None) -> None:
    sites = (config or {}).get("sites")
    if not isinstance(sites, dict) or not url:
        return
    for key, override in sites.items():
        if not isinstance(override, dict):
            continue
        if str(key).lower() not in url.lower():
            continue
        if override.get("template"):
            base = next((item for item in load_catalog().rules if item.id == str(override["template"])), None)
            if base:
                payload.update(_rule_payload(
                    base,
                    confidence="high",
                    evidence=[f"站点覆盖:{key}"],
                    generic=base.id == "generic",
                ))
        for field_name in ("name", "vendor", "notes"):
            if override.get(field_name):
                payload[field_name] = str(override[field_name])
        for field_name in ("entry_buttons", "next_buttons", "final_buttons"):
            if isinstance(override.get(field_name), list):
                payload[field_name] = [str(item) for item in override[field_name]]
        if isinstance(override.get("ignore_sections"), list):
            payload["ignore_sections"] = list(payload.get("ignore_sections") or []) + [
                str(item) for item in override["ignore_sections"]
            ]
        if isinstance(override.get("hints"), dict):
            payload["hints"] = {**(payload.get("hints") or {}), **override["hints"]}
        payload["site_override"] = str(key)
        payload["confidence"] = "high"
        payload["evidence"] = list(payload.get("evidence") or []) + [f"站点覆盖:{key}"]
        break
