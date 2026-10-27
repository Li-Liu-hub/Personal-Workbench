from pathlib import Path

from officialapply.profile import is_uncertain, normalize_label


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
    assert shown["email"] != "test@example.com"
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
