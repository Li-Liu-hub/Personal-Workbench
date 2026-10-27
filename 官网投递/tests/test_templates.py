from officialapply.templates import detect_template, template_catalog


def _page(**values):
    page = {
        "url": "https://example.com/apply",
        "title": "网申",
        "page_text": "",
        "headings": [],
        "buttons": [],
        "tech": {},
    }
    page.update(values)
    return page


def test_catalog_contains_major_ats_families():
    items = template_catalog()
    ids = {item["id"] for item in items}
    assert len(items) >= 25
    assert {"moka", "beisen", "dayee", "gllue", "workday", "greenhouse", "lever", "generic"} <= ids
    moka = next(item for item in items if item["id"] == "moka")
    assert "预览并提交" in moka["final_buttons"]
    assert moka["hints"]["split_dates"] is True


def test_detect_moka_by_url_and_markers():
    page = _page(
        url="https://app.mokahr.com/campus_apply/yadi/123#/job/abc/apply",
        title="雅迪科技集团校园招聘",
        tech={"scripts": ["https://cdn.mokahr.com/sdk.js"], "class_hint": "sd-Input-input"},
        buttons=[{"text": "预览并提交"}],
    )
    template = detect_template(page)
    assert template["id"] == "moka"
    assert template["generic"] is False
    assert template["confidence"] == "high"
    assert template["final_buttons"] == ["预览并提交"]
    assert any(item.startswith("URL:") for item in template["evidence"])


def test_detect_beisen_by_url():
    page = _page(url="https://company.zhiye.com/campus/detail?jobAdId=1")
    template = detect_template(page)
    assert template["id"] == "beisen"
    assert template["generic"] is False


def test_unknown_site_falls_back_to_generic():
    page = _page(url="https://jobs.example.org/positions/42")
    template = detect_template(page)
    assert template["id"] == "generic"
    assert template["generic"] is True
    assert template["confidence"] == "low"
    assert template["final_buttons"]


def test_site_override_wins_without_changing_global_catalog():
    page = _page(url="https://careers.example.org/apply/42", title="Example Careers")
    config = {
        "sites": {
            "careers.example.org": {
                "name": "Example 校招",
                "entry_buttons": ["立即申请"],
                "next_buttons": ["前往下一步"],
                "final_buttons": ["确认投递"],
                "ignore_sections": ["家庭成员"],
                "hints": {"multi_step": True},
            }
        }
    }
    template = detect_template(page, config)
    assert template["name"] == "Example 校招"
    assert template["entry_buttons"] == ["立即申请"]
    assert template["next_buttons"] == ["前往下一步"]
    assert template["final_buttons"] == ["确认投递"]
    assert "家庭成员" in template["ignore_sections"]
    assert template["hints"]["multi_step"] is True
    assert template["site_override"] == "careers.example.org"
    assert template["confidence"] == "high"

    global_template = detect_template(page)
    assert global_template["id"] == "generic"
    assert "site_override" not in global_template
