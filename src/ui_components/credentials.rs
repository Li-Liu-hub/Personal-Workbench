use super::{collection::EntryLayout, *};
use crate::{model::CredentialSummary, views::ViewMode};
use std::sync::Arc;

fn layout(ui: &Ui, value: &str, width: f32, size: f32) -> Arc<egui::Galley> {
    aligned_layout(ui, value, width, size, egui::Align::Min)
}

fn aligned_layout(
    ui: &Ui,
    value: &str,
    width: f32,
    size: f32,
    align: egui::Align,
) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(
        value.to_owned(),
        egui::FontId::proportional(size),
        ink(),
        width.max(20.0),
    );
    job.wrap.break_anywhere = true;
    job.halign = align;
    ui.painter().layout_job(job)
}

pub fn credential_list_header(ui: &mut Ui, mode: ViewMode) {
    if mode != ViewMode::List {
        return;
    }
    let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), 28.0));
    let inner = (rect.width() - 108.0).max(1.0);
    let left = rect.left() + 20.;
    for (value, x) in [
        ("名称", left + (inner * 0.22 - 12.) / 2.),
        ("账号", left + inner * 0.22 + (inner * 0.27 - 62.) / 2.),
        ("密码", left + inner * 0.49 + (inner * 0.34 - 62.) / 2.),
        ("分类", left + inner * 0.83 + (inner * 0.17 - 8.) / 2.),
    ] {
        ui.painter().text(
            egui::pos2(x, rect.center().y),
            egui::Align2::CENTER_CENTER,
            value,
            egui::FontId::proportional(12.0),
            muted(),
        );
    }
    ui.painter().text(
        egui::pos2(rect.right() - 44.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        "操作",
        egui::FontId::proportional(12.0),
        muted(),
    );
}

pub struct CredentialEntryResponse {
    pub response: Response,
    pub changed: bool,
    pub save: bool,
    pub cancel: bool,
    pub details: bool,
    pub classification_changed: bool,
}

fn editable_field(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut String,
    rect: egui::Rect,
    size: f32,
    multiline: bool,
) -> Response {
    let field = if multiline {
        egui::TextEdit::multiline(value)
            .return_key(None)
            .desired_rows(1)
    } else {
        egui::TextEdit::singleline(value)
    };
    let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, width: f32| {
        aligned_layout(
            ui,
            text.as_str(),
            if multiline { width } else { f32::INFINITY },
            size,
            egui::Align::Center,
        )
    };
    if ui.rect_contains_pointer(rect) || ui.ctx().memory(|m| m.has_focus(id)) {
        ui.painter()
            .rect_filled(rect.expand(2.), 5, crate::theme::input());
        ui.painter().rect_stroke(
            rect.expand(2.),
            5,
            egui::Stroke::new(1., border()),
            egui::StrokeKind::Inside,
        );
    }
    let response = ui.place(
        rect,
        field
            .id(id)
            .font(egui::FontId::proportional(size))
            .frame(egui::Frame::NONE)
            .layouter(&mut layouter)
            .margin(0)
            .align(egui::Align2::CENTER_CENTER)
            .min_size(rect.size())
            .desired_width(rect.width()),
    );
    super::select_on_focus(&response, value);
    response
}

fn readonly_value(ui: &mut Ui, value: &str, rect: egui::Rect, size: f32) -> Response {
    let galley = layout(ui, value, rect.width(), size);
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(rect.min, galley, ink());
    ui.interact(
        rect,
        ui.scope_id().with((
            "readonly",
            value,
            rect.min.x.to_bits(),
            rect.min.y.to_bits(),
        )),
        egui::Sense::click(),
    )
}

fn credential_metrics(ui: &Ui, item: &CredentialSummary, entry: EntryLayout) -> (f32, f32, f32) {
    let grid = entry.mode == ViewMode::Grid;
    let inner = (entry.width - if grid { 40.0 } else { 108.0 }).max(1.0);
    let username = layout(
        ui,
        &item.username,
        if grid { inner } else { inner * 0.27 - 62.0 },
        14.0,
    )
    .size()
    .y;
    let password = layout(
        ui,
        &item.password,
        if grid { inner } else { inner * 0.34 - 62.0 },
        15.0,
    )
    .size()
    .y;
    let body = if grid {
        entry.width.max(182.0 + username + password)
    } else {
        56.0_f32.max(username.max(password) + 24.0)
    };
    (username, password, body)
}

pub fn credential_height(
    ui: &Ui,
    item: &CredentialSummary,
    entry: EntryLayout,
    editing: bool,
) -> f32 {
    credential_metrics(ui, item, entry).2 + if editing { 40.0 } else { 0.0 }
}

