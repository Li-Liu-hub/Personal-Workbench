use super::{Settings, browser::*, err, official, otp};
use serde_json::{Value, json};
use std::{process::Command, time::Duration};

#[test]
#[ignore = "launches a dedicated headless Chrome against local fixtures only"]
fn native_browser_forms_upload_otp_preflight_and_submit() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path();
    let server = std::sync::Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let fixture = dir.join("resume.pdf");
    std::fs::write(&fixture, b"fixture attachment").unwrap();
    std::fs::write(
        dir.join("profile.md"),
        format!(
            "## 基本信息\n- 姓名：本地测试\n- 邮箱：test@example.test\n## 投递材料\n- 简历 PDF：{}",
            fixture.display()
        ),
    )
    .unwrap();
    let http = server.clone();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let quit = stop.clone();
    let worker = std::thread::spawn(move || {
        while !quit.load(std::sync::atomic::Ordering::Acquire) {
            if let Ok(Some(r)) = http.recv_timeout(Duration::from_millis(100)) {
                let html = r#"<!doctype html><html><meta charset="utf-8"><title>招聘测试表单</title><form onsubmit="event.preventDefault();document.body.innerHTML='提交成功'">
     <label>姓名<input name="name" required></label><label>邮箱<input type="email" required></label>
     <label>短信验证码<input name="otp" required></label><label>简历<input type="file" required></label>
     <label>我同意隐私协议<input type="checkbox" required></label><button type="submit">提交申请</button></form></html>"#;
                let _ = r.respond(
                    tiny_http::Response::from_string(html).with_header(
                        tiny_http::Header::from_bytes("Content-Type", "text/html; charset=utf-8")
                            .unwrap(),
                    ),
                );
            }
        }
    });
    let chrome = std::env::var_os("PERSONAL_DESK_TEST_BROWSER")
        .unwrap_or("C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe".into());
    assert!(
        targets(Site::Official).is_err(),
        "the dedicated test port must be free"
    );
    let mut child = Command::new(chrome)
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            "--remote-debugging-address=127.0.0.1",
            "--remote-debugging-port=19444",
        ])
        .arg(format!(
            "--user-data-dir={}",
            dir.join("browser-test").display()
        ))
        .arg("about:blank")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let result = (|| -> super::Result<()> {
        for _ in 0..50 {
            if targets(Site::Official).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let url = format!("http://{}/", server.server_addr());
        open(dir, &Settings::default(), Site::Official, &url)?;
        let mut tab = selected(dir, Site::Official)?;
        tab.wait_ready()?;
        let report = official::autofill(dir, false, true)?;
        assert!(report.contains("已填写 2 个字段"), "{report}");
        let values=tab.eval("Array.from(document.querySelectorAll('input')).map(e=>e.type==='file'?e.files.length:e.type==='checkbox'?e.checked:e.value)")?;
        assert_eq!(values[0], "本地测试");
        assert_eq!(values[1], "test@example.test");
        assert_eq!(values[2], "");
        assert_eq!(values[3], 1);
        assert_eq!(values[4], false);
        assert!(official::submit(dir, &Settings::default(), false, true, "测试", "测试").is_err());
        let receiver = otp::Receiver::start(dir, "127.0.0.1", 0)?;
        let webhook = std::fs::read_to_string(dir.join("otp_url.txt")).map_err(err)?;
        let agent: ureq::Agent = ureq::Agent::config_builder().proxy(None).build().into();
        agent
            .post(webhook)
            .send_json(json!({"content":"验证码123456"}))
            .map_err(err)?;
        let reply = official::otp_fill(dir, receiver.inbox.clone())?;
        assert!(!reply.contains("123456"));
        tab.eval("document.querySelector('input[type=checkbox]').click()")?;
        official::submit(dir, &Settings::default(), false, true, "测试", "测试")?;
        tab.eval("document.querySelector('input[name=name]').value='changed'")?;
        assert!(official::submit(dir, &Settings::default(), true, false, "测试", "测试").is_err());
        official::submit(dir, &Settings::default(), false, true, "测试", "测试")?;
        let text = official::submit(dir, &Settings::default(), true, false, "测试", "测试")?;
        assert_eq!(text, "网页显示投递成功");
        assert_eq!(tab.eval("document.body.innerText")?, "提交成功");
        Ok(())
    })();
    if let Ok(mut r) = ureq::get("http://127.0.0.1:19444/json/version").call()
        && let Ok(v) = r.body_mut().read_json::<Value>()
        && let Some(ws) = v["webSocketDebuggerUrl"].as_str()
        && let Ok(mut c) = Cdp::connect(ws, 19444)
    {
        let _ = c.call("Browser.close", json!({}));
    }
    let _ = child.wait();
    stop.store(true, std::sync::atomic::Ordering::Release);
    server.unblock();
    let _ = worker.join();
    result.unwrap();
}
