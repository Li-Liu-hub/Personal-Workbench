from pathlib import Path

from officialapply.mapping import (
    detect_document_role,
    detect_semantic,
    plan_mapping,
    public_plan,
)
from officialapply.profile import Profile

ROOT = Path(__file__).resolve().parents[1]


def _config(**apply):
    base = {
        "auto_upload_documents": ["resume", "photo", "transcript"],
        "fill_sensitive_fields": True,
        "require_no_unknown_required": True,
        "accept_terms_default": False,
    }
    base.update(apply)
    return {"apply": base}


def _field(**values):
    field = {
        "id": "f1",
        "kind": "input",
        "input_type": "text",
        "label": "",
        "name": "",
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
        "accept": "",
        "max_length": 0,
    }
    field.update(values)
    return field


def _profile(tmp_path, text: str) -> Profile:
    path = tmp_path / "profile.md"
    path.write_text(text, encoding="utf-8")
    return Profile(path)


def test_real_profile_maps_known_fields():
    profile = Profile(ROOT / "官网投递资料.md")
    page = {
        "fields": [
            _field(id="name", label="姓名", required=True),
            _field(id="phone", label="手机号", required=True),
            _field(id="email", label="电子邮箱", required=True),
            _field(id="city", label="现居城市", required=True),
            _field(id="grade", label="学历", kind="select", required=True,
                   options=[{"label": "本科", "value": "bachelor"}]),
        ],
        "url": "https://example.com/apply",
    }
    plan = plan_mapping(profile, page, _config())
    actions = {item["semantic"]: item for item in plan["actions"]}
    assert actions["name"]["value"] == "张三"
    assert actions["phone"]["value"] == "13800138000"
    assert actions["email"]["value"] == "test@example.com"
    assert "current_city" not in actions
    assert any(item["semantic"] == "education_degree" for item in plan["actions"])
    assert plan["unknown_required"]
    assert plan["summary"]["cannot_submit"] is True


def test_uncertain_value_is_never_filled(tmp_path):
    profile = _profile(tmp_path, "## 基本信息\n- 姓名：张三\n- 现居城市：待补充\n")
    page = {"fields": [_field(id="city", label="现居城市", required=True)]}
    plan = plan_mapping(profile, page, _config())
    assert plan["actions"] == []
    assert plan["unknown_required"]
    assert plan["missing_known"][0]["reason"] == "资料中该字段仍待确认"


def test_common_question_uncertain_is_not_filled(tmp_path):
    profile = _profile(
        tmp_path,
        "## 基本信息\n- 姓名：张三\n\n## 网申常见问题\n- 是否有亲友在本公司任职：待补充\n",
    )
    page = {"fields": [_field(id="q1", label="是否有亲友在本公司任职", required=True)]}
    plan = plan_mapping(profile, page, _config())
    assert plan["actions"] == []
    assert plan["unknown_required"]


def test_common_question_known_falls_back(tmp_path):
    profile = _profile(
        tmp_path,
        "## 基本信息\n- 姓名：张三\n\n## 网申常见问题\n- 个人优势：熟悉 Java 后端开发\n",
    )
    page = {"fields": [_field(id="q1", label="个人优势", kind="textarea")]}
    plan = plan_mapping(profile, page, _config())
    action = plan["actions"][0]
    assert action["value"] == "熟悉 Java 后端开发"
    assert action["source"] == "common_questions"


def test_sensitive_field_skipped_when_disabled(tmp_path):
    profile = _profile(
        tmp_path,
        "## 基本信息\n- 姓名：张三\n- 身份证号：110101200501010000\n",
    )
    page = {"fields": [_field(id="id", label="身份证号", required=True)]}
    plan = plan_mapping(profile, page, _config(fill_sensitive_fields=False))
    assert plan["actions"] == []
    assert any("敏感信息自动填写已关闭" in str(item.get("reason")) for item in plan["unknown_required"])
    public = public_plan(plan)
    assert "110101200501010000" not in str(public)


