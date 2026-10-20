use super::*;
use crate::{model::FileItem, views::ViewMode};

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
    pub fn with_file_controls(self) -> Self {
        Self {
            actions: true,
            ..self
        }
    }
}
struct RowLayout {
    size: egui::Vec2,
    actions: bool,
}

pub fn view_switcher(ui: &mut Ui, mode: &mut ViewMode) -> bool {
    let previous = *mode;
    ui.selectable_value(mode, ViewMode::List, "列表");
    ui.selectable_value(mode, ViewMode::Grid, "宫格");
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

pub fn file_list_header(ui: &mut Ui, mode: ViewMode) {
    if mode != ViewMode::List {
        return;
    }
    let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), 30.0));
    let columns_area = egui::Rect::from_min_max(rect.min, rect.max - egui::vec2(62.0, 0.0));
    for (rect, value) in column_rects(columns_area, &[0.46, 0.13, 0.13, 0.28])
        .into_iter()
        .zip(["名称", "类型", "大小", "修改时间"])
    {
        text(ui, rect, value, 12.0, muted());
    }
}

pub fn file_entry(ui: &mut Ui, item: &FileItem, layout: EntryLayout) -> Response {
    let kind = if item.is_dir {
        TileKind::Folder
    } else {
        TileKind::Document
    };
    let id = item.path.to_string_lossy();
    if layout.mode == ViewMode::Grid {
        let detail = if item.is_dir {
            format!("文件夹 · {}", item.modified)
        } else {
            format!("{} · {}", size(item.size), item.modified)
        };
        tile(ui, &id, &item.name, &detail, kind, layout.width)
    } else {
        let extension = item
            .path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_uppercase();
        let file_type = if item.is_dir {
            "文件夹".into()
        } else if extension.is_empty() {
            "文件".into()
        } else {
            format!("{extension} 文件")
        };
        let size = if item.is_dir || item.modified.is_empty() {
            "—".into()
        } else {
            size(item.size)
        };
        let modified = if item.modified.is_empty() {
            "—"
        } else {
            &item.modified
        };
        row(
            ui,
            &id,
            &item.name,
            &[(&file_type, 0.13), (&size, 0.13), (modified, 0.28)],
            kind,
            RowLayout {
                size: egui::vec2(layout.width, 40.0),
                actions: true,
            },
        )
    }
}

fn size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

pub fn file_transfer_button(ui: &mut Ui, response: &Response, mode: ViewMode) -> bool {
    let rect = if mode == ViewMode::Grid {
        egui::Rect::from_min_size(
            response.rect.right_top() + egui::vec2(-62.0, 12.0),
            egui::vec2(50.0, 28.0),
        )
    } else {
        egui::Rect::from_center_size(
            response.rect.right_center() - egui::vec2(33.0, 0.0),
            egui::vec2(50.0, 28.0),
        )
    };
    let button = ui.interact(rect, response.id.with("copy-file"), egui::Sense::click());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(
        rect,
        7,
        if button.hovered() {
            surface().lerp_to_gamma(accent(), 0.13)
        } else {
            surface()
        },
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "复制",
        egui::FontId::proportional(12.0),
        accent(),
    );
    button
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_list_rows_keep_columns_readable_and_long_names_inside_their_cells() {
        for width in [680.0, 950.0, 1400.0] {
            let ctx = egui::Context::default();
            let mut rects = vec![];
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
                        file_list_header(ui, ViewMode::List);
                        collection(ui, "file-rows", ViewMode::List, |ui, layout| {
                            for index in 0..8 {
                                let file = FileItem {
                                    name: "very long filename ".repeat(20),
                                    path: format!("{index}.txt").into(),
                                    is_dir: false,
                                    size: 1024,
                                    modified: "2026-10-19 09:00".into(),
                                };
                                rects.push(file_entry(ui, &file, layout).rect);
                            }
                        });
                    });
                },
            );
            output.textures_delta.clear();
            assert_eq!(rects.len(), 8);
            for (index, rect) in rects.iter().enumerate() {
                assert_eq!(rect.height(), 40.0);
                assert!(rect.left() >= 0.0 && rect.right() <= width);
                for other in &rects[index + 1..] {
                    assert!(!rect.intersects(*other));
                }
                for shape in &output.shapes {
                    if let egui::Shape::Text(text) = &shape.shape
                        && rect.contains(text.pos)
                    {
                        assert_eq!(text.galley.rows.len(), 1);
                        assert!(text.pos.x + text.galley.size().x <= rect.right());
                        if text.galley.text().starts_with("very long filename") {
                            assert!(text.galley.size().x < (rect.width() - 76.0) * 0.46);
                        }
                    }
                }
            }
            let values = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some(t.galley.text()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            for label in [
                "名称",
                "类型",
                "大小",
                "修改时间",
                "TXT 文件",
                "1.0 KB",
                "2026-10-19 09:00",
            ] {
                assert!(
                    values.contains(&label),
                    "missing column/value {label} at {width}"
                );
            }
        }
    }
}
