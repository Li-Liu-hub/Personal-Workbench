//! Game rules are independent of egui, file operations and account data.
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
pub mod records;
pub mod tetris;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}
impl Direction {
    fn delta(self) -> (i32, i32) {
        match self {
            Self::Up => (0, -1),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Ready,
    Running,
    Paused,
    Over,
    Filled,
    Cleared,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Snake {
    pub width: i32,
    pub height: i32,
    pub body: VecDeque<Cell>,
    pub food: Option<Cell>,
    pub direction: Direction,
    pub status: Status,
    pub score: u32,
    #[serde(default)]
    pub level: u8,
    #[serde(default)]
    pub obstacles: Vec<Cell>,
    pending: VecDeque<Direction>,
    random: u64,
    elapsed: f64,
}
impl Snake {
    pub fn new(seed: u64) -> Self {
        let mut game = Self {
            width: 28,
            height: 20,
            body: VecDeque::from([
                Cell { x: 8, y: 10 },
                Cell { x: 7, y: 10 },
                Cell { x: 6, y: 10 },
            ]),
            food: None,
            direction: Direction::Right,
            status: Status::Ready,
            score: 0,
            level: 0,
            obstacles: vec![],
            pending: VecDeque::new(),
            random: seed.max(1),
            elapsed: 0.0,
        };
        game.spawn_food();
        game
    }
    /// Level zero preserves the original endless, wraparound game.
    pub fn stage(seed: u64, level: u8) -> Self {
        let mut game = Self::new(seed);
        game.level = level.clamp(1, 12);
        game.obstacles = Self::walls(game.level);
        game.spawn_food();
        game
    }
    fn walls(level: u8) -> Vec<Cell> {
        let mut walls = Vec::new();
        if level >= 3 {
            for y in 3..8 {
                walls.push(Cell { x: 14, y });
            }
            for y in 13..17 {
                walls.push(Cell { x: 14, y });
            }
        }
        if level >= 6 {
            for x in 3..10 {
                walls.push(Cell { x, y: 5 });
            }
            for x in 19..25 {
                walls.push(Cell { x, y: 15 });
            }
        }
        if level >= 9 {
            for y in 12..17 {
                walls.push(Cell { x: 5, y });
            }
            for y in 3..8 {
                walls.push(Cell { x: 23, y });
            }
        }
        walls
    }
    pub fn goal(&self) -> u32 {
        if self.level == 0 {
            0
        } else {
            (4 + u32::from(self.level)) * 10
        }
    }
    pub fn interval(&self) -> f64 {
        if self.level == 0 {
            0.14
        } else {
            (0.20 - f64::from(self.level) * 0.011).max(0.068)
        }
    }
    pub fn valid(&self) -> bool {
        self.width == 28
            && self.height == 20
            && self.level <= 12
            && self.body.len() >= 3
            && self.body.len() <= 560
            && self.body.iter().all(|c| {
                (0..28).contains(&c.x) && (0..20).contains(&c.y) && !self.obstacles.contains(c)
            })
            && self
                .body
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                == self.body.len()
            && self.obstacles == Self::walls(self.level)
            && self.food.is_none_or(|c| {
                (0..28).contains(&c.x)
                    && (0..20).contains(&c.y)
                    && !self.body.contains(&c)
                    && !self.obstacles.contains(&c)
            })
            && self.pending.len() <= 2
            && self.elapsed.is_finite()
    }
    pub fn toggle(&mut self) {
        self.status = match self.status {
            Status::Ready | Status::Paused => Status::Running,
            Status::Running => Status::Paused,
            other => other,
        };
        self.elapsed = 0.0;
    }
    pub fn pause(&mut self) {
        if self.status == Status::Running {
            self.status = Status::Paused;
        }
        self.elapsed = 0.0;
    }
    pub fn turn(&mut self, direction: Direction) {
        if self.status != Status::Running || self.pending.len() >= 2 {
            return;
        }
        let previous = self.pending.back().copied().unwrap_or(self.direction);
        let (x, y) = previous.delta();
        let (a, b) = direction.delta();
        if direction != previous && (x + a, y + b) != (0, 0) {
            self.pending.push_back(direction);
        }
    }
    pub fn advance(&mut self, seconds: f64) {
        if self.status != Status::Running {
            self.elapsed = 0.0;
            return;
        }
        self.elapsed += seconds.clamp(0.0, 0.25);
        while self.elapsed >= self.interval() && self.status == Status::Running {
            self.elapsed -= self.interval();
            self.step();
        }
    }
    pub fn step(&mut self) {
        if self.status != Status::Running {
            return;
        }
        if let Some(direction) = self.pending.pop_front() {
            self.direction = direction;
        }
        let head = self.body[0];
        let (x, y) = self.direction.delta();
        let mut next = Cell {
            x: head.x + x,
            y: head.y + y,
        };
        if self.level == 0 {
            next.x = next.x.rem_euclid(self.width);
            next.y = next.y.rem_euclid(self.height);
        } else if !(0..self.width).contains(&next.x) || !(0..self.height).contains(&next.y) {
            self.status = Status::Over;
            return;
        }
        let eating = self.food == Some(next);
        let occupied = self.body.len() - usize::from(!eating);
        if self.obstacles.contains(&next)
            || self.body.iter().take(occupied).any(|cell| *cell == next)
        {
            self.status = Status::Over;
            return;
        }
        self.body.push_front(next);
        if eating {
            self.score += 10;
            if self.level > 0 && self.score >= self.goal() {
                self.status = Status::Cleared;
                self.food = None;
                return;
            }
            self.spawn_food();
        } else {
            self.body.pop_back();
        }
    }
    fn spawn_food(&mut self) {
        let free = (0..self.height)
            .flat_map(|y| (0..self.width).map(move |x| Cell { x, y }))
            .filter(|cell| !self.body.contains(cell) && !self.obstacles.contains(cell))
            .collect::<Vec<_>>();
        if free.is_empty() {
            self.food = None;
            self.status = Status::Filled;
            return;
        }
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        self.food = Some(free[(self.random % free.len() as u64) as usize]);
    }
}

pub fn random_game() -> Snake {
    Snake::new(uuid::Uuid::new_v4().as_u128() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn campaign_has_obstacles_goals_and_walls_while_endless_still_wraps() {
        for level in 1..=12 {
            for seed in 1..=12 {
                let g = Snake::stage(seed, level);
                assert!(g.valid());
                assert_eq!(g.goal(), (4 + u32::from(level)) * 10);
            }
        }
        let mut g = Snake::stage(2, 1);
        g.status = Status::Running;
        g.score = g.goal() - 10;
        g.food = Some(Cell { x: 9, y: 10 });
        g.step();
        assert_eq!(g.status, Status::Cleared);
        assert!(g.valid());
        let mut g = Snake::stage(2, 1);
        g.toggle();
        g.body = VecDeque::from([
            Cell { x: 27, y: 4 },
            Cell { x: 26, y: 4 },
            Cell { x: 25, y: 4 },
        ]);
        g.step();
        assert_eq!(g.status, Status::Over);
        let mut g = Snake::stage(2, 3);
        g.toggle();
        g.body = VecDeque::from([
            Cell { x: 13, y: 4 },
            Cell { x: 12, y: 4 },
            Cell { x: 11, y: 4 },
        ]);
        g.step();
        assert_eq!(g.status, Status::Over);
        assert!(Snake::stage(1, 12).interval() < Snake::stage(1, 1).interval());
    }
    #[test]
    fn endless_snake_wraps_eats_grows_and_pauses_without_a_level_timer() {
        let mut g = Snake::new(7);
        let initial = g.body.clone();
        g.advance(1.0);
        assert_eq!(g.body, initial);
        g.toggle();
        g.body = VecDeque::from([
            Cell { x: 27, y: 4 },
            Cell { x: 26, y: 4 },
            Cell { x: 25, y: 4 },
        ]);
        g.food = Some(Cell { x: 0, y: 4 });
        g.step();
        assert_eq!(g.score, 10);
        assert_eq!(g.body.len(), 4);
        assert_eq!(g.body[0], Cell { x: 0, y: 4 });
        assert!(!g.body.contains(&g.food.unwrap()));
        g.pause();
        let paused = g.body.clone();
        g.advance(50.0);
        assert_eq!(g.body, paused);
        g.toggle();
        g.food = Some(Cell { x: 0, y: 0 });
        for _ in 0..2000 {
            g.step();
        }
        assert_eq!(g.status, Status::Running);
    }
    #[test]
    fn queued_turns_reject_reverse_and_self_collision_ends_only_the_current_run() {
        let mut g = Snake::new(3);
        g.toggle();
        g.turn(Direction::Left);
        assert!(g.pending.is_empty());
        g.turn(Direction::Up);
        g.turn(Direction::Left);
        g.step();
        assert_eq!(g.direction, Direction::Up);
        g.step();
        assert_eq!(g.direction, Direction::Left);
        g.body = VecDeque::from([
            Cell { x: 2, y: 2 },
            Cell { x: 2, y: 3 },
            Cell { x: 3, y: 3 },
            Cell { x: 3, y: 2 },
            Cell { x: 3, y: 1 },
        ]);
        g.direction = Direction::Right;
        g.step();
        assert_eq!(g.status, Status::Over);
        let fresh = Snake::new(8);
        assert_eq!(fresh.score, 0);
        assert_eq!(fresh.status, Status::Ready);
    }
    #[test]
    fn a_vacating_tail_is_valid_and_a_full_board_does_not_loop_forever() {
        let mut g = Snake::new(2);
        g.status = Status::Running;
        g.width = 2;
        g.height = 2;
        g.body = VecDeque::from([
            Cell { x: 0, y: 0 },
            Cell { x: 0, y: 1 },
            Cell { x: 1, y: 1 },
            Cell { x: 1, y: 0 },
        ]);
        g.food = None;
        g.step();
        assert_eq!(g.status, Status::Running);
        g.spawn_food();
        assert_eq!(g.status, Status::Filled);
        assert!(g.food.is_none());
    }
}
