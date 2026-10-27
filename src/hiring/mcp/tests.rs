use super::*;
use std::io::Cursor;

fn session(messages: Vec<Value>, directory: &std::path::Path) -> Vec<Value> {
    let input = messages
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut output = Vec::new();
    serve(Cursor::new(input), &mut output, directory.to_path_buf()).unwrap();
    String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn init() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":PROTOCOL,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
}
fn ready() -> Value {
    json!({"jsonrpc":"2.0","method":"notifications/initialized"})
}
fn call(id: u32, name: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}})
}

#[test]
fn handshake_catalog_read_status_and_validation_are_clean_json_rpc() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("not-created");
    let replies = session(
        vec![
            call(0, "hiring_status", json!({})),
            init(),
            ready(),
            json!({"jsonrpc":"2.0","id":"tools","method":"tools/list"}),
            call(2, "hiring_status", json!({})),
            call(3, "boss_queue_run", json!({"limit":999,"confirmed":true})),
            call(
                4,
                "boss_apply",
                json!({"url":"https://www.zhipin.com/job_detail/fixture.html","dry_run":"false"}),
            ),
            call(5, "otp_status", json!({"token":"must-not-be-accepted"})),
            call(6, "no_such_tool", json!({})),
            json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":2}}),
            json!({"jsonrpc":"2.0","id":7,"method":"ping"}),
        ],
        &directory,
    );
    assert_eq!(replies.len(), 9);
    assert_eq!(replies[0]["error"]["code"], -32002);
    assert_eq!(replies[1]["result"]["protocolVersion"], PROTOCOL);
    let tools = replies[2]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 49);
    assert!(tools.iter().any(|t| t["name"] == "boss_apply"));
    assert!(tools.iter().any(|t| t["name"] == "official_submit"));
    assert!(tools.iter().any(|t| t["name"] == "otp_start"));
    assert_eq!(replies[3]["result"]["structuredContent"]["queue_count"], 0);
    assert!(
        !directory.exists(),
        "discovery and empty status must not initialize user data"
    );
    for reply in &replies[4..8] {
        assert_eq!(reply["error"]["code"], -32602);
    }
    assert_eq!(replies[8]["result"], json!({}));
}

#[test]
fn settings_patch_queue_dedup_and_send_confirmation_do_not_send() {
    let temp = tempfile::tempdir().unwrap();
    let replies = session(
        vec![
            init(),
            ready(),
            call(
                2,
                "hiring_settings_update",
                json!({"settings":{"city":"北京","greeting":"持久配置"}}),
            ),
            call(
                3,
                "hiring_settings_update",
                json!({"settings":{"city":"深圳"}}),
            ),
            call(4, "hiring_settings_get", json!({})),
            call(
                5,
                "boss_queue_add",
                json!({"url":"https://www.zhipin.com/job_detail/fixture.html?source=one","title":"测试岗位"}),
            ),
            call(
                6,
                "boss_queue_add",
                json!({"url":"https://www.zhipin.com/job_detail/fixture.html?source=two"}),
            ),
            call(
                7,
                "boss_apply",
                json!({"url":"https://www.zhipin.com/job_detail/fixture.html","dry_run":false}),
            ),
            call(8, "official_submit", json!({"dry_run":false})),
            call(9, "boss_queue_run", json!({"confirmed":false})),
            call(10, "boss_queue_list", json!({})),
            call(11, "hiring_history", json!({})),
        ],
        temp.path(),
    );
    assert_eq!(replies[3]["result"]["structuredContent"]["city"], "深圳");
    assert_eq!(
        replies[3]["result"]["structuredContent"]["greeting"],
        "持久配置"
    );
    assert_eq!(replies[4]["result"]["structuredContent"]["added"], true);
    assert_eq!(replies[5]["result"]["structuredContent"]["added"], false);
    for reply in &replies[6..9] {
        assert_eq!(reply["result"]["isError"], true);
    }
    assert_eq!(
        replies[9]["result"]["structuredContent"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        replies[10]["result"]["structuredContent"]["items"],
        json!([])
    );
}

#[test]
fn requested_daily_limit_150_is_persisted_and_151_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let replies = session(
        vec![
            init(),
            ready(),
            call(
                2,
                "hiring_settings_update",
                json!({"settings":{"daily_limit":150}}),
            ),
            call(3, "hiring_settings_get", json!({})),
            call(
                4,
                "hiring_settings_update",
                json!({"settings":{"daily_limit":151}}),
            ),
        ],
        temp.path(),
    );
    assert_eq!(replies[1]["result"]["isError"], false);
    assert_eq!(
        replies[2]["result"]["structuredContent"]["daily_limit"],
        150
    );
    assert_eq!(replies[3]["error"]["code"], -32602);
    assert_eq!(
        crate::hiring::Settings::load(temp.path())
            .unwrap()
            .daily_limit,
        150
    );
}

