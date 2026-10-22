//! Shared geometry for editable table headers and rows.
use super::*;

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
    let (_, allocated) = ui.allocate_space(egui::vec2(width(columns), 62.0));
    let rect = allocated.shrink2(egui::vec2(0., 3.));
    let response = ui.interact(rect, ui.scope_id().with(id), egui::Sense::click());
    let painter = ui.painter().with_clip_rect(ui.clip_rect());
    painter.rect_filled(
        rect,
        10,
        if response.hovered() {
            surface().lerp_to_gamma(accent(), 0.025)
        } else {
            surface()
        },
    );
    painter.rect_stroke(
        rect,
        10,
        egui::Stroke::new(0.6, border().gamma_multiply(0.6)),
        egui::StrokeKind::Inside,
    );
    if response.hovered() {
        painter.rect_stroke(
            rect,
            9,
            egui::Stroke::new(1.0, border()),
            egui::StrokeKind::Inside,
        );
    }
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
            .hint_text("—")
            .desired_width(rect.width()),
    );
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
