from pathlib import Path

from officialapply.mapping import plan_mapping
from officialapply.profile import Profile

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Profile(ROOT / "官网投递资料.md")

CONFIG = {
    "apply": {
        "auto_upload_documents": ["resume", "photo", "transcript"],
        "fill_sensitive_fields": True,
        "require_no_unknown_required": True,
        "accept_terms_default": False,
    }
}

MOKA_TEMPLATE = {
    "id": "moka",
    "name": "Moka 招聘",
    "ignore_sections": ["家庭状况", "本人与本公司的亲友关系", "手机号登录", "邮箱登录"],
    "final_buttons": ["预览并提交"],
    "next_buttons": ["下一步"],
    "hints": {"split_dates": True, "login_modal": True},
}


def _field(fid, label="", **values):
    field = {
        "id": fid,
        "kind": "input",
        "input_type": "text",
        "label": label,
        "name": fid,
        "placeholder": "",
        "aria_label": "",
        "required": False,
        "hidden": False,
        "disabled": False,
        "readonly": False,
        "value": "",
        "checked": False,
        "options": [],
        "custom_select": False,
        "context": "",
        "section": "",
        "group_label": "",
        "order": 0,
        "in_dialog": False,
        "dialog_login": False,
        "searchable": False,
        "accept": "",
        "max_length": 0,
    }
    field.update(values)
    return field


def _select(fid, label, options, **values):
    return _field(
        fid,
        label,
        kind="select",
        options=[{"label": option, "value": option} for option in options],
        **values,
    )


def _moka_page():
    return {
        "url": "https://app.mokahr.com/campus_apply/demo/1#/job/abc/apply",
        "title": "示例公司校园招聘",
        "fields": [
            _field("name", "姓名", section="个人信息", required=True),
            _select("highest_degree", "最高学历", ["本科", "硕士"], section="个人信息", required=True),
            _field("school", "毕业院校", section="教育背景", required=True),
            _field("major", "专业", section="教育背景", required=True),
            _select("edu_degree", "学历", ["本科", "硕士"], section="教育背景", required=True),
            _select(
                "edu_start_year", "开始时间|年", ["2023", "2024"],
                section="教育背景", group_label="就读时间", order=5,
                custom_select=True, required=True,
            ),
            _select(
                "edu_start_month", "开始时间|月", ["09", "10"],
                section="教育背景", group_label="就读时间", order=6,
                custom_select=True, required=True,
            ),
            _select(
                "edu_end_year", "结束时间|年", ["2026", "2027"],
                section="教育背景", group_label="就读时间", order=7,
                custom_select=True, required=True,
            ),
            _select(
                "edu_end_month", "结束时间|月", ["06", "07"],
                section="教育背景", group_label="就读时间", order=8,
                custom_select=True, required=True,
            ),
            _field("family_name", "姓名", section="家庭成员", required=True),
            _field(
                "login_phone", "手机号", section="手机号登录", required=True,
                in_dialog=True, dialog_login=True,
            ),
            _field(
                "sync_resume", "同步更新在线简历", kind="checkbox", section="声明",
            ),
            _field("declare", "确认", kind="checkbox", section="声明", required=True),
        ],
        "buttons": [{"id": "next", "text": "下一步", "disabled": False, "in_dialog": False}],
    }


def test_moka_sections_split_dates_login_and_consent():
    plan = plan_mapping(PROFILE, _moka_page(), CONFIG, template=MOKA_TEMPLATE)
    actions = {item["id"]: item for item in plan["actions"]}

    assert actions["name"]["value"] == "张三"
    assert actions["name"]["section"] == "个人信息"
    assert actions["highest_degree"]["value"] == "本科"
    assert actions["edu_degree"]["value"] == "本科"
    assert actions["school"]["value"]
    assert actions["major"]["value"]
    assert actions["edu_start_year"]["value"] == "2023"
    assert actions["edu_start_month"]["value"] == "09"
    assert actions["edu_end_year"]["value"] == "2027"
    assert actions["edu_end_month"]["value"] == "06"
    assert actions["edu_start_year"]["semantic"] == "education_start_year"
    assert actions["edu_end_month"]["semantic"] == "education_end_month"

    assert any(item["reason"] == "login_field" for item in plan["skipped"])
    assert "login_phone" not in actions
    assert [item["id"] for item in plan["consent_pending"]] == ["declare"]
    assert "sync_resume" not in actions
    assert any(item["id"] == "family_name" for item in plan["unknown_required"])
    assert plan["summary"]["cannot_submit"] is True
    assert plan["template"]["id"] == "moka"


def test_family_required_field_blocks_submit_even_when_ignored():
    page = {
        "url": "https://app.mokahr.com/apply",
        "fields": [
            _field("name", "姓名", section="个人信息", required=True),
            _field("relative", "亲属姓名", section="家庭成员", required=True),
        ],
    }
    plan = plan_mapping(PROFILE, page, CONFIG, template=MOKA_TEMPLATE)
    unknown = next(item for item in plan["unknown_required"] if item["id"] == "relative")
    assert "家庭成员" in unknown["reason"]
    assert plan["summary"]["cannot_submit"] is True


def test_login_dialog_required_fields_do_not_block_submit():
    page = {
        "url": "https://app.mokahr.com/apply",
        "fields": [
            _field("name", "姓名", section="个人信息", required=True),
            _field(
                "login_code", "验证码", section="个人信息", required=True,
                in_dialog=True, dialog_login=True,
            ),
        ],
    }
    plan = plan_mapping(PROFILE, page, CONFIG, template=MOKA_TEMPLATE)
    assert plan["unknown_required"] == []
    assert any(item["reason"] == "login_field" for item in plan["skipped"])


def test_declaration_checkbox_is_consent_but_sync_resume_is_not():
    page = {
        "url": "https://app.mokahr.com/apply",
        "fields": [
            _field("name", "姓名", section="个人信息"),
            _field("sync_resume", "同步更新在线简历", kind="checkbox", section="声明"),
            _field("declare", "确认", kind="checkbox", section="声明", required=True),
        ],
    }
    plan = plan_mapping(PROFILE, page, CONFIG, template=MOKA_TEMPLATE)
    assert [item["id"] for item in plan["consent_pending"]] == ["declare"]

    accepted = plan_mapping(
        PROFILE,
        page,
        CONFIG,
        template=MOKA_TEMPLATE,
        accept_terms=True,
    )
    consent = [item for item in accepted["actions"] if item["semantic"] == "consent"]
    assert [item["id"] for item in consent] == ["declare"]


def test_split_dates_disabled_by_template_hint():
    page = {"url": "https://example.com/apply", "fields": _moka_page()["fields"]}
    template = {**MOKA_TEMPLATE, "hints": {**MOKA_TEMPLATE["hints"], "split_dates": False}}
    plan = plan_mapping(PROFILE, page, CONFIG, template=template)
    assert not any(
        str(item.get("semantic") or "").startswith("education_start_")
        for item in plan["actions"]
    )
