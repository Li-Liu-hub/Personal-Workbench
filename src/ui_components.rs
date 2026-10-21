use eframe::egui::{self, Color32, Response, Ui};

mod collection;
mod credentials;
pub mod table;
mod website;
pub use collection::EntryLayout;
pub use collection::{collection, entry, tagged_entry, view_switcher};
pub use credentials::{credential_entry, credential_height, credential_list_header};
pub use website::website_entry;

pub use crate::theme::{
    accent, border, button_text, canvas, content_fill, ink, muted, sidebar, sidebar_text, surface,
};

pub fn primary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(label).color(button_text()))
        .fill(accent())
        .stroke(egui::Stroke::NONE)
        .corner_radius(10)
        .min_size(egui::vec2(0.0, 32.0))
}

pub fn toolbar_title(ui: &mut Ui, title: &str, width: f32, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 38.0), egui::Sense::click());
    let mut job = egui::text::LayoutJob::simple(
        title.to_owned(),
        egui::FontId::proportional(size),
        ink(),
        width,
    );
    job.wrap.max_rows = 1;
    let galley = ui.painter().layout_job(job);
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(
            egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0),
            galley,
            ink(),
        );
    response.on_hover_text(title)
}

pub fn classification_dropdown(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    selected: &mut Vec<String>,
    options: &[String],
    width: f32,
) -> bool {
    let before = selected.clone();
    egui::ComboBox::from_id_salt(id)
        .width(width.max(60.0))
        .truncate()
        .selected_text(if selected.is_empty() {
            "未分类".into()
        } else {
            selected.join(" · ")
        })
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            if ui.selectable_label(selected.is_empty(), "未分类").clicked() {
                selected.clear();
            }
            for name in options {
                let mut checked = selected.contains(name);
                if ui.checkbox(&mut checked, name).changed() {
                    if checked {
                        selected.push(name.clone());
                    } else {
                        selected.retain(|tag| tag != name);
                    }
                }
            }
        });
    before != *selected
}

/// All application dialogs share the viewport center, including after a resize.
/// The backdrop blocks underlying controls; only Close or Escape dismisses a draft.
pub fn centered_dialog(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    _subtitle: &str,
    width: f32,
    content: impl FnOnce(&mut Ui),
) -> egui::ModalResponse<bool> {
    let viewport = ctx.content_rect();
    let width = width.min((viewport.width() - 96.0).max(160.0));
    let generation = ctx
        .data(|data| data.get_temp::<u64>(egui::Id::unique((id, "scroll-generation"))))
        .unwrap_or(0);
    egui::Modal::new(egui::Id::unique(id))
        .area(
            egui::Modal::default_area(egui::Id::unique(id))
                .fade_in(false)
                .constrain_to(viewport)
                .default_size(egui::vec2(width + 48.0, 0.0)),
        )
        .backdrop_color(Color32::from_rgba_unmultiplied(19, 25, 43, 115))
        .frame(
            egui::Frame::new()
                .fill(surface())
                .stroke(egui::Stroke::new(1.0, border()))
                .corner_radius(20)
                .inner_margin(24)
                .shadow(egui::epaint::Shadow {
                    offset: [0, 12],
                    blur: 40,
                    spread: 0,
                    color: Color32::from_black_alpha(40),
                }),
        )
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 12.0);
            ui.spacing_mut().text_edit_width = width;
            let mut close = false;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).size(23.0).strong().color(ink()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::click());
                    if response.hovered() {
                        ui.painter().rect_filled(rect, 8, canvas());
                    }
                    let center = rect.center();
                    for direction in [-1.0, 1.0] {
                        ui.painter().line_segment(
                            [
                                center + egui::vec2(-5.0, -5.0 * direction),
                                center + egui::vec2(5.0, 5.0 * direction),
                            ],
                            egui::Stroke::new(1.5, muted()),
                        );
                    }
                    close = response.on_hover_text("关闭 · Esc").clicked();
                });
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt((id, "body", generation))
                .max_height((viewport.height() - 200.0).max(80.0))
                // Areas remember their previous size. Without this lower bound,
                // the scroll viewport loses header height on every frame.
                .min_scrolled_height((viewport.height() - 200.0).max(80.0))
                .auto_shrink([false, true])
                .show(ui, content);
            close
        })
}

