from __future__ import annotations

import time
from datetime import datetime

import pytest

from bosshunter.config import load_config
from bosshunter.safety import SafetyError, SafetyState, next_window_text, send_window_active


def make_state(tmp_path, **delivery):
    config = load_config(tmp_path / "config.yaml")
    config["delivery"].update(delivery)
    return SafetyState(tmp_path / "state.json", config)


def test_page_budget(tmp_path):
    state = make_state(tmp_path)
    state.reserve_page("search", daily_limit=2)
    state.reserve_page("search", daily_limit=2)
    with pytest.raises(SafetyError) as exc:
        state.reserve_page("search", daily_limit=2)
    assert exc.value.code == "daily_search_limit"


def test_total_page_budget(tmp_path):
    state = make_state(tmp_path)
    state.config["safety"]["daily_page_limit"] = 2
    state.reserve_page("detail")
    state.reserve_page("chat")
    with pytest.raises(SafetyError) as exc:
        state.reserve_page("search")
    assert exc.value.code == "daily_page_limit"


def test_send_windows_helpers():
    windows = ["09:00-12:00", "14:00-18:00"]
    assert send_window_active(windows, datetime(2026, 1, 1, 10, 0))
    assert not send_window_active(windows, datetime(2026, 1, 1, 13, 0))
    assert next_window_text(windows, datetime(2026, 1, 1, 13, 0)) == "今日 14:00"
    assert next_window_text(windows, datetime(2026, 1, 1, 20, 0)) == "明日 09:00"
    assert send_window_active([])


def test_send_daily_limit_and_cooldown(tmp_path):
    state = make_state(tmp_path, send_windows=[], daily_limit=2, interval_min=60, interval_max=60)
    state.check_send_allowed(0)
    state.record_delivery("job1", status="sent")
    state.data["last_send_at"] = time.time()
    with pytest.raises(SafetyError) as exc:
        state.check_send_allowed(60)
    assert exc.value.code == "send_cooldown"
    state.check_send_allowed(0)
    state.record_delivery("job2", status="sent")
    with pytest.raises(SafetyError) as exc:
        state.check_send_allowed(0)
    assert exc.value.code == "daily_send_limit"


def test_delivery_statuses_and_dedup(tmp_path):
    state = make_state(tmp_path, send_windows=[])
    state.record_delivery("sent", status="sent")
    state.record_delivery("ambiguous", status="ambiguous")
    state.record_delivery("failed", status="failed")
    assert state.is_delivered("sent")
    assert state.is_delivered("ambiguous")
    assert not state.is_delivered("failed")
    assert state.data["sends"] == 2
    assert state.data["consecutive_failures"] == 1
    state.record_delivery("ok", status="sent")
    assert state.data["consecutive_failures"] == 0


def test_risk_lock_and_snapshot(tmp_path):
    state = make_state(tmp_path)
    state.lock("captcha", 5)
    with pytest.raises(SafetyError) as exc:
        state.ensure_unlocked()
    assert exc.value.code == "risk_lock"
    snapshot = state.snapshot()
    assert snapshot["lock"]["reason"] == "captcha"


def test_reload_and_day_rollover(tmp_path):
    state = make_state(tmp_path, send_windows=[])
    state.record_delivery("old", status="sent")
    state.data["date"] = "2000-01-01"
    state.save()
    reloaded = make_state(tmp_path, send_windows=[])
    assert reloaded.data["date"] != "2000-01-01"
    assert reloaded.data["sends"] == 0
    assert "old" in reloaded.data["deliveries"]
    reloaded.reserve_page("search")
    reloaded.save()
    state.reload()
    assert state.data["pages"]["search"] == 1
