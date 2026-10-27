"""Configuration for the BOSS-only job tool."""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

import yaml

PACKAGE_DIR = Path(__file__).resolve().parent
BOSS_CITIES_FILE = PACKAGE_DIR / "data" / "boss_cities.json"

DEFAULT_GREETING = "您好，我对这个岗位很感兴趣，方便聊聊吗？"

DEFAULTS: dict[str, Any] = {
    "profile": {
        "resume_path": "",
    },
    "search": {
        "keywords": [],
        "cities": ["北京"],
        "city_codes": {},
        "max_pages": 3,
        "sort": "default",
        "filters": {},
    },
    "delivery": {
        "greeting": DEFAULT_GREETING,
        "images": [],
        "daily_limit": 50,
        "interval_min": 60,
        "interval_max": 180,
        "send_windows": ["09:00-18:00"],
        "browse_before_greet": False,
        "browse_duration_min": 3,
        "browse_duration_max": 8,
    },
    "safety": {
        "daily_page_limit": 1100,
        "daily_search_page_limit": 60,
        "daily_detail_page_limit": 900,
        "risk_lock_minutes": 10,
        "max_consecutive_failures": 3,
    },
    "browser": {
        "chrome_path": "",
        "debug_port": 9333,
        "user_data_dir": "./data/chrome-profile",
        "auto_launch": True,
        "launch_timeout_seconds": 25,
        "headless": False,
    },
}

_SEND_WINDOW = re.compile(r"\s*([0-9]{2}):([0-9]{2})\s*-\s*([0-9]{2}):([0-9]{2})\s*")


class ConfigError(ValueError):
    """Raised when config.yaml cannot be used safely."""


def load_config(config_path: str | Path | None = None) -> dict[str, Any]:
    path = Path(config_path or "config.yaml").expanduser()
    cfg = _deep_copy(DEFAULTS)
    if path.exists():
        raw = _load_yaml(path)
        _deep_merge(cfg, raw)
    _normalize(cfg)
    validate_config(cfg)
    cfg["_config_path"] = str(path.resolve())
    cfg["_base_dir"] = str(path.resolve().parent)
    cfg["_state_path"] = str(path.resolve().parent / "data" / "state.json")
    cfg["_queue_path"] = str(path.resolve().parent / "data" / "apply_queue.json")
    cfg["_queue_log_path"] = str(path.resolve().parent / "data" / "apply_queue_log.jsonl")
    return cfg


def save_config(config: dict[str, Any], config_path: str | Path | None = None) -> Path:
    path = Path(config_path or "config.yaml").expanduser()
    public = {key: value for key, value in config.items() if not key.startswith("_")}
    _normalize(public)
    validate_config(public)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        yaml.safe_dump(public, allow_unicode=True, sort_keys=False, default_flow_style=False),
        encoding="utf-8",
    )
    return path


def validate_config(config: dict[str, Any]) -> None:
    search = config["search"]
    if not isinstance(search["keywords"], list) or not all(isinstance(item, str) for item in search["keywords"]):
        raise ConfigError("search.keywords 必须是字符串列表")
    if not isinstance(search["cities"], list) or not all(isinstance(item, str) for item in search["cities"]):
        raise ConfigError("search.cities 必须是字符串列表")
    if not isinstance(search["city_codes"], dict):
        raise ConfigError("search.city_codes 必须是城市名称到编码的映射")
    search["max_pages"] = _bounded_int(search["max_pages"], 1, 50, "search.max_pages")
    if search["sort"] not in {"default", "newest"}:
        raise ConfigError("search.sort 只能是 default 或 newest")
    if not isinstance(search["filters"], dict):
        raise ConfigError("search.filters 必须是配置对象")

    delivery = config["delivery"]
    if not isinstance(delivery["greeting"], str):
        raise ConfigError("delivery.greeting 必须是字符串")
    if not isinstance(delivery["images"], list) or not all(isinstance(item, str) for item in delivery["images"]):
        raise ConfigError("delivery.images 必须是图片路径列表")
    delivery["daily_limit"] = _bounded_int(delivery["daily_limit"], 1, 10000, "delivery.daily_limit")
    delivery["interval_min"] = _bounded_int(delivery["interval_min"], 1, 86400, "delivery.interval_min")
    delivery["interval_max"] = _bounded_int(delivery["interval_max"], 1, 86400, "delivery.interval_max")
    if delivery["interval_max"] < delivery["interval_min"]:
        raise ConfigError("delivery.interval_max 不能小于 interval_min")
    delivery["send_windows"] = validate_send_windows(delivery["send_windows"])
    for key in ("browse_duration_min", "browse_duration_max"):
        delivery[key] = _bounded_int(delivery[key], 0, 120, f"delivery.{key}")
    if delivery["browse_duration_max"] < delivery["browse_duration_min"]:
        raise ConfigError("delivery.browse_duration_max 不能小于 browse_duration_min")

    safety = config["safety"]
    for key in (
        "daily_page_limit",
        "daily_search_page_limit",
        "daily_detail_page_limit",
        "risk_lock_minutes",
        "max_consecutive_failures",
    ):
        safety[key] = _bounded_int(safety[key], 1, 100000, f"safety.{key}")

    browser = config["browser"]
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


