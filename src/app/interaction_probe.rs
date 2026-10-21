//! Development-only integration checks through the normal eframe event loop.
use super::*;
pub(super) struct Probe {
    output: PathBuf,
    phase: u8,
    since: Instant,
    frames: usize,
    local_url: String,
}
impl PersonalDeskApp {
    pub fn start_interaction_probe(&mut self, output: PathBuf) {
        fs::create_dir_all(&output).unwrap();
        self.open_terminal(self.data_dir.clone());
        self.interaction_probe = Some(Probe {
            output,
            phase: 0,
            since: Instant::now(),
            frames: 0,
            local_url: super::preview::start_browser_fixture(),
        });
    }
    pub(super) fn probe_input_events(&mut self, input: &mut egui::RawInput) {
        let Some(probe) = self.interaction_probe.as_mut() else {
            return;
        };
        if matches!(probe.phase, 0 | 1 | 3 | 4) {
            // This isolated probe is launched hidden. Simulate the host focus event
            // together with keyboard input; it does not automate physical input.
            input.focused = true;
        }
        if matches!(probe.phase, 0 | 3)
            && !self.terminal_starting
            && self.terminal.is_some()
            && input.focused
            && self.terminal_focused
            && probe.since.elapsed() > Duration::from_secs(2)
            && probe.frames > 5
        {
            let _ = fs::write(
                probe
                    .output
                    .join(format!("input-focus-{}.txt", probe.phase)),
                format!(
                    "window_focused={} canvas_focused={}",
                    input.focused, self.terminal_focused
                ),
            );
            let expression = if probe.phase == 0 {
                "[Console]::WriteLine(('FIRST_UI_'+(40+2)))"
            } else {
                "[Console]::WriteLine(('RETURN_UI_'+(40+2)))"
            };
            input.events.push(egui::Event::Text(expression.into()));
            input.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
            probe.phase += 1;
            probe.since = Instant::now();
        }
    }
    pub(super) fn advance_interaction_probe(&mut self, ctx: &egui::Context) {
        let Some(mut probe) = self.interaction_probe.take() else {
            return;
        };
        probe.frames += 1;
        ctx.request_repaint_after(Duration::from_millis(40));
        let _ = fs::write(probe.output.join("phase.txt"), probe.phase.to_string());
        match probe.phase {
            1 | 4 => {
                let marker = if probe.phase == 1 {
                    "FIRST_UI_42"
                } else {
                    "RETURN_UI_42"
                };
                let output = self
                    .terminal
                    .as_ref()
                    .map(|t| t.parser.screen().contents())
                    .unwrap_or_default();
                if output.contains(marker) {
                    let _ = fs::write(
                        probe.output.join(format!("terminal-{}.txt", probe.phase)),
                        output,
                    );
                    self.section = Section::Browser;
                    self.browser.open(if probe.phase == 1 {
                        &probe.local_url
                    } else {
                        "https://chat.deepseek.com/"
                    });
                    probe.phase += 1;
                    probe.since = Instant::now();
                    probe.frames = 0;
                } else if probe.since.elapsed() > Duration::from_secs(12) {
                    let _ = fs::write(
                        probe.output.join("FAILED.txt"),
                        format!(
                            "phase={} terminal={} error={}",
                            probe.phase, output, self.terminal_error
                        ),
                    );
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
            }
            2 => {
                let file = probe.output.join("local-browser.json");
                if !self.browser.loading() && probe.frames > 15 {
                    self.browser.probe_input(file.clone());
                }
                if file.exists() {
                    self.section = Section::Terminal;
                    self.terminal_focus_requested = true;
                    probe.phase = 3;
                    probe.since = Instant::now();
                    probe.frames = 0;
                }
            }
            5 | 6 => {
                let label = if probe.phase == 5 {
                    "deepseek"
                } else {
                    "chatgpt"
                };
                let page_file = probe.output.join(format!("{label}-page.json"));
                if !self.browser.loading() && probe.frames.is_multiple_of(15) {
                    self.browser.probe_input(page_file.clone());
                }
                let ready = fs::read(&page_file)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                    .is_some_and(|page| {
                        page["bodyLength"].as_u64().unwrap_or(0) > 0
                            && page["url"]
                                .as_str()
                                .is_some_and(|url| url.starts_with("https://"))
                    });
                if (!self.browser.loading() && probe.frames > 30 && ready)
                    || probe.since.elapsed() > Duration::from_secs(25)
                {
                    let _=fs::write(probe.output.join(format!("{label}-status.json")),serde_json::to_vec_pretty(&serde_json::json!({"loading":self.browser.loading(),"error":self.browser.page_error(),"engine_error":self.browser.error})).unwrap());
                    if !self.browser.loading() {
                        self.browser
                            .probe_input(probe.output.join(format!("{label}-page.json")));
                    }
                    probe.phase += 1;
                    probe.since = Instant::now();
                    probe.frames = 0;
                    if probe.phase == 6 {
                        self.browser.open("https://chatgpt.com/");
                    }
                }
            }
            7 if probe.since.elapsed() > Duration::from_secs(2) => {
                self.browser.selected = 0;
                self.browser.input = self.browser.tabs[0].address.clone();
                probe.phase = 8;
                probe.frames = 0;
                probe.since = Instant::now();
            }
            8 => {
                let file = probe.output.join("return-local-browser.json");
                if probe.frames.is_multiple_of(10) {
                    self.browser.probe_input(file.clone());
                }
                let focused = fs::read(&file)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                    .is_some_and(|page| page["focused"] == true && page["active"] == true);
                if focused {
                    probe.phase = 9;
                    probe.since = Instant::now();
                }
            }
            9 if probe.since.elapsed() > Duration::from_secs(2) => {
                self.browser.tabs.clear();
                self.terminal = None;
                let _ = fs::write(
                    probe.output.join("COMPLETE.txt"),
                    "normal GUI event loop completed",
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            _ => {}
        }
        if probe.since.elapsed() > Duration::from_secs(35) {
            let _ = fs::write(
                probe.output.join("FAILED.txt"),
                format!("phase={}", probe.phase),
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.interaction_probe = Some(probe);
    }
}
