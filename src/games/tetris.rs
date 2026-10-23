//! Small deterministic falling-block engine. No rendering, timers or database handles.
use super::{Cell, Status};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const WIDTH: i32 = 10;
pub const HEIGHT: i32 = 20;
const SHAPES: [[(i32, i32); 4]; 7] = [
    [(0, 1), (1, 1), (2, 1), (3, 1)],
    [(1, 0), (2, 0), (1, 1), (2, 1)],
    [(1, 0), (0, 1), (1, 1), (2, 1)],
    [(1, 0), (2, 0), (0, 1), (1, 1)],
    [(0, 0), (1, 0), (1, 1), (2, 1)],
    [(0, 0), (0, 1), (1, 1), (2, 1)],
    [(2, 0), (0, 1), (1, 1), (2, 1)],
];

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Piece {
    pub kind: u8,
    pub rotation: u8,
    pub x: i32,
    pub y: i32,
}
impl Piece {
    pub fn cells(self) -> [Cell; 4] {
        let mut cells = [Cell { x: 0, y: 0 }; 4];
        for (i, (mut x, mut y)) in SHAPES[self.kind as usize].iter().copied().enumerate() {
            if self.kind != 1 {
                let edge = if self.kind == 0 { 3 } else { 2 };
                for _ in 0..self.rotation {
                    (x, y) = (edge - y, x);
                }
            }
            cells[i] = Cell {
                x: self.x + x,
                y: self.y + y,
            };
        }
        cells
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Tetris {
    pub board: Vec<Vec<u8>>,
    pub active: Piece,
    pub next: VecDeque<u8>,
    pub hold: Option<u8>,
    pub held: bool,
    pub status: Status,
    /// Zero is marathon; 1..=12 are independent, progressively faster stages.
    pub stage: u8,
    pub score: u32,
    pub lines: u32,
    pub combo: i32,
    bag: Vec<u8>,
    random: u64,
    elapsed: f64,
}
impl Tetris {
    pub fn new(seed: u64, stage: u8) -> Self {
        let mut g = Self {
            board: vec![vec![0; WIDTH as usize]; HEIGHT as usize],
            active: Piece {
                kind: 0,
                rotation: 0,
                x: 3,
                y: -1,
            },
            next: VecDeque::new(),
            hold: None,
            held: false,
            status: Status::Ready,
            stage: stage.min(12),
            score: 0,
            lines: 0,
            combo: -1,
            bag: vec![],
            random: seed.max(1),
            elapsed: 0.,
        };
        for _ in 0..5 {
            let kind = g.draw();
            g.next.push_back(kind);
        }
        g.spawn();
        // Later stages start with a few perforated rows, never a full row.
        if g.stage >= 4 {
            let count = usize::from((g.stage - 2) / 2).min(5);
            for y in (HEIGHT as usize - count)..HEIGHT as usize {
                let hole = (g.rng() % WIDTH as u64) as usize;
                for x in 0..WIDTH as usize {
                    if x != hole {
                        g.board[y][x] = 8;
                    }
                }
            }
        }
        g
    }
    fn rng(&mut self) -> u64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        self.random
    }
    fn draw(&mut self) -> u8 {
        if self.bag.is_empty() {
            self.bag = (0..7).collect();
            for i in (1..7).rev() {
                let j = (self.rng() % (i + 1) as u64) as usize;
                self.bag.swap(i, j);
            }
        }
        self.bag.pop().unwrap()
    }
    pub fn speed_level(&self) -> u32 {
        if self.stage == 0 {
            1 + self.lines / 10
        } else {
            u32::from(self.stage)
        }
    }
    pub fn goal(&self) -> u32 {
        if self.stage == 0 {
            0
        } else {
            4 + u32::from(self.stage) * 2
        }
    }
    pub fn interval(&self) -> f64 {
        (0.8 * 0.84_f64.powi(self.speed_level().min(20) as i32 - 1)).max(0.055)
    }
    pub fn toggle(&mut self) {
        self.status = match self.status {
            Status::Ready | Status::Paused => Status::Running,
            Status::Running => Status::Paused,
            other => other,
        };
        self.elapsed = 0.;
    }
    pub fn pause(&mut self) {
        if self.status == Status::Running {
            self.status = Status::Paused;
        }
        self.elapsed = 0.;
    }
    pub fn fits(&self, piece: Piece) -> bool {
        piece.cells().iter().all(|c| {
            c.x >= 0
                && c.x < WIDTH
                && c.y >= -2
                && c.y < HEIGHT
                && (c.y < 0 || self.board[c.y as usize][c.x as usize] == 0)
        })
    }
    pub fn shift(&mut self, dx: i32) {
        if self.status != Status::Running {
            return;
        }
        let p = Piece {
            x: self.active.x + dx,
            ..self.active
        };
        if self.fits(p) {
            self.active = p;
        }
    }
    pub fn rotate(&mut self, clockwise: bool) {
        if self.status != Status::Running {
            return;
        }
        let rotation = (self.active.rotation + if clockwise { 1 } else { 3 }) % 4;
        // Simple wall/floor kicks, deliberately independent of any branded rotation system.
        for (dx, dy) in [(0, 0), (-1, 0), (1, 0), (-2, 0), (2, 0), (0, -1), (0, -2)] {
            let p = Piece {
                rotation,
                x: self.active.x + dx,
                y: self.active.y + dy,
                ..self.active
            };
            if self.fits(p) {
                self.active = p;
                break;
            }
        }
    }
    pub fn ghost(&self) -> Piece {
        let mut p = self.active;
        while self.fits(Piece { y: p.y + 1, ..p }) {
            p.y += 1;
        }
        p
    }
    pub fn hard_drop(&mut self) {
        if self.status != Status::Running {
            return;
        }
        let p = self.ghost();
        self.score = self
            .score
            .saturating_add((p.y - self.active.y).max(0) as u32 * 2);
        self.active = p;
        self.lock();
    }
    pub fn soft_drop(&mut self) {
        if self.status == Status::Running && self.down() {
            self.score = self.score.saturating_add(1);
        }
    }
    fn down(&mut self) -> bool {
        let p = Piece {
            y: self.active.y + 1,
            ..self.active
        };
        if self.fits(p) {
            self.active = p;
            true
        } else {
            self.lock();
            false
        }
    }
    pub fn swap_hold(&mut self) {
        if self.status != Status::Running || self.held {
            return;
        }
        let current = self.active.kind;
        match self.hold {
            None => self.spawn(),
            Some(kind) => {
                self.active = Piece {
                    kind,
                    rotation: 0,
                    x: 3,
                    y: -1,
                };
                if !self.fits(self.active) {
                    self.status = Status::Over;
                }
            }
        }
        self.hold = Some(current);
        self.held = true;
        self.elapsed = 0.;
    }
    fn spawn(&mut self) {
        let kind = self.next.pop_front().unwrap();
        let next = self.draw();
        self.next.push_back(next);
        self.active = Piece {
            kind,
            rotation: 0,
            x: 3,
            y: -1,
        };
        self.held = false;
        self.elapsed = 0.;
        if !self.fits(self.active) {
            self.status = Status::Over;
        }
    }
    fn lock(&mut self) {
        if self.active.cells().iter().any(|c| c.y < 0) {
            self.status = Status::Over;
            return;
        }
        for c in self.active.cells() {
            self.board[c.y as usize][c.x as usize] = self.active.kind + 1;
        }
        let before = self.board.len();
        self.board.retain(|row| row.contains(&0));
        let cleared = before - self.board.len();
        while self.board.len() < HEIGHT as usize {
            self.board.insert(0, vec![0; WIDTH as usize]);
        }
        if cleared > 0 {
            self.combo += 1;
            let base = [0, 100, 300, 500, 800][cleared];
            self.score = self
                .score
                .saturating_add((base + 50 * self.combo.max(0) as u32) * self.speed_level());
            self.lines += cleared as u32;
        } else {
            self.combo = -1;
        }
        if self.stage > 0 && self.lines >= self.goal() {
            self.status = Status::Cleared;
            return;
        }
        self.spawn();
    }
    pub fn advance(&mut self, delta: f64) {
        if self.status != Status::Running {
            self.elapsed = 0.;
            return;
        }
        self.elapsed += delta.clamp(0., 0.25);
        while self.elapsed >= self.interval() && self.status == Status::Running {
            self.elapsed -= self.interval();
            self.down();
        }
    }
    pub fn valid(&self) -> bool {
        self.board.len() == HEIGHT as usize
            && self
                .board
                .iter()
                .all(|r| r.len() == WIDTH as usize && r.iter().all(|v| *v <= 8))
            && self.active.kind < 7
            && self.active.rotation < 4
            && (-4..=10).contains(&self.active.x)
            && (-2..20).contains(&self.active.y)
            && self.next.len() == 5
            && self.next.iter().all(|k| *k < 7)
            && self.hold.is_none_or(|k| k < 7)
            && self.bag.len() <= 7
            && self.bag.iter().all(|k| *k < 7)
            && self.stage <= 12
            && self.elapsed.is_finite()
            && (matches!(self.status, Status::Over | Status::Cleared) || self.fits(self.active))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bags_hold_ghost_and_pause_survive_roundtrip() {
        let mut g = Tetris::new(7, 0);
        let mut first = vec![g.active.kind];
        first.extend(g.next.iter().copied());
        assert_eq!(
            first.iter().collect::<std::collections::HashSet<_>>().len(),
            6
        );
        g.toggle();
        let kind = g.active.kind;
        g.swap_hold();
        assert_eq!(g.hold, Some(kind));
        let held = g.active;
        g.swap_hold();
        assert_eq!(held, g.active);
        let ghost = g.ghost();
        assert!(g.fits(ghost));
        assert!(!g.fits(Piece {
            y: ghost.y + 1,
            ..ghost
        }));
        g.hard_drop();
        assert!(!g.held);
        assert!(g.board.iter().flatten().any(|v| *v > 0));
        g.pause();
        let json = serde_json::to_string(&g).unwrap();
        let mut restored: Tetris = serde_json::from_str(&json).unwrap();
        assert!(restored.valid());
        restored.advance(100.);
        assert_eq!(restored.active, g.active);
    }
    #[test]
    fn four_lines_clear_and_a_stage_completes_without_corrupting_the_board() {
        let mut g = Tetris::new(2, 1);
        g.toggle();
        for row in &mut g.board[16..] {
            row.fill(1);
            row[5] = 0;
        }
        g.active = Piece {
            kind: 0,
            rotation: 1,
            x: 3,
            y: 16,
        };
        g.hard_drop();
        assert_eq!(g.lines, 4);
        assert!(g.score >= 800);
        assert!(g.board.iter().flatten().all(|v| *v == 0));
        g.lines = g.goal() - 1;
        g.board[19].fill(1);
        g.board[19][4] = 0;
        g.board[19][5] = 0;
        g.active = Piece {
            kind: 1,
            rotation: 0,
            x: 3,
            y: 17,
        };
        g.hard_drop();
        assert_eq!(g.status, Status::Cleared);
        assert!(g.valid());
    }
    #[test]
    fn blocked_spawn_and_rotation_never_write_outside_board() {
        let mut g = Tetris::new(9, 12);
        g.toggle();
        for _ in 0..20 {
            g.shift(-1);
            g.rotate(true);
        }
        assert!(g.fits(g.active));
        g.board[0].fill(1);
        g.active = Piece {
            kind: 1,
            rotation: 0,
            x: 3,
            y: -1,
        };
        g.hard_drop();
        assert_eq!(g.status, Status::Over);
        for stage in 0..=12 {
            assert!(Tetris::new(24, stage).valid());
        }
    }
}
