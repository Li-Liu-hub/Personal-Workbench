import http.client
import json
import threading
import time

from officialapply.forms import (
    _preview_value,
    find_otp_fields,
    otp_fill_actions,
)
from officialapply.otp import (
    OtpClient,
    OtpInbox,
    OtpReceiver,
    build_otp_client,
    extract_verification_code,
    load_or_create_token,
    mask_url_token,
    read_runtime_endpoint,
    write_runtime_file,
)


def test_extract_verification_code_patterns():
    assert extract_verification_code("【某公司】您的验证码为 123456，5分钟内有效") == "123456"
    assert extract_verification_code("验证码：1234") == "1234"
    assert extract_verification_code("动态密码 87654321") == "87654321"
    assert extract_verification_code("123456 是您的验证码") == "123456"
    assert extract_verification_code("验证码 123456，订单号 654321") == "123456"


def test_extract_verification_code_ignores_plain_numbers():
    assert extract_verification_code("您的手机号 13800138000 已注册") is None
    assert extract_verification_code("订单金额 2026 元") is None
    assert extract_verification_code("订单号 123456") is None
    assert extract_verification_code("") is None


def test_inbox_expires_codes():
    now = [1000.0]
    inbox = OtpInbox(ttl_seconds=10, clock=lambda: now[0])
    item = inbox.add("1069", "您的验证码是 654321")
    assert item is not None
    assert item.code == "654321"
    assert inbox.latest() is not None
    now[0] += 11
    assert inbox.latest() is None
    assert inbox.status()["pending"] == 0


def test_receiver_accepts_json_and_form():
    inbox = OtpInbox(ttl_seconds=60)
    receiver = OtpReceiver(inbox, "test-token", host="127.0.0.1", port=0, max_clock_skew_seconds=60)
    receiver.start()
    try:
        port = receiver.actual_port
        timestamp = int(time.time() * 1000)
        status, payload = _post(
            port,
            "/otp/test-token",
            json.dumps({"from": "1069", "content": "您的验证码是 654321", "timestamp": timestamp}),
            "application/json",
        )
        assert status == 200
        assert payload["code_found"] is True
        latest = inbox.latest()
        assert latest is not None and latest.code == "654321"

        status, payload = _post(
            port,
            "/otp/test-token",
            f"from=1069&content=验证码：112233&timestamp={timestamp}",
            "application/x-www-form-urlencoded",
        )
        assert status == 200
        assert payload["code_found"] is True
        assert inbox.latest().code == "112233"

        status, payload = _get(port, "/otp/test-token/status")
        assert status == 200
        assert payload["with_code"] == 2

        status, payload = _get(port, "/otp/test-token/latest")
        assert status == 200
        assert payload["code"] == "112233"
    finally:
        receiver.stop()


def test_receiver_rejects_bad_token_and_stale_timestamp():
    inbox = OtpInbox(ttl_seconds=60)
    receiver = OtpReceiver(inbox, "test-token", host="127.0.0.1", port=0, max_clock_skew_seconds=60)
    receiver.start()
    try:
        port = receiver.actual_port
        status, _ = _post(
            port,
            "/otp/wrong-token",
            json.dumps({"from": "1069", "content": "验证码 654321"}),
            "application/json",
        )
        assert status == 403
        assert inbox.status()["received"] == 0

        stale = int((time.time() - 3600) * 1000)
        status, payload = _post(
            port,
            "/otp/test-token",
            json.dumps({"from": "1069", "content": "验证码 654321", "timestamp": stale}),
            "application/json",
        )
        assert status == 400
        assert payload["error"] == "stale_or_invalid_timestamp"
    finally:
        receiver.stop()


def test_receiver_rate_limits_requests():
    inbox = OtpInbox(ttl_seconds=60)
    receiver = OtpReceiver(
        inbox,
        "test-token",
        host="127.0.0.1",
        port=0,
        max_clock_skew_seconds=60,
        max_requests_per_minute=2,
    )
    receiver.start()
    try:
        port = receiver.actual_port
        body = json.dumps({"from": "1069", "content": "验证码 654321"})
        first, _ = _post(port, "/otp/test-token", body, "application/json")
        second, _ = _post(port, "/otp/test-token", body, "application/json")
        third, payload = _post(port, "/otp/test-token", body, "application/json")
        assert (first, second) == (200, 200)
        assert third == 429
        assert payload["error"] == "too_many_requests"
    finally:
        receiver.stop()


def test_load_or_create_token_rotates_legacy_token(tmp_path):
    token_path = tmp_path / "otp_token.txt"
    token_path.write_text("shortlegacytoken\n", encoding="utf-8")
    token = load_or_create_token(token_path)
    assert len(token) >= 32
    assert token_path.read_text(encoding="utf-8").strip() == token
    rotated = load_or_create_token(token_path, rotate=True)
    assert rotated != token
    assert token_path.read_text(encoding="utf-8").strip() == rotated
    assert mask_url_token(f"http://172.20.10.5:8765/otp/{token}").find(token) == -1