pub fn reset_dialog_scroll(ctx: &egui::Context, id: &str) {
    ctx.input_mut(|input| {
        input.smooth_scroll_delta = egui::Vec2::ZERO;
    });
    let key = egui::Id::unique((id, "scroll-generation"));
    ctx.data_mut(|data| {
        let value = data.get_temp::<u64>(key).unwrap_or(0);
        data.insert_temp(key, value + 1);
    });
}

pub fn dialog_closed(response: &egui::ModalResponse<bool>) -> bool {
    response.inner
        || (response.is_top_modal
            && !response.any_popup_open
            && response
                .response
                .ctx
                .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)))
}

pub fn navigation_item(
    ui: &mut Ui,
    title: &str,
    kind: TileKind,
    selected: bool,
    count: Option<usize>,
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.0), egui::Sense::click());
    let color = if selected {
        button_text()
    } else {
        sidebar_text()
    };
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            10,
            if selected {
                accent()
            } else {
                sidebar().lerp_to_gamma(ink(), 0.05)
            },
        );
    }
    paint_icon(
        ui.painter(),
        egui::Rect::from_center_size(
            rect.left_center() + egui::vec2(20.0, 0.0),
            egui::vec2(18.0, 18.0),
        ),
        kind,
        color,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(39.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(14.0),
        color,
    );
    if let Some(count) = count {
        ui.painter().text(
            rect.right_center() - egui::vec2(15.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            count.to_string(),
            egui::FontId::proportional(12.0),
            color,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[derive(Clone, Copy)]
pub enum TileKind {
    Game,
    Contact,
    Folder,
    Document,
    Credential,
    Item,
    Setting,
    Website,
    Add,
}

/// Allocate each tile as a single square. Child text cannot change its geometry.
pub fn gallery(ui: &mut Ui, id: &str, content: impl FnOnce(&mut Ui, f32)) {
    let gap = 16.0;
    let available = ui.available_width();
    let columns = ((available + gap) / 206.0).floor().max(1.0);
    let side = (available - gap * (columns - 1.0)) / columns;
    ui.push_id(id, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
            |ui| {
                ui.set_min_width(available);
                content(ui, side);
            },
        );
    });
}

pub fn tile(
    ui: &mut Ui,
    id: &str,
    title: &str,
    detail: &str,
    kind: TileKind,
    side: f32,
) -> Response {
    let (_, rect) = ui.allocate_space(egui::vec2(side, side));
    let response = ui.interact(rect, ui.scope_id().with(id), egui::Sense::click_and_drag());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let is_add = matches!(kind, TileKind::Add);
    let hover = ui
        .ctx()
        .animate_bool(response.id.with("hover"), response.hovered());
    let (icon_color, _tint) = match kind {
        TileKind::Folder => (
            Color32::from_rgb(64, 156, 255),
            Color32::from_rgb(231, 242, 255),
        ),
        TileKind::Document => (
            Color32::from_rgb(85, 135, 213),
            Color32::from_rgb(235, 243, 255),
        ),
        TileKind::Credential => (
            Color32::from_rgb(139, 104, 208),
            Color32::from_rgb(244, 237, 253),
        ),
        TileKind::Item => (
            Color32::from_rgb(71, 151, 132),
            Color32::from_rgb(232, 246, 240),
        ),
        _ => (accent(), Color32::from_rgb(239, 237, 252)),
    };
    painter.rect_filled(
        rect.translate(egui::vec2(0.0, 3.0)),
        16,
        Color32::from_black_alpha(4 + (hover * 5.0) as u8),
    );
    painter.rect_filled(
        rect,
        16,
        if is_add {
            surface().lerp_to_gamma(accent(), 0.07)
        } else {
            surface()
        },
    );
    painter.rect_stroke(
        rect,
        16,
        egui::Stroke::new(
            1.0,
            if response.hovered() {
                accent()
            } else if is_add {
                border().lerp_to_gamma(accent(), 0.18)
            } else {
                border()
            },
        ),
        egui::StrokeKind::Inside,
    );
    let icon_rect = if is_add {
        egui::Rect::from_center_size(
            rect.center_top() + egui::vec2(0.0, side * 0.36),
            egui::vec2(40.0, 40.0),
        )
    } else {
        egui::Rect::from_min_size(rect.min + egui::vec2(22.0, 56.0), egui::vec2(32.0, 32.0))
    };
    if is_add {
        painter.circle_filled(
            icon_rect.center(),
            28.0,
            surface().lerp_to_gamma(accent(), 0.10),
        );
    }
    paint_icon(&painter, icon_rect, kind, icon_color);
    let text_origin = rect.left_bottom() + egui::vec2(20.0, -68.0);
    let mut title_job = egui::text::LayoutJob::simple(
        title.replace(['\n', '\r'], " "),
        egui::FontId::proportional(16.0),
        ink(),
        side - 48.0,
    );
    title_job.wrap.max_rows = 1;
    title_job.wrap.break_anywhere = true;
    painter.galley(text_origin, painter.layout_job(title_job), ink());
    if !is_add {
        let mut detail_job = egui::text::LayoutJob::simple(
            detail.replace(['\n', '\r'], " "),
            egui::FontId::proportional(12.0),
            muted(),
            side - 48.0,
        );
        detail_job.wrap.max_rows = 1;
        detail_job.wrap.break_anywhere = true;
        painter.galley(
            text_origin + egui::vec2(0.0, 30.0),
            painter.layout_job(detail_job),
            muted(),
        );
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("{title}\n{detail}"))
}

pub fn tagged_tile(
    ui: &mut Ui,
    id: &str,
    title: &str,
    details: (&str, &[String]),
    kind: TileKind,
    side: f32,
) -> Response {
    let response = tile(ui, id, title, details.0, kind, side);
    let text = if details.1.is_empty() {
        "未分类".to_owned()
    } else {
        details.1.join(" · ")
    };
    let mut job = egui::text::LayoutJob::simple(
        text,
        egui::FontId::proportional(11.0),
        accent(),
        (side - 60.0).max(20.0),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let painter = ui
        .painter()
        .with_clip_rect(response.rect.intersect(ui.clip_rect()));
    let galley = painter.layout_job(job);
    let position = response.rect.min + egui::vec2(20.0, 19.0);
    let badge = egui::Rect::from_min_size(position, egui::vec2(galley.size().x + 14.0, 23.0));
    painter.rect_filled(badge, 7, surface());
    painter.galley(position + egui::vec2(7.0, 3.0), galley, accent());
    response
}

pub fn tag_filter_bar(ui: &mut Ui, filter: &mut crate::tags::Filter, names: &[String]) -> bool {
    let selected = match filter {
        crate::tags::Filter::All => "全部分类",
        crate::tags::Filter::Untagged => "未分类",
        crate::tags::Filter::Named(name) => name,
    }
    .to_owned();
    egui::ComboBox::from_id_salt("page-category-filter")
        .width(112.0)
        .truncate()
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(filter, crate::tags::Filter::All, "全部分类");
            ui.selectable_value(filter, crate::tags::Filter::Untagged, "未分类");
            for name in names {
                ui.selectable_value(filter, crate::tags::Filter::Named(name.clone()), name);
            }
        });
    ui.button("标签").on_hover_text("管理分类标签").clicked()
}

pub fn tag_choices(ui: &mut Ui, selected: &mut Vec<String>, names: &[String]) -> bool {
    ui.label("分类标签（可多选）");
    ui.horizontal_wrapped(|ui| {
        if names.is_empty() {
            ui.label(
                egui::RichText::new("还没有自定义标签")
                    .small()
                    .color(muted()),
            );
        }
        for name in names {
            let mut checked = selected.contains(name);
            if ui.checkbox(&mut checked, name).changed() {
                if checked {
                    selected.push(name.clone());
                } else {
                    selected.retain(|tag| tag != name);
                }
            }
        }
    });
    ui.button("管理标签").clicked()
}

/// Vector icons stay sharp at Windows display scaling and need no icon font.
pub fn paint_icon(painter: &egui::Painter, rect: egui::Rect, kind: TileKind, color: Color32) {
    let scale = rect.width() / 24.0;
    let point = |x, y| rect.min + egui::vec2(x * scale, y * scale);
    let stroke = egui::Stroke::new(1.6 * scale, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        painter.line_segment([point(a.0, a.1), point(b.0, b.1)], stroke);
    };
    let box_rect =
        |x, y, w, h| egui::Rect::from_min_size(point(x, y), egui::vec2(w * scale, h * scale));
    match kind {
        TileKind::Game => {
            painter.rect_stroke(
                box_rect(2.0, 6.0, 20.0, 13.0),
                4,
                stroke,
                egui::StrokeKind::Inside,
            );
            line((5.0, 12.0), (11.0, 12.0));
            line((8.0, 9.0), (8.0, 15.0));
            painter.circle_filled(point(17.0, 10.0), 1.3 * scale, color);
            painter.circle_filled(point(19.0, 14.0), 1.3 * scale, color);
        }
        TileKind::Contact => {
            painter.circle_stroke(point(12.0, 7.0), 4.0 * scale, stroke);
            painter.rect_stroke(
                box_rect(4.0, 14.0, 16.0, 8.0),
                4,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        TileKind::Folder => {
            painter.rect_filled(box_rect(2.0, 5.0, 9.0, 5.0), 2, color);
            painter.rect_filled(box_rect(2.0, 8.0, 20.0, 13.0), 3, color);
            painter.rect_filled(
                box_rect(3.5, 11.0, 17.0, 8.0),
                2,
                color.lerp_to_gamma(Color32::WHITE, 0.25),
            );
        }
        TileKind::Document => {
            painter.rect_filled(box_rect(5.0, 2.0, 15.0, 20.0), 3, color);
            let paper = egui::Stroke::new(1.3 * scale, Color32::WHITE);
            for y in [8.0, 12.0, 16.0] {
                painter.line_segment(
                    [point(8.0, y), point(if y == 16.0 { 14.0 } else { 17.0 }, y)],
                    paper,
                );
            }
        }
        TileKind::Credential => {
            painter.circle_stroke(point(8.0, 8.0), 5.5 * scale, stroke);
            line((12.0, 12.0), (21.0, 21.0));
            line((16.0, 16.0), (19.0, 13.0));
            line((19.0, 19.0), (22.0, 16.0));
        }
        TileKind::Item => {
            painter.rect_stroke(
                box_rect(3.0, 4.0, 18.0, 17.0),
                3,
                stroke,
                egui::StrokeKind::Inside,
            );
            line((3.0, 9.0), (21.0, 9.0));
            line((10.0, 4.0), (10.0, 13.0));
            line((14.0, 4.0), (14.0, 13.0));
            line((7.0, 17.0), (11.0, 17.0));
        }
        TileKind::Website => {
            painter.circle_stroke(point(12.0, 12.0), 10.0 * scale, stroke);
            line((2.0, 12.0), (22.0, 12.0));
            let meridian = (0..=32)
                .map(|i| {
                    let angle = std::f32::consts::TAU * i as f32 / 32.0;
                    point(12.0 + 4.8 * angle.cos(), 12.0 + 10.0 * angle.sin())
                })
                .collect();
            painter.add(egui::Shape::line(meridian, stroke));
        }
        TileKind::Setting => {
            for (y, x) in [(5.0, 8.0), (12.0, 16.0), (19.0, 10.0)] {
                line((3.0, y), (21.0, y));
                painter.circle_filled(point(x, y), 2.5 * scale, color);
            }
        }
        TileKind::Add => {
            line((6.0, 12.0), (18.0, 12.0));
            line((12.0, 6.0), (12.0, 18.0));
        }
    }
}

/// Inline actions keep their exact rectangle and do not affect collection layout.
fn quick_action(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    rect: egui::Rect,
    label: &str,
) -> Response {
    let response = ui.interact(rect, ui.scope_id().with(id), egui::Sense::click());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(
        rect,
        6,
        if response.hovered() {
            canvas()
        } else {
            surface()
        },
    );
    painter.rect_stroke(
        rect,
        6,
        egui::Stroke::new(
            1.0,
            if response.hovered() {
                accent()
            } else {
                border()
            },
        ),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.0),
        ink(),
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_blocks_background_clicks_and_only_explicit_close_dismisses_it() {
        let ctx = egui::Context::default();
        let mut background_clicked = false;
        let mut closed = false;
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 680.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        background_clicked |= ui
                            .add_sized([150.0, 80.0], egui::Button::new("Background action"))
                            .clicked();
                    });
                    let dialog =
                        centered_dialog(&ctx, "modal-test", "Details", "Draft", 400.0, |ui| {
                            ui.label("Content");
                        });
                    closed |= dialog_closed(&dialog);
                },
            );
            output.textures_delta.clear();
        };
        for _ in 0..3 {
            frame(vec![]);
        }
        let position = egui::pos2(50.0, 40.0);
        for pressed in [true, false] {
            frame(vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        assert!(!closed, "clicking the backdrop must not discard the draft");
        assert!(
            !background_clicked,
            "modal must block the background action"
        );
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |_| {
                let dialog = centered_dialog(&ctx, "modal-test", "Details", "Draft", 400.0, |ui| {
                    ui.label("Content");
                });
                assert!(dialog_closed(&dialog));
            },
        );
        output.textures_delta.clear();
    }

    #[test]
    fn long_names_keep_square_tiles_and_single_line_text_at_supported_widths() {
        for width in [680.0, 950.0, 1400.0] {
            let ctx = egui::Context::default();
            let mut rects = Vec::new();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        gallery(ui, "regression", |ui, side| {
                            rects.clear();
                            for index in 0..12 {
                                rects.push(
                                    tile(
                                        ui,
                                        &index.to_string(),
                                        &"very long document title ".repeat(12),
                                        &"C:\\Users\\Owner\\Documents\\".repeat(12),
                                        TileKind::Document,
                                        side,
                                    )
                                    .rect,
                                );
                            }
                        });
                    });
                },
            );
            output.textures_delta.clear();
            assert_eq!(rects.len(), 12);
            for (index, rect) in rects.iter().enumerate() {
                assert!((rect.width() - rect.height()).abs() < 0.1);
                assert!(rect.width() >= 160.0 && rect.width() <= 228.1);
                assert!(
                    rect.left() >= 0.0 && rect.right() <= width,
                    "viewport={width}, index={index}, rect={rect:?}; all={rects:?}"
                );
                for other in &rects[index + 1..] {
                    assert!(!rect.intersects(*other));
                }
            }
            let mut text_count = 0;
            for shape in output.shapes {
                if let egui::epaint::Shape::Text(text) = shape.shape {
                    assert_eq!(text.galley.rows.len(), 1);
                    assert!(text.galley.size().x <= 228.0);
                    text_count += 1;
                }
            }
            assert_eq!(text_count, 24);
        }
    }

    #[test]
    fn grid_and_list_entries_open_on_left_click_and_expose_a_menu_on_right_click() {
        for mode in [crate::views::ViewMode::Grid, crate::views::ViewMode::List] {
            let ctx = egui::Context::default();
            let mut rect = egui::Rect::NOTHING;
            let mut clicked = false;
            let mut menu_open = false;
            let mut frame = |events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            collection(ui, "menu-regression", mode, |ui, layout| {
                                let response = entry(
                                    ui,
                                    "record",
                                    "record",
                                    "details",
                                    TileKind::Item,
                                    layout,
                                );
                                rect = response.rect;
                                clicked |= response.clicked();
                                response.context_menu(|ui| {
                                    ui.button("Delete").clicked();
                                });
                                menu_open |= response.context_menu_opened();
                            });
                        });
                    },
                );
                output.textures_delta.clear();
            };
            frame(vec![]);
            let position = egui::pos2(70.0, 30.0);
            frame(vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            frame(vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            frame(vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }]);
            frame(vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            assert!(rect.contains(position));
            assert!(clicked);
            assert!(menu_open);
        }
    }
}
