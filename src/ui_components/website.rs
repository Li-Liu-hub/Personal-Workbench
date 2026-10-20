use super::{collection::EntryLayout, *};
use crate::{model::Website, views::ViewMode};

pub fn website_entry(
    ui: &mut Ui,
    item: &Website,
    layout: EntryLayout,
    labels: &[String],
) -> (Response, bool) {
    let grid = layout.mode == ViewMode::Grid;
    let response = if grid {
        tagged_entry(
            ui,
            &item.id,
            &item.title,
            (&item.url, labels),
            TileKind::Website,
            layout,
        )
    } else {
        entry(
            ui,
            &item.id,
            &item.title,
            if grid { &item.url } else { "" },
            TileKind::Website,
            layout,
        )
    };
    let rect = response.rect;
    if !grid {
        let width = (rect.width() - 76.0) * 0.55 - 230.0;
        let mut job = egui::text::LayoutJob::simple(
            item.url.clone(),
            egui::FontId::proportional(12.0),
            muted(),
            width.max(20.0),
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        ui.painter().galley(
            egui::pos2(
                rect.left() + 56.0 + (rect.width() - 76.0) * 0.45,
                rect.center().y - 8.0,
            ),
            ui.painter().layout_job(job),
            muted(),
        );
        if !labels.is_empty() {
            let rect = egui::Rect::from_min_size(
                rect.min + egui::vec2(56.0, 36.0),
                egui::vec2(rect.width() * 0.35, 16.0),
            );
            let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
            let mut job = egui::text::LayoutJob::simple(
                labels.join(" · "),
                egui::FontId::proportional(10.0),
                accent(),
                rect.width(),
            );
            job.wrap.max_rows = 1;
            painter.galley(rect.min, painter.layout_job(job), accent());
        }
    }
    let button = if grid {
        egui::Rect::from_min_size(
            egui::pos2(rect.right() - 80.0, rect.top() + 18.0),
            egui::vec2(60.0, 30.0),
        )
    } else {
        egui::Rect::from_center_size(
            egui::pos2(rect.right() - 48.0, rect.center().y),
            egui::vec2(60.0, 30.0),
        )
    };
    let clicked = quick_action(ui, (&item.id, "quick-open"), button, "打开")
        .on_hover_text(&item.url)
        .clicked();
    (response, clicked)
}
