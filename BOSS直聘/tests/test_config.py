from __future__ import annotations

import pytest
import yaml

from bosshunter.config import (
    ConfigError,
    city_code_for,
    load_config,
    resolve_path,
    validate_send_windows,
)


def write_config(tmp_path, data):
    path = tmp_path / "config.yaml"
    path.write_text(yaml.safe_dump(data, allow_unicode=True), encoding="utf-8")
    return path


def test_missing_file_uses_defaults(tmp_path):
    config = load_config(tmp_path / "config.yaml")
    assert config["search"]["cities"] == ["北京"]
    assert config["delivery"]["greeting"]
    assert config["browser"]["debug_port"] == 9333
    assert config["_base_dir"] == str(tmp_path.resolve())
    assert config["_state_path"] == str((tmp_path / "data" / "state.json").resolve())


def test_custom_values_merge_with_defaults(tmp_path):
    path = write_config(
        tmp_path,
        {
            "search": {"keywords": ["Java"], "cities": ["上海"], "city_codes": {"上海": "999"}},
            "delivery": {"images": ["a.png"], "interval_min": 10, "interval_max": 20},
            "browser": {"debug_port": 9444},
        },
    )
    config = load_config(path)
    assert config["search"]["keywords"] == ["Java"]
    assert config["search"]["max_pages"] == 3
    assert config["delivery"]["daily_limit"] == 50
    assert config["delivery"]["interval_min"] == 10
    assert config["browser"]["debug_port"] == 9444


def test_invalid_interval_order_rejected(tmp_path):
    path = write_config(tmp_path, {"delivery": {"interval_min": 100, "interval_max": 10}})
    with pytest.raises(ConfigError, match="interval_max"):
        load_config(path)


def test_send_windows_validation_and_sorting():
    assert validate_send_windows(["14:00-18:00", "09:00-12:00"]) == ["09:00-12:00", "14:00-18:00"]
    with pytest.raises(ConfigError, match="跨午夜"):
        validate_send_windows(["18:00-09:00"])
    with pytest.raises(ConfigError, match="重叠"):
        validate_send_windows(["09:00-12:00", "11:00-14:00"])
    with pytest.raises(ConfigError, match="格式"):
        validate_send_windows(["9:00-18:00"])


def test_city_code_custom_and_builtin(tmp_path):
    config = load_config(tmp_path / "config.yaml")
    assert city_code_for("上海", config) == "101020100"
    assert city_code_for("上海市", config) == "101020100"
    config["search"]["city_codes"]["测试城"] = "123456"
    assert city_code_for("测试城", config) == "123456"
    assert city_code_for("不存在城市", config) is None


def test_relative_paths_resolve_against_config_dir(tmp_path):
    config = load_config(tmp_path / "config.yaml")
    assert resolve_path("data/a.png", config) == (tmp_path / "data" / "a.png").resolve()
