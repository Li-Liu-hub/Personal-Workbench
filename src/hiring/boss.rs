use super::{
    Result, Settings,
    browser::{self, Cdp, Site},
    store::{Job, Store},
};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};
mod pool;
pub use pool::run_pool;
const LIST: &str = include_str!("../../assets/hiring/boss_search_js_extract_list.js");
const DETAIL: &str = include_str!("../../assets/hiring/boss_search_js_extract_detail.js");
fn risk(tab: &mut Cdp, store: &mut Store) -> Result<()> {
    let r = tab.eval(include_str!(
        "../../assets/hiring/boss_search_js_detect_risk.js"
    ))?;
    if r["risk"].as_str().is_some() {
        store.risk_lock("boss")?;
        return Err("BOSS 页面出现登录或风险提示，已停止操作；请在浏览器检查".into());
    }
    Ok(())
}
pub fn search_url(settings: &Settings, page: u32) -> Result<String> {
    let cities: Value = serde_json::from_str(include_str!("../../assets/hiring/boss_cities.json"))
        .map_err(super::err)?;
    let code = cities["cities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == settings.city.trim())
        .and_then(|c| c["code"].as_str())
        .ok_or("未找到城市，请输入城市简称，如上海、北京")?;
    let mut url = url::Url::parse("https://www.zhipin.com/web/geek/job").unwrap();
    url.query_pairs_mut()
        .append_pair("query", settings.keyword.trim())
        .append_pair("city", code)
        .append_pair("page", &page.max(1).to_string());
    let mappings = [
        (
            "experience",
            settings.experience.as_str(),
            vec![
                ("经验不限", "101"),
                ("应届生", "102"),
                ("1年以内", "103"),
                ("1-3年", "104"),
                ("3-5年", "105"),
                ("5-10年", "106"),
                ("10年以上", "107"),
                ("在校生", "108"),
            ],
        ),
        (
            "degree",
            settings.degree.as_str(),
            vec![
                ("学历不限", "201"),
                ("大专", "202"),
                ("本科", "203"),
                ("硕士", "204"),
                ("博士", "205"),
            ],
        ),
        (
            "jobType",
            settings.job_type.as_str(),
            vec![("全职", "1901"), ("兼职", "1903"), ("实习", "1902")],
        ),
    ];
    for (key, value, options) in mappings {
        if value.trim().is_empty() {
            continue;
        }
        let code = options
            .iter()
            .find(|(k, _)| *k == value)
            .ok_or_else(|| format!("不支持的筛选值：{value}"))?
            .1;
        url.query_pairs_mut().append_pair(key, code);
    }
    Ok(url.to_string())
}
pub fn search(dir: &Path, settings: &Settings, page: u32) -> Result<Vec<Job>> {
    let mut store = Store::for_account(dir, &settings.account_id)?;
    store.reserve_page("search")?;
    let url = search_url(settings, page)?;
    let runtime = super::accounts::active(dir, settings)?.browser_directory(dir);
    let mut tab = if runtime.join("boss-tab.json").is_file()
        && browser::targets_for(dir, settings, Site::Boss).is_ok()
    {
        let mut tab = browser::selected_for(dir, settings, Site::Boss)?;
        tab.call("Page.navigate", json!({"url":url}))?;
        tab
    } else {
        browser::open(dir, settings, Site::Boss, &url)?;
        browser::selected_for(dir, settings, Site::Boss)?
    };
    tab.wait_location(&url)?;
    wait_list(&mut tab, &mut store)
}
fn wait_list(tab: &mut Cdp, store: &mut Store) -> Result<Vec<Job>> {
    let deadline = Instant::now() + Duration::from_secs(35);
    loop {
        risk(tab, store)?;
        let jobs = read_list(tab)?;
        if !jobs.is_empty() {
            return Ok(jobs);
        }
        let empty = tab.eval(r#"(()=>{const t=document.body?.innerText||'';return /暂无相关职位|没有找到相关职位|未找到相关职位|暂无搜索结果/.test(t)})()"#)?;
        if empty == true {
            return Ok(vec![]);
        }
        if Instant::now() >= deadline {
            return Err("岗位列表尚未加载或页面结构发生变化，未将读取失败记为空岗位".into());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
fn read_list(tab: &mut Cdp) -> Result<Vec<Job>> {
    let mut jobs: Vec<Job> = serde_json::from_value(tab.eval(LIST)?).map_err(super::err)?;
    for job in &mut jobs {
        if job
            .salary
            .chars()
            .any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c))
        {
            job.salary = "薪资需在网页核验".into();
        }
    }
    Ok(jobs)
}
pub fn scroll(dir: &Path, settings: &Settings) -> Result<Vec<Job>> {
    let mut store = Store::for_account(dir, &settings.account_id)?;
    store.reserve_page("search")?;
    let mut tab = browser::selected_for(dir, settings, Site::Boss)?;
    risk(&mut tab, &mut store)?;
    tab.eval(include_str!(
        "../../assets/hiring/boss_search_js_scroll_list.js"
    ))?;
    std::thread::sleep(Duration::from_secs(1));
    wait_list(&mut tab, &mut store)
}
pub fn detail(dir: &Path, settings: &Settings, url: &str) -> Result<Job> {
    super::store::canonical(url, true)?;
    let mut store = Store::for_account(dir, &settings.account_id)?;
    store.reserve_page("detail")?;
    let runtime = super::accounts::active(dir, settings)?.browser_directory(dir);
    let mut tab = if runtime.join("boss-tab.json").is_file()
        && browser::targets_for(dir, settings, Site::Boss).is_ok()
    {
        let mut tab = browser::selected_for(dir, settings, Site::Boss)?;
        tab.call("Page.navigate", json!({"url":url}))?;
        tab
    } else {
        browser::open(dir, settings, Site::Boss, url)?;
        browser::selected_for(dir, settings, Site::Boss)?
    };
    tab.wait_location(url)?;
    let deadline = Instant::now() + Duration::from_secs(35);
    loop {
        risk(&mut tab, &mut store)?;
        let value = tab.eval(DETAIL)?;
        if value["jd_truncated"] == true {
            return Err("岗位简介未完整显示，请先登录并在网页核验".into());
        }
        let job: Job = serde_json::from_value(value).map_err(super::err)?;
        if !job.title.is_empty() && !job.company.is_empty() && !job.jd.is_empty() {
            return Ok(job);
        }
        if Instant::now() >= deadline {
            return Err("岗位详情尚未加载完整或页面结构发生变化，已停止读取".into());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
pub fn login_status(dir: &Path, settings: &Settings) -> Result<String> {
    let mut tab = browser::selected_for(dir, settings, Site::Boss)?;
    let value = tab.eval(include_str!(
        "../../assets/hiring/boss_search_js_visible_login.js"
    ))?;
    Store::for_account(dir, &settings.account_id)?.mark_login(
        super::accounts::id(&settings.account_id)?,
        value["logged_in"] == true,
    )?;
    Ok(if value["logged_in"] == true {
        "检测到登录界面信息；请在浏览器核对本人账号及旧队列状态"
    } else {
        "尚未确认登录，请在浏览器扫码或完成验证"
    }
    .into())
}
fn wait_point(tab: &mut Cdp, script: &str) -> Result<Value> {
    for _ in 0..15 {
        let p = tab.eval(script)?;
        if p["x"].as_f64().is_some() {
            return Ok(p);
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Err("未找到预期的输入框或发送按钮，已停止".into())
}
fn focus_chat_script() -> &'static str {
    r#"(()=>{const input=document.querySelector('#chat-input, .chat-editor [contenteditable="true"], .chat-input [contenteditable="true"], .chat-editor textarea.input-area');if(!input)return {};input.focus();const r=input.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()"#
}
fn message_state(tab: &mut Cdp, text: &str) -> Result<String> {
    let script = include_str!("../../assets/hiring/boss_deliver_js_message_state.js")
        .replace("__TEXT__", &json!(text).to_string());
    Ok(tab.eval(&script)?["state"]
        .as_str()
        .unwrap_or("missing")
        .into())
}
fn images(tab: &mut Cdp, files: &[String]) -> Result<()> {
    for path in files {
        let before = tab.eval(include_str!(
            "../../assets/hiring/boss_deliver_js_own_images.js"
        ))?["count"]
            .as_u64()
            .unwrap_or(0);
        let mut probe = tab.eval(include_str!(
            "../../assets/hiring/boss_deliver_js_probe_file_input.js"
        ))?;
        if probe["found"] != true {
            tab.eval(include_str!(
                "../../assets/hiring/boss_deliver_js_probe_image_trigger.js"
            ))?;
            tab.eval("document.querySelector('[data-bosshunter-image-trigger]')?.click()")?;
            std::thread::sleep(Duration::from_millis(400));
            probe = tab.eval(include_str!(
                "../../assets/hiring/boss_deliver_js_probe_file_input.js"
            ))?;
        }
        if probe["found"] != true {
            return Err("未找到图片上传入口".into());
        }
        tab.upload(
            "document.querySelector('[data-bosshunter-upload]')",
            std::slice::from_ref(path),
        )?;
        std::thread::sleep(Duration::from_millis(800));
        let point = tab.eval(include_str!(
            "../../assets/hiring/boss_deliver_js_preview_send_button.js"
        ))?;
        if point["found"] == true || point["x"].is_number() {
            if point["x"].is_number() {
                tab.click(&point)?;
            } else {
                tab.eval("document.querySelector('[data-bosshunter-preview-send]')?.click()")?;
            }
        }
        let mut verified = false;
        for _ in 0..15 {
            std::thread::sleep(Duration::from_millis(300));
            if tab.eval(include_str!(
                "../../assets/hiring/boss_deliver_js_own_images.js"
            ))?["count"]
                .as_u64()
                .unwrap_or(0)
                > before
            {
                verified = true;
                break;
            }
        }
        if !verified {
            return Err("图片发送结果未确认".into());
        }
    }
    Ok(())
}
fn prepare(dir: &Path, settings: &Settings, job: &Job) -> Result<Job> {
    if settings.greeting.trim().is_empty() {
        return Err("请先填写招呼语".into());
    }
    for path in &settings.images {
        if !Path::new(path).is_file() {
            return Err("简历图片路径已失效，请重新选择".into());
        }
    }
    let detail = detail(dir, settings, &job.url)?;
    if detail.title.is_empty() || detail.company.is_empty() {
        return Err("未能确认岗位与公司，请在网页检查".into());
    }
    if !job.title.is_empty() && job.title != detail.title
        || !job.company.is_empty() && job.company != detail.company
    {
        return Err("网页岗位信息与队列不符，已停止投递".into());
    }
    let mut tab = browser::selected_for(dir, settings, Site::Boss)?;
    let mut store = Store::for_account(dir, &settings.account_id)?;
    risk(&mut tab, &mut store)?;
    Ok(detail)
}
pub fn deliver(dir: &Path, settings: &Settings, job: &Job, dry_run: bool) -> Result<String> {
    let detail = prepare(dir, settings, job)?;
    if dry_run {
        return Ok(format!(
            "预检通过：{} · {}；尚未点击沟通或发送",
            detail.company, detail.title
        ));
    }
    send_prepared(dir, settings, &detail)
}
fn send_prepared(dir: &Path, settings: &Settings, detail: &Job) -> Result<String> {
    let mut tab = browser::selected_for(dir, settings, Site::Boss)?;
    tab.wait_location(&detail.url)?;
    let current: Job = serde_json::from_value(tab.eval(DETAIL)?).map_err(super::err)?;
    if super::dedupe::fingerprint(&current) != super::dedupe::fingerprint(detail)
        || current.url != detail.url
    {
        return Err("预检后的岗位内容发生变化，已停止发送，请重新预检".into());
    }
    let mut store = Store::for_account(dir, &settings.account_id)?;
    risk(&mut tab, &mut store)?;
    let id = store.reserve("boss", detail, settings)?;
    let outcome = (|| -> Result<&str> {
        let before = browser::targets_for(dir, settings, Site::Boss)?;
        tab.eval(include_str!(
            "../../assets/hiring/boss_deliver_chat_button_script.js"
        ))?;
        std::thread::sleep(Duration::from_millis(700));
        // A start-chat dialog belongs to the exact job detail page just verified above.
        let start = tab.eval(include_str!(
            "../../assets/hiring/boss_deliver_js_startchat_input_rect.js"
        ))?;
        if start["success"] == true {
            tab.click(&start)?;
            tab.call("Input.dispatchKeyEvent",json!({"type":"keyDown","key":"a","code":"KeyA","modifiers":2,"windowsVirtualKeyCode":65}))?;
            tab.call("Input.dispatchKeyEvent",json!({"type":"keyUp","key":"a","code":"KeyA","modifiers":2,"windowsVirtualKeyCode":65}))?;
            tab.call("Input.insertText", json!({"text":settings.greeting}))?;
            let button = wait_point(
                &mut tab,
                include_str!("../../assets/hiring/boss_deliver_js_startchat_submit_rect.js"),
            )?;
            tab.click(&button)?;
        } else {
            // Reconnect only to our selected target, then verify BOTH company and title.
            std::thread::sleep(Duration::from_millis(500));
            browser::adopt_new_chat(dir, settings, &before)?;
            tab = browser::selected_for(dir, settings, Site::Boss)?;
            let script = include_str!("../../assets/hiring/boss_deliver_js_chat_ready.js")
                .replace("__COMPANY__", &json!(detail.company).to_string())
                .replace("__TITLE__", &json!(detail.title).to_string());
            let ready = tab.eval(&script)?;
            if ready["input"] != true
                || ready["company_match"] != true
                || ready["title_match"] != true
            {
                return Err("当前会话无法同时确认公司和岗位，请手动核验".into());
            }
            if message_state(&mut tab, &settings.greeting)? == "delivered" {
                return Err("会话中已有相同招呼语，请核验历史发送".into());
            }
            let point = wait_point(&mut tab, focus_chat_script())?;
            tab.click(&point)?;
            let existing = tab
                .eval("(()=>{const e=document.activeElement;return e.value||e.innerText||''})()")?;
            if existing.as_str().is_some_and(|s| !s.trim().is_empty()) {
                return Err("聊天输入框已有草稿，请先在网页处理".into());
            }
            tab.call("Input.insertText", json!({"text":settings.greeting}))?;
            let point = wait_point(
                &mut tab,
                r#"(()=>{const e=document.querySelector('.chat-editor .btn-send, .btn-send');if(!e||e.disabled||e.classList.contains('disabled'))return {};const r=e.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()"#,
            )?;
            tab.click(&point)?;
        }
        std::thread::sleep(Duration::from_secs(1));
        browser::adopt_new_chat(dir, settings, &before)?;
        tab = browser::selected_for(dir, settings, Site::Boss)?;
        risk(&mut tab, &mut store)?;
        let mut sent = false;
        for _ in 0..15 {
            if message_state(&mut tab, &settings.greeting)? == "delivered" {
                sent = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        if !sent {
            return Ok("ambiguous");
        }
        if !settings.images.is_empty() && images(&mut tab, &settings.images).is_err() {
            return Ok("partial");
        }
        Ok("sent")
    })();
    match outcome {
        Ok(status) => {
            store.finish(
                id,
                status,
                match status {
                    "sent" => "网页已确认发送",
                    "partial" => "文字已发送，图片结果待核验",
                    _ => "发送结果待核验",
                },
            )?;
            Ok(match status {
                "sent" => "投递已确认发送",
                "partial" => "文字已发送，图片待核验；不会自动重发",
                _ => "发送结果待核验；不会自动重试",
            }
            .into())
        }
        Err(error) => {
            store.finish(id, "ambiguous", "操作结果未确认，请到网页核验")?;
            Err(error)
        }
    }
}
pub fn run_queue(
    dir: &Path,
    settings: &Settings,
    limit: usize,
    cancel: Arc<AtomicBool>,
    progress: impl FnMut(String),
) -> Result<String> {
    pool::run_single(dir, settings, limit, cancel, progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_escapes_and_maps_filters() {
        let s = Settings {
            keyword: "Java & Rust".into(),
            ..Default::default()
        };
        let u = search_url(&s, 1).unwrap();
        let url = url::Url::parse(&u).unwrap();
        assert!(
            url.query_pairs()
                .any(|(k, v)| k == "query" && v == s.keyword)
        );
        assert!(
            url.query_pairs()
                .any(|(k, v)| k == "experience" && v == "108")
        );
    }
}
