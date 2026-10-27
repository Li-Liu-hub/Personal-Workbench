from datetime import datetime, timedelta, timezone

from officialapply.registry import Registry, normalize_url


def _registry(tmp_path):
    return Registry(tmp_path / "applications.jsonl")


def test_normalize_url_strips_tracking_and_fragment():
    left = "HTTPS://Jobs.Example.com/apply/123/?utm_source=x&ref=abc#top"
    right = "https://jobs.example.com/apply/123"
    assert normalize_url(left) == right
    assert normalize_url("https://jobs.example.com/apply/123/") == right
    assert normalize_url("https://jobs.example.com:443/apply/123") == right


def test_append_today_and_duplicate_guard(tmp_path):
    registry = _registry(tmp_path)
    now = datetime.now(timezone.utc)
    registry.append({
        "status": "submitted",
        "url": "https://jobs.example.com/apply/1?utm_source=a",
        "company": "示例公司",
        "title": "后端开发",
        "at": now.isoformat(timespec="seconds"),
        "date": now.date().isoformat(),
    })
    today = registry.today(now.date().isoformat())
    assert today["submitted"] == 1
    assert today["counted"] == 1

    guard = registry.check(
        "https://jobs.example.com/apply/1",
        company="示例公司",
        title="后端开发",
        daily_limit=10,
        min_interval_seconds=0,
        now=now,
    )
    assert guard["duplicate_url"] is not None
    assert guard["can_submit"] is False


def test_daily_limit_and_interval(tmp_path):
    registry = _registry(tmp_path)
    now = datetime.now(timezone.utc).replace(microsecond=0)
    registry.append({
        "status": "unconfirmed",
        "url": "https://jobs.example.com/apply/2",
        "at": now.isoformat(timespec="seconds"),
        "date": now.date().isoformat(),
    })
    guard = registry.check(
        "https://jobs.example.com/apply/3",
        daily_limit=1,
        min_interval_seconds=60,
        now=now + timedelta(seconds=10),
    )
    assert guard["limit_reached"] is True
    assert guard["interval_remaining_seconds"] == 50
    assert guard["can_submit"] is False

    later = registry.check(
        "https://jobs.example.com/apply/3",
        daily_limit=5,
        min_interval_seconds=60,
        now=now + timedelta(seconds=61),
    )
    assert later["limit_reached"] is False
    assert later["interval_remaining_seconds"] == 0
    assert later["can_submit"] is True


def test_similar_title_is_flagged_but_not_blocking(tmp_path):
    registry = _registry(tmp_path)
    registry.append({
        "status": "submitted",
        "url": "https://jobs.example.com/apply/4",
        "company": "示例公司",
        "title": "后端开发",
    })
    guard = registry.check(
        "https://jobs.example.com/apply/5",
        company="示例公司",
        title="后端开发",
        daily_limit=10,
    )
    assert guard["duplicate_url"] is None
    assert guard["similar_title"] is not None
    assert guard["can_submit"] is True


def test_update_rewrites_record(tmp_path):
    registry = _registry(tmp_path)
    record = registry.append({"status": "unconfirmed", "url": "https://jobs.example.com/apply/6"})
    updated = registry.update(record["id"], {"status": "submitted"})
    assert updated["status"] == "submitted"
    assert registry.history()[0]["status"] == "submitted"
