use super::*;
use crate::games::{
    Cell,
    tetris::{self, Piece},
};
const PALETTE: [Color32; 9] = [
    Color32::TRANSPARENT,
    Color32::from_rgb(84, 200, 220),
    Color32::from_rgb(247, 202, 80),
    Color32::from_rgb(176, 132, 229),
    Color32::from_rgb(95, 201, 139),
    Color32::from_rgb(241, 120, 135),
    Color32::from_rgb(106, 157, 237),
    Color32::from_rgb(243, 170, 93),
    Color32::from_rgb(117, 132, 151),
];
fn board(ui: &mut egui::Ui, size: egui::Vec2) -> (egui::Rect, egui::Painter) {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        response.request_focus();
    }
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 8, Color32::from_rgb(24, 32, 48));
    (rect, painter)
}
fn block(p: &egui::Painter, rect: egui::Rect, cell: f32, c: Cell, color: Color32) {
    if c.y < 0 {
        return;
    }
    p.rect_filled(
        egui::Rect::from_min_size(
            rect.min + egui::vec2(c.x as f32 * cell, c.y as f32 * cell),
            egui::vec2(cell, cell),
        )
        .shrink(1.),
        3,
        color,
    );
}
fn overlay(p: &egui::Painter, rect: egui::Rect, status: Status, keys: &str) {
    if status == Status::Running {
        return;
    }
    p.rect_filled(rect, 8, Color32::from_black_alpha(115));
    let title = match status {
        Status::Ready => "准备开始",
        Status::Paused => "已暂停",
        Status::Over => "本局结束",
        Status::Filled | Status::Cleared => "挑战完成",
        _ => "",
    };
    p.text(
        rect.center() - egui::vec2(0., 16.),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(24.),
        Color32::WHITE,
    );
    p.text(
        rect.center() + egui::vec2(0., 20.),
        egui::Align2::CENTER_CENTER,
        if ended(status) {
            "成绩已保存"
        } else {
            keys
        },
        egui::FontId::proportional(12.),
        Color32::from_gray(218),
    );
}
fn preview_piece(ui: &mut egui::Ui, kind: u8, scale: f32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(scale * 4., scale * 2.), egui::Sense::hover());
    for c in (Piece {
        kind,
        rotation: 0,
        x: 0,
        y: 0,
    })
    .cells()
    {
        block(ui.painter(), rect, scale, c, PALETTE[kind as usize + 1]);
    }
}
impl PersonalDeskApp {
    pub(in crate::app) fn draw_snake(&self, ui: &mut egui::Ui) {
        let cell = ((ui.available_height() - 46.) / self.snake.height as f32)
            .min((ui.available_width() - 12.) / self.snake.width as f32)
            .clamp(7., 26.);
        let size = egui::vec2(
            cell * self.snake.width as f32,
            cell * self.snake.height as f32,
        );
        ui.vertical_centered(|ui| {
            let (rect, p) = board(ui, size);
            for c in &self.snake.obstacles {
                block(&p, rect, cell, *c, PALETTE[8]);
            }
            if let Some(food) = self.snake.food {
                p.circle_filled(
                    rect.min
                        + egui::vec2((food.x as f32 + 0.5) * cell, (food.y as f32 + 0.5) * cell),
                    cell * 0.32,
                    PALETTE[5],
                );
            }
            for (i, c) in self.snake.body.iter().enumerate().rev() {
                block(
                    &p,
                    rect,
                    cell,
                    *c,
                    if i == 0 {
                        Color32::from_rgb(170, 245, 208)
                    } else {
                        PALETTE[4]
                    },
                );
            }
            overlay(&p, rect, self.snake.status, "空格开始 / 继续");
            ui.add_space(8.);
            ui.label(
                RichText::new(if self.snake.level == 0 {
                    "方向键 / WASD · 空格暂停 · Esc 暂停 · 无尽模式可穿越边界"
                } else {
                    "方向键 / WASD · 空格暂停 · 吃满目标过关 · 避开边界、蛇身和障碍"
                })
                .small()
                .color(muted()),
            );
        });
    }
    pub(in crate::app) fn draw_tetris(&self, ui: &mut egui::Ui) {
        let g = &self.games.tetris;
        let cell = ((ui.available_height() - 43.) / tetris::HEIGHT as f32)
            .min((ui.available_width() - 170.) / tetris::WIDTH as f32)
            .clamp(7., 27.);
        let size = egui::vec2(cell * tetris::WIDTH as f32, cell * tetris::HEIGHT as f32);
        let left = ((ui.available_width() - size.x - 145.) / 2.).max(0.);
        ui.horizontal_top(|ui| {
            ui.add_space(left);
            let (rect, p) = board(ui, size);
            for x in 1..tetris::WIDTH {
                p.line_segment(
                    [
                        rect.min + egui::vec2(x as f32 * cell, 0.),
                        rect.min + egui::vec2(x as f32 * cell, size.y),
                    ],
                    egui::Stroke::new(0.5, Color32::from_rgb(39, 48, 65)),
                );
            }
            for y in 1..tetris::HEIGHT {
                p.line_segment(
                    [
                        rect.min + egui::vec2(0., y as f32 * cell),
                        rect.min + egui::vec2(size.x, y as f32 * cell),
                    ],
                    egui::Stroke::new(0.5, Color32::from_rgb(39, 48, 65)),
                );
            }
            for (y, row) in g.board.iter().enumerate() {
                for (x, v) in row.iter().enumerate() {
                    if *v > 0 {
                        block(
                            &p,
                            rect,
                            cell,
                            Cell {
                                x: x as i32,
                                y: y as i32,
                            },
                            PALETTE[*v as usize],
                        );
                    }
                }
            }
            if !ended(g.status) {
                for c in g.ghost().cells() {
                    block(
                        &p,
                        rect,
                        cell,
                        c,
                        PALETTE[g.active.kind as usize + 1].gamma_multiply(0.22),
                    );
                }
                for c in g.active.cells() {
                    block(&p, rect, cell, c, PALETTE[g.active.kind as usize + 1]);
                }
            }
            overlay(&p, rect, g.status, "Enter 开始 / P 继续");
            ui.add_space(12.);
            ui.vertical(|ui| {
                ui.set_width(120.);
                ui.label(RichText::new("暂存 · C").strong());
                if let Some(kind) = g.hold {
                    preview_piece(ui, kind, 16.);
                } else {
                    ui.label("—");
                }
                ui.add_space(12.);
                ui.label(RichText::new("接下来").strong());
                for kind in g.next.iter().take(3) {
                    preview_piece(ui, *kind, 16.);
                    ui.add_space(6.);
                }
                ui.add_space(8.);
                ui.label(
                    RichText::new(
                        "← → 移动\n↑ / X 旋转\nZ 反向旋转\n↓ 软降\n空格 硬降\nP / Enter 暂停",
                    )
                    .small()
                    .color(muted()),
                );
            });
        });
    }
}
