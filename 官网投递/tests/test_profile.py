from pathlib import Path

from officialapply.profile import Profile, is_uncertain, normalize_label

ROOT = Path(__file__).resolve().parents[1]


def test_real_profile_parses_known_facts():
    profile = Profile(ROOT / "官网投递资料.md")
    assert profile.exists
    assert profile.basic["name"] == "张三"
    assert profile.basic["phone"] == "13800138000"
    assert profile.basic["email"] == "test@example.com"
    assert profile.age() is not None

    education = profile.education_primary()
    assert education["school"] == "示例大学"
    assert education["major"] == "计算机科学与技术"
    assert education["end"] == "2027-06"
    assert education["gpa"] == "2.90/4.0"

    internship = profile.internship_primary()
    assert "示例公司" in internship["company"]
    assert internship["title"] == "后端开发实习生"
    assert internship["start"] == "2026-03"

    assert len(profile.entries["projects"]) == 2
    assert profile.job_intent["expected_position"] == "Java后端开发工程师"
    assert profile.expected_cities() == ["上海", "深圳", "广州", "杭州"]


def test_real_profile_documents_resolve():
    profile = Profile(ROOT / "官网投递资料.md")
    roles = {item.role for item in profile.documents}
    assert {"resume", "resume_image", "photo", "photo_square", "photo_avatar"} <= roles
    assert {"transcript", "enrollment_proof", "gpa_proof", "enrollment_report"} <= roles
    assert {"cet4", "internship_proof", "id_front", "id_back"} <= roles
    assert all(item.exists for item in profile.documents)
    assert profile.resume_path() and profile.resume_path().suffix == ".pdf"
    assert profile.photo_path() is not None


def test_real_profile_reports_missing_required():
    profile = Profile(ROOT / "官网投递资料.md")
    check = profile.check()
    missing = {(item["section"], item["field"]) for item in check["missing_required"]}
    assert ("基本信息", "现居城市") in missing
    assert ("求职意向", "到岗时间") in missing
    assert check["missing_documents"] == []
    assert check["complete_required"] is False
    uncertain_fields = {item["field"] for item in check["uncertain"]}
    assert "民族" in uncertain_fields
    assert "身份证号" in uncertain_fields


def test_uncertain_values_are_not_blank_but_detected():
    assert is_uncertain("待补充")
    assert is_uncertain("待确认（工学学士）")
    assert is_uncertain("TODO")
    assert not is_uncertain("")
    assert not is_uncertain("男")
    assert normalize_label(" 身份证 号码 ") == "身份证号码"


def test_profile_show_masks_sensitive(tmp_path):
    from officialapply.app import OfficialApply

    config = _write_config(tmp_path)
    app = OfficialApply(config)
    shown = app.profile_show()
    assert shown["phone"] == "138****8000"
    assert "1234567890" not in shown["email"]
    assert shown["success"] is True


def _write_config(tmp_path: Path) -> Path:
    profile = tmp_path / "profile.md"
    profile.write_text(
        "## 基本信息\n- 姓名：测试\n- 手机号：13800138000\n- 邮箱：test@example.com\n"
        "## 求职意向\n- 期望岗位：后端开发\n",
        encoding="utf-8",
    )
    config = tmp_path / "config.yaml"
    config.write_text(
        "profile:\n  path: ./profile.md\n"
        "browser:\n  debug_port: 19444\n  user_data_dir: ./data/chrome\n"
        "apply:\n  daily_submit_limit: 5\n  auto_upload_documents: [resume]\n",
        encoding="utf-8",
    )
    return config
