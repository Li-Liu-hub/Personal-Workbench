"""Configuration loading and validation."""

from __future__ import annotations

import ipaddress
from pathlib import Path
from typing import Any

import yaml
from officialapply.constants import DOCUMENT_ROLES


class ConfigError(ValueError):
    """Raised when config.yaml cannot be used safely."""


DEFAULTS: dict[str, Any] = {
    "profile": {
        "path": "./官网投递资料.md",
    },
    "browser": {
        "chrome_path": "",
        "debug_port": 9444,
        "user_data_dir": "./data/chrome-profile",
        "auto_launch": True,
        "launch_timeout_seconds": 25,
        "headless": False,
    },
    "apply": {
        "daily_submit_limit": 20,
        "min_submit_interval_seconds": 30,
        "require_no_unknown_required": True,
        "fill_sensitive_fields": True,
        "auto_upload_documents": ["resume", "photo", "transcript"],
        "screenshot_dir": "./data/screenshots",
        "page_load_timeout": 25,
        "accept_terms_default": False,
    },
    "autofill": {
        "max_steps": 6,
        "wait_seconds": 1.2,
        "upload_wait_seconds": 0.8,
        "resume_parse_wait_seconds": 4,
    },
    "otp": {
        "bind_ip": "auto",
        "port": 8765,
        "ttl_seconds": 120,
        "token_file": "./data/otp_token.txt",
        "url_file": "./data/otp_url.txt",
        "runtime_file": "./data/otp_runtime.json",
        "auto_start": True,
        "max_clock_skew_seconds": 600,
        "max_requests_per_minute": 60,
    },
    "sites": {},
}


def load_config(config_path: str | Path | None = None) -> dict[str, Any]:
    path = Path(config_path or "config.yaml").expanduser()
    cfg = _deep_copy(DEFAULTS)
    if path.exists():
        raw = _load_yaml(path)
        _deep_merge(cfg, raw)
    _normalize(cfg)
    validate_config(cfg)
    base_dir = path.resolve().parent
    cfg["_config_path"] = str(path.resolve())
    cfg["_base_dir"] = str(base_dir)
    cfg["_profile_path"] = str(resolve_path(str(cfg["profile"]["path"]), cfg))
    cfg["_data_dir"] = str(base_dir / "data")
    cfg["_registry_path"] = str(base_dir / "data" / "applications.jsonl")
    cfg["_session_path"] = str(base_dir / "data" / "session.json")
    cfg["_screenshot_dir"] = str(resolve_path(str(cfg["apply"]["screenshot_dir"]), cfg))
    cfg["_otp_token_path"] = str(resolve_path(str(cfg["otp"]["token_file"]), cfg))
    cfg["_otp_url_path"] = str(resolve_path(str(cfg["otp"]["url_file"]), cfg))
    cfg["_otp_runtime_path"] = str(resolve_path(str(cfg["otp"]["runtime_file"]), cfg))
    return cfg


