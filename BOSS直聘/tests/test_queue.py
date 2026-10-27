from __future__ import annotations

from datetime import datetime

import pytest

from bosshunter.boss.search import job_id_from_url
from bosshunter.config import load_config
from bosshunter.errors import ToolError
from bosshunter.queue import ApplyQueue, QueueRunner, seconds_until_next_window
from bosshunter.safety import SafetyState


def make_config(tmp_path, **delivery):
    config = load_config(tmp_path / "config.yaml")
    config["delivery"].update(
        {
            "send_windows": [],
            "daily_limit": 50,
            "interval_min": 1,
            "interval_max": 1,
            **delivery,
        }
    )
    return config


def make_state(tmp_path, **delivery):
    config = make_config(tmp_path, **delivery)
    return config, SafetyState(config["_state_path"], config)


def job(job_id: str, **extra):
    return {
        "job_id": job_id,
        "url": f"https://www.zhipin.com/job_detail/{job_id}.html",
        "title": f"岗位 {job_id}",
        "company": "测试公司",
        "city": "上海",
        **extra,
    }


class FakeDeliverer:
    def __init__(self, state, outcomes=None):
        self.state = state
        self.calls = []
        self.outcomes = list(outcomes or [])

    def deliver(self, url, **kwargs):
        self.calls.append({"url": url, **kwargs})
        if self.outcomes:
            outcome = self.outcomes.pop(0)
            if isinstance(outcome, Exception):
                raise outcome
        job_id = job_id_from_url(url)
        self.state.record_delivery(
            job_id,
            status="sent",
            title=f"岗位 {job_id}",
            company="测试公司",
            url=url,
            greeting="你好",
            images=[],
        )
        return {
            "skipped": False,
            "partial": False,
            "job_id": job_id,
            "title": f"岗位 {job_id}",
            "company": "测试公司",
            "greeting": {"requested": True, "state": "sent"},
            "images": {"requested": 0, "sent": 0},
            "delivery": {"title": f"岗位 {job_id}", "company": "测试公司"},
            "warnings": [],
        }


def make_queue(config):
    return ApplyQueue(config["_queue_path"], config)


def test_queue_add_update_remove_and_reset(tmp_path):
    config = make_config(tmp_path)
    queue = make_queue(config)
    result = queue.add([job("a"), job("b"), job("a"), {"title": "无效"}])
    assert result["added"] == 2
    assert result["duplicates"] == 1
    assert result["invalid"] == 1
    assert queue.summary()["pending"] == 2

    item = queue.update("a", {"status": "failed", "priority": 9, "note": "手动标记"})
    assert item["status"] == "failed"
    assert item["priority"] == 9
    assert item["note"] == "手动标记"

    reset = queue.reset(["failed"])
    assert reset["reset"] == 1
    assert queue.find("a")["status"] == "pending"

    removed = queue.remove(["b"])
    assert removed["removed"] == 1
    assert queue.summary()["total"] == 1

    reloaded = make_queue(config)
    assert reloaded.find("a") is not None
    assert reloaded.find("b") is None


def test_queue_rejects_corrupt_file(tmp_path):
    config = make_config(tmp_path)
    config["_queue_path"] = str(tmp_path / "bad.json")
    (tmp_path / "bad.json").write_text("{not json", encoding="utf-8")
    with pytest.raises(ToolError) as exc:
        make_queue(config)
    assert exc.value.code == "queue_corrupt"


def test_runner_priority_and_daily_limit(tmp_path):
    config, state = make_state(tmp_path, daily_limit=2)
    queue = make_queue(config)
    queue.add([job("low", priority=0), job("high", priority=10), job("mid", priority=5)])
    deliverer = FakeDeliverer(state)
    runner = QueueRunner(config, state, deliverer=deliverer, log_path=tmp_path / "run.jsonl")

    result = runner.run()
    queue = make_queue(config)

    assert [job_id_from_url(call["url"]) for call in deliverer.calls] == ["high", "mid"]
    assert result["reason"] == "daily_send_limit"
    assert result["sent"] == 2
    assert queue.find("high")["status"] == "sent"
    assert queue.find("mid")["status"] == "sent"
    assert queue.find("low")["status"] == "pending"
    assert queue.summary()["pending"] == 1


