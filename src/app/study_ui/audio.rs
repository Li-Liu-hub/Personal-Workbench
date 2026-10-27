use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) enum AudioMessage {
    Spoken(Result<(), String>),
    Started(Result<(), String>),
    Installed(Result<(), String>),
}
pub(in crate::app) struct AudioState {
    engine: Arc<crate::speech::SpeechEngine>,
    pub settings: bool,
    busy: bool,
    installing: bool,
    status: String,
    speed: f32,
    voice: u32,
    cancel: Arc<AtomicBool>,
}
impl Default for AudioState {
    fn default() -> Self {
        Self {
            engine: Arc::default(),
            settings: false,
            busy: false,
            installing: false,
            status: String::new(),
            speed: 1.,
            voice: 0,
            cancel: Arc::default(),
        }
    }
}
impl Drop for AudioState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl PersonalDeskApp {
    pub(super) fn render_kokoro_controls(&mut self, ui: &mut egui::Ui) {
        let available = !self.study.audio.busy && !self.study.audio.installing;
        let root = crate::speech::runtime_root(&self.data_dir);
        if ui
            .add_enabled(
                available && !self.study.audio.engine.is_loaded(),
                egui::Button::new("启动模型"),
            )
            .clicked()
        {
            if crate::speech::ready(&root) {
                self.start_speech_model();
            } else {
                self.study.audio.settings = true;
                self.study.audio.status = "请先下载 Kokoro 语音模型".into();
            }
        }
        if ui
            .add_enabled(
                self.study.audio.engine.is_loaded() || self.study.audio.busy,
                egui::Button::new(if self.study.audio.busy {
                    "停止朗读"
                } else {
                    "停止模型"
                }),
            )
            .clicked()
        {
            if self.study.audio.busy {
                self.study.audio.cancel.store(true, Ordering::Relaxed);
            } else {
                self.study.audio.engine.stop();
                self.study.audio.status = "语音模型已停止".into();
            }
        }
        ui.label(if self.study.audio.busy {
            "正在加载 / 朗读…"
        } else if self.study.audio.engine.is_loaded() {
            "Kokoro 已加载"
        } else {
            "Kokoro 未加载"
        });
        if !self.study.audio.status.is_empty() {
            ui.label(
                RichText::new(&self.study.audio.status)
                    .small()
                    .color(muted()),
            );
        }
    }
    pub(in crate::app) fn audio_active(&self) -> bool {
        self.study.audio.busy || self.study.audio.installing
    }
    fn speak(&mut self, text: &str) {
        if self.study.audio.busy || self.study.audio.installing {
            return;
        }
        if let Err(e) = crate::speech::validate(text, self.study.audio.speed) {
            self.study.audio.status = e;
            return;
        }
        let root = crate::speech::runtime_root(&self.data_dir);
        if !crate::speech::ready(&root) {
            self.study.audio.settings = true;
            self.study.audio.status = "首次使用需要下载语音模型".into();
            return;
        }
        self.study.audio.busy = true;
        self.study.audio.status = "正在生成 / 朗读…".into();
        self.study.audio.cancel = Arc::default();
        let cancel = self.study.audio.cancel.clone();
        let speed = self.study.audio.speed;
        let voice = self.study.audio.voice;
        let text = text.to_string();
        let sender = self.sender.clone();
        let engine = self.study.audio.engine.clone();
        thread::spawn(move || {
            let result = engine
                .synthesize(&root, &text, speed, voice, &cancel)
                .and_then(|path| crate::speech::play(&path, &cancel));
            let _ = sender.send(BackgroundMessage::Audio(AudioMessage::Spoken(result)));
        });
    }
    fn start_speech_model(&mut self) {
        self.study.audio.busy = true;
        self.study.audio.status = "正在加载 Kokoro 语音模型…".into();
        self.study.audio.cancel = Arc::default();
        let cancel = self.study.audio.cancel.clone();
        let engine = self.study.audio.engine.clone();
        let root = crate::speech::runtime_root(&self.data_dir);
        let voice = self.study.audio.voice;
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = engine.warmup(&root, voice, &cancel);
            let _ = sender.send(BackgroundMessage::Audio(AudioMessage::Started(result)));
        });
    }
    pub(super) fn install_learning_runtime(&mut self, component: &str) {
        if self.study.audio.installing || self.study.audio.busy {
            return;
        }
        self.study.audio.installing = true;
        self.study.audio.engine.stop();
        self.study.audio.status = if component == "Compiler" {
            "正在安装轻量 C 编译器…"
        } else {
            "正在下载语音模型，首次需要几分钟…"
        }
        .into();
        self.study.audio.cancel = Arc::default();
        let cancel = self.study.audio.cancel.clone();
        let root = crate::speech::runtime_root(&self.data_dir);
        let component = component.to_string();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
                let script = dir.path().join("install.ps1");
                fs::write(
                    &script,
                    include_str!("../../../scripts/install-learning-runtime.ps1"),
                )
                .map_err(|e| e.to_string())?;
                let (ok, out, err) = crate::runner::run(
                    std::process::Command::new("powershell.exe")
                        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                        .arg(script)
                        .arg("-Root")
                        .arg(root)
                        .arg("-Component")
                        .arg(component),
                    dir.path(),
                    "",
                    Duration::from_secs(1800),
                    &cancel,
                )?;
                if ok {
                    Ok(())
                } else {
                    Err(format!("安装未完成，可重试。{out}\n{err}"))
                }
            })();
            let _ = sender.send(BackgroundMessage::Audio(AudioMessage::Installed(result)));
        });
    }
    pub(in crate::app) fn accept_audio(&mut self, message: AudioMessage) {
        let result = match message {
            AudioMessage::Started(result) => {
                self.study.audio.busy = false;
                result.map(|_| "语音模型已加载，连续朗读将复用模型")
            }
            AudioMessage::Spoken(result) => {
                self.study.audio.busy = false;
                result.map(|_| "朗读完成")
            }
            AudioMessage::Installed(result) => {
                self.study.audio.installing = false;
                result.map(|_| "安装完成，可以开始练习")
            }
        };
        self.study.audio.status = match result {
            Ok(status) => status.to_string(),
            Err(error) => error,
        };
    }
    pub(super) fn render_speech_controls(
        &mut self,
        ui: &mut egui::Ui,
        s: &mut Session,
        card: &Card,
    ) {
        let hidden = card.topic == "听写" && s.result.is_none();
        if card.id.starts_with("english:grammar:") {
            if s.speech_for != card.id {
                s.speech_for = card.id.clone();
                s.speech_text = english_lines(&card.prompt);
            }
            ui.collapsing("句子朗读 / 跟读", |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut s.speech_text)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                if ui
                    .add_enabled(!self.study.audio.busy, egui::Button::new("朗读句子"))
                    .clicked()
                {
                    self.speak(&s.speech_text);
                }
                if self.study.audio.busy && ui.button("停止朗读").clicked() {
                    self.study.audio.cancel.store(true, Ordering::Relaxed);
                }
                if ui.small_button("语音设置").clicked() {
                    self.study.audio.settings = true;
                }
                ui.label(&self.study.audio.status);
            });
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if !card.id.starts_with("english:grammar:")
                && ui
                    .add_enabled(
                        !self.study.audio.busy,
                        egui::Button::new(if hidden {
                            "播放单词"
                        } else {
                            "朗读单词"
                        }),
                    )
                    .clicked()
            {
                self.speak(&card.title);
            }
            ui.selectable_value(&mut self.study.audio.speed, 1., "正常");
            ui.selectable_value(&mut self.study.audio.speed, 0.75, "慢速");
            egui::ComboBox::from_id_salt(("speech-voice", &card.id))
                .selected_text(if self.study.audio.voice == 2 {
                    "英音"
                } else {
                    "美音"
                })
                .width(60.)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.study.audio.voice, 0, "美音");
                    ui.selectable_value(&mut self.study.audio.voice, 2, "英音");
                });
            if self.study.audio.busy && ui.button("停止朗读").clicked() {
                self.study.audio.cancel.store(true, Ordering::Relaxed);
            }
            if ui.small_button("语音设置").clicked() {
                self.study.audio.settings = true;
            }
        });
        if !self.study.audio.status.is_empty() {
            ui.label(
                RichText::new(&self.study.audio.status)
                    .small()
                    .color(muted()),
            );
        }
        if !hidden {
            if s.speech_for != card.id {
                s.speech_for = card.id.clone();
                s.speech_text = english_lines(&card.prompt);
            }
            ui.collapsing("句子朗读 / 跟读", |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut s.speech_text)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .hint_text("输入或粘贴英文句子，也可以朗读自己的英文作答"),
                );
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            !self.study.audio.busy && !s.speech_text.trim().is_empty(),
                            egui::Button::new("朗读句子"),
                        )
                        .clicked()
                    {
                        self.speak(&s.speech_text);
                    }
                    if ui
                        .add_enabled(
                            !self.study.audio.busy && !s.answer.trim().is_empty(),
                            egui::Button::new("朗读我的作答"),
                        )
                        .clicked()
                    {
                        self.speak(&s.answer);
                    }
                });
            });
        }
    }
    pub(super) fn render_audio_settings(&mut self, ctx: &egui::Context) {
        if !self.study.audio.settings {
            return;
        }
        let root = crate::speech::runtime_root(&self.data_dir);
        let result = centered_dialog(
            ctx,
            "speech-settings",
            "离线语音与编码环境",
            "",
            520.,
            |ui| {
                ui.label("Kokoro · 英文单词与句子朗读");
                ui.label("首次下载约 180 MB，之后离线使用。首次朗读加载模型，连续朗读复用；也可提前启动。重复内容直接播放缓存。");
                ui.label(
                    "朗读模型：Kokoro v1.1；讲解与批改：Spark2.6 Q4。可在“模型管理”切换启动对象。",
                );
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(
                        !self.study.audio.busy && !self.study.audio.installing,
                        |ui| {
                            ui.selectable_value(&mut self.study.audio.voice, 0, "美音");
                            ui.selectable_value(&mut self.study.audio.voice, 2, "英音");
                        },
                    );
                    ui.label(if self.study.audio.engine.is_loaded() {
                        "语音模型已加载"
                    } else {
                        "语音模型未加载"
                    });
                });
                ui.horizontal(|ui| {
                    let available = !self.study.audio.busy && !self.study.audio.installing;
                    if ui
                        .add_enabled(
                            available && crate::speech::ready(&root),
                            primary_button("启动语音模型"),
                        )
                        .clicked()
                    {
                        self.start_speech_model();
                    }
                    if ui
                        .add_enabled(
                            available && self.study.audio.engine.is_loaded(),
                            egui::Button::new("停止语音模型"),
                        )
                        .clicked()
                    {
                        self.study.audio.engine.stop();
                        self.study.audio.status = "语音模型已停止，内存已释放".into();
                    }
                    if self.study.audio.busy && ui.button("取消 / 停止朗读").clicked() {
                        self.study.audio.cancel.store(true, Ordering::Relaxed);
                    }
                });
                ui.label(if crate::speech::ready(&root) {
                    "语音模型：已就绪"
                } else {
                    "语音模型：尚未安装"
                });
                if ui
                    .add_enabled(
                        !self.study.audio.installing && !self.study.audio.busy,
                        primary_button("下载 / 修复语音模型"),
                    )
                    .clicked()
                {
                    self.install_learning_runtime("Speech");
                }
                ui.separator();
                ui.label("TinyCC · 轻量 C 编译器（约 0.5 MB）");
                if ui
                    .add_enabled(
                        !self.study.audio.installing && !self.study.audio.busy,
                        egui::Button::new("安装 / 修复 C 编译器"),
                    )
                    .clicked()
                {
                    self.install_learning_runtime("Compiler");
                }
                ui.label("Java 编码题使用已安装的 JDK 17+。");
                if self.study.audio.installing && ui.button("取消下载").clicked() {
                    self.study.audio.cancel.store(true, Ordering::Relaxed);
                }
                ui.label(&self.study.audio.status);
                ui.hyperlink_to(
                    "语音模型与许可证",
                    "https://huggingface.co/hexgrad/Kokoro-82M-v1.1-zh",
                );
            },
        );
        if dialog_closed(&result) {
            self.study.audio.settings = false;
        }
    }
}
fn english_lines(text: &str) -> String {
    text.lines()
        .filter(|line| {
            line.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 5
                && !line.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
