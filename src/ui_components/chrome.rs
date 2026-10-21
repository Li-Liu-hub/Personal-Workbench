use super::*;

/// Native controls with one shared visual rhythm; no image or web dependencies.
pub fn segmented<T: Copy + PartialEq>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) {
    let width = options.len() as f32 * 60. + options.len().saturating_sub(1) as f32 * 3. + 6.;
    ui.allocate_ui_with_layout(
        egui::vec2(width, 34.),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            egui::Frame::new()
                .fill(crate::theme::input())
                .corner_radius(9)
                .inner_margin(3)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 3.;
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        for &(option, label) in options {
                            let active = *value == option;
                            let response = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(label).size(13.).color(if active {
                                        ink()
                                    } else {
                                        muted()
                                    }),
                                )
                                .min_size(egui::vec2(60., 28.))
                                .corner_radius(7)
                                .fill(if active {
                                    surface()
                                } else {
                                    Color32::TRANSPARENT
                                })
                                .stroke(if active {
                                    egui::Stroke::new(1., border().gamma_multiply(0.7))
                                } else {
                                    egui::Stroke::NONE
                                }),
                            );
                            if response.clicked() {
                                *value = option;
                            }
                        }
                    });
                });
        },
    );
}

pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(surface().lerp_to_gamma(color, 0.07))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(12.).color(color));
        });
}

pub fn paper() -> egui::Frame {
    egui::Frame::new()
        .fill(surface())
        .stroke(egui::Stroke::new(1., border().gamma_multiply(0.65)))
        .corner_radius(16)
        .inner_margin(28)
}

pub fn empty_state(ui: &mut Ui, title: &str, detail: &str, kind: TileKind) {
    ui.vertical_centered(|ui| {
        ui.add_space(38.);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(56., 56.), egui::Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 28., surface().lerp_to_gamma(accent(), 0.06));
        paint_icon(ui.painter(), rect.shrink(15.), kind, accent());
        ui.add_space(14.);
        ui.label(egui::RichText::new(title).size(18.).strong());
        ui.add_space(5.);
        ui.label(egui::RichText::new(detail).size(13.).color(muted()));
    });
}
