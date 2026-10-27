"""Local end-to-end check for autofill on a Moka-style form.

Run from the OfficialApply directory:
    PYTHONPATH=src python tests/e2e_autofill.py

The fixture never submits a real application. It only proves that autofill
fills the page, uploads the resume, recognises the declaration, and stops.
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from officialapply.app import OfficialApply  # noqa: E402
from officialapply.browser.session import SessionStore  # noqa: E402
from officialapply.registry import Registry  # noqa: E402

PORT = 19444
WORK = ROOT / "data" / "e2e-autofill"
FIXTURE = ROOT / "tests" / "fixtures" / "moka_form.html"
MULTI_FIXTURE = ROOT / "tests" / "fixtures" / "multi_step_form.html"


def _write_fixture_files() -> None:
    WORK.mkdir(parents=True, exist_ok=True)
    for name in ("applications.jsonl", "session.json"):
        try:
            (WORK / name).unlink()
        except FileNotFoundError:
            pass
    (WORK / "resume.pdf").write_bytes(b"%PDF-1.4\n1 0 obj<<>>endobj\ntrailer<<>>\n%%EOF\n")
    (WORK / "profile.md").write_text(
        """## 基本信息
- 姓名：测试用户
- 手机号：13800138000
- 邮箱：test@example.com

## 教育经历
### 测试大学
- 学校：测试大学
- 专业：软件工程
- 学历：本科
- 入学时间：2023-09
- 毕业时间：2027-06

## 投递材料
- 简历PDF：./resume.pdf
""",
        encoding="utf-8",
    )
    (WORK / "config.yaml").write_text(
        f"""profile:
  path: ./profile.md
browser:
  debug_port: {PORT}
  user_data_dir: ./chrome-profile
  auto_launch: true
  headless: true
apply:
  daily_submit_limit: 5
  min_submit_interval_seconds: 0
  require_no_unknown_required: true
  fill_sensitive_fields: true
  auto_upload_documents: [resume]
  screenshot_dir: ./screenshots
autofill:
  max_steps: 3
  wait_seconds: 0.2
  upload_wait_seconds: 0.2
  resume_parse_wait_seconds: 0.2
sites:
  moka_form.html:
    template: moka
