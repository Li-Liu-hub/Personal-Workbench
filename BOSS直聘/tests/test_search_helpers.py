from __future__ import annotations

import json

from bosshunter.boss.search import (
    build_filter_query,
    inspect_risk,
    is_closed_page,
    job_id_from_url,
    normalize_filters,
    read_risk,
)


class FakeTab:
    def __init__(self, payload):
        self.payload = payload

    def evaluate(self, _expression):
        return json.dumps(self.payload, ensure_ascii=False)


def test_job_id_from_url():
    assert job_id_from_url("https://www.zhipin.com/job_detail/abc123.html?ka=1") == "abc123"
    assert job_id_from_url("https://www.zhipin.com/job_detail/abc123.html#x") == "abc123"
    assert job_id_from_url("https://www.zhipin.com/web/geek/job") == ""


def test_normalize_filters():
    filters = normalize_filters({
        "experience": "1-3年，应届生",
        "degree": ["本科", "不存在的学历", "本科"],
        "industry": ["12345", "abc"],
        "unknown": ["x"],
    })
    assert filters == {
        "experience": ["1-3年", "应届生"],
        "degree": ["本科"],
        "industry": ["12345"],
    }


def test_build_filter_query():
    query = build_filter_query({"experience": ["1-3年"], "degree": ["本科"]})
    assert "experience=104" in query
    assert "degree=203" in query


def test_risk_detection():
    tab = FakeTab({"risk": "captcha", "evidence": "captcha_element"})
    assert read_risk(tab) == {"kind": "captcha", "evidence": "captcha_element"}
    assert inspect_risk(tab, sleeper=lambda _seconds: None) == {
        "kind": "captcha",
        "evidence": "captcha_element",
    }
    assert read_risk(FakeTab({})) is None


def test_closed_page_detection():
    assert is_closed_page(FakeTab({"closed": True}))
    assert not is_closed_page(FakeTab({"closed": False}))