pub fn credential_entry(
    ui: &mut Ui,
    item: &mut CredentialSummary,
    entry: EntryLayout,
    editing: bool,
    categories: &[String],
) -> CredentialEntryResponse {
    let grid = entry.mode == ViewMode::Grid;
    let inner = (entry.width - if grid { 40.0 } else { 108.0 }).max(1.0);
    let (username_height, password_height, body_height) = credential_metrics(ui, item, entry);
    let height = (body_height + if editing { 40.0 } else { 0.0 }).max(entry.min_height);
    let (_, rect) = ui.allocate_space(egui::vec2(entry.width, height));
    let response = ui.interact(rect, ui.scope_id().with(&item.id), egui::Sense::click());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, if grid { 16 } else { 9 }, surface());
    painter.rect_stroke(
        rect,
        if grid { 16 } else { 9 },
        egui::Stroke::new(
            1.0,
            if editing {
                accent()
            } else if response.hovered() {
                border().lerp_to_gamma(accent(), 0.4)
            } else {
                border()
            },
        ),
        egui::StrokeKind::Inside,
    );
    let left = rect.left() + 20.0;
    let top = rect.top();
    let (title_rect, username_rect, password_rect, detail_rect, user_copy, password_copy) = if grid
    {
        paint_icon(
            &painter,
            egui::Rect::from_min_size(rect.min + egui::vec2(18.0, 18.0), egui::vec2(26.0, 26.0)),
            TileKind::Credential,
            accent(),
        );
        painter.line_segment(
            [
                egui::pos2(left, top + 85.0),
                egui::pos2(rect.right() - 20.0, top + 85.0),
            ],
            egui::Stroke::new(1.0, border()),
        );
        let password_y = top + 132.0 + username_height;
        for (label, y) in [("账号", top + 98.0), ("密码", password_y)] {
            painter.text(
                egui::pos2(left, y),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::proportional(11.0),
                muted(),
            );
        }
        (
            egui::Rect::from_min_size(
                rect.min + egui::vec2(54.0, 20.0),
                egui::vec2((entry.width - 134.0).max(30.0), 24.0),
            ),
            egui::Rect::from_min_size(
                egui::pos2(left, top + 117.0),
                egui::vec2(inner, username_height.max(22.0)),
            ),
            egui::Rect::from_min_size(
                egui::pos2(left, password_y + 19.0),
                egui::vec2(inner, password_height.max(22.0)),
            ),
            egui::Rect::from_min_size(
                egui::pos2(rect.right() - 68.0, top + 16.0),
                egui::vec2(48.0, 26.0),
            ),
            egui::Rect::from_min_size(
                egui::pos2(rect.right() - 68.0, top + 91.0),
                egui::vec2(48.0, 25.0),
            ),
            egui::Rect::from_min_size(
                egui::pos2(rect.right() - 68.0, password_y - 6.0),
                egui::vec2(48.0, 25.0),
            ),
        )
    } else {
        let center = top + body_height / 2.0;
        (
            egui::Rect::from_center_size(
                egui::pos2(left + (inner * 0.22 - 12.0) / 2., center),
                egui::vec2(inner * 0.22 - 12.0, 32.0),
            ),
            egui::Rect::from_center_size(
                egui::pos2(left + inner * 0.22 + (inner * 0.27 - 62.) / 2., center),
                egui::vec2(inner * 0.27 - 62.0, username_height.max(32.0)),
            ),
            egui::Rect::from_center_size(
                egui::pos2(left + inner * 0.49 + (inner * 0.34 - 62.) / 2., center),
                egui::vec2(inner * 0.34 - 62.0, password_height.max(32.0)),
            ),
            egui::Rect::from_center_size(
                egui::pos2(rect.right() - 44.0, center),
                egui::vec2(48.0, 26.0),
            ),
            egui::Rect::from_center_size(
                egui::pos2(left + inner * 0.49 - 30.0, center),
                egui::vec2(48.0, 25.0),
            ),
            egui::Rect::from_center_size(
                egui::pos2(left + inner * 0.83 - 30.0, center),
                egui::vec2(48.0, 25.0),
            ),
        )
    };
    let title = if grid {
        ui.place(
            title_rect,
            egui::Label::new(&item.title)
                .truncate()
                .sense(egui::Sense::click()),
        )
    } else {
        editable_field(
            ui,
            egui::Id::unique(("credential-inline", &item.id, "title")),
            &mut item.title,
            title_rect,
            if grid { 16.0 } else { 14.0 },
            false,
        )
    };
    let category_rect = if grid {
        egui::Rect::from_min_size(rect.min + egui::vec2(20.0, 52.0), egui::vec2(inner, 28.0))
    } else {
        egui::Rect::from_min_size(
            egui::pos2(left + inner * 0.83, top + body_height / 2.0 - 14.0),
            egui::vec2(inner * 0.17 - 8.0, 28.0),
        )
    };
    let mut category_ui = ui.new_child(egui::UiBuilder::new().max_rect(category_rect).layout(
        if grid {
            egui::Layout::top_down(egui::Align::Min)
        } else {
            egui::Layout::left_to_right(egui::Align::Center)
        },
    ));
    let classification_changed = if grid {
        category_ui.label(
            egui::RichText::new(if item.tags.is_empty() {
                "未分类".into()
            } else {
                item.tags.join(" · ")
            })
            .small()
            .color(muted()),
        );
        false
    } else {
        classification_dropdown(
            &mut category_ui,
            ("account-category", &item.id),
            &mut item.tags,
            categories,
            category_rect.width(),
        )
    };
    let username = if grid {
        readonly_value(ui, &item.username, username_rect, 14.0)
    } else {
        editable_field(
            ui,
            egui::Id::unique(("credential-inline", &item.id, "username")),
            &mut item.username,
            username_rect,
            14.0,
            true,
        )
    };
    let password = if grid {
        readonly_value(ui, &item.password, password_rect, 15.0)
    } else {
        editable_field(
            ui,
            egui::Id::unique(("credential-inline", &item.id, "password")),
            &mut item.password,
            password_rect,
            15.0,
            true,
        )
    };
    copy_button(ui, &item.id, "username", &item.username, user_copy);
    copy_button(ui, &item.id, "password", &item.password, password_copy);
    let details = quick_action(ui, (&item.id, "details"), detail_rect, "详情").clicked()
        || (grid && (title.clicked() || username.clicked() || password.clicked()));
    let mut save = false;
    let mut cancel = false;
    if editing {
        let y = rect.bottom() - 20.0;
        save = quick_action(
            ui,
            (&item.id, "inline-save"),
            egui::Rect::from_center_size(
                egui::pos2(rect.right() - 108.0, y),
                egui::vec2(58.0, 28.0),
            ),
            "保存",
        )
        .clicked();
        cancel = quick_action(
            ui,
            (&item.id, "inline-cancel"),
            egui::Rect::from_center_size(
                egui::pos2(rect.right() - 42.0, y),
                egui::vec2(58.0, 28.0),
            ),
            "取消",
        )
        .clicked();
        if title.has_focus() || username.has_focus() || password.has_focus() {
            save |=
                ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
            cancel |=
                ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
    }
    CredentialEntryResponse {
        response,
        changed: title.changed() || username.changed() || password.changed(),
        save,
        cancel,
        details,
        classification_changed,
    }
}

