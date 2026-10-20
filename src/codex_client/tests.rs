use super::*;
fn state(path: &Path) -> (CodexState, Receiver<Value>, Sender<Result<Value, String>>) {
    let mut state = CodexState::load(path, &BTreeMap::new());
    let (send, out) = mpsc::channel();
    let (incoming, receive) = mpsc::channel();
    state.bridge = Some(Bridge {
        child: None,
        send,
        receive,
    });
    (state, out, incoming)
}
#[test]
fn handshake_stream_approvals_interrupt_and_history_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, out, incoming) = state(dir.path());
    state.request("initialize", json!({}));
    let id = out.recv().unwrap()["id"].clone();
    incoming.send(Ok(json!({"id":id,"result":{}}))).unwrap();
    state.poll();
    assert!(state.ready);
    assert_eq!(out.recv().unwrap()["method"], "initialized");
    assert_eq!(out.recv().unwrap()["method"], "account/read");
    assert_eq!(out.recv().unwrap()["method"], "model/list");
    state.project = dir.path().to_path_buf();
    state.prompt = "Synthetic request".into();
    state.submit();
    let request = out.recv().unwrap();
    assert_eq!(request["method"], "thread/start");
    assert!(request["params"].get("approvalPolicy").is_none());
    assert!(request["params"].get("sandbox").is_none());
    incoming
        .send(Ok(
            json!({"id":request["id"],"result":{"thread":{"id":"test-thread"}}}),
        ))
        .unwrap();
    state.poll();
    let turn = out.recv().unwrap();
    assert_eq!(turn["method"], "turn/start");
    assert_eq!(turn["params"]["input"][0]["text"], "Synthetic request");
    incoming
        .send(Ok(
            json!({"id":turn["id"],"result":{"turn":{"id":"test-turn"}}}),
        ))
        .unwrap();
    for delta in ["Synthetic ", "reply"] {
        incoming.send(Ok(json!({"method":"item/agentMessage/delta","params":{"threadId":"test-thread","itemId":"answer","delta":delta}}))).unwrap();
    }
    state.poll();
    assert_eq!(state.current.messages[1].text, "Synthetic reply");
    incoming.send(Ok(json!({"id":"approval-1","method":"item/commandExecution/requestApproval","params":{"command":"test command"}}))).unwrap();
    state.poll();
    assert!(state.has_request());
    assert!(out.try_recv().is_err());
    state.respond(json!({"decision":"decline"}));
    let response = out.recv().unwrap();
    assert_eq!(response["id"], "approval-1");
    assert_eq!(response["result"]["decision"], "decline");
    state.stop();
    let stop = out.recv().unwrap();
    assert_eq!(stop["method"], "turn/interrupt");
    assert_eq!(stop["params"]["turnId"], "test-turn");
    incoming.send(Ok(json!({"method":"turn/completed","params":{"threadId":"test-thread","turn":{"id":"test-turn","status":"interrupted"}}}))).unwrap();
    state.poll();
    assert!(!state.busy);
    assert!(state.turn_id.is_none());
    drop(state);
    let restored = CodexState::load(dir.path(), &BTreeMap::new());
    assert_eq!(restored.history.len(), 1);
    assert_eq!(restored.history[0].messages[1].text, "Synthetic reply");
}
#[test]
fn errors_wrong_threads_and_unsupported_requests_do_not_stall_or_autoapprove() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, out, incoming) = state(dir.path());
    state.ready = true;
    state.current.thread_id = Some("own-thread".into());
    incoming.send(Ok(json!({"method":"item/agentMessage/delta","params":{"threadId":"other-thread","itemId":"ignored","delta":"ignored"}}))).unwrap();
    state.poll();
    assert!(state.current.messages.is_empty());
    incoming
        .send(Ok(json!({"id":19,"method":"unknown/request","params":{}})))
        .unwrap();
    state.poll();
    assert_eq!(out.recv().unwrap()["error"]["code"], -32601);
    state.project = dir.path().to_path_buf();
    state.prompt = "Keep this on failure".into();
    state.submit();
    let resume = out.recv().unwrap();
    incoming
        .send(Ok(
            json!({"id":resume["id"],"error":{"message":"Synthetic failure","code":-1}}),
        ))
        .unwrap();
    state.poll();
    assert!(!state.busy);
    assert_eq!(state.prompt, "Keep this on failure");
    assert!(state.error.contains("Synthetic failure"));
    incoming.send(Err("Disconnected".into())).unwrap();
    state.poll();
    assert!(!state.ready);
    assert!(!state.busy);
}
#[test]
#[ignore = "Requires an installed local Codex; read-only handshake, no inference"]
fn live_local_codex_bridge_handshake() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = CodexState::load(dir.path(), &BTreeMap::new());
    state.connect();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(35);
    while std::time::Instant::now() < deadline {
        state.poll();
        if state.ready && state.pending.is_empty() {
            break;
        }
        assert!(state.error.is_empty(), "{}", state.error);
        thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(state.ready);
    assert!(state.pending.is_empty());
    assert!(!state.models.is_empty());
}
#[test]
#[ignore = "Uses the installed local Codex for one synthetic reply in an empty project"]
fn live_local_codex_chat_completion() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = CodexState::load(dir.path(), &BTreeMap::new());
    state.project = dir.path().to_path_buf();
    state.connect();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let mut sent = false;
    while std::time::Instant::now() < deadline {
        state.poll();
        assert!(state.error.is_empty(), "{}", state.error);
        if state.ready && state.pending.is_empty() && !sent {
            state.prompt="Reply with exactly PersonalDesk connection OK. Do not use tools or read or modify any files.".into();
            state.submit();
            sent = true;
        }
        if sent && !state.busy {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(30));
    }
    assert!(sent);
    assert!(!state.busy, "Timed out waiting for reply");
    assert!(
        state
            .current
            .messages
            .iter()
            .any(|m| m.role == "assistant" && m.text.contains("PersonalDesk connection OK"))
    );
    assert!(state.requests.is_empty());
}
