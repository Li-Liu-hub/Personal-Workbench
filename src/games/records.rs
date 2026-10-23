//! Versioned saves are separate from the rules and include immutable completed-run summaries.
use super::{Snake, Status, tetris::Tetris};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Saved<T> {
    pub run_id: String,
    pub game: T,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub run_id: String,
    pub game: String,
    pub stage: u8,
    pub score: u32,
    pub lines: u32,
    pub outcome: String,
    pub at: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Library {
    pub version: u8,
    pub snake: Option<Saved<Snake>>,
    pub tetris: Option<Saved<Tetris>>,
    pub snake_unlocked: u8,
    pub tetris_unlocked: u8,
    pub snake_best: u32,
    pub tetris_best: u32,
    pub records: Vec<Record>,
    #[serde(default)]
    pub stage_best: std::collections::BTreeMap<String, u32>,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            version: 1,
            snake: None,
            tetris: None,
            snake_unlocked: 1,
            tetris_unlocked: 1,
            snake_best: 0,
            tetris_best: 0,
            records: vec![],
            stage_best: Default::default(),
        }
    }
}
impl Library {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || !(1..=12).contains(&self.snake_unlocked)
            || !(1..=12).contains(&self.tetris_unlocked)
            || self.records.len() > 100
            || self.records.iter().any(|r| {
                !matches!(r.game.as_str(), "snake" | "tetris")
                    || r.stage > 12
                    || r.run_id.is_empty()
            })
            || self
                .snake
                .as_ref()
                .is_some_and(|s| s.run_id.is_empty() || !s.game.valid())
            || self
                .tetris
                .as_ref()
                .is_some_and(|s| s.run_id.is_empty() || !s.game.valid())
        {
            return Err("游戏存档格式不完整，原存档已保留".into());
        }
        Ok(())
    }
    pub fn record(&mut self, record: Record) {
        if self.records.iter().any(|r| r.run_id == record.run_id) {
            return;
        }
        let (best, unlocked) = if record.game == "snake" {
            (&mut self.snake_best, &mut self.snake_unlocked)
        } else {
            (&mut self.tetris_best, &mut self.tetris_unlocked)
        };
        *best = (*best).max(record.score);
        if record.outcome == "通关" && record.stage > 0 {
            *unlocked = (*unlocked).max((record.stage + 1).min(12));
            let best = self
                .stage_best
                .entry(format!("{}:{}", record.game, record.stage))
                .or_default();
            *best = (*best).max(record.score);
        }
        self.records.insert(0, record);
        self.records.truncate(100);
    }
}
pub fn outcome(status: Status) -> &'static str {
    match status {
        Status::Cleared | Status::Filled => "通关",
        Status::Over => "结束",
        _ => "重开",
    }
}
