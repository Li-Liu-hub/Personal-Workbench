//! Shared geometry for editable table headers and rows.
use super::*;
pub const ROW_HEIGHT: f32 = 62.;

/// Reserve all row space, but create widgets only inside the scroll viewport.
/// Callers must use stable record IDs for interactive cells.
pub fn virtual_rows(
    ui: &mut Ui,
    count: usize,
    render: impl FnOnce(&mut Ui, std::ops::Range<usize>),
) {
    let origin = ui.cursor().min.y;
    let start = (((ui.clip_rect().top() - origin) / ROW_HEIGHT)
        .floor()
        .max(0.) as usize)
        .saturating_sub(1)
        .min(count);
    let end = (((ui.clip_rect().bottom() - origin) / ROW_HEIGHT)
        .ceil()
        .max(0.) as usize
        + 1)
    .min(count)
    .max(start);
    if start > 0 {
        ui.allocate_space(egui::vec2(0., start as f32 * ROW_HEIGHT));
    }
    render(ui, start..end);
    if end < count {
        ui.allocate_space(egui::vec2(0., (count - end) as f32 * ROW_HEIGHT));
    }
}

pub fn add_row(ui: &mut Ui, columns: &[f32], id: &str, label: &str) -> Response {
    let (_, rect) = ui.allocate_space(egui::vec2(width(columns), 40.0));
    let response = ui.interact(rect, ui.scope_id().with(id), egui::Sense::click());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(
        rect,
        6,
        if response.hovered() {
            surface().lerp_to_gamma(accent(), 0.06)
        } else {
            surface()
        },
    );
    painter.text(
        rect.left_center() + egui::vec2(26.0, 0.0),
        egui::Align2::LEFT_CENTER,
        format!("+ {label}"),
        egui::FontId::proportional(14.0),
        accent(),
    );
    response
}

pub fn width(columns: &[f32]) -> f32 {
    columns.iter().sum::<f32>() + 32.0
}
pub fn fit(columns: &[f32], available: f32) -> Vec<f32> {
    let scale = ((available - 32.).max(1.) / columns.iter().sum::<f32>()).max(0.85);
    columns.iter().map(|value| value * scale).collect()
}

pub fn cells(rect: egui::Rect, columns: &[f32]) -> Vec<egui::Rect> {
    let mut x = rect.left() + 26.0;
    columns
        .iter()
        .map(|width| {
            let cell = egui::Rect::from_min_size(
                egui::pos2(x, rect.top() + 8.0),
                egui::vec2(width - 10.0, rect.height() - 16.0),
            );
            x += width;
            cell
        })
        .collect()
}

pub fn header(ui: &mut Ui, columns: &[f32], labels: &[&str]) {
    let (_, rect) = ui.allocate_space(egui::vec2(width(columns), 36.0));
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        egui::Stroke::new(0.7, border()),
    );
    for (cell, label) in cells(rect, columns).iter().zip(labels) {
        ui.painter().text(
            cell.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.0),
            muted(),
        );
    }
}

pub fn row(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    columns: &[f32],
) -> (Response, Vec<egui::Rect>) {
    let (_, allocated) = ui.allocate_space(egui::vec2(width(columns), ROW_HEIGHT));
    let rect = allocated.shrink2(egui::vec2(0., 3.));
    let response = ui.interact(rect, ui.scope_id().with(id), egui::Sense::click());
    motion::card(ui, &response, 11, false, false);
    (response, cells(rect, columns))
}

pub fn text(
    ui: &mut Ui,
    rect: egui::Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut String,
) -> bool {
    let field = egui::Id::unique(id);
    let focused = ui.ctx().memory(|m| m.has_focus(field));
    if focused || ui.rect_contains_pointer(rect) {
        ui.painter().rect_filled(rect, 5, crate::theme::input());
    }
    if focused {
        ui.painter().rect_stroke(
            rect,
            5,
            egui::Stroke::new(1., accent()),
            egui::StrokeKind::Inside,
        );
    }
    let response = ui.place(
        rect,
        egui::TextEdit::singleline(value)
            .id(field)
            .frame(egui::Frame::NONE)
            .margin(0)
            .align(egui::Align2::CENTER_CENTER)
            .min_size(rect.size())
            .font(egui::FontId::proportional(14.))
            .desired_width(rect.width()),
    );
    // Decide after TextEdit handles input, so the placeholder disappears on
    // the very frame that mouse or keyboard focus enters this empty cell.
    if value.is_empty() && !ui.memory(|memory| memory.has_focus(field)) {
        ui.painter()
            .with_clip_rect(rect.intersect(ui.clip_rect()))
            .text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "—",
                egui::FontId::proportional(14.),
                muted(),
            );
    }
    super::select_on_focus(&response, value);
    response.on_hover_text(value.as_str()).changed()
}

