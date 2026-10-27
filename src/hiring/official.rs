use super::{
    Result, Settings,
    browser::{self, Cdp, Site},
    profile::{Profile, norm},
    store::{Job, Store},
};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
const HELPER: &str = include_str!("../../assets/hiring/official_helper_js.js");
const INSPECT: &str = include_str!("../../assets/hiring/official_inspect_js.js");
const FILL: &str = include_str!("../../assets/hiring/official_fill_js.js");
fn script(template: &str, key: &str, value: &Value) -> String {
    template.replace(key, &value.to_string())
}
pub fn inspect(tab: &mut Cdp) -> Result<Value> {
    tab.eval(HELPER)?;
    tab.eval(&INSPECT.replace("__MAX_TEXT__", "0"))
}
fn blocked(page: &Value) -> Result<()> {
    if page["signals"]["captcha_evidence"]
        .as_array()
        .is_some_and(|a| !a.is_empty())
    {
        return Err("页面需要人工验证，请完成后重试".into());
    }
    if page["signals"]["login_evidence"]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v.as_str() != Some("已登录提示")))
    {
        return Err("页面需要人工登录，请完成后重试".into());
    }
    if page["signals"]["login_modal_visible"] == true {
        return Err("请先在浏览器完成登录".into());
    }
    Ok(())
}
pub fn summary(page: &Value) -> String {
    let fields = page["fields"].as_array().cloned().unwrap_or_default();
    let mut out = format!(
        "{}\n识别到 {} 个字段\n",
        page["title"].as_str().unwrap_or("招聘表单"),
        fields.len()
    );
    for field in fields {
        let filled =
            field["checked"] == true || field["value"].as_str().is_some_and(|s| !s.is_empty());
        out.push_str(&format!(
            "{}{} · {}\n",
            if field["required"] == true { "* " } else { "" },
            field["label"].as_str().unwrap_or("未命名字段"),
            if filled { "已填写" } else { "待填写" }
        ));
    }
    if page["iframes"].as_array().is_some_and(|fs| {
        fs.iter()
            .any(|f| f["same_origin"] == false && f["visible"] == true)
    }) {
        out.push_str("此页含跨域表单，请在浏览器检查未识别部分。\n");
    }
    out
}
fn fill_actions(tab: &mut Cdp, actions: &[Value]) -> Result<usize> {
    let mut done = 0;
    for action in actions {
        if action["custom_select"] == true {
            let point = tab.eval(&script(
                include_str!("../../assets/hiring/official_select_option_js.js"),
                "__OA_ID__",
                &action["id"],
            ))?;
            tab.click(&point)?;
            std::thread::sleep(Duration::from_millis(200));
            let option = tab.eval(&script(
                include_str!("../../assets/hiring/official_options_js.js"),
                "__VALUE__",
                &action["value"],
            ))?;
            if option["ok"] == true {
                tab.click(&option)?;
                done += 1;
            }
        } else {
            let result = tab.eval(&script(FILL, "__ACTIONS__", &json!([action])))?;
            if result["results"][0]["ok"] == true {
                done += 1;
            }
        }
    }
    Ok(done)
}
fn upload_materials(tab: &mut Cdp, page: &Value, profile: &Profile, base: &Path) -> Result<usize> {
    let mut count = 0;
    for field in page["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| f["kind"] == "file")
    {
        let label = norm(field["label"].as_str().unwrap_or(""));
        if label.contains("身份证") || label.contains("证件正面") || label.contains("证件反面")
        {
            continue;
        }
        let candidates: Vec<_> = profile
            .fields
            .iter()
            .filter(|f| {
                f.section == "documents"
                    && !super::profile::blank(&f.value)
                    && !f.label.contains("身份证")
            })
            .filter(|f| {
                let name = norm(&f.label);
                !label.is_empty()
                    && (name.contains(&label)
                        || label.contains(&name)
                        || label.contains("简历") && name.contains("简历PDF"))
            })
            .collect();
        if candidates.len() != 1 {
            continue;
        }
        let path = std::path::PathBuf::from(
            candidates[0]
                .value
                .trim()
                .trim_matches('`')
                .trim_matches('"'),
        );
        let path = if path.is_absolute() {
            path
        } else {
            base.join(path)
        };
        let expression = script(
            include_str!("../../assets/hiring/official_upload_object_js.js"),
            "__OA_ID__",
            &field["id"],
        );
        tab.upload(&expression, &[path.to_string_lossy().into_owned()])?;
        tab.eval(&script(
            include_str!("../../assets/hiring/official_dispatch_change_js.js"),
            "__OA_ID__",
            &field["id"],
        ))?;
        count += 1;
    }
    Ok(count)
}
pub fn autofill(dir: &Path, advance: bool, upload: bool) -> Result<String> {
    let profile = Profile::read(&dir.join("profile.md"))?;
    let mut tab = browser::selected(dir, Site::Official)?;
    let mut total = 0;
    let mut files = 0;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..8 {
        let page = inspect(&mut tab)?;
        blocked(&page)?;
        let signature = page["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| format!("{}:{}", f["label"], f["section"]))
            .collect::<Vec<_>>()
            .join("|");
        if !seen.insert(signature) {
            break;
        }
        let (actions, _) = profile.plan(&page);
        total += fill_actions(&mut tab, &actions)?;
        if upload {
            files += upload_materials(&mut tab, &page, &profile, dir)?;
        }
        if !advance {
            break;
        }
        let after = inspect(&mut tab)?;
        if !missing_required(&after).is_empty() {
            break;
        }
        let next = after["buttons"].as_array().into_iter().flatten().find(|b| {
            matches!(
                b["text"].as_str().unwrap_or(""),
                "下一步" | "下一页" | "Next" | "继续填写"
            )
        });
        let Some(next) = next else {
            break;
        };
        let point = tab.eval(&script(
            include_str!("../../assets/hiring/official_click_prepare_js.js"),
            "__OA_ID__",
            &next["id"],
        ))?;
        tab.click(&point)?;
        std::thread::sleep(Duration::from_millis(650));
    }
    let page = inspect(&mut tab)?;
    Ok(format!(
        "已填写 {total} 个字段，上传 {files} 份材料；停在最终提交前。\n{}",
        summary(&page)
    ))
}
pub fn missing_required(page: &Value) -> Vec<String> {
    page["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| f["required"] == true && f["disabled"] != true)
        .filter(|f| {
            if f["kind"] == "checkbox" || f["kind"] == "radio" {
                f["checked"] != true
            } else {
                f["value"].as_str().is_none_or(|s| s.trim().is_empty())
            }
        })
        .map(|f| f["label"].as_str().unwrap_or("未命名必填项").to_owned())
        .collect()
}
pub fn otp_fill(dir: &Path, inbox: Arc<Mutex<super::otp::Inbox>>) -> Result<String> {
    let code = inbox
        .lock()
        .map_err(super::err)?
        .latest()
        .ok_or("暂无 120 秒内的验证码，请先获取验证码")?;
    let mut tab = browser::selected(dir, Site::Official)?;
    let page = inspect(&mut tab)?;
    let fields: Vec<_> = page["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| {
            let hint = norm(&format!(
                "{} {} {}",
                f["label"].as_str().unwrap_or(""),
                f["name"].as_str().unwrap_or(""),
                f["placeholder"].as_str().unwrap_or("")
            ));
            !["图形", "图片", "滑块", "captcha"]
                .iter()
                .any(|k| hint.contains(k))
                && [
                    "验证码",
                    "校验码",
                    "动态码",
                    "otp",
                    "smscode",
                    "verificationcode",
                ]
                .iter()
                .any(|k| hint.contains(k))
                && ["input", ""].contains(&f["kind"].as_str().unwrap_or(""))
        })
        .collect();
    let actions = if fields.len() == 1 {
        vec![json!({"id":fields[0]["id"],"kind":"input","value":code.as_str()})]
    } else if fields.len() == code.len() && fields.iter().all(|f| f["max_length"] == 1) {
        fields
            .iter()
            .zip(code.chars())
            .map(|(f, c)| json!({"id":f["id"],"kind":"input","value":c.to_string()}))
            .collect()
    } else {
        return Err("验证码输入框不明确，请在网页手动核验".into());
    };
    let count = fill_actions(&mut tab, &actions)?;
    if count != actions.len() {
        return Err("验证码填写未完成，请检查输入框".into());
    }
    Ok(format!("验证码已直接填入 {count} 个输入框"))
}
pub fn submit(
    dir: &Path,
    settings: &Settings,
    accepted: bool,
    dry_run: bool,
    title: &str,
    company: &str,
) -> Result<String> {
    let mut tab = browser::selected(dir, Site::Official)?;
    let page = inspect(&mut tab)?;
    blocked(&page)?;
    if page["signals"]["already_applied_evidence"]
        .as_array()
        .is_some_and(|a| !a.is_empty())
    {
        return Err("网页已提示投递或申请完成，请核验历史记录".into());
    }
    let missing = missing_required(&page);
    if !missing.is_empty() {
        return Err(format!(
            "尚有 {} 个必填项未完成：{}",
            missing.len(),
            missing.join("、")
        ));
    }
    if page["signals"]["validation_errors"]
        .as_array()
        .is_some_and(|v| !v.is_empty())
    {
        return Err("页面仍有校验错误，请检查表单".into());
    }
    let buttons: Vec<_> = page["buttons"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| {
            matches!(
                b["text"].as_str().unwrap_or(""),
                "提交"
                    | "确认提交"
                    | "提交申请"
                    | "投递简历"
                    | "确认投递"
                    | "Submit"
                    | "Submit application"
            )
        })
        .collect();
    if buttons.len() != 1 {
        return Err("未能唯一识别最终提交按钮，请在网页手动提交".into());
    }
    use sha2::Digest;
    let review = json!({"url":page["url"],"fields":page["fields"].as_array().into_iter().flatten().map(|f|json!([f["label"],f["value"],f["checked"]])).collect::<Vec<_>>()});
    let digest = format!("{:x}", sha2::Sha256::digest(review.to_string().as_bytes()));
    let review_path = dir.join("official-review.json");
    if dry_run {
        super::atomic_json(
            &review_path,
            &json!({"digest":digest,"at":chrono::Local::now().timestamp()}),
        )?;
        return Ok("提交预检通过；尚未点击提交。请检查网页、条款及资料后确认。".into());
    }
    let prior: Value = serde_json::from_slice(
        &std::fs::read(&review_path).map_err(|_| "请先完成提交预检".to_owned())?,
    )
    .map_err(super::err)?;
    if prior["digest"] != digest
        || prior["at"]
            .as_i64()
            .is_none_or(|t| chrono::Local::now().timestamp() - t > 300)
    {
        return Err("页面内容已变化或预检已过期，请重新提交预检".into());
    }
    if !accepted {
        return Err("请先确认网页内容、条款和隐私协议".into());
    }
    let job = Job {
        url: page["url"].as_str().ok_or("无网页地址")?.into(),
        title: title.into(),
        company: company.into(),
        ..Default::default()
    };
    let mut store = Store::open(dir)?;
    let id = store.reserve("official", &job, settings)?;
    let result = (|| -> Result<String> {
        let point = tab.eval(&script(
            include_str!("../../assets/hiring/official_click_prepare_js.js"),
            "__OA_ID__",
            &buttons[0]["id"],
        ))?;
        tab.click(&point)?;
        std::thread::sleep(Duration::from_secs(2));
        let after = inspect(&mut tab)?;
        if after["signals"]["success_evidence"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
        {
            Ok("sent".into())
        } else {
            Ok("ambiguous".into())
        }
    })();
    match result {
        Ok(status) => {
            store.finish(
                id,
                &status,
                if status == "sent" {
                    "网页显示提交成功"
                } else {
                    "已点击提交，结果待人工核验"
                },
            )?;
            Ok(if status == "sent" {
                "网页显示投递成功"
            } else {
                "已点击提交，结果待核验；不会自动重试"
            }
            .into())
        }
        Err(e) => {
            store.finish(id, "ambiguous", "提交过程未确认，请在网页核验")?;
            Err(e)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mandatory_checkbox_and_file_block_submit() {
        let page = json!({"fields":[{"label":"协议","required":true,"kind":"checkbox","checked":false,"value":"on"},{"label":"简历","required":true,"kind":"file","value":""}]});
        assert_eq!(missing_required(&page), vec!["协议", "简历"]);
    }
}
