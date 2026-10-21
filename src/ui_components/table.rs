//! Shared geometry for editable table headers and rows.
use super::*;

pub fn width(columns: &[f32]) -> f32 {
    columns.iter().sum::<f32>() + 32.0
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
    ui.painter().rect_filled(rect, 7, crate::theme::input());
    for (cell, label) in cells(rect, columns).iter().zip(labels) {
        ui.painter().text(
            cell.left_center(),
            egui::Align2::LEFT_CENTER,
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
    let (_, rect) = ui.allocate_space(egui::vec2(width(columns), 52.0));
    let response = ui.interact(rect, ui.scope_id().with(id), egui::Sense::click());
    let painter = ui.painter().with_clip_rect(ui.clip_rect());
    painter.rect_filled(rect, 0, surface());
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        egui::Stroke::new(0.5, border()),
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
            .margin(egui::Margin::symmetric(0, 8))
            .hint_text("—")
            .desired_width(rect.width()),
    );
    response.on_hover_text(value.as_str()).changed()
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