def test_runner_skips_already_delivered(tmp_path):
    config, state = make_state(tmp_path)
    queue = make_queue(config)
    queue.add([job("old"), job("new")])
    state.record_delivery("old", status="sent", title="岗位 old")
    deliverer = FakeDeliverer(state)
    runner = QueueRunner(config, state, deliverer=deliverer, log_path=tmp_path / "run.jsonl")

    result = runner.run()
    queue = make_queue(config)

    assert [job_id_from_url(call["url"]) for call in deliverer.calls] == ["new"]
    assert result["skipped"] == 1
    assert result["sent"] == 1
    assert queue.find("old")["status"] == "sent"
    assert "已有投递记录" in queue.find("old")["note"]


def test_runner_stops_on_risk_lock(tmp_path):
    config, state = make_state(tmp_path)
    queue = make_queue(config)
    queue.add([job("risky")])
    deliverer = FakeDeliverer(state, outcomes=[ToolError("risk_lock", "平台风险冷却中", retry_after_seconds=600)])
    runner = QueueRunner(config, state, deliverer=deliverer, log_path=tmp_path / "run.jsonl")

    result = runner.run()
    queue = make_queue(config)

    assert result["reason"] == "risk_lock"
    assert result["attempted"] == 0
    assert queue.find("risky")["status"] == "pending"


def test_runner_marks_ambiguous(tmp_path):
    config, state = make_state(tmp_path)
    queue = make_queue(config)
    queue.add([job("ambiguous")])
    deliverer = FakeDeliverer(state, outcomes=[ToolError("greeting_unverified", "文字消息已提交但无法确认送达")])
    runner = QueueRunner(config, state, deliverer=deliverer, log_path=tmp_path / "run.jsonl")

    result = runner.run()
    queue = make_queue(config)

    assert result["ambiguous"] == 1
    assert result["reason"] == "queue_empty"
    assert queue.find("ambiguous")["status"] == "ambiguous"


def test_runner_stops_after_consecutive_failures(tmp_path):
    config, state = make_state(tmp_path)
    queue = make_queue(config)
    queue.add([job("a"), job("b"), job("c"), job("d")])
    deliverer = FakeDeliverer(
        state,
        outcomes=[ToolError("no_chat_button", "未找到沟通按钮") for _ in range(4)],
    )
    runner = QueueRunner(config, state, deliverer=deliverer, log_path=tmp_path / "run.jsonl")

    result = runner.run()
    queue = make_queue(config)

    assert result["reason"] == "consecutive_failures"
    assert result["failed"] == 3
    assert [queue.find(item)["status"] for item in ("a", "b", "c")] == ["failed", "failed", "failed"]
    assert queue.find("d")["status"] == "pending"


def test_runner_retry_failed(tmp_path):
    config, state = make_state(tmp_path)
    queue = make_queue(config)
    queue.add([job("retry")])
    queue.update("retry", {"status": "failed", "note": "上次失败"})
    deliverer = FakeDeliverer(state)
    runner = QueueRunner(config, state, deliverer=deliverer, log_path=tmp_path / "run.jsonl")

    result = runner.run(retry_failed=True)
    queue = make_queue(config)

    assert result["sent"] == 1
    assert queue.find("retry")["status"] == "sent"


def test_seconds_until_next_window():
    windows = ["09:00-12:00", "14:00-18:00"]
    assert seconds_until_next_window(windows, datetime(2026, 1, 1, 10, 0)) == 0
    assert seconds_until_next_window(windows, datetime(2026, 1, 1, 8, 30)) == 1800
    assert seconds_until_next_window(windows, datetime(2026, 1, 1, 13, 0)) == 3600
    assert seconds_until_next_window(windows, datetime(2026, 1, 1, 20, 0)) == 13 * 3600
    assert seconds_until_next_window([], datetime(2026, 1, 1, 20, 0)) == 0