fn copy_button(ui: &mut Ui, record: &str, field: &str, value: &str, rect: egui::Rect) {
    let id = egui::Id::unique(("credential-copy", record, field));
    let now = ui.input(|i| i.time);
    let recent = ui
        .data(|d| d.get_temp::<f64>(id))
        .is_some_and(|t| now - t < 1.2);
    let label = if recent { "已复制" } else { "复制" };
    let clicked = quick_action(ui, (record, field), rect, label)
        .on_hover_text(if field == "username" {
            "复制账号"
        } else {
            "复制密码"
        })
        .clicked();
    if clicked {
        ui.ctx().copy_text(value.to_owned());
        ui.data_mut(|d| d.insert_temp(id, now));
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_plaintext_values_fit_inside_grid_cards_and_list_rows_at_display_scales() {
        for mode in [ViewMode::Grid, ViewMode::List] {
            for scale in [1.0, 1.25, 1.5] {
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(scale);
                let mut record = CredentialSummary {
                    id: "synthetic".into(),
                    title: "test".into(),
                    username: "synthetic-account@example.test".repeat(3),
                    password: "Synthetic-Password-123!".repeat(5),
                    ..Default::default()
                };
                let width = if mode == ViewMode::Grid { 220.0 } else { 680.0 };
                let mut bounds = egui::Rect::NOTHING;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000.0, 1600.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            bounds = credential_entry(
                                ui,
                                &mut record,
                                EntryLayout::new(mode, width),
                                false,
                                &[],
                            )
                            .response
                            .rect;
                        });
                    },
                );
                output.textures_delta.clear();
                for value in [&record.username, &record.password] {
                    let text = output
                        .shapes
                        .iter()
                        .find_map(|s| match &s.shape {
                            egui::Shape::Text(t) if t.galley.text() == value => Some(t),
                            _ => None,
                        })
                        .unwrap();
                    assert!(
                        bounds
                            .contains_rect(egui::Rect::from_min_size(text.pos, text.galley.size())),
                        "{mode:?} at {scale}"
                    );
                    if mode == ViewMode::List {
                        let text_rect = text.galley.rect.translate(text.pos.to_vec2());
                        assert!(
                            (text_rect.center().y - bounds.center().y).abs() < 1.5,
                            "list value not vertically centered at {scale}: {text_rect:?} {bounds:?}"
                        );
                    }
                }
            }
        }
    }
}
