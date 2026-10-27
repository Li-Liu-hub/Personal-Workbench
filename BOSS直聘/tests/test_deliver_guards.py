from __future__ import annotations

import pytest

from bosshunter.boss.deliver import BossDeliverer
from bosshunter.config import load_config
from bosshunter.errors import ToolError
from bosshunter.safety import SafetyState

JOB_URL = "https://www.zhipin.com/job_detail/abc123.html"


def make_deliverer(tmp_path):
    config = load_config(tmp_path / "config.yaml")
    config["delivery"]["send_windows"] = []
    state = SafetyState(tmp_path / "state.json", config)
    return BossDeliverer(config, state)


def test_greeting_too_long_rejected(tmp_path):
    deliverer = make_deliverer(tmp_path)
    with pytest.raises(ToolError) as exc:
        deliverer.deliver(JOB_URL, greeting="x" * 501)
    assert exc.value.code == "greeting_too_long"


def test_explicit_empty_images_rejected(tmp_path):
    deliverer = make_deliverer(tmp_path)
    with pytest.raises(ToolError) as exc:
        deliverer.deliver(JOB_URL, images=[])
    assert exc.value.code == "no_images"


def test_missing_image_rejected(tmp_path):
    deliverer = make_deliverer(tmp_path)
    with pytest.raises(ToolError) as exc:
        deliverer.deliver(JOB_URL, images=[str(tmp_path / "missing.png")])
    assert exc.value.code == "image_not_found"


def test_unsupported_image_rejected(tmp_path):
    deliverer = make_deliverer(tmp_path)
    path = tmp_path / "resume.txt"
    path.write_text("x", encoding="utf-8")
    with pytest.raises(ToolError) as exc:
        deliverer.deliver(JOB_URL, images=[str(path)])
    assert exc.value.code == "unsupported_image"


def test_delivered_job_is_skipped_without_browser(tmp_path):
    deliverer = make_deliverer(tmp_path)
    image = tmp_path / "intro.png"
    image.write_bytes(b"\x89PNG\r\n\x1a\n")
    deliverer.state.record_delivery("abc123", status="sent", title="Java")
    result = deliverer.deliver(JOB_URL, images=[str(image)])
    assert result["skipped"] is True
    assert result["reason"] == "already_delivered"


def test_send_images_requires_configured_images(tmp_path):
    deliverer = make_deliverer(tmp_path)
    with pytest.raises(ToolError) as exc:
        deliverer.send_images(JOB_URL)
    assert exc.value.code == "no_images"


def test_invalid_job_url_rejected(tmp_path):
    deliverer = make_deliverer(tmp_path)
    with pytest.raises(ToolError) as exc:
        deliverer.deliver("https://www.zhipin.com/web/geek/job")
    assert exc.value.code == "invalid_job_url"
