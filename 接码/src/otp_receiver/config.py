"""Configuration loading and validation for the standalone OTP receiver."""

from __future__ import annotations

import copy
import ipaddress
from pathlib import Path
from typing import Any

import yaml


class ConfigError(ValueError):
    """Raised when config.yaml cannot be used safely."""


DEFAULTS: dict[str, Any] = {
    "otp": {
        "bind_ip": "auto",
        "port": 8765,
        "ttl_seconds": 120,
        "token_file": "./data/otp_token.txt",
        "url_file": "./data/otp_url.txt",
        "runtime_file": "./data/otp_runtime.json",
        "max_clock_skew_seconds": 600,
        "max_requests_per_minute": 60,
    }
}


def resolve_path(value: str, base_dir: Path) -> Path:
    path = Path(value).expanduser()
    if not path.is_absolute():
        path = base_dir / path
    return path.resolve()


def load_config(config_path: str | Path | None = None) -> dict[str, Any]:
    path = Path(config_path or "config.yaml").expanduser()
    cfg = copy.deepcopy(DEFAULTS)
    if path.exists():
        raw = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
        if not isinstance(raw, dict):
            raise ConfigError("配置文件必须是 YAML 对象")
        section = raw.get("otp", raw)
        if not isinstance(section, dict):
            raise ConfigError("otp 必须是配置对象")
        unknown = sorted(set(section) - set(DEFAULTS["otp"]))
        if unknown:
            raise ConfigError(f"未知配置项: {', '.join(unknown)}")
        cfg["otp"].update(section)

    otp = cfg["otp"]
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
    for key in ("token_file", "url_file", "runtime_file"):
        if not isinstance(otp[key], str) or not otp[key].strip():
            raise ConfigError(f"otp.{key} 必须是非空字符串")

    base_dir = path.resolve().parent
    cfg["_config_path"] = str(path.resolve())
    cfg["_base_dir"] = str(base_dir)
    cfg["_data_dir"] = str(base_dir / "data")
    cfg["_token_path"] = str(resolve_path(otp["token_file"], base_dir))
    cfg["_url_path"] = str(resolve_path(otp["url_file"], base_dir))
    cfg["_runtime_path"] = str(resolve_path(otp["runtime_file"], base_dir))
    return cfg


def _bounded_int(value: Any, low: int, high: int, name: str) -> int:
    if isinstance(value, bool):
        raise ConfigError(f"{name} 必须是整数")
    try:
        number = int(value)
    except (TypeError, ValueError) as exc:
        raise ConfigError(f"{name} 必须是整数") from exc
    if not low <= number <= high:
        raise ConfigError(f"{name} 必须在 {low} 到 {high} 之间")
    return number
