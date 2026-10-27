import officialapply.app as app_module
from officialapply.app import OfficialApply


def _plan(**values):
    plan = {
        "actions": [],
        "uploads": [],
        "unknown_required": [],
        "missing_known": [],
        "consent_pending": [],
        "warnings": [],
    }
    plan.update(values)
    return plan


def _page(**values):
    page = {
        "url": "https://app.mokahr.com/apply/1",
        "title": "校园招聘网申",
        "page_text": "",
        "field_count": 10,
        "fields": [{"id": "name", "label": "姓名", "kind": "input"}],
        "buttons": [],
        "signals": {},
        "template": {
            "id": "moka",
            "final_buttons": ["预览并提交"],
            "next_buttons": ["下一步"],
        },
    }
    page.update(values)
    return page


def _client(page, plan, *, config=None):
    client = object.__new__(OfficialApply)
    client.config = {
        "autofill": {
            "max_steps": 3,
            "wait_seconds": 0.001,
            "upload_wait_seconds": 0,
            "resume_parse_wait_seconds": 0,
        },
        "apply": {},
        "screenshot_dir": "",
        **(config or {}),
    }
    client.profile = None
    client._current = lambda front=False: object()
    client._inspect = lambda tab, max_text=0: page
    client._inspect_retry = lambda tab, attempts=3: page
    client._plan_page = lambda page, **kwargs: plan
    client._apply_plan = lambda tab, plan, **kwargs: ([], [])
    client._upload_plan = lambda tab, uploads: []
    return client


def test_autofill_stops_before_final_submit(monkeypatch):
    clicks = []
    monkeypatch.setattr(app_module, "click_oa", lambda tab, oid: clicks.append(oid) or {"ok": True})
    page = _page(buttons=[{"id": "final", "text": "预览并提交", "disabled": False}])
    result = _client(page, _plan()).autofill()

    assert result["status"] == "ready_to_submit"
    assert result["success"] is True
    assert result["stopped_before_submit"] is True
    assert result["final_button"]["text"] == "预览并提交"
    assert result["steps"][0]["status"] == "ready_to_submit"
    assert clicks == []


def test_autofill_returns_terms_required_before_submit():
    page = _page(buttons=[{"id": "final", "text": "预览并提交", "disabled": False}])
    plan = _plan(consent_pending=[{"id": "declare", "label": "确认"}])
    result = _client(page, plan).autofill()

    assert result["status"] == "terms_required"
    assert result["success"] is True
    assert result["stopped_before_submit"] is True
    assert result["consent_pending"]


def test_autofill_reports_needs_attention_for_unknown_required():
    page = _page(buttons=[{"id": "final", "text": "预览并提交", "disabled": False}])
    plan = _plan(unknown_required=[{"id": "family", "reason": "家庭成员区块不自动填写"}])
    result = _client(page, plan).autofill()

    assert result["status"] == "needs_attention"
    assert result["success"] is False
    assert "1 个必填字段未匹配到资料" in result["message"]


def test_autofill_hands_off_on_login_modal():
    page = _page(signals={"login_modal_visible": True})
    result = _client(page, _plan()).autofill()

    assert result["status"] == "human_handoff"
    assert result["success"] is False
    assert result["steps"] == []


def test_autofill_does_not_continue_after_success_text():
    page = _page(page_text="你的申请已提交成功")
    result = _client(page, _plan()).autofill()

    assert result["status"] == "already_submitted"
    assert result["success"] is True


def test_autofill_detects_no_progress_when_page_does_not_change(monkeypatch):
    clicks = []
    monkeypatch.setattr(app_module, "click_oa", lambda tab, oid: clicks.append(oid) or {"ok": True})
    page = _page(buttons=[{"id": "next", "text": "下一步", "disabled": False}])
    result = _client(page, _plan()).autofill()

    assert result["status"] == "no_progress"
    assert clicks == ["next"]
    assert result["steps"][0]["status"] == "next"
