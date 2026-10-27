from __future__ import annotations

import json

import pytest
import yaml
from click.testing import CliRunner

from bosshunter import mcp_server
from bosshunter.cli import cli
from bosshunter.errors import ToolError


def write_config(tmp_path):
    path = tmp_path / "config.yaml"
    path.write_text(
        yaml.safe_dump(
            {
                "search": {"keywords": ["Java"], "cities": ["上海"]},
                "delivery": {"send_windows": ["09:00-18:00"]},
                "browser": {"user_data_dir": "./data/chrome-profile"},
            },
            allow_unicode=True,
        ),
        encoding="utf-8",
    )
    return path


def test_cli_status_json(tmp_path):
    config = write_config(tmp_path)
    result = CliRunner().invoke(cli, ["--config", str(config), "--json", "status"])
    assert result.exit_code == 0, result.output
    payload = json.loads(result.output)
    assert payload["success"] is True
    assert payload["state"]["state_path"].endswith("data\\state.json") or payload["state"]["state_path"].endswith("data/state.json")
    assert payload["config"]["search"]["keywords"] == ["Java"]


def test_cli_rejects_unknown_filter(tmp_path, monkeypatch):
    config = write_config(tmp_path)

    def fail_search(*_args, **_kwargs):
        raise AssertionError("search should not run")

    monkeypatch.setattr("bosshunter.api.BossHunter.search", fail_search)
    result = CliRunner().invoke(
        cli,
        ["--config", str(config), "--json", "search", "--filter", "unknown=x"],
    )
    assert result.exit_code == 1
    payload = json.loads(result.output)
    assert payload["error"]["code"] == "invalid_filter"


def test_mcp_initialize_and_tools_list():
    response = mcp_server._handle_request({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {"protocolVersion": "2024-11-05"},
    })
    assert response["result"]["serverInfo"]["name"] == "bosshunter"
    assert "tools" in response["result"]["capabilities"]

    listed = mcp_server._handle_request({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
    names = {tool["name"] for tool in listed["result"]["tools"]}
    assert {
        "boss_search_jobs",
        "boss_apply",
        "boss_send_images",
        "boss_status",
        "boss_queue_list",
        "boss_queue_add",
        "boss_queue_update",
        "boss_queue_remove",
        "boss_queue_reset",
        "boss_queue_run",
    } <= names


def test_mcp_tool_error_is_reported(monkeypatch):
    def fail_tool(_name, _args):
        raise ToolError("login_required", "请先登录")

    monkeypatch.setattr(mcp_server, "call_tool", fail_tool)
    response = mcp_server._handle_request({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {"name": "boss_apply", "arguments": {"job_url": "x"}},
    })
    result = response["result"]
    assert result["isError"] is True
    assert result["structuredContent"]["error"]["code"] == "login_required"


def test_mcp_unknown_method():
    response = mcp_server._handle_request({"jsonrpc": "2.0", "id": 4, "method": "unknown"})
    assert response["error"]["code"] == -32601


def test_cli_queue_add_list_update_remove(tmp_path):
    config = write_config(tmp_path)
    url = "https://www.zhipin.com/job_detail/testjob123.html"
    runner = CliRunner()

    added = runner.invoke(
        cli,
        ["--config", str(config), "--json", "queue", "add", url, "--title", "后端实习", "--priority", "5"],
    )
    assert added.exit_code == 0, added.output
    payload = json.loads(added.output)
    assert payload["added"] == 1
    assert payload["summary"]["pending"] == 1

    listed = runner.invoke(cli, ["--config", str(config), "--json", "queue", "list"])
    assert listed.exit_code == 0, listed.output
    payload = json.loads(listed.output)
    assert payload["count"] == 1
    assert payload["items"][0]["title"] == "后端实习"
    assert payload["items"][0]["priority"] == 5

    updated = runner.invoke(
        cli,
        ["--config", str(config), "--json", "queue", "update", "testjob123", "--status", "skipped"],
    )
    assert updated.exit_code == 0, updated.output
    assert json.loads(updated.output)["item"]["status"] == "skipped"

    removed = runner.invoke(
        cli,
        ["--config", str(config), "--json", "queue", "remove", "testjob123"],
    )
    assert removed.exit_code == 0, removed.output
    assert json.loads(removed.output)["removed"] == 1
