//! Game rules are independent of egui, file operations and account data.
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ready,
    Running,
    Paused,
    Over,
    Filled,
}

pub struct Snake {
    pub width: i32,
    pub height: i32,
    pub body: VecDeque<Cell>,
    pub food: Option<Cell>,
    pub direction: Direction,
    pub status: Status,
    pub score: u32,
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
            pending: VecDeque::new(),
            random: seed.max(1),
            elapsed: 0.0,
        };
        game.spawn_food();
        game
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
        while self.elapsed >= 0.14 && self.status == Status::Running {
            self.elapsed -= 0.14;
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
        let next = Cell {
            x: (head.x + x).rem_euclid(self.width),
            y: (head.y + y).rem_euclid(self.height),
        };
        let eating = self.food == Some(next);
        let occupied = self.body.len() - usize::from(!eating);
        if self.body.iter().take(occupied).any(|cell| *cell == next) {
            self.status = Status::Over;
            return;
        }
        self.body.push_front(next);
        if eating {
            self.score += 10;
            self.spawn_food();
        } else {
            self.body.pop_back();
        }
    }
    fn spawn_food(&mut self) {
        let free = (0..self.height)
            .flat_map(|y| (0..self.width).map(move |x| Cell { x, y }))
            .filter(|cell| !self.body.contains(cell))
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
