use super::*;

/// Native controls with one shared visual rhythm; no image or web dependencies.
pub fn segmented<T: Copy + PartialEq>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) {
    if options.is_empty() {
        return;
    }
    let width = options.len() as f32 * 63.0 + 3.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::hover());
    let mut selected = options
        .iter()
        .position(|(option, _)| *option == *value)
        .unwrap_or(0);
    let mut buttons = Vec::with_capacity(options.len());
    for (index, &(option, label)) in options.iter().enumerate() {
        let button = egui::Rect::from_min_size(
            rect.min + egui::vec2(3.0 + index as f32 * 63.0, 3.0),
            egui::vec2(60.0, 28.0),
        );
        let response = ui.interact(button, response.id.with(index), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Tab, ui.is_enabled(), *value == option, label)
        });
        if response.clicked() {
            *value = option;
            selected = index;
        }
        buttons.push((
            button,
            response.on_hover_cursor(egui::CursorIcon::PointingHand),
            label,
        ));
    }
    let offset = if crate::theme::animations() {
        ui.ctx().animate_value_with_time(
            response.id.with("selection-offset"),
            selected as f32 * 63.0,
            0.18,
        )
    } else {
        selected as f32 * 63.0
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 10, crate::theme::input());
    let pill = egui::Rect::from_min_size(
        rect.min + egui::vec2(3.0 + offset, 3.0),
        egui::vec2(60.0, 28.0),
    );
    painter.add(
        egui::epaint::Shadow {
            offset: [0, 1],
            blur: 5,
            spread: 0,
            color: Color32::from_black_alpha(12),
        }
        .as_shape(pill, egui::CornerRadius::same(8)),
    );
    painter.rect_filled(pill, 8, surface());
    for (index, (rect, response, label)) in buttons.iter().enumerate() {
        let hover = motion::hover(ui, response);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(13.0),
            if index == selected {
                accent()
            } else {
                muted().lerp_to_gamma(ink(), hover)
            },
        );
    }
}
