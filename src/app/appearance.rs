use super::*;

pub(super) fn configure_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    ctx.style_mut_of(egui::Theme::Light, |style| {
        style.visuals = egui::Visuals::light();
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.0));
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.animation_time = 0.12;
        style.spacing.interact_size.y = 32.0;
        style.visuals.panel_fill = canvas();
        style.visuals.window_fill = surface();
        style.visuals.window_corner_radius = egui::CornerRadius::same(14);
        style.visuals.window_stroke = egui::Stroke::new(1.0, border());
        style.visuals.menu_corner_radius = egui::CornerRadius::same(10);
        style.visuals.extreme_bg_color = canvas();
        style.visuals.text_edit_bg_color = Some(Color32::from_rgb(249, 250, 253));
        style.visuals.faint_bg_color = canvas();
        style.visuals.weak_text_color = Some(muted());
        style.visuals.hyperlink_color = accent();
        style.visuals.selection.bg_fill = Color32::from_rgb(210, 230, 255);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, accent());
        style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, ink());
        style.visuals.widgets.noninteractive.bg_fill = surface();
        style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, border());
        style.visuals.widgets.inactive.bg_fill = surface();
        style.visuals.widgets.inactive.weak_bg_fill = surface();
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, ink());
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border());
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(243, 244, 247);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(243, 244, 247);
        style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, accent());
        style.visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0, border().lerp_to_gamma(accent(), 0.3));
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(225, 235, 250);
        style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(225, 235, 250);
        style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, accent());
        style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, accent());
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(9);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(9);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(9);
    });
    #[cfg(windows)]
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\msyh.ttc") {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "chinese".into(),
            egui::FontData::from_owned(bytes)
                .tweak(egui::FontTweak {
                    hinting: Some(true),
                    hinting_target: egui::epaint::text::HintingTarget::Smooth(
                        egui::epaint::text::SmoothHinting {
                            light: false,
                            symmetric_rendering: false,
                            preserve_linear_metrics: false,
                        },
                    ),
                    subpixel_binning: Some(false),
                    ..Default::default()
                })
                .into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "chinese".into());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("chinese".into());
        if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
            fonts
                .font_data
                .insert("system-ui".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "system-ui".into());
        }
        ctx.set_fonts(fonts);
    }
}