""",
        encoding="utf-8",
    )


def _wait_port(seconds: float = 25) -> bool:
    deadline = time.time() + seconds
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json/version", timeout=1) as response:
                return response.status == 200
        except OSError:
            time.sleep(0.4)
    return False


def _kill_test_chrome() -> None:
    try:
        output = subprocess.check_output(["netstat", "-ano"], text=True, errors="replace")
    except (OSError, subprocess.SubprocessError):
        return
    pids = set()
    for line in output.splitlines():
        if f":{PORT} " in line and "LISTENING" in line.upper():
            parts = line.split()
            if parts:
                pids.add(parts[-1])
    for pid in pids:
        subprocess.run(["taskkill", "/PID", pid, "/F", "/T"], capture_output=True)


def _values(tab) -> dict[str, object]:
    return tab.evaluate(
        """({
          name: document.getElementById('name').value,
          highestDegree: document.getElementById('highestDegree').value,
          school: document.getElementById('school').value,
          major: document.getElementById('major').value,
          eduDegree: document.getElementById('eduDegree').value,
          eduStartYear: document.getElementById('eduStartYear').value,
          eduStartMonth: document.getElementById('eduStartMonth').value,
          eduEndYear: document.getElementById('eduEndYear').value,
          eduEndMonth: document.getElementById('eduEndMonth').value,
          resume: (document.getElementById('resume').files[0] || {}).name || '',
          declare: document.getElementById('declare').checked,
          syncResume: document.getElementById('syncResume').checked,
          submitted: document.body.dataset.submitted || '',
        })"""
    )


def main() -> int:
    _write_fixture_files()
    app = OfficialApply(WORK / "config.yaml")
    app.browser.session = SessionStore(WORK / "session.json")
    app.registry = Registry(WORK / "applications.jsonl")
    report: dict[str, object] = {}
    try:
        app.open(FIXTURE.as_uri(), new=True)
        assert _wait_port(), "Chrome 调试端口未就绪"

        inspected = app.inspect()
        report["inspect"] = {
            "template": inspected["template"].get("id"),
            "confidence": inspected["template"].get("confidence"),
            "sections": inspected["page"].get("sections"),
            "fields": [item.get("label") for item in inspected["page"]["fields"]],
            "buttons": [item.get("text") for item in inspected["page"]["buttons"]],
        }
        assert inspected["template"]["id"] == "moka", inspected["template"]
        assert "个人信息" in inspected["page"]["sections"]
        assert "教育背景" in inspected["page"]["sections"]
        assert "家庭成员" in inspected["page"]["sections"]
        assert "手机号登录" not in inspected["page"]["sections"]

        result = app.autofill(
            job_title="后端开发工程师",
            job_company="示例公司",
        )
        report["autofill"] = {
            key: result[key] for key in (
                "status",
                "success",
                "stopped_before_submit",
                "filled_count",
                "uploaded_count",
                "final_button",
                "consent_pending",
                "unknown_required",
            )
        }
        assert result["status"] == "terms_required", result
        assert result["success"] is True, result
        assert result["stopped_before_submit"] is True
        assert result["final_button"]["text"] == "预览并提交", result["final_button"]
        assert any(item["id"] for item in result["consent_pending"]), result
        assert not result["unknown_required"], result["unknown_required"]
        assert result["filled_count"] >= 7, result
        assert result["uploaded_count"] >= 1, result

        values = _values(app.browser.current())
        report["values"] = values
        assert values["name"] == "测试用户", values
        assert values["highestDegree"] == "本科", values
        assert values["school"] == "测试大学", values
        assert values["major"] == "软件工程", values
        assert values["eduDegree"] == "本科", values
        assert values["eduStartYear"] == "2023", values
        assert values["eduStartMonth"] == "09", values
        assert values["eduEndYear"] == "2027", values
        assert values["eduEndMonth"] == "06", values
        assert values["resume"] == "resume.pdf", values
        assert values["declare"] is False, values
        assert values["syncResume"] is False, values
        assert values["submitted"] == "", values

        tab = app.browser.current()
        tab.evaluate("window.showLoginModal()")
        handoff = app.autofill()
        report["login_modal"] = {
            "status": handoff["status"],
            "message": handoff["message"],
        }
        assert handoff["status"] == "human_handoff", handoff
        tab.evaluate("window.hideLoginModal()")

        app.open(MULTI_FIXTURE.as_uri())
        multi = app.autofill()
        multi_values = tab.evaluate(
            """({
              name: document.getElementById('name').value,
              phone: document.getElementById('phone').value,
              school: document.getElementById('school').value,
              major: document.getElementById('major').value,
              submitted: document.body.dataset.submitted || '',
            })"""
        )
        report["multi_step"] = {
            "status": multi["status"],
            "step_count": multi["step_count"],
            "steps": multi["steps"],
            "final_button": multi["final_button"],
            "values": multi_values,
        }
        assert multi["status"] == "ready_to_submit", multi
        assert multi["stopped_before_submit"] is True
        assert multi["step_count"] == 2, multi["steps"]
        assert multi["steps"][0]["status"] == "next", multi["steps"]
        assert multi["steps"][1]["status"] == "ready_to_submit", multi["steps"]
        assert multi["final_button"]["text"] == "提交申请", multi["final_button"]
        assert multi_values["name"] == "测试用户", multi_values
        assert multi_values["phone"] == "13800138000", multi_values
        assert multi_values["school"] == "测试大学", multi_values
        assert multi_values["major"] == "软件工程", multi_values
        assert multi_values["submitted"] == "", multi_values

        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0
    except Exception as exc:
        report["error"] = f"{type(exc).__name__}: {exc}"
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 1
    finally:
        if "--keep-chrome" not in sys.argv:
            _kill_test_chrome()


if __name__ == "__main__":
    raise SystemExit(main())
