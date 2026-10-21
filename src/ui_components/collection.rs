use super::*;
use crate::views::ViewMode;

#[derive(Clone, Copy)]
pub struct EntryLayout {
    pub(super) mode: ViewMode,
    pub(super) width: f32,
    actions: bool,
}
impl EntryLayout {
    pub fn new(mode: ViewMode, width: f32) -> Self {
        Self {
            mode,
            width,
            actions: false,
        }
    }
}
struct RowLayout {
    size: egui::Vec2,
    actions: bool,
}

pub fn view_switcher(ui: &mut Ui, mode: &mut ViewMode) -> bool {
    let previous = *mode;
    egui::Frame::new()
        .fill(crate::theme::input())
        .corner_radius(8)
        .inner_margin(3)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.;
            ui.visuals_mut().selection.bg_fill = surface();
            ui.visuals_mut().selection.stroke = egui::Stroke::new(1., ink());
            ui.horizontal(|ui| {
                ui.selectable_value(mode, ViewMode::Grid, "宫格");
                ui.selectable_value(mode, ViewMode::List, "列表");
            });
        });
    previous != *mode
}

/// Both views share the same item callbacks and response-based interactions.
pub fn collection(
    ui: &mut Ui,
    id: &str,
    mode: ViewMode,
    content: impl FnOnce(&mut Ui, EntryLayout),
) {
    if mode == ViewMode::Grid {
        gallery(ui, id, |ui, width| {
            content(ui, EntryLayout::new(mode, width))
        });
    } else {
        ui.push_id(id, |ui| {
            ui.spacing_mut().item_spacing.y = 7.0;
            ui.vertical(|ui| {
                let width = ui.available_width();
                content(ui, EntryLayout::new(mode, width));
            });
        });
    }
}

pub fn entry(
    ui: &mut Ui,
    id: &str,
    title: &str,
    detail: &str,
    kind: TileKind,
    layout: EntryLayout,
) -> Response {
    if layout.mode == ViewMode::Grid {
        tile(ui, id, title, detail, kind, layout.width)
    } else {
        row(
            ui,
            id,
            title,
            &[(detail, 0.55)],
            kind,
            RowLayout {
                size: egui::vec2(layout.width, 56.0),
                actions: layout.actions,
            },
        )
    }
}

pub fn tagged_entry(
    ui: &mut Ui,
    id: &str,
    title: &str,
    details: (&str, &[String]),
    kind: TileKind,
    layout: EntryLayout,
) -> Response {
    if layout.mode == ViewMode::Grid {
        tagged_tile(ui, id, title, details, kind, layout.width)
    } else {
        let tags = if details.1.is_empty() {
            "未分类".into()
        } else {
            details.1.join(" · ")
        };
        row(
            ui,
            id,
            title,
            &[(details.0, 0.36), (&tags, 0.24)],
            kind,
            RowLayout {
                size: egui::vec2(layout.width, 56.0),
                actions: layout.actions,
            },
        )
    }
}

fn column_rects(rect: egui::Rect, weights: &[f32]) -> Vec<egui::Rect> {
    let left = rect.left() + 56.0;
    let width = (rect.width() - 76.0).max(1.0);
    let mut x = left;
    weights
        .iter()
        .map(|weight| {
            let cell = egui::Rect::from_min_size(
                egui::pos2(x, rect.top()),
                egui::vec2(width * weight, rect.height()),
            );
            x += width * weight;
            cell
        })
        .collect()
}

fn text(ui: &Ui, rect: egui::Rect, value: &str, size: f32, color: Color32) {
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let mut job = egui::text::LayoutJob::simple(
        value.replace(['\r', '\n'], " "),
        egui::FontId::proportional(size),
        color,
        (rect.width() - 12.0).max(1.0),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = painter.layout_job(job);
    let pos = egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
    painter.galley(pos, galley, color);
}

fn row(
    ui: &mut Ui,
    id: &str,
    title: &str,
    fields: &[(&str, f32)],
    kind: TileKind,
    layout: RowLayout,
) -> Response {
    let size = layout.size;
    let (_, rect) = ui.allocate_space(size);
    let response = ui.interact(rect, ui.scope_id().with(id), egui::Sense::click_and_drag());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let add = matches!(kind, TileKind::Add);
    painter.rect_filled(
        rect,
        9,
        if add {
            surface().lerp_to_gamma(accent(), 0.07)
        } else if response.hovered() {
            surface().lerp_to_gamma(accent(), 0.04)
        } else {
            surface()
        },
    );
    if add || response.hovered() {
        painter.rect_stroke(
            rect,
            9,
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
    }
    paint_icon(
        &painter,
        egui::Rect::from_center_size(
            rect.left_center() + egui::vec2(28.0, 0.0),
            egui::vec2(23.0, 23.0),
        ),
        kind,
        accent(),
    );
    let detail_weight: f32 = fields.iter().map(|field| field.1).sum();
    let weights = std::iter::once(1.0 - detail_weight)
        .chain(fields.iter().map(|field| field.1))
        .collect::<Vec<_>>();
    let mut columns_area = rect;
    if layout.actions {
        columns_area.max.x -= 62.0;
    }
    let columns = column_rects(columns_area, &weights);
    text(
        ui,
        columns[0],
        title,
        14.0,
        if add { accent() } else { ink() },
    );
    if !add {
        for (column, (value, _)) in columns[1..].iter().zip(fields) {
            text(ui, *column, value, 12.0, muted());
        }
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(
            std::iter::once(title)
                .chain(fields.iter().map(|field| field.0))
                .collect::<Vec<_>>()
                .join("\n"),
        )
}
