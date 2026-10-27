"""Manual end-to-end check against the local fixture form.

Run from the OfficialApply directory:
    PYTHONPATH=src python tests/e2e_browser.py
"""

from __future__ import annotations

import base64
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
WORK = ROOT / "data" / "e2e"
FIXTURE = ROOT / "tests" / "fixtures" / "official_form.html"


def _write_fixture_files() -> None:
    WORK.mkdir(parents=True, exist_ok=True)
    for name in ("applications.jsonl", "session.json"):
        try:
            (WORK / name).unlink()
        except FileNotFoundError:
            pass
    (WORK / "resume.pdf").write_bytes(b"%PDF-1.4\n1 0 obj<<>>endobj\ntrailer<<>>\n%%EOF\n")
    (WORK / "photo.png").write_bytes(base64.b64decode(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII="
    ))
    (WORK / "profile.md").write_text(
        """## 基本信息
- 姓名：测试用户
- 性别：男
- 出生日期：2005-01-01
- 手机号：13800138000
- 邮箱：test@example.com
- 现居城市：上海

## 求职意向
- 期望岗位：后端开发工程师
- 期望城市：上海、深圳
- 到岗时间：2026-11

## 教育经历
### 测试大学
- 学校：测试大学
- 专业：软件工程
- 学历：本科
- 毕业时间：2027-06

## 投递材料
- 简历PDF：./resume.pdf
- 证件照：./photo.png
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
  auto_upload_documents: [resume, photo]
  screenshot_dir: ./screenshots
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


def main() -> int:
    _write_fixture_files()
    app = OfficialApply(WORK / "config.yaml")
    app.browser.session = SessionStore(WORK / "session.json")
    app.registry = Registry(WORK / "applications.jsonl")
    report: dict[str, object] = {}
    try:
        opened = app.open(FIXTURE.as_uri(), new=True)
        report["open"] = opened
        assert _wait_port(), "Chrome 调试端口未就绪"

        inspected = app.inspect()
        fields = inspected["page"]["fields"]
        report["inspect"] = {
            "field_count": len(fields),
            "labels": [item.get("label") for item in fields],
            "buttons": [item.get("text") for item in inspected["page"]["buttons"]],
        }
        assert len(fields) >= 8, fields

        plan = app.plan(job_title="后端开发工程师", job_company="示例公司")
        report["plan"] = {
            "actions": [
                {"label": item["label"], "value": item.get("value_preview"), "kind": item["kind"]}
                for item in plan["actions"]
            ],
            "uploads": [item["filename"] for item in plan["uploads"]],
            "unknown_required": plan["unknown_required"],
            "consent_pending": plan["consent_pending"],
        }
        assert len(plan["actions"]) >= 7, plan
        assert plan["uploads"], plan
        assert not plan["unknown_required"], plan["unknown_required"]

        filled = app.fill(
            job_title="后端开发工程师",
            job_company="示例公司",
            upload_documents=True,
        )
        report["fill"] = {
            "filled_count": filled["filled_count"],
            "failed": filled["failed"],
            "uploads": filled["uploads"],
        }
        assert filled["failed_count"] == 0, filled["failed"]
        assert all(item.get("ok") for item in filled["uploads"]), filled["uploads"]

        preview = app.submit(
            accept_terms=True,
            job_title="后端开发工程师",
            job_company="示例公司",
            dry_run=True,
        )
        report["submit_dry_run"] = {
            "button": preview["submit_button"].get("text"),
            "cannot_submit": preview["plan"]["summary"]["cannot_submit"],
        }

        submitted = app.submit(
            accept_terms=True,
            job_title="后端开发工程师",
            job_company="示例公司",
        )
        report["submit"] = submitted
        assert submitted["status"] == "submitted", submitted
        assert submitted["success"] is True, submitted

        history = app.history()
        report["history"] = history["items"]
        assert history["items"], history
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
