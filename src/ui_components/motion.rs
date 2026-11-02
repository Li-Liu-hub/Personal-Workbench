//! Short, event-driven transitions. Geometry never moves beneath the pointer.
use super::*;

pub fn amount(ui: &Ui, id: egui::Id, active: bool) -> f32 {
    if !crate::theme::animations() {
        return if active { 1.0 } else { 0.0 };
    }
    ui.ctx()
        .animate_bool_with_time_and_easing(id, active, 0.16, |t| 1.0 - (1.0 - t).powi(3))
}

pub fn hover(ui: &Ui, response: &Response) -> f32 {
    amount(
        ui,
        response.id.with("surface-hover"),
        response.enabled() && (response.contains_pointer() || response.has_focus()),
    )
}

pub fn card(ui: &Ui, response: &Response, radius: u8, selected: bool, raised: bool) {
    if !ui.is_rect_visible(response.rect) {
        return;
    }
    let hover = hover(ui, response);
    let selected = amount(ui, response.id.with("surface-selected"), selected);
    let painter = ui.painter();
    if raised {
        painter.add(
            egui::epaint::Shadow {
                offset: [0, 2],
                blur: (8.0 + hover * 8.0) as u8,
                spread: 0,
                color: Color32::from_black_alpha((5.0 + hover * 9.0) as u8),
            }
            .as_shape(response.rect, egui::CornerRadius::same(radius)),
        );
    }
    painter.rect_filled(
        response.rect,
        radius,
        surface().lerp_to_gamma(accent(), 0.025 * hover + 0.04 * selected),
    );
    painter.rect_stroke(
        response.rect,
        radius,
        egui::Stroke::new(
            0.8,
            border().lerp_to_gamma(accent(), 0.24 * hover + 0.3 * selected),
        ),
        egui::StrokeKind::Inside,
    );
}

/// Fade only the contents; keep scroll positions, hit targets and text IDs stable.
/// No repaint is requested after the transition has settled.
pub fn page_opacity(ui: &Ui, key: impl std::hash::Hash + std::fmt::Debug) -> f32 {
    let key = egui::Id::unique(key);
    let state_id = egui::Id::unique(("page-transition", ui.ctx().viewport_id()));
    let now = ui.input(|input| input.time);
    let start = ui.data_mut(|data| {
        let previous = data.get_temp::<(egui::Id, f64)>(state_id);
        let start = match previous {
            Some((last, time)) if last == key => time,
            Some(_) => now,
            None => now - 0.18,
        };
        data.insert_temp(state_id, (key, start));
        start
    });
    if !crate::theme::animations() {
        return 1.0;
    }
    let progress = ((now - start) as f32 / 0.18).clamp(0.0, 1.0);
    if progress < 1.0 {
        ui.ctx().request_repaint();
    }
    1.0 - 0.22 * (1.0 - progress).powi(3)
}
