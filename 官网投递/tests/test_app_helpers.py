from officialapply.app import (
    _OTP_LOGIN_EXCLUDE,
    _OTP_LOGIN_TEXTS,
    _OTP_SEND_EXCLUDE,
    _OTP_SEND_TEXTS,
    _find_advance_button,
    _find_confirm_button,
    _find_final_button,
    _find_submit_button,
    _find_text_button,
    _login_required,
    _public_guard,
    _step_signature,
    _success_by_text,
)


def _button(text, *, dialog=False, disabled=False):
    return {"id": text, "text": text, "in_dialog": dialog, "disabled": disabled}


def test_submit_button_prefers_exact_submit_text():
    buttons = [
        _button("下一步"),
        _button("取消"),
        _button("提交申请"),
        _button("保存"),
    ]
    target = _find_submit_button(buttons)
    assert target is not None
    assert target["text"] == "提交申请"


def test_submit_button_can_be_requested_by_partial_text():
    buttons = [_button("确认并提交申请"), _button("取消")]
    target = _find_submit_button(buttons, "提交申请")
    assert target["text"] == "确认并提交申请"


def test_submit_button_excludes_navigation_and_disabled():
    buttons = [_button("下一步"), _button("保存草稿"), _button("提交", disabled=True)]
    assert _find_submit_button(buttons) is None


def test_confirm_button_only_inside_dialog():
    page = {
        "page_text": "请确认是否提交申请",
        "buttons": [_button("确认提交", dialog=True), _button("确定", dialog=True), _button("取消", dialog=True)],
    }
    target = _find_confirm_button(page)
    assert target is not None
    assert target["text"] == "确认提交"


def test_generic_confirm_requires_confirmation_context():
    page = {"page_text": "请确认你的信息", "buttons": [_button("确定", dialog=True)]}
    assert _find_confirm_button(page) is None


def test_success_text_detection():
    assert _success_by_text({"page_text": "提交成功，请等待通知"})
    assert _success_by_text({"title": "投递成功"})
    assert not _success_by_text({"page_text": "请完善必填信息"})


def test_login_required_only_for_short_pages_with_login_evidence():
    assert _login_required({"field_count": 2, "signals": {"login_evidence": ["请登录"]}})
    assert not _login_required({
        "field_count": 10,
        "signals": {"login_evidence": ["请登录"], "logged_in_hint": "用户中心"},
    })
    assert not _login_required({"field_count": 2, "signals": {}})


def test_otp_button_selection_avoids_voice_and_disabled():
    buttons = [
        _button("语音获取验证码"),
        _button("获取验证码", disabled=True),
        _button("获取验证码(60s)"),
        _button("登录"),
    ]
    target = _find_text_button(buttons, "", _OTP_SEND_TEXTS, _OTP_SEND_EXCLUDE)
    assert target is not None and target["text"] == "获取验证码(60s)"


def test_otp_login_button_excludes_otp_and_register_buttons():
    buttons = [
        _button("验证码登录"),
        _button("注册"),
        _button("忘记密码"),
        _button("立即登录"),
    ]
    target = _find_text_button(buttons, "", _OTP_LOGIN_TEXTS, _OTP_LOGIN_EXCLUDE)
    assert target is not None and target["text"] == "立即登录"


def test_public_guard_does_not_leak_full_records():
    guard = {
        "url_key": "https://example.com/job/1",
        "daily": {"counted": 1},
        "duplicate_url": {"id": "secret-record"},
        "similar_title": {"id": "other-record"},
        "can_submit": False,
    }
    public = _public_guard(guard)
    assert "duplicate_url" not in public
    assert "similar_title" not in public
    assert public["duplicate_submitted"] is True
    assert public["similar_title_submitted"] is True


def test_moka_final_and_next_buttons_are_separated():
    page = {
        "buttons": [
            _button("下一步"),
            _button("预览并提交"),
            _button("保存"),
        ]
    }
    template = {"next_buttons": ["下一步"], "final_buttons": ["预览并提交"]}
    assert _find_final_button(page, template)["text"] == "预览并提交"
    assert _find_advance_button(page, template)["text"] == "下一步"


def test_advance_button_never_returns_preview_submit():
    page = {"buttons": [_button("预览并提交")]}
    template = {"next_buttons": ["下一步"], "final_buttons": ["预览并提交"]}
    assert _find_advance_button(page, template) is None


def test_final_button_ignores_dialog_buttons():
    page = {"buttons": [_button("预览并提交", dialog=True)]}
    template = {"final_buttons": ["预览并提交"]}
    assert _find_final_button(page, template) is None


def test_login_modal_marks_page_as_login_required_even_with_many_fields():
    page = {
        "field_count": 40,
        "signals": {"login_modal_visible": True, "logged_in_hint": "用户中心"},
    }
    assert _login_required(page) is True


def test_step_signature_changes_when_section_or_button_changes():
    page = {
        "url": "https://example.com/apply",
        "fields": [
            {"label": "姓名", "section": "个人信息", "kind": "input"},
        ],
        "buttons": [{"text": "下一步"}],
    }
    assert _step_signature(page) == _step_signature({**page, "fields": [dict(page["fields"][0])]})
    assert _step_signature(page) != _step_signature({
        **page,
        "fields": [{"label": "姓名", "section": "教育背景", "kind": "input"}],
    })
    assert _step_signature(page) != _step_signature({
        **page,
        "buttons": [{"text": "预览并提交"}],
    })