def test_otp_client_reads_and_waits_for_new_code():
    inbox = OtpInbox(ttl_seconds=60)
    receiver = OtpReceiver(inbox, "test-token", host="127.0.0.1", port=0, max_clock_skew_seconds=60)
    receiver.start()
    try:
        client = OtpClient(f"http://127.0.0.1:{receiver.actual_port}", "test-token", timeout=2)
        assert client.status()["ok"] is True
        assert client.latest() is None

        def send() -> None:
            time.sleep(0.4)
            _post(
                receiver.actual_port,
                "/otp/test-token",
                json.dumps({"from": "1069", "content": "验证码是 246810"}),
                "application/json",
            )

        worker = threading.Thread(target=send)
        worker.start()
        message = client.wait_for_new_code(since=time.time(), timeout=5, poll_interval=0.1)
        worker.join()
        assert message is not None
        assert message.code == "246810"
        assert message.sender == "1069"
        assert message.received_at_text
    finally:
        receiver.stop()


def test_otp_client_rejects_wrong_token_and_masks_existing_code(tmp_path):
    inbox = OtpInbox(ttl_seconds=60)
    receiver = OtpReceiver(inbox, "test-token", host="127.0.0.1", port=0, max_clock_skew_seconds=60)
    receiver.start()
    try:
        bad = OtpClient(f"http://127.0.0.1:{receiver.actual_port}", "wrong-token", timeout=2)
        try:
            bad.status()
        except Exception as exc:
            assert getattr(exc, "code", "") == "otp_forbidden"
        else:
            raise AssertionError("wrong token must be rejected")

        _post(
            receiver.actual_port,
            "/otp/test-token",
            json.dumps({"from": "13800138000", "content": "验证码是 135790"}),
            "application/json",
        )
        client = OtpClient(f"http://127.0.0.1:{receiver.actual_port}", "test-token", timeout=2)
        message = client.latest()
        assert message is not None and message.code == "135790"
        assert message.sender == "185****26"
    finally:
        receiver.stop()


def test_runtime_file_and_build_client(tmp_path):
    inbox = OtpInbox(ttl_seconds=60)
    receiver = OtpReceiver(inbox, "test-token", host="127.0.0.1", port=0, max_clock_skew_seconds=60)
    receiver.start()
    try:
        runtime_path = write_runtime_file(tmp_path / "otp_runtime.json", receiver.host, receiver.actual_port)
        assert read_runtime_endpoint(runtime_path) == ("127.0.0.1", receiver.actual_port)
        token_path = tmp_path / "otp_token.txt"
        token_path.write_text("test-token\n", encoding="utf-8")
        config = {
            "_otp_token_path": str(token_path),
            "_otp_runtime_path": str(runtime_path),
            "otp": {"bind_ip": "auto", "port": 1},
        }
        client = build_otp_client(config)
        assert client.base_url == f"http://127.0.0.1:{receiver.actual_port}"
    finally:
        receiver.stop()


def test_find_otp_fields_excludes_password_and_image_captcha():
    page = {
        "fields": [
            _field("phone", label="手机号", input_type="tel", max_length=11),
            _field("password", label="密码", input_type="password"),
            _field("captcha", label="图形验证码", max_length=4),
            _field("otp", label="短信验证码", max_length=6),
        ]
    }
    fields = find_otp_fields(page)
    assert [item["id"] for item in fields] == ["otp"]


def test_find_otp_fields_groups_split_boxes_and_maps_digits():
    fields = [
        _field(
            f"box-{index}",
            label="验证码" if index == 0 else "",
            max_length=1,
            order=10 + index,
            section="登录",
        )
        for index in range(6)
    ]
    fields.insert(0, _field("phone", label="手机号", input_type="tel", max_length=11, order=1, section="登录"))
    page = {"fields": fields}
    found = find_otp_fields(page, field_id="box-0")
    assert [item["id"] for item in found] == [f"box-{index}" for index in range(6)]
    actions = otp_fill_actions(found, "102938")
    assert [item["value"] for item in actions] == list("102938")
    assert all(item["semantic"] == "otp" for item in actions)


def test_otp_preview_never_contains_code():
    assert _preview_value("102938", "otp") == "***"
    page = {"fields": [_field("otp", label="短信验证码", max_length=6)]}
    actions = otp_fill_actions(find_otp_fields(page), "102938")
    assert actions[0]["value"] == "102938"


def _field(
    field_id: str,
    *,
    label: str = "",
    input_type: str = "text",
    max_length: int = 0,
    order: int = 1,
    section: str = "",
) -> dict:
    return {
        "id": field_id,
        "kind": "input",
        "input_type": input_type,
        "label": label,
        "placeholder": "",
        "aria_label": "",
        "group_label": "",
        "name": field_id,
        "max_length": max_length,
        "order": order,
        "section": section,
        "hidden": False,
        "disabled": False,
        "readonly": False,
    }


def _post(port: int, path: str, body: str, content_type: str) -> tuple[int, dict]:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    try:
        connection.request("POST", path, body=body.encode("utf-8"), headers={"Content-Type": content_type})
        response = connection.getresponse()
        return response.status, json.loads(response.read().decode("utf-8"))
    finally:
        connection.close()


def _get(port: int, path: str) -> tuple[int, dict]:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    try:
        connection.request("GET", path)
        response = connection.getresponse()
        return response.status, json.loads(response.read().decode("utf-8"))
    finally:
        connection.close()