/// Keep one or two row actions centered beneath their shared column heading.
pub fn actions(ui: &mut Ui, id: &str, rect: egui::Rect, editing: bool) -> (bool, bool, bool) {
    let labels: &[&str] = if editing {
        &["保存", "取消"]
    } else {
        &["详情"]
    };
    let button_size = egui::vec2(42., 26.);
    let gap = 6.;
    let width = labels.len() as f32 * button_size.x + (labels.len() - 1) as f32 * gap;
    let left = rect.center().x - width / 2.;
    let mut clicked = [false; 2];
    for (index, label) in labels.iter().enumerate() {
        let area = egui::Rect::from_center_size(
            egui::pos2(
                left + index as f32 * (button_size.x + gap) + button_size.x / 2.,
                rect.center().y,
            ),
            button_size,
        );
        clicked[index] = quick_action(ui, ("table-action", id, label), area, label).clicked();
    }
    (
        editing && clicked[0],
        editing && clicked[1],
        !editing && clicked[0],
    )
}

pub fn cell_ui(ui: &mut Ui, rect: egui::Rect, content: impl FnOnce(&mut Ui)) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    content(&mut child);
}
pub fn date_hint(ui: &mut Ui, rect: egui::Rect, value: &str) {
    if let Err(error) = crate::model::validate_date(value) {
        ui.painter().rect_stroke(
            rect,
            5,
            egui::Stroke::new(1., egui::Color32::from_rgb(195, 73, 72)),
            egui::StrokeKind::Inside,
        );
        ui.interact(
            rect,
            ui.scope_id()
                .with(("date-error", rect.left().to_bits(), rect.top().to_bits())),
            egui::Sense::hover(),
        )
        .on_hover_text(error.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_placeholder_hides_on_focus_without_editing_and_returns_only_after_blur() {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::pos2(30., 30.), egui::vec2(180., 40.));
        let render = |value: &mut String, events| {
            let mut changed = false;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 200.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    changed = text(ui, rect, "empty-cell", value);
                },
            );
            output.textures_delta.clear();
            let placeholder = output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "—"));
            (changed, placeholder)
        };
        let pointer = |position, pressed| {
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let mut value = String::new();
        render(&mut value, vec![]);
        assert_eq!(render(&mut value, vec![]), (false, true));
        // The mouse-down frame must already be clean, even before typing.
        assert_eq!(
            render(&mut value, pointer(rect.center(), true)),
            (false, false)
        );
        assert!(value.is_empty());
        assert_eq!(
            render(&mut value, pointer(rect.center(), false)),
            (false, false)
        );
        assert_eq!(
            render(&mut value, vec![egui::Event::Text("中文输入".into())]),
            (true, false)
        );
        assert_eq!(value, "中文输入");
        // Deleting all text while editing must not bring the dash back.
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        assert_eq!(
            render(
                &mut value,
                vec![
                    key(
                        egui::Key::A,
                        egui::Modifiers {
                            ctrl: true,
                            command: true,
                            ..Default::default()
                        }
                    ),
                    key(egui::Key::Backspace, egui::Modifiers::NONE)
                ]
            ),
            (true, false)
        );
        assert!(value.is_empty());
        assert_eq!(
            render(&mut value, pointer(egui::pos2(300., 150.), true)),
            (false, false)
        );
        assert_eq!(
            render(&mut value, pointer(egui::pos2(300., 150.), false)),
            (false, true)
        );
        render(&mut value, vec![key(egui::Key::Tab, egui::Modifiers::NONE)]);
        assert_eq!(render(&mut value, vec![]), (false, false));
        assert!(value.is_empty());
        let output = ctx.run_ui(
            egui::RawInput {
                focused: false,
                ..Default::default()
            },
            |ui| {
                text(ui, rect, "empty-cell", &mut value);
            },
        );
        assert!(
            !output.shapes.iter().any(
                |shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "—")
            ),
            "temporarily switching away from the window must not restore the dash in the active cell"
        );
    }

    #[test]
    fn entering_a_cell_selects_without_mutating_and_typing_replaces_the_value() {
        for original in ["1", "7799.00", "原来的中文姓名"] {
            let ctx = egui::Context::default();
            let mut value = original.to_string();
            let rect = egui::Rect::from_min_size(egui::pos2(30., 30.), egui::vec2(180., 40.));
            let mut frame = |events| {
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        events,
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400., 200.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        text(ui, rect, "cell", &mut value);
                    },
                );
                out.textures_delta.clear();
            };
            frame(vec![]);
            for pressed in [true, false] {
                frame(vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            let state = egui::TextEdit::load_state(&ctx, egui::Id::unique("cell")).unwrap();
            assert!(!state.cursor.char_range().unwrap().is_empty());
            frame(vec![egui::Event::Text("12".into())]);
            assert_eq!(value, "12", "first input must replace {original}");
        }
    }
}