def test_explicit_override_wins_and_masks_in_public_plan(tmp_path):
    profile = _profile(tmp_path, "## 基本信息\n- 姓名：张三\n")
    page = {"fields": [_field(id="id", label="身份证号")]}
    plan = plan_mapping(profile, page, _config(), overrides={"id": "110101200501010000"})
    assert plan["actions"][0]["value"] == "110101200501010000"
    assert "110101200501010000" not in str(public_plan(plan))


def test_already_uploaded_required_file_does_not_block():
    profile = Profile(ROOT / "官网投递资料.md")
    page = {
        "fields": [
            _field(id="cv", kind="file", label="上传简历", required=True, value="resume.pdf"),
        ]
    }
    plan = plan_mapping(profile, page, _config())
    assert plan["uploads"] == []
    assert plan["unknown_required"] == []


def test_resume_upload_is_planned():
    profile = Profile(ROOT / "官网投递资料.md")
    page = {"fields": [_field(id="cv", kind="file", label="上传简历", required=True)]}
    plan = plan_mapping(profile, page, _config())
    assert len(plan["uploads"]) == 1
    upload = plan["uploads"][0]
    assert upload["role"] == "resume"
    assert Path(upload["path"]).exists()


def test_sensitive_id_upload_is_blocked_by_default():
    profile = Profile(ROOT / "官网投递资料.md")
    page = {"fields": [_field(id="idfront", kind="file", label="身份证正面", required=True)]}
    plan = plan_mapping(profile, page, _config())
    assert plan["uploads"] == []
    assert any("敏感证件" in str(item.get("reason")) for item in plan["unknown_required"])


def test_consent_requires_explicit_acceptance():
    profile = Profile(ROOT / "官网投递资料.md")
    page = {
        "fields": [
            _field(id="agree", kind="checkbox", label="我已阅读并同意《隐私政策》", required=True),
        ]
    }
    plan = plan_mapping(profile, page, _config())
    assert plan["consent_pending"]
    assert plan["actions"] == []

    accepted = plan_mapping(profile, page, _config(), accept_terms=True)
    assert accepted["actions"][0]["semantic"] == "consent"
    assert accepted["actions"][0]["value"] == "是"


def test_custom_select_gets_alternatives(tmp_path):
    profile = _profile(
        tmp_path,
        "## 基本信息\n- 姓名：张三\n\n## 求职意向\n- 期望城市：上海、深圳、广州、杭州\n",
    )
    page = {
        "fields": [
            _field(
                id="city",
                kind="select",
                label="期望城市",
                custom_select=True,
                options=[{"label": "上海", "value": "sh"}, {"label": "深圳", "value": "sz"}],
            )
        ]
    }
    plan = plan_mapping(profile, page, _config())
    action = plan["actions"][0]
    assert action["kind"] == "custom_select"
    assert action["value"] == "上海"
    assert "深圳" in action["alternatives"]


def test_detect_document_roles():
    assert detect_document_role(_field(label="简历附件")) == "resume"
    assert detect_document_role(_field(label="简历图片")) == "resume_image"
    assert detect_document_role(_field(label="一寸证件照")) == "photo"
    assert detect_document_role(_field(label="学籍在线验证报告")) == "enrollment_report"
    assert detect_document_role(_field(label="CET4 成绩单")) == "cet4"
    assert detect_document_role(_field(label="身份证反面")) == "id_back"
    assert detect_document_role(_field(label="作品集")) == "other"
    assert detect_document_role(_field(label="随便上传")) is None


def test_detect_semantics_for_common_labels():
    assert detect_semantic(_field(label="姓名")) == "name"
    assert detect_semantic(_field(label="手机号码")) == "phone"
    assert detect_semantic(_field(label="电子邮箱")) == "email"
    assert detect_semantic(_field(label="出生日期")) == "birth_date"
    assert detect_semantic(_field(label="毕业院校")) == "education_school"
    assert detect_semantic(_field(label="所学专业")) == "education_major"
    assert detect_semantic(_field(label="紧急联系人电话")) == "emergency_contact_phone"
