use super::*;
impl PersonalDeskApp {
    pub(super) fn open_terminal(&mut self, directory: PathBuf) {
        if self.terminal.is_none() {
            match crate::terminal::Terminal::start(&directory) {
                Ok(terminal) => {
                    self.terminal = Some(terminal);
                    self.terminal_directory = directory;
                    self.terminal_error.clear();
                }
                Err(error) => self.terminal_error = error,
            }
        }
        self.section = Section::Terminal;
    }
    pub(super) fn render_terminal(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("PowerShell");
            ui.label(
                RichText::new(self.terminal_directory.display().to_string())
                    .small()
                    .color(muted()),
            );
        });
        let mut close = false;
        ui.horizontal(|ui| {
            if self.terminal.is_none() && ui.button("启动 PowerShell").clicked() {
                self.open_terminal(self.terminal_directory.clone());
            }
            if let Some(terminal) = &mut self.terminal {
                terminal.poll();
                if ui.button("复制输出").clicked() {
                    ui.ctx().copy_text(terminal.parser.screen().contents());
                }
                if ui.button("中断 Ctrl+C").clicked() {
                    let _ = terminal.write(&[3]);
                }
                close = ui.button("关闭终端").clicked();
            }
        });
        if close {
            self.terminal = None;
            self.terminal_focused = false;
            self.terminal_inline = false;
        }
        if !self.terminal_error.is_empty() {
            ui.colored_label(Color32::from_rgb(185, 65, 71), &self.terminal_error);
        }
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size().max(egui::vec2(100.0, 100.0)),
            egui::Sense::click(),
        );
        ui.painter()
            .rect_filled(rect, 12, Color32::from_rgb(21, 25, 36));
        if response.clicked() {
            response.request_focus();
        }
        self.terminal_focused = response.has_focus();
        let Some(terminal) = &mut self.terminal else {
            return;
        };
        let font = egui::FontId::monospace(14.0);
        let cell = ui
            .fonts_mut(|fonts| fonts.layout_no_wrap("M".into(), font.clone(), Color32::WHITE))
            .size()
            .x;
        let row_height = 20.0;
        let area = rect.shrink(16.0);
        let rows = (area.height() / row_height).floor().clamp(5.0, 200.0) as u16;
        let cols = (area.width() / cell).floor().clamp(20.0, 300.0) as u16;
        terminal.resize(rows, cols);
        terminal.poll();
        if response.hovered() {
            let delta = ui.input(|input| input.smooth_scroll_delta.y);
            if delta.abs() > 0.1 {
                let current = terminal.parser.screen().scrollback() as i64;
                terminal
                    .parser
                    .screen_mut()
                    .set_scrollback((current + (delta / 10.0) as i64).max(0) as usize);
            }
        }
        let painter = ui.painter().with_clip_rect(area);
        for (index, line) in terminal.parser.screen().rows(0, cols).enumerate() {
            painter.text(
                area.min + egui::vec2(0.0, index as f32 * row_height),
                egui::Align2::LEFT_TOP,
                line,
                font.clone(),
                Color32::from_rgb(224, 231, 244),
            );
        }
        if self.terminal_focused {
            let (row, col) = terminal.parser.screen().cursor_position();
            let cursor = egui::Rect::from_min_size(
                area.min + egui::vec2(col as f32 * cell, row as f32 * row_height),
                egui::vec2(cell, row_height),
            );
            painter.rect_stroke(
                cursor,
                0,
                egui::Stroke::new(1.0, Color32::from_rgb(171, 161, 255)),
                egui::StrokeKind::Inside,
            );
            let events = ui.input_mut(|input| std::mem::take(&mut input.events));
            for event in events {
                let bytes = match event {
                    egui::Event::Text(value) | egui::Event::Paste(value) => value.into_bytes(),
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        if modifiers.ctrl || modifiers.command {
                            match key {
                                egui::Key::C => vec![3],
                                egui::Key::D => vec![4],
                                egui::Key::Z => vec![26],
                                egui::Key::L => vec![12],
                                egui::Key::A => vec![1],
                                egui::Key::E => vec![5],
                                egui::Key::U => vec![21],
                                _ => vec![],
                            }
                        } else {
                            match key {
                                egui::Key::Enter => b"\r".to_vec(),
                                egui::Key::Backspace => vec![127],
                                egui::Key::Tab => b"\t".to_vec(),
                                egui::Key::Escape => vec![27],
                                egui::Key::ArrowUp => b"\x1b[A".to_vec(),
                                egui::Key::ArrowDown => b"\x1b[B".to_vec(),
                                egui::Key::ArrowRight => b"\x1b[C".to_vec(),
                                egui::Key::ArrowLeft => b"\x1b[D".to_vec(),
                                egui::Key::Home => b"\x1b[H".to_vec(),
                                egui::Key::End => b"\x1b[F".to_vec(),
                                egui::Key::Delete => b"\x1b[3~".to_vec(),
                                _ => vec![],
                            }
                        }
                    }
                    _ => vec![],
                };
                if !bytes.is_empty()
                    && let Err(error) = terminal.write(&bytes)
                {
                    self.terminal_error = error;
                }
            }
        }
        ui.ctx().request_repaint_after(Duration::from_millis(40));
    }
}
