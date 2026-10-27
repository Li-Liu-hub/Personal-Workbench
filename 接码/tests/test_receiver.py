from __future__ import annotations

import json
import time
import urllib.error
import urllib.request

import pytest

from otp_receiver.server import (
    OtpInbox,
    OtpReceiver,
    extract_verification_code,
    load_or_create_token,
    mask_sender,
    read_runtime_endpoint,
    write_runtime_file,
)

OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def _base_url(receiver: OtpReceiver, token: str) -> str:
    return f"http://127.0.0.1:{receiver.actual_port}/otp/{token}"


def _post(url: str, payload: dict[str, object]):
    request = urllib.request.Request(
        url,
        data=json.dumps(payload, ensure_ascii=False).encode("utf-8"),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    return OPENER.open(request, timeout=5)


def _get(url: str):
    return OPENER.open(urllib.request.Request(url), timeout=5)


def test_extract_verification_code_variants():
    assert extract_verification_code("【平台】您的验证码为 123456，5 分钟内有效") == "123456"
    assert extract_verification_code("校验码：4321") == "4321"
    assert extract_verification_code("Your verification code is 987654.") == "987654"
    assert extract_verification_code("您的订单 123456 已发货") is None
    assert extract_verification_code("验证码已发送，请查收") is None


def test_mask_sender():
    assert mask_sender("13800138000") == "138****00"
    assert mask_sender("") == ""


def test_receiver_webhook_status_and_latest():
    token = "unit-test-" + "a" * 40
    receiver = OtpReceiver(
        OtpInbox(ttl_seconds=120),
        token,
        host="127.0.0.1",
        port=0,
        max_requests_per_minute=0,
    )
    receiver.start()
    try:
        base = _base_url(receiver, token)
        with _post(base, {"from": "13800138000", "content": "验证码 123456"}) as response:
            assert json.load(response) == {"ok": True, "code_found": True}

        with _get(base + "/status") as response:
            status = json.load(response)
        assert status["received"] == 1
        assert status["with_code"] == 1
        assert status["pending"] == 1

        with _get(base + "/latest") as response:
            latest = json.load(response)
        assert latest["code"] == "123456"
        assert latest["sender"] == "138****00"

        with pytest.raises(urllib.error.HTTPError) as exc_info:
            _get(f"http://127.0.0.1:{receiver.actual_port}/otp/wrong-token/status")
        assert exc_info.value.code == 403
    finally:
        receiver.stop()


def test_receiver_rejects_stale_timestamp():
    token = "unit-test-" + "b" * 40
    receiver = OtpReceiver(
        OtpInbox(ttl_seconds=120),
        token,
        host="127.0.0.1",
        port=0,
        max_requests_per_minute=0,
        max_clock_skew_seconds=600,
    )
    receiver.start()
    try:
        with pytest.raises(urllib.error.HTTPError) as exc_info:
            _post(
                _base_url(receiver, token),
                {"from": "13800138000", "content": "验证码 123456", "timestamp": time.time() - 3600},
            )
        assert exc_info.value.code == 400
    finally:
        receiver.stop()


def test_receiver_rate_limit():
    token = "unit-test-" + "c" * 40
    receiver = OtpReceiver(
        OtpInbox(ttl_seconds=120),
        token,
        host="127.0.0.1",
        port=0,
        max_requests_per_minute=1,
    )
    receiver.start()
    try:
        payload = {"from": "13800138000", "content": "验证码 123456"}
        with _post(_base_url(receiver, token), payload) as response:
            assert response.status == 200
        with pytest.raises(urllib.error.HTTPError) as exc_info:
            _post(_base_url(receiver, token), payload)
        assert exc_info.value.code == 429
    finally:
        receiver.stop()


def test_token_file_is_reused_and_rotated(tmp_path):
    token_path = tmp_path / "otp_token.txt"
    first = load_or_create_token(token_path)
    assert len(first) >= 32
    assert load_or_create_token(token_path) == first
    rotated = load_or_create_token(token_path, rotate=True)
    assert rotated != first
    assert token_path.read_text(encoding="utf-8").strip() == rotated


def test_runtime_file_round_trip(tmp_path):
    runtime_path = tmp_path / "otp_runtime.json"
    write_runtime_file(runtime_path, "127.0.0.1", 8765)
    assert read_runtime_endpoint(runtime_path) == ("127.0.0.1", 8765)
