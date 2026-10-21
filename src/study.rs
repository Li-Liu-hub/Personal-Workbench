use crate::error::{DeskError, Result};
use serde::{Deserialize, Serialize};

pub const TOPICS: [&str; 7] = [
    "场景题",
    "八股题",
    "SQL 命令",
    "Linux 命令",
    "Bug 排查",
    "性能调优",
    "项目修 Bug",
];
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rubric {
    pub id: String,
    pub text: String,
    pub points: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub topic: String,
    pub prompt: String,
    pub answer: String,
    pub rubric: Vec<Rubric>,
    pub source: String,
    #[serde(default)]
    pub phonetic: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql: Option<crate::sql_practice::Lab>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}
impl Card {
    pub fn validate(&self) -> Result<()> {
        if let Some(sql) = &self.sql {
            sql.validate().map_err(DeskError::InvalidPath)?;
        }
        if !matches!(self.kind.as_str(), "engineering" | "word")
            || self.id.is_empty()
            || self.title.trim().is_empty()
            || self.answer.trim().is_empty()
            || self.rubric.is_empty()
            || self.rubric.iter().map(|r| r.points as u32).sum::<u32>() != 100
        {
            return Err(DeskError::InvalidPath(
                "题目需要标题、参考答案及合计 100 分的评分点".into(),
            ));
        }
        let unique = self
            .rubric
            .iter()
            .map(|r| &r.id)
            .collect::<std::collections::HashSet<_>>();
        if unique.len() != self.rubric.len()
            || self
                .rubric
                .iter()
                .any(|r| r.id.is_empty() || r.text.is_empty() || r.points == 0)
        {
            return Err(DeskError::InvalidPath("评分点编号不能重复或留空".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum MasteredMode {
    #[default]
    Rare,
    Stop,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rules {
    pub threshold: u8,
    pub consecutive: u32,
    pub mastered: MasteredMode,
    pub daily_words: u32,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            threshold: 85,
            consecutive: 5,
            mastered: MasteredMode::Rare,
            daily_words: 20,
        }
    }
}
impl Rules {
    pub fn validate(&self) -> Result<()> {
        if self.threshold == 0
            || self.threshold > 100
            || self.consecutive == 0
            || self.consecutive > 20
            || self.daily_words == 0
            || self.daily_words > 200
        {
            return Err(DeskError::InvalidPath("掌握规则超出允许范围".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Progress {
    pub card_id: String,
    pub attempts: u32,
    pub streak: u32,
    pub mastered: bool,
    pub latest: u8,
}
impl Progress {
    pub fn from_scores(id: String, scores: &[u8], rules: &Rules) -> Self {
        let streak = scores
            .iter()
            .rev()
            .take_while(|&&score| score >= rules.threshold)
            .count() as u32;
        Self {
            card_id: id,
            attempts: scores.len() as u32,
            streak,
            mastered: streak >= rules.consecutive,
            latest: scores.last().copied().unwrap_or(0),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Check {
    pub id: String,
    pub earned: u8,
    pub reason: String,
    pub evidence: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grade {
    pub checks: Vec<Check>,
    pub summary: String,
    #[serde(default)]
    pub weak_points: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql: Option<crate::sql_practice::Report>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<crate::challenges::Report>,
}
impl Grade {
    /// A model cannot invent a final score: validate every rubric result and sum it here.
    pub fn completion(&self, card: &Card, answer: &str) -> Result<u8> {
        if self.checks.len() != card.rubric.len() {
            return Err(DeskError::InvalidPath(
                "模型没有逐项完成评分，请重试".into(),
            ));
        }
        let mut total = 0u16;
        let mut ids = std::collections::HashSet::new();
        for check in &self.checks {
            let r = card
                .rubric
                .iter()
                .find(|r| r.id == check.id)
                .ok_or_else(|| DeskError::InvalidPath("模型返回了未知评分点".into()))?;
            if !ids.insert(&check.id) || check.earned > r.points || check.reason.trim().is_empty() {
                return Err(DeskError::InvalidPath("模型评分格式无效，请重试".into()));
            }
            if check.earned > 0
                && (check.evidence.trim().is_empty()
                    || !answer
                        .to_lowercase()
                        .contains(&check.evidence.to_lowercase()))
            {
                return Err(DeskError::InvalidPath(
                    "模型给分缺少答案原文依据，请重试".into(),
                ));
            }
            total += check.earned as u16;
        }
        Ok(total as u8)
    }
    pub fn unknown(card: &Card) -> Self {
        Self {
            checks: card
                .rubric
                .iter()
                .map(|r| Check {
                    id: r.id.clone(),
                    earned: 0,
                    reason: "本次未能作答".into(),
                    evidence: String::new(),
                })
                .collect(),
            summary: "已记入记录本，下次继续练习。".into(),
            weak_points: vec![],
            sql: None,
            project: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attempt {
    pub id: String,
    pub card_id: String,
    pub number: u32,
    pub at: String,
    pub answer: String,
    pub completion: u8,
    pub feedback: Grade,
    pub model: String,
    pub elapsed_ms: u64,
    pub question: Card,
    pub rules: Rules,
    #[serde(default)]
    pub assisted: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotebookEntry {
    pub card: Card,
    pub progress: Progress,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pending {
    pub id: String,
    pub card_id: String,
    pub answer: String,
    pub at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bundle {
    pub version: u32,
    pub cards: Vec<Card>,
    #[serde(default)]
    pub attempts: Vec<Attempt>,
    #[serde(default)]
    pub dialogue: Vec<SavedDialogue>,
    #[serde(default)]
    pub projects: Vec<crate::challenges::Saved>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedDialogue {
    pub card_id: String,
    pub message: crate::local_ai::Dialogue,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn five_successes_are_consecutive_and_a_lapse_restores_the_question() {
        let r = Rules::default();
        assert!(!Progress::from_scores("x".into(), &[100, 100, 100, 100], &r).mastered);
        assert!(Progress::from_scores("x".into(), &[85, 90, 95, 100, 85], &r).mastered);
        assert!(!Progress::from_scores("x".into(), &[100, 100, 40, 100, 100, 100], &r).mastered);
        assert!(!Progress::from_scores("x".into(), &[100, 100, 100, 100, 100, 20], &r).mastered);
    }
}