#[test]
fn account_tools_share_data_and_do_not_start_unconfirmed_or_unlogged_pool() {
    let temp = tempfile::tempdir().unwrap();
    let replies = session(
        vec![
            init(),
            ready(),
            call(2, "hiring_account_create", json!({"label":"账号2"})),
            call(3, "hiring_account_create", json!({"label":"账号3"})),
            call(4, "hiring_accounts_list", json!({})),
            call(5, "hiring_account_create", json!({"label":"账号4"})),
            call(
                6,
                "boss_pool_run",
                json!({"account_ids":["default"],"confirmed":false}),
            ),
            call(
                7,
                "boss_pool_plan",
                json!({"account_ids":["default"],"limit":450,"dry_run":true}),
            ),
            call(
                8,
                "hiring_dedupe_check",
                json!({"url":"https://www.zhipin.com/job_detail/test.html","company":"公司","title":"岗位","jd":"完整简介"}),
            ),
            call(9, "hiring_status", json!({})),
        ],
        temp.path(),
    );
    let accounts = replies[3]["result"]["structuredContent"]["items"]
        .as_array()
        .unwrap();
    assert_eq!(accounts.len(), 3);
    assert!(
        accounts
            .iter()
            .all(|a| a["daily_limit"] == 150 && a["remaining"] == 150)
    );
    assert_eq!(replies[4]["result"]["isError"], true);
    assert_eq!(replies[5]["result"]["isError"], true);
    assert_eq!(replies[6]["result"]["structuredContent"]["assigned"], 0);
    assert_eq!(
        replies[7]["result"]["structuredContent"]["shared_across_accounts"],
        true
    );
    assert_eq!(
        replies[8]["result"]["structuredContent"]["shared_dedup_days"],
        15
    );
    assert!(replies[8]["result"]["structuredContent"]["task"].is_null());
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let result = crate::hiring::boss::run_pool(
        temp.path(),
        &crate::hiring::Settings::default(),
        &["default".into()],
        450,
        cancel,
        |_| {},
    );
    assert!(result.unwrap_err().contains("尚未核验登录"));
    assert!(
        crate::hiring::store::Store::open(temp.path())
            .unwrap()
            .history()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn form_inspection_never_echoes_values_or_query_secrets() {
    let page = json!({"url":"https://example.test/apply?token=secret-token#secret-fragment","title":"测试表单",
        "fields":[{"id":"a","label":"短信验证码","kind":"input","value":"123456","required":true},
        {"id":"b","label":"姓名","kind":"input","value":"机密姓名","required":true}],
        "signals":{"validation_errors":["123456 is invalid"],"login_evidence":[],"captcha_evidence":[]}});
    let out = operations::redacted_page(&page);
    let rendered = out.to_string();
    for secret in ["123456", "机密姓名", "secret-token", "secret-fragment"] {
        assert!(!rendered.contains(secret));
    }
    assert_eq!(out["fields"][0]["filled"], true);
    assert_eq!(out["signals"]["has_validation_errors"], true);
}

#[test]
fn malformed_frames_do_not_poison_the_next_request() {
    let temp = tempfile::tempdir().unwrap();
    let input = b"not json\n[]\n{\"jsonrpc\":\"2.0\",\"id\":42,\"method\":\"ping\"}\n";
    let mut output = Vec::new();
    serve(Cursor::new(input), &mut output, temp.path().to_path_buf()).unwrap();
    let replies: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert_eq!(replies[1]["error"]["code"], -32600);
    assert_eq!(replies[2]["id"], 42);
}
#[test]
fn all_platform_mcp_queue_views_use_one_table_and_keep_confirmation_required() {
    let temp = tempfile::tempdir().unwrap();
    let first = json!({"platform":"51job","url":"https://jobs.51job.com/shanghai/12345678.html","title":"Java开发","company":"测试科技","jd":"负责后端开发"});
    let mut second = first.clone();
    second["platform"] = json!("zhaopin");
    second["url"] = json!("https://jobs.zhaopin.com/CC123456789J00000000001.htm");
    let mut separate = second.clone();
    separate["title"] = json!("测试开发");
    let replies = session(
        vec![
            init(),
            ready(),
            call(2, "platform_queue_add", first),
            call(3, "platform_queue_add", second),
            call(4, "platform_queue_add", separate),
            call(5, "hiring_queue_list", json!({})),
            call(6, "boss_queue_list", json!({})),
            call(7, "hiring_status", json!({})),
            call(
                8,
                "platform_queue_run",
                json!({"platform":"51job","confirmed":false}),
            ),
            call(
                9,
                "platform_apply",
                json!({"platform":"zhaopin","url":"https://jobs.zhaopin.com/CC123456789J00000000001.htm","dry_run":false}),
            ),
        ],
        temp.path(),
    );
    assert_eq!(replies[1]["result"]["structuredContent"]["added"], true);
    assert_eq!(replies[2]["result"]["structuredContent"]["added"], false);
    assert_eq!(replies[3]["result"]["structuredContent"]["added"], true);
    assert_eq!(
        replies[4]["result"]["structuredContent"]["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        replies[5]["result"]["structuredContent"]["items"],
        json!([])
    );
    assert_eq!(replies[6]["result"]["structuredContent"]["queue_count"], 2);
    assert_eq!(
        replies[6]["result"]["structuredContent"]["shared_across_platforms"],
        true
    );
    for reply in &replies[7..] {
        assert_eq!(reply["result"]["isError"], true);
    }
    let conn = rusqlite::Connection::open(temp.path().join("hiring.sqlite")).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM queue", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn otp_owned_and_external_receiver_share_codes_without_mcp_disclosure() {
    let temp = tempfile::tempdir().unwrap();
    let mut owner = operations::Server::new(temp.path().to_path_buf());
    let started = owner
        .call("otp_start", &json!({"host":"127.0.0.1","port":0}))
        .unwrap();
    let webhook = std::fs::read_to_string(temp.path().join("otp_url.txt")).unwrap();
    let agent: ureq::Agent = ureq::Agent::config_builder().proxy(None).build().into();
    agent
        .post(&webhook)
        .send_json(json!({"content":"验证码 654321"}))
        .unwrap();
    let mut second = operations::Server::new(temp.path().to_path_buf());
    let external = second.call("otp_start", &json!({})).unwrap();
    assert_eq!(external["status"]["owner"], "other_process");
    assert_eq!(external["status"]["counts"]["pending"], 1);
    let inbox = crate::hiring::otp::service_inbox(temp.path()).unwrap();
    assert_eq!(inbox.lock().unwrap().latest().unwrap().as_str(), "654321");
    assert_eq!(
        second.call("otp_stop", &json!({})).unwrap()["stopped"],
        false
    );
    let status = owner.call("otp_status", &json!({})).unwrap();
    for value in [started, external, status] {
        assert!(!value.to_string().contains("654321"));
        assert!(
            !value
                .to_string()
                .contains(webhook.split('/').next_back().unwrap())
        );
    }
    assert_eq!(owner.call("otp_stop", &json!({})).unwrap()["stopped"], true);
    assert!(crate::hiring::otp::service_status(temp.path()).is_err());
}

#[test]
#[cfg(windows)]
fn desktop_and_mcp_browser_lease_excludes_concurrent_process_work() {
    let temp = tempfile::tempdir().unwrap();
    let first = crate::hiring::lease::Lease::acquire(temp.path()).unwrap();
    assert!(crate::hiring::lease::Lease::acquire(temp.path()).is_err());
    drop(first);
    assert!(crate::hiring::lease::Lease::acquire(temp.path()).is_ok());
}
