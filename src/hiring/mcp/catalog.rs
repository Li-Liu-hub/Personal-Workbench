use super::*;

fn string() -> Value {
    json!({"type":"string","maxLength":10000})
}
fn boolean() -> Value {
    json!({"type":"boolean"})
}
fn integer(min: u64, max: u64) -> Value {
    json!({"type":"integer","minimum":min,"maximum":max})
}
fn strings() -> Value {
    json!({"type":"array","items":string(),"maxItems":50})
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn tool(
    name: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    read_only: bool,
    external: bool,
) -> Value {
    json!({"name":name,"description":description,"inputSchema":object(properties,required),
        "annotations":{"readOnlyHint":read_only,"destructiveHint":!read_only,"idempotentHint":read_only,"openWorldHint":external}})
}
pub fn tools() -> Vec<Value> {
    let settings = object(
        json!({"keyword":string(),"city":string(),"experience":string(),"degree":string(),"job_type":string(),
        "greeting":string(),"images":strings(),"daily_limit":integer(1,u64::from(crate::hiring::BOSS_MAX_DAILY)),"interval_min":integer(30,3600),"interval_max":integer(30,3600),
        "send_windows":strings(),"browser_path":string(),"otp_host":string(),"otp_port":integer(0,65535),"unified_max_pages":integer(1,50)}),
        &[],
    );
    let mut catalog = vec![
        tool(
            "hiring_status",
            "查看原生求职工具状态、资料是否存在、队列数量和后台任务进度；不返回短信或密钥。",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "hiring_import",
            "将原三个工具的资料、设置、接码密钥和队列复制导入；原文件保留，旧待投队列默认跳过。",
            json!({"source_directory":string()}),
            &["source_directory"],
            false,
            false,
        ),
        tool(
            "hiring_settings_get",
            "读取求职工具配置，不包含接码密钥或登录 Cookie。",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "hiring_settings_update",
            "按字段更新配置，保留未传字段；运行中的队列使用启动时的配置。",
            json!({"settings":settings}),
            &["settings"],
            false,
            false,
        ),
        tool(
            "hiring_history",
            "读取最近 200 条 BOSS/官网投递结果和待核验记录。",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "boss_open_login",
            "打开 BOSS 专用浏览器，用户在网页完成登录和人工验证。",
            json!({}),
            &[],
            false,
            true,
        ),
        tool(
            "boss_check_login",
            "检查 BOSS 专用浏览器是否登录。",
            json!({}),
            &[],
            true,
            true,
        ),
        tool(
            "boss_search_jobs",
            "按关键词、城市、经验、学历、类型搜索 BOSS 岗位；仅本次搜索使用覆盖参数，不改持久配置。",
            json!({"keyword":string(),"city":string(),"experience":string(),"degree":string(),"job_type":string(),"page":integer(1,10)}),
            &[],
            false,
            true,
        ),
        tool(
            "boss_scroll_jobs",
            "加载当前 BOSS 列表的更多岗位。",
            json!({}),
            &[],
            false,
            true,
        ),
        tool(
            "boss_job_detail",
            "打开指定 BOSS 详情并读取岗位、公司和要求。",
            json!({"url":string()}),
            &["url"],
            false,
            true,
        ),
        tool(
            "boss_apply",
            "默认 dry_run=true 仅预检。正式发送招呼和配置的图片需用户明确授权、confirmed=true、dry_run=false；保留限额、时段、岗位核验和重复发送保护。",
            json!({"url":string(),"title":string(),"company":string(),"dry_run":boolean(),"confirmed":boolean()}),
            &["url"],
            false,
            true,
        ),
        tool(
            "boss_queue_list",
            "查看持久投递队列，可按状态筛选。",
            json!({"status":string()}),
            &[],
            true,
            false,
        ),
        tool(
            "boss_queue_add",
            "将 BOSS 职位加入待投队列；规范化链接去重，不会发送消息。",
            json!({"url":string(),"title":string(),"company":string(),"jd":json!({"type":"string","maxLength":100000}),"salary":string(),"location":string(),"experience":string(),"education":string(),"hr_name":string(),"note":string(),"priority":integer(0,100),"account_id":string()}),
            &["url"],
            false,
            false,
        ),
        tool(
            "boss_queue_update",
            "将队列项设为 pending 或 skipped；已有发送和待核验记录不能重新设为待投。",
            json!({"url":string(),"status":{"type":"string","enum":["pending","skipped"]}}),
            &["url", "status"],
            false,
            false,
        ),
        tool(
            "boss_queue_run",
            "用户明确授权后 confirmed=true 启动后台队列，立即返回 task_id；用 hiring_status 查询，boss_queue_stop 停止。保留随机间隔和限额。",
            json!({"limit":integer(1,u64::from(crate::hiring::BOSS_MAX_DAILY)),"confirmed":boolean()}),
            &["confirmed"],
            false,
            true,
        ),
        tool(
            "boss_queue_stop",
            "停止共享数据目录内的全部账号队列，包括其他 MCP 会话和桌面任务；当前正在发送的一项核验完成后停止。",
            json!({}),
            &[],
            false,
            false,
        ),
        tool(
            "official_open",
            "打开公司招聘网址，用户完成登录和验证码后再识别表单。",
            json!({"url":string()}),
            &["url"],
            false,
            true,
        ),
        tool(
            "official_inspect",
            "识别官网表单，返回字段标签、类型、选项、必填/已填状态与登录验证信号；不返回字段值、验证码或 Cookie。",
            json!({}),
            &[],
            false,
            true,
        ),
        tool(
            "official_profile_check",
            "检查本地资料，返回已配置标签和空白项，不返回资料正文。",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "official_plan",
            "只预览资料到官网字段的映射和待处理必填项，不返回具体资料值，不修改表单内容。",
            json!({}),
            &[],
            false,
            true,
        ),
        tool(
            "official_autofill",
            "使用本地资料填写字段；upload_documents=true 才上传材料，advance=true 才推进明确下一步；保留已有值，停在最终提交前。",
            json!({"upload_documents":boolean(),"advance":boolean()}),
            &[],
            false,
            true,
        ),
        tool(
            "official_submit",
            "默认 dry_run=true 预检。正式提交需 confirmed=true、dry_run=false、五分钟内预检且页面未变化；用户须在网页核验资料和条款。",
            json!({"dry_run":boolean(),"confirmed":boolean(),"job_title":string(),"job_company":string()}),
            &[],
            false,
            true,
        ),
        tool(
            "official_otp_fill",
            "把接码服务最近 120 秒的验证码直接填入官网专用浏览器，不将验证码传给模型。支持桌面或本 MCP 启动的原生接码服务。",
            json!({}),
            &[],
            false,
            true,
        ),
        tool(
            "otp_start",
            "启动原生接码服务或连接已运行的原生服务；返回监听地址和手机配置文件路径，不返回 token。默认读取已有设置。",
            json!({"host":string(),"port":integer(0,65535)}),
            &[],
            false,
            false,
        ),
        tool(
            "otp_status",
            "查看接收计数和有效验证码数量，不返回短信或验证码。",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "otp_stop",
            "停止本 MCP 启动的接码服务并清除内存验证码，不停止桌面程序拥有的服务。",
            json!({}),
            &[],
            false,
            false,
        ),
    ];
    for item in &mut catalog {
        let name = item["name"].as_str().unwrap_or("");
        if matches!(
            name,
            "boss_open_login"
                | "boss_check_login"
                | "boss_search_jobs"
                | "boss_scroll_jobs"
                | "boss_job_detail"
                | "boss_apply"
                | "boss_queue_run"
                | "boss_queue_list"
        ) {
            item["inputSchema"]["properties"]["account_id"] = string();
        }
    }
    catalog.extend([
        tool("hiring_accounts_list","查看最多三个独立BOSS账号、登录核验状态、今日已占用额度及已确认发送数；不返回Cookie或登录密钥。",json!({}),&[],true,false),
        tool("hiring_account_create","创建一个独立浏览器账号槽位，最多三个；不会复制其他账号登录态，也不会清除历史记录。",json!({"label":string()}),&["label"],false,false),
        tool("hiring_account_update","修改账号名称、启用状态或每日额度（1–150），不删除账号、不清零计数。",json!({"account_id":string(),"label":string(),"enabled":boolean(),"daily_limit":integer(1,150)}),&["account_id"],false,false),
        tool("hiring_account_select","选择桌面及未指定account_id的BOSS调用所使用的账号；切换不会共享登录Cookie。",json!({"account_id":string()}),&["account_id"],false,false),
        tool("boss_pool_plan","按全局15天去重窗口和每账号剩余额度分配不同岗位；默认dry_run=true只查看计划，false保存分配但不发送。",json!({"account_ids":{"type":"array","items":string(),"maxItems":3},"limit":integer(1,450),"dry_run":boolean()}),&["account_ids"],false,false),
        tool("boss_pool_run","用户明确授权并confirmed=true后执行已登录的1–3个账号，每账号最多150、合计最多450；各账号独立间隔，原子共享去重。任一任务风险或发送结果不明时停止整组。",json!({"account_ids":{"type":"array","items":string(),"maxItems":3},"limit":integer(1,450),"confirmed":boolean()}),&["account_ids","confirmed"],false,true),
        tool("hiring_dedupe_check","根据公司、岗位名称、完整岗位简介及原链接检查共享15天去重状态；未核验发送记录不随时间自动放行。",json!({"url":string(),"title":string(),"company":string(),"jd":json!({"type":"string","maxLength":100000})}),&["url"],true,false),
        tool("hiring_attempt_resolve","仅在用户已于网页人工核验实际结果后确认记录；verified_on_site和confirmed都必须为true。部分发送不能标为未发送以重发文字。",json!({"attempt_id":integer(1,i64::MAX as u64),"sent":boolean(),"verified_on_site":boolean(),"confirmed":boolean()}),&["attempt_id","sent","verified_on_site","confirmed"],false,false),
    ]);
    let platform = json!({"type":"string","enum":["51job","zhaopin","shixiseng"]});
    catalog.push(tool("hiring_run","一次启动四平台同时搜索、携带平台账号入库、统一去重筛选与补充岗位循环。固定优先级BOSS→智联→前程无忧→实习僧。默认dry_run=true只搜不投；正式自动投递须用户明确授权且confirmed=true,dry_run=false。今日全平台总额度达到上限即停止；无新岗位、验证或结果不明时暂停。",json!({"dry_run":boolean(),"confirmed":boolean(),"daily_limit":integer(1,150),"max_pages":integer(1,50)}),&[],false,true));
    catalog.push(tool(
        "hiring_stop",
        "停止统一搜索与投递任务及各平台队列；当前项核验完成后退出。",
        json!({}),
        &[],
        false,
        false,
    ));
    catalog.push(tool("hiring_queue_list","查看所有平台共用的一张岗位队列表，可按平台和状态筛选；每条返回来源平台。",json!({"platform":{"type":"string","enum":["boss","51job","zhaopin","shixiseng"]},"status":string()}),&[],true,false));
    catalog.push(tool(
        "hiring_platforms",
        "列出新增招聘平台及能力；各平台独立登录，所有平台共用一张岗位队列表、投递记录和15天去重。",
        json!({}),
        &[],
        true,
        false,
    ));
    for (name, description, mut properties, required, read_only, external) in [
        (
            "platform_open_login",
            "打开指定平台专用浏览器；用户自行登录并完善在线简历。",
            json!({}),
            vec!["platform"],
            false,
            true,
        ),
        (
            "platform_check_login",
            "检查当前平台网页的登录信号；未识别状态不会假定已登录。",
            json!({}),
            vec!["platform"],
            true,
            true,
        ),
        (
            "platform_search_jobs",
            "搜索前程无忧/智联/实习僧岗位，支持关键词、城市、页码；其他筛选在网页操作。",
            json!({"keyword":string(),"city":string(),"page":integer(1,50)}),
            vec!["platform"],
            false,
            true,
        ),
        (
            "platform_scroll_jobs",
            "读取当前平台列表加载后的更多岗位。",
            json!({}),
            vec!["platform"],
            false,
            true,
        ),
        (
            "platform_job_detail",
            "读取平台职位详情；必须识别岗位、公司及完整简介。",
            json!({"url":string()}),
            vec!["platform", "url"],
            false,
            true,
        ),
        (
            "platform_apply",
            "默认dry_run=true仅预检。正式投递须用户明确授权且confirmed=true,dry_run=false，五分钟内预检及当前网页未变化；使用网页在线简历，结果不明阻止重投。",
            json!({"url":string(),"title":string(),"company":string(),"dry_run":boolean(),"confirmed":boolean()}),
            vec!["platform", "url"],
            false,
            true,
        ),
        (
            "platform_queue_list",
            "查看指定平台持久待投队列。",
            json!({"status":string()}),
            vec!["platform"],
            true,
            false,
        ),
        (
            "platform_queue_add",
            "将职位加入指定平台队列，不发送申请。",
            json!({"url":string(),"title":string(),"company":string(),"jd":json!({"type":"string","maxLength":100000}),"priority":integer(0,100)}),
            vec!["platform", "url"],
            false,
            false,
        ),
        (
            "platform_queue_update",
            "跳过或恢复指定平台队列项；待核验和冷却中的岗位不能恢复待投。",
            json!({"url":string(),"status":{"type":"string","enum":["pending","skipped"]}}),
            vec!["platform", "url", "status"],
            false,
            false,
        ),
        (
            "platform_queue_run",
            "获得用户明确授权后confirmed=true执行平台队列；每项先预检，保留时段、间隔、每日上限和去重，遇到验证或结果不明停止。",
            json!({"limit":integer(1,150),"confirmed":boolean()}),
            vec!["platform", "confirmed"],
            false,
            true,
        ),
    ] {
        properties["platform"] = platform.clone();
        catalog.push(tool(
            name,
            description,
            properties,
            &required,
            read_only,
            external,
        ));
    }
    catalog.push(tool(
        "platform_queue_stop",
        "停止所有平台和BOSS后台队列，等待当前项完成核验。",
        json!({}),
        &[],
        false,
        false,
    ));
    catalog
}
pub fn find(name: &str) -> Option<Value> {
    tools().into_iter().find(|t| t["name"] == name)
}

pub fn validate(schema: &Value, value: &Value) -> Result<()> {
    let valid = match schema["type"].as_str().unwrap_or("") {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        _ => false,
    };
    if !valid {
        return Err(format!("参数类型应为 {}", schema["type"]));
    }
    if let Some(options) = schema["enum"].as_array()
        && !options.contains(value)
    {
        return Err("参数值不在允许范围".into());
    }
    if let Some(n) = value.as_i64() {
        if schema["minimum"].as_i64().is_some_and(|m| n < m)
            || schema["maximum"].as_i64().is_some_and(|m| n > m)
        {
            return Err("数字超出允许范围".into());
        }
    } else if value.is_u64() && value.as_i64().is_none() {
        return Err("数字超出允许范围".into());
    }
    if let Some(text) = value.as_str()
        && schema["maxLength"]
            .as_u64()
            .is_some_and(|m| text.len() as u64 > m)
    {
        return Err("文本过长".into());
    }
    if let Some(items) = value.as_array() {
        if schema["maxItems"]
            .as_u64()
            .is_some_and(|m| items.len() as u64 > m)
        {
            return Err("数组项目过多".into());
        }
        for item in items {
            validate(&schema["items"], item)?;
        }
    }
    if let Some(map) = value.as_object() {
        for required in schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !map.contains_key(required) {
                return Err(format!("缺少参数 {required}"));
            }
        }
        for (key, v) in map {
            let Some(property) = schema["properties"].get(key) else {
                return Err(format!("未知参数 {key}"));
            };
            validate(property, v).map_err(|e| format!("{key}: {e}"))?;
        }
    }
    Ok(())
}