def validate_send_windows(value: Any) -> list[str]:
    if not isinstance(value, list):
        raise ConfigError("delivery.send_windows 必须是 HH:MM-HH:MM 字符串列表")
    parsed: list[tuple[int, int, int, int, str]] = []
    seen: set[tuple[int, int, int, int]] = set()
    for item in value:
        if not isinstance(item, str):
            raise ConfigError("delivery.send_windows 中的时间段必须是字符串")
        match = _SEND_WINDOW.fullmatch(item)
        if not match:
            raise ConfigError("发送时间窗口格式必须为 HH:MM-HH:MM")
        start_h, start_m, end_h, end_m = map(int, match.groups())
        if start_h > 23 or end_h > 23 or start_m > 59 or end_m > 59:
            raise ConfigError("发送时间窗口必须使用有效的 00:00 至 23:59 时间")
        if start_h * 60 + start_m >= end_h * 60 + end_m:
            raise ConfigError("发送时间窗口的结束时间必须晚于开始时间，不支持跨午夜")
        key = (start_h, start_m, end_h, end_m)
        if key in seen:
            raise ConfigError("发送时间窗口不能重复")
        seen.add(key)
        parsed.append((*key, f"{start_h:02d}:{start_m:02d}-{end_h:02d}:{end_m:02d}"))
    parsed.sort()
    for (_, _, prev_end_h, prev_end_m, _), (next_h, next_m, _, _, _) in zip(parsed, parsed[1:]):
        if next_h * 60 + next_m < prev_end_h * 60 + prev_end_m:
            raise ConfigError("发送时间窗口不能重叠；相邻时间段可以共用边界")
    return [item[-1] for item in parsed]


def parse_send_windows(windows: list[str]) -> list[tuple[int, int]]:
    result = []
    for item in windows:
        match = _SEND_WINDOW.fullmatch(item)
        if not match:
            continue
        start_h, start_m, end_h, end_m = map(int, match.groups())
        result.append((start_h * 60 + start_m, end_h * 60 + end_m))
    return result


_city_cache: dict[str, str] | None = None


def _city_lookup() -> dict[str, str]:
    global _city_cache
    if _city_cache is not None:
        return _city_cache
    lookup: dict[str, str] = {"全国": "100010000"}
    try:
        data = json.loads(BOSS_CITIES_FILE.read_text(encoding="utf-8"))
        for item in data.get("cities", []):
            name = str(item.get("name") or "").strip()
            code = str(item.get("code") or "").strip()
            if name and code:
                lookup[name] = code
    except (OSError, json.JSONDecodeError):
        pass
    lookup.update({
        "北京": "101010100",
        "上海": "101020100",
        "深圳": "101280600",
        "广州": "101280100",
        "杭州": "101210100",
        "成都": "101270100",
        "武汉": "101200100",
        "南京": "101190100",
        "西安": "101110100",
        "苏州": "101190400",
        "天津": "101030100",
        "重庆": "101040100",
        "郑州": "101180100",
        "长沙": "101250100",
        "东莞": "101281600",
        "佛山": "101280800",
        "合肥": "101220100",
        "厦门": "101230200",
        "青岛": "101120200",
        "大连": "101070200",
        "无锡": "101190200",
        "宁波": "101210400",
    })
    _city_cache = lookup
    return lookup


def city_code_for(city: str, config: dict[str, Any]) -> str | None:
    name = str(city or "").strip()
    if not name:
        return None
    custom = config.get("search", {}).get("city_codes", {})
    if isinstance(custom, dict):
        code = str(custom.get(name) or "").strip()
        if code:
            return code
    lookup = _city_lookup()
    for candidate in (name, name.removesuffix("市")):
        code = lookup.get(candidate)
        if code:
            return code
    return None


def resolve_path(value: str, config: dict[str, Any]) -> Path:
    path = Path(value).expanduser()
    if not path.is_absolute():
        path = Path(config.get("_base_dir") or ".") / path
    return path.resolve()


def resolve_paths(values: list[str], config: dict[str, Any]) -> list[str]:
    return [str(resolve_path(value, config)) for value in values]


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