def validate_config(config: dict[str, Any]) -> None:
    profile = config["profile"]
    if not isinstance(profile, dict) or not isinstance(profile.get("path"), str) or not profile["path"].strip():
        raise ConfigError("profile.path 必须是非空字符串")

    browser = config["browser"]
    if not isinstance(browser, dict):
        raise ConfigError("browser 必须是配置对象")
    browser["debug_port"] = _bounded_int(browser["debug_port"], 1, 65535, "browser.debug_port")
    browser["launch_timeout_seconds"] = _bounded_int(
        browser["launch_timeout_seconds"], 3, 120, "browser.launch_timeout_seconds"
    )
    if not isinstance(browser["chrome_path"], str):
        raise ConfigError("browser.chrome_path 必须是字符串")
    if not isinstance(browser["user_data_dir"], str) or not browser["user_data_dir"].strip():
        raise ConfigError("browser.user_data_dir 必须是非空字符串")
    for key in ("auto_launch", "headless"):
        if not isinstance(browser[key], bool):
            raise ConfigError(f"browser.{key} 必须是布尔值")

    apply = config["apply"]
    if not isinstance(apply, dict):
        raise ConfigError("apply 必须是配置对象")
    apply["daily_submit_limit"] = _bounded_int(apply["daily_submit_limit"], 1, 1000, "apply.daily_submit_limit")
    apply["min_submit_interval_seconds"] = _bounded_int(
        apply["min_submit_interval_seconds"], 0, 86400, "apply.min_submit_interval_seconds"
    )
    apply["page_load_timeout"] = _bounded_int(apply["page_load_timeout"], 3, 120, "apply.page_load_timeout")
    for key in ("require_no_unknown_required", "fill_sensitive_fields", "accept_terms_default"):
        if not isinstance(apply[key], bool):
            raise ConfigError(f"apply.{key} 必须是布尔值")
    if not isinstance(apply["screenshot_dir"], str) or not apply["screenshot_dir"].strip():
        raise ConfigError("apply.screenshot_dir 必须是非空字符串")
    roles = apply["auto_upload_documents"]
    if not isinstance(roles, list) or not all(isinstance(item, str) for item in roles):
        raise ConfigError("apply.auto_upload_documents 必须是字符串列表")
    unknown = [role for role in roles if role not in DOCUMENT_ROLES]
    if unknown:
        raise ConfigError(f"apply.auto_upload_documents 含未知材料角色: {', '.join(unknown)}")

    autofill = config["autofill"]
    if not isinstance(autofill, dict):
        raise ConfigError("autofill 必须是配置对象")
    autofill["max_steps"] = _bounded_int(autofill["max_steps"], 1, 20, "autofill.max_steps")
    for key in ("wait_seconds", "upload_wait_seconds", "resume_parse_wait_seconds"):
        autofill[key] = _bounded_float(autofill[key], 0, 60, f"autofill.{key}")

    otp = config["otp"]
    if not isinstance(otp, dict):
        raise ConfigError("otp 必须是配置对象")
    if not isinstance(otp["bind_ip"], str) or not otp["bind_ip"].strip():
        raise ConfigError("otp.bind_ip 必须是非空字符串")
    if otp["bind_ip"].strip().lower() != "auto":
        try:
            ipaddress.ip_address(otp["bind_ip"].strip())
        except ValueError as exc:
            raise ConfigError("otp.bind_ip 必须是 auto 或合法的 IP 地址") from exc
    otp["port"] = _bounded_int(otp["port"], 1, 65535, "otp.port")
    otp["ttl_seconds"] = _bounded_int(otp["ttl_seconds"], 30, 600, "otp.ttl_seconds")
    otp["max_clock_skew_seconds"] = _bounded_int(
        otp["max_clock_skew_seconds"], 0, 3600, "otp.max_clock_skew_seconds"
    )
    otp["max_requests_per_minute"] = _bounded_int(
        otp["max_requests_per_minute"], 0, 600, "otp.max_requests_per_minute"
    )
    if not isinstance(otp["auto_start"], bool):
        raise ConfigError("otp.auto_start 必须是布尔值")
    for key in ("token_file", "url_file", "runtime_file"):
        if not isinstance(otp[key], str) or not otp[key].strip():
            raise ConfigError(f"otp.{key} 必须是非空字符串")

    if not isinstance(config["sites"], dict):
        raise ConfigError("sites 必须是对象")


def resolve_path(value: str, config: dict[str, Any]) -> Path:
    path = Path(value).expanduser()
    if not path.is_absolute():
        path = Path(config.get("_base_dir") or ".") / path
    return path.resolve()


def _normalize(config: dict[str, Any]) -> None:
    for section, defaults in DEFAULTS.items():
        current = config.get(section)
        if not isinstance(current, dict):
            config[section] = _deep_copy(defaults)
            continue
        for key, value in defaults.items():
            if key not in current:
                current[key] = _deep_copy(value) if isinstance(value, (dict, list)) else value


def _load_yaml(path: Path) -> dict[str, Any]:
    try:
        raw = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
    except yaml.YAMLError as exc:
        raise ConfigError(f"无法解析配置文件 {path}: {exc}") from exc
    return raw if isinstance(raw, dict) else {}


def _bounded_int(value: Any, minimum: int, maximum: int, name: str) -> int:
    try:
        number = int(value)
    except (TypeError, ValueError) as exc:
        raise ConfigError(f"{name} 必须是整数") from exc
    if not minimum <= number <= maximum:
        raise ConfigError(f"{name} 必须在 {minimum} 到 {maximum} 之间")
    return number


def _bounded_float(value: Any, minimum: float, maximum: float, name: str) -> float:
    try:
        number = float(value)
    except (TypeError, ValueError) as exc:
        raise ConfigError(f"{name} 必须是数字") from exc
    if not minimum <= number <= maximum:
        raise ConfigError(f"{name} 必须在 {minimum} 到 {maximum} 之间")
    return number


def _deep_copy(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: _deep_copy(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_deep_copy(item) for item in value]
    return value


def _deep_merge(base: dict[str, Any], override: dict[str, Any]) -> None:
    for key, value in override.items():
        if key in base and isinstance(base[key], dict) and isinstance(value, dict):
            _deep_merge(base[key], value)
        else:
            base[key] = value
