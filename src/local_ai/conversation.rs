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
    pub fn english_exercise(
        &self,
        config: &Config,
        log: &Path,
        base: &Card,
        mode: &str,
    ) -> Result<Card, String> {
        if mode == "语法" {
            return crate::english::exercise(base, mode, None);
        }
        self.begin("正在生成英语练习")?;
        let result = (|| {
            let port = self.ensure_started(config, log)?;
            let schema = serde_json::json!({"type":"object","additionalProperties":false,
                "required":["prompt","reference","explanation"],"properties":{
                    "prompt":{"type":"string"},"reference":{"type":"string"},"explanation":{"type":"string"}}});
            let instruction = match mode {
                "中译英" => {
                    "prompt只给一句中文待翻译句子，不要泄露英文答案。reference给准确英文译文，必须使用目标词或其词形变化。"
                }
                "英译中" => "prompt给一句使用目标词的英文待翻译句子。reference给准确中文译文。",
                _ => {
                    "prompt给一句包含目标词、且只有一处常见语法错误的英文句子，请学生改正整句。reference给正确完整句子。不要只考拼写。"
                }
            };
            let text=self.complete(port,serde_json::json!({"model":"personaldesk-grader","max_tokens":650,"temperature":0.4,
                "chat_template_kwargs":{"enable_thinking":false},"response_format":{"type":"json_schema","json_schema":{"name":"english_exercise","strict":true,"schema":schema}},
                "messages":[{"role":"system","content":"生成准确、自然、简短的英语练习。以雅思常见生活、教育、环境主题为主，兼顾日常和技术语境。难度中等。explanation用中文讲清用法和语法，不能泛泛而谈。只返回JSON。"},
                    {"role":"user","content":format!("目标词：{}；中文词义：{}；练习方式：{}。{}",base.title,base.answer,mode,instruction)}]}))?;
            let draft: crate::english::Draft =
                serde_json::from_str(&text).map_err(|e| e.to_string())?;
            crate::english::exercise(base, mode, Some(draft))
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
