use super::*;
use std::io::{BufRead, BufReader};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dialogue {
    pub id: String,
    pub role: String,
    pub content: String,
    pub at: String,
}
impl Dialogue {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: role.into(),
            content: content.into(),
            at: chrono::Local::now().to_rfc3339(),
        }
    }
}
#[derive(Default)]
pub(super) struct Telemetry {
    started: Option<Instant>,
    elapsed: Duration,
    pub tokens: Option<u64>,
    pub speed: Option<f64>,
    pub partial: String,
    pub prompt: Option<(u64, u64)>,
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub active: bool,
    pub seconds: f64,
    pub tokens: Option<u64>,
    pub speed: Option<f64>,
    pub partial: String,
    pub prompt: Option<(u64, u64)>,
}
impl LocalAi {
    pub(super) fn begin(&self, label: &str) -> Result<(), String> {
        if self.busy.swap(true, Ordering::AcqRel) {
            return Err("模型正在处理上一项任务".into());
        }
        self.cancelled.store(false, Ordering::Release);
        if let Ok(mut metrics) = self.telemetry.lock() {
            *metrics = Telemetry {
                started: Some(Instant::now()),
                ..Default::default()
            };
        }
        self.set_status(label);
        Ok(())
    }
    pub(super) fn finish(&self, success: bool) {
        if let Ok(mut metrics) = self.telemetry.lock()
            && let Some(start) = metrics.started.take()
        {
            metrics.elapsed = start.elapsed();
        }
        if let Ok(mut r) = self.runtime.lock() {
            r.last_used = Instant::now();
        }
        self.set_status(if self.cancelled.load(Ordering::Acquire) {
            "已停止 · 可重试"
        } else if success {
            "已就绪 · 本地模型"
        } else {
            "未完成 · 可重试"
        });
        self.busy.store(false, Ordering::Release);
    }
    pub fn snapshot(&self) -> Snapshot {
        self.telemetry
            .lock()
            .map(|m| Snapshot {
                active: m.started.is_some(),
                seconds: m
                    .started
                    .map(|s| s.elapsed())
                    .unwrap_or(m.elapsed)
                    .as_secs_f64(),
                tokens: m.tokens,
                speed: m.speed,
                partial: m.partial.clone(),
                prompt: m.prompt,
            })
            .unwrap_or_default()
    }
    pub fn warmup(&self, config: &Config, log: &Path) -> Result<(), String> {
        self.begin("正在启动模型")?;
        let result = self.ensure_started(config, log).map(|_| ());
        self.finish(result.is_ok());
        result
    }
    pub(super) fn complete(
        &self,
        port: u16,
        mut request: serde_json::Value,
    ) -> Result<String, String> {
        request["stream"] = true.into();
        request["timings_per_token"] = true.into();
        request["return_progress"] = true.into();
        request["stream_options"] = serde_json::json!({"include_usage":true});
        let structured = request.get("response_format").is_some();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(180)))
            .build()
            .into();
        let mut response = agent
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .send_json(request)
            .map_err(|e| format!("模型请求失败：{e}"))?;
        self.set_status("正在读取题目");
        let mut reader = BufReader::new(response.body_mut().as_reader());
        let mut content = String::new();
        let mut line = String::new();
        let mut finished = false;
        loop {
            if self.cancelled.load(Ordering::Acquire) || self.closed.load(Ordering::Acquire) {
                return Err("本次操作已停止，原答案和记录仍保留".into());
            }
            line.clear();
            if reader
                .read_line(&mut line)
                .map_err(|e| format!("模型连接中断：{e}"))?
                == 0
            {
                break;
            }
            if line.len() > 262_144 {
                return Err("模型返回的数据过大".into());
            }
            let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else {
                continue;
            };
            if data == "[DONE]" {
                finished = true;
                break;
            }
            let chunk: serde_json::Value =
                serde_json::from_str(data).map_err(|e| format!("模型流格式错误：{e}"))?;
            if let Some(error) = chunk.get("error") {
                return Err(format!("模型返回错误：{error}"));
            }
            if let Some(delta) = chunk["choices"][0]["delta"]["content"].as_str() {
                content.push_str(delta);
                if content.len() > 65_536 {
                    return Err("回复超过长度限制".into());
                }
                self.set_status("正在生成回复");
            }
            if let Some(reason) = chunk["choices"][0]["finish_reason"].as_str() {
                if reason == "length" && structured {
                    return Err("回复达到长度上限，请重试或缩短答案".into());
                }
                finished = true;
            }
            if let Ok(mut metrics) = self.telemetry.lock() {
                update_metrics(&mut metrics, &chunk, &content);
            }
        }
        if !finished || content.trim().is_empty() {
            return Err("模型回复未完整返回，可重试".into());
        }
        Ok(content)
    }
    pub fn discuss(
        &self,
        config: &Config,
        log: &Path,
        card: &Card,
        answer: &str,
        feedback: &str,
        history: &[Dialogue],
    ) -> Result<String, String> {
        self.begin("正在准备讲解")?;
        let result = (|| {
            let port = self.ensure_started(config, log)?;
            let clip = |s: &str, n: usize| s.chars().take(n).collect::<String>();
            let context = serde_json::json!({"题目":clip(&card.prompt,1500),"参考解答":clip(&card.answer,1800),
                "学生答案":clip(answer,1800),"批改反馈":clip(feedback,800),
                "建表数据":card.sql.as_ref().map(|l|clip(&l.setup,1500))});
            let sql_rule = if card.sql.is_some() {
                "SQL 事实：WHERE 只保留 TRUE，FALSE 和 UNKNOWN 都不会被保留。请以题目参考事实为准。"
            } else {
                ""
            };
            let mut messages = vec![serde_json::json!({"role":"system","content":format!(
                "你是耐心的学习老师，用简明中文解释概念，给一个具体例子，再指出下一步怎么练。根据学生追问回答，不要只罗列评分点。英语题给英文示例和中文解释，合理的不同译法都接受。SQL 使用 SQLite。只讨论当前练习，不声称执行过任何程序。{sql_rule}上下文：{context}")})];
            for message in history.iter().skip(history.len().saturating_sub(6)) {
                messages.push(
                    serde_json::json!({"role":message.role,"content":clip(&message.content,1000)}),
                );
            }
            self.complete(port,serde_json::json!({"model":"personaldesk-grader","messages":messages,
                "temperature":0.3,"max_tokens":1000,"chat_template_kwargs":{"enable_thinking":false}}))
        })();
        self.finish(result.is_ok());
        result
    }
    #[cfg(test)]
    pub fn english_exercise(
        &self,
        config: &Config,
        log: &Path,
        base: &Card,
        mode: &str,
    ) -> Result<Card, String> {
        self.english_variant(config, log, base, mode, &crate::english::plan(mode, 0))
    }
    pub fn english_variant(
        &self,
        config: &Config,
        log: &Path,
        base: &Card,
        mode: &str,
        plan: &crate::english::Plan,
    ) -> Result<Card, String> {
        if mode == "语法" {
            return crate::english::grammar_card(plan.sequence);
        }
        self.begin("正在生成英语练习")?;
        let result = (|| {
            let port = self.ensure_started(config, log)?;
            let schema = serde_json::json!({"type":"object","additionalProperties":false,
                "required":["a_prompt","b_reference","c_explanation"],"properties":{
                    "a_prompt":{"type":"string"},"b_reference":{"type":"string"},"c_explanation":{"type":"string"}}});
            let instruction = match mode {
                "中译英" => {
                    "a_prompt必须是一句中文原句，不得给英文！b_reference必须是对应的英文译文，使用目标词或词形变化。c_explanation用中文解释译法。"
                }
                "英译中" => {
                    "a_prompt必须是一句使用目标词的完整英文原句；b_reference必须是中文译文，不得给英文！c_explanation用中文解释译法。不要设置空格或出填空题。"
                }
                "搭配填空" => {
                    "给出一个含空格的英文句子及候选词/原形，要求填写并说明理由，目标词参与搭配；reference必须给答案和理由。"
                }
                "句式改写" => {
                    "给英文原句及明确句式转换要求，目标词自然出现在句子中；reference给保留原意的改写和理由。"
                }
                "短文理解" => {
                    "给60到90词英文短文，自然使用目标词。给2道围绕主要信息或推断的问题，并问目标词在此处的意思，接受中文或英文回答。reference逐问回答并指明原文依据。"
                }
                _ => {
                    "严格按照指定的语法知识点和题目形式出题，自然使用目标词。改错题原句必须有一处主要语法错误，reference只给改正后的完整句子，不附解释。其他题型给完整解答。具体规则和理由放explanation。"
                }
            };
            let mut request = serde_json::json!({"model":"personaldesk-grader","max_tokens":1100,"temperature":0.55,"seed":plan.sequence,
                "chat_template_kwargs":{"enable_thinking":false},"response_format":{"type":"json_schema","json_schema":{"name":"english_exercise","strict":true,"schema":schema}},
                "messages":[{"role":"system","content":"生成准确、自然、简短的英语练习。以雅思生活、教育、环境主题为主，兼顾日常和技术语境。先生成a_prompt题干，再生成b_reference解答，最后c_explanation用中文具体解释这道题的用法与理由，不能泛泛而谈。只返回JSON。"},
                    {"role":"user","content":format!("目标词：{}；中文词义：{}；练习方式：{}。{}\n{}",base.title,base.answer,mode,instruction,plan.instructions())}]});
            let example = match mode {
                "中译英" => Some(
                    serde_json::json!({"a_prompt":"每天阅读能提高我们的写作能力。","b_reference":"Reading every day can improve our writing skills.","c_explanation":"improve 后接提升的对象；can 后用动词原形。"}),
                ),
                "英译中" => Some(
                    serde_json::json!({"a_prompt":"Reading every day can improve our writing skills.","b_reference":"每天阅读能提高我们的写作能力。","c_explanation":"improve 表示提高，writing skills 是写作能力。"}),
                ),
                "搭配填空" => Some(
                    serde_json::json!({"a_prompt":"用 improve 的合适形式填空并解释搭配：Regular practice can ___ your writing skills.","b_reference":"improve；Regular practice can improve your writing skills.","c_explanation":"can 后用动词原形；improve skills 表示提升技能，improve 为及物动词，直接接宾语。"}),
                ),
                "语法" if plan.form == "改错并解释" => Some(
                    serde_json::json!({"a_prompt":"改正下面句子并说明原因：These two changes has improved reliability.","b_reference":"These two changes have improved reliability.","c_explanation":"原句changes是复数主语，has应改为have；现在完成时其余结构保留。"}),
                ),
                _ => None,
            };
            if let Some(example) = example {
                let messages = request["messages"].as_array_mut().unwrap();
                messages.insert(1, serde_json::json!({"role":"user","content":format!("格式示例：目标词improve，练习方式{mode}。注意题干与答案的语言和区别。实际题目必须按后面给的词条、知识点和语境重新编写。")}));
                messages.insert(
                    2,
                    serde_json::json!({"role":"assistant","content":example.to_string()}),
                );
            }
            for attempt in 0..2 {
                let text = self.complete(port, request.clone())?;
                #[cfg(test)]
                eprintln!("GENERATED {mode} attempt={attempt}: {text}");
                let result = serde_json::from_str::<crate::english::Draft>(&text)
                    .map_err(|e| e.to_string())
                    .and_then(|draft| plan.exercise(base, mode, Some(draft)));
                match result {
                    Ok(card) => return Ok(card),
                    Err(error) if attempt == 0 => {
                        self.set_status("正在校验并重做英语题…");
                        request["seed"] = serde_json::json!(plan.sequence.wrapping_add(1));
                        request["messages"].as_array_mut().unwrap().extend([
                            serde_json::json!({"role":"assistant","content":text}),
                            serde_json::json!({"role":"user","content":format!("题目未通过检查：{error}。请重新生成完整JSON，保留指定知识点和题目形式。")}),
                        ]);
                    }
                    Err(error) => return Err(format!("生成的练习未通过检查：{error}")),
                }
            }
            unreachable!()
        })();
        self.finish(result.is_ok());
        result
    }
}
fn update_metrics(metrics: &mut Telemetry, chunk: &serde_json::Value, content: &str) {
    metrics.partial = content.into();
    if let Some(n) = chunk["timings"]["predicted_n"]
        .as_u64()
        .or_else(|| chunk["usage"]["completion_tokens"].as_u64())
    {
        metrics.tokens = Some(n);
    }
    if let Some(speed) = chunk["timings"]["predicted_per_second"]
        .as_f64()
        .filter(|s| s.is_finite() && *s >= 0.)
    {
        metrics.speed = Some(speed);
    }
    if let (Some(done), Some(total)) = (
        chunk["prompt_progress"]["processed"].as_u64(),
        chunk["prompt_progress"]["total"].as_u64(),
    ) {
        metrics.prompt = Some((done, total));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_speed_comes_from_runtime_timings_not_stream_chunk_counts() {
        let mut telemetry = Telemetry::default();
        update_metrics(
            &mut telemetry,
            &serde_json::json!({"choices":[{"delta":{"content":"hello"}}]}),
            "hello",
        );
        assert_eq!(telemetry.tokens, None);
        assert_eq!(telemetry.speed, None);
        update_metrics(
            &mut telemetry,
            &serde_json::json!({"timings":{"predicted_n":8,"predicted_per_second":42.5},"prompt_progress":{"processed":40,"total":100}}),
            "hello world",
        );
        assert_eq!(telemetry.tokens, Some(8));
        assert_eq!(telemetry.speed, Some(42.5));
        assert_eq!(telemetry.prompt, Some((40, 100)));
    }
}
