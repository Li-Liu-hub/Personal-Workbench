use super::*;

impl PersonalDeskApp {
    pub(super) fn start_text_model(&mut self) {
        if self.study.busy {
            return;
        }
        self.study.config.pin_spark_q4();
        let saved = self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| {
                db.set_setting(
                    "study.ai.config",
                    &serde_json::to_string(&self.study.config)?,
                )
            });
        if let Err(error) = saved {
            self.study.error = error.to_string();
            return;
        }
        self.study.busy = true;
        self.study.operation = "启动模型".into();
        self.study.error.clear();
        let ai = self.study.ai.clone();
        let config = self.study.config.clone();
        let log = self.data_dir.join("study-model.log");
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = ai.warmup(&config, &log);
            let _ = sender.send(BackgroundMessage::Learning(LearningMessage::Started(
                result,
            )));
        });
    }

    pub(super) fn render_model_settings(&mut self, ctx: &egui::Context) {
        if !self.study.model_settings {
            return;
        }
        let dialog = centered_dialog(ctx, "model-manager", "模型管理", "", 540., |ui| {
            ui.label("Spark-X2.6-4B · Q4_K_M：英语练习、AI 讲解和批改。");
            ui.label("Kokoro v1.1：英文单词与句子朗读。");
            ui.label("选择下方模型后启动；两者可以同时运行。");
            ui.push_id("manager-controls", |ui| self.render_model_controls(ui));
            ui.separator();
            ui.label("Spark2.6 Q4 文件");
            ui.label(self.study.config.model.display().to_string());
            ui.label(if self.study.config.model.is_file() {
                "模型文件已找到"
            } else {
                "未找到模型文件，请在练习设置中重新定位"
            });
            if ui.button("语音设置 / 下载 Kokoro").clicked() {
                self.study.model_settings = false;
                self.study.audio.settings = true;
            }
            if ui.button("练习设置 / 文件位置").clicked() {
                self.study.model_settings = false;
                self.study.settings = true;
            }
            if !self.study.error.is_empty() {
                ui.colored_label(Color32::from_rgb(184, 65, 71), &self.study.error);
            }
        });
        if dialog_closed(&dialog) {
            self.study.model_settings = false;
        }
    }
}
