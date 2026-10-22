//! Personal backend learning route. Profiles classify legacy cards without rewriting them.
use crate::study::{Card, MasteredMode, Rules};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::OnceLock};

pub const AREAS: [&str; 8] = [
    "编程与算法",
    "Java 与并发",
    "数据库",
    "操作系统与 Linux",
    "网络与 Web",
    "工程可靠性",
    "测试与排障",
    "性能与交付",
];
pub const MIXED: &str = "__free_mixed__";
#[cfg(test)]
pub const BUILTIN_COUNT: u32 = 70;
pub fn with_profile(mut card: Card) -> Card {
    if card.kind == "engineering" && card.training.is_none() {
        card.training = Some(profile(&card));
    }
    card
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Training {
    pub area: String,
    pub level: u8,
    pub goal: String,
    pub deliverable: String,
}
impl Training {
    pub fn validate(&self) -> Result<(), String> {
        if !AREAS.contains(&self.area.as_str())
            || !(1..=3).contains(&self.level)
            || self.goal.trim().is_empty()
            || self.deliverable.trim().is_empty()
            || self.goal.chars().count() > 400
            || self.deliverable.chars().count() > 800
        {
            return Err("训练题需要有效能力方向、1～3 级难度、练习目标及作答要求".into());
        }
        Ok(())
    }
}
pub fn level_name(level: u8) -> &'static str {
    match level {
        1 => "基础理解",
        2 => "应用实现",
        _ => "综合进阶",
    }
}
pub fn category(topic: &str) -> &str {
    match topic {
        "场景题" => "工程设计",
        "SQL 命令" => "SQL 训练",
        "Bug 排查" | "性能调优" => "排障与调优",
        "项目修 Bug" => "项目实战",
        other => other,
    }
}
#[derive(Deserialize)]
struct Profile {
    id: String,
    #[serde(flatten)]
    training: Training,
}
pub fn profile(card: &Card) -> Training {
    if let Some(t) = &card.training {
        return t.clone();
    }
    static PROFILES: OnceLock<BTreeMap<String, Training>> = OnceLock::new();
    let known = PROFILES.get_or_init(|| {
        serde_json::from_str::<Vec<Profile>>(include_str!("../assets/curriculum_profiles.json"))
            .expect("validated built-in profiles")
            .into_iter()
            .map(|p| (p.id, p.training))
            .collect()
    });
    if let Some(t) = known.get(&card.id) {
        return t.clone();
    }
    Training {
        area: match category(&card.topic) {
            "SQL 训练" => AREAS[2],
            "Linux 命令" => AREAS[3],
            "八股题" => AREAS[1],
            "代码实战" => AREAS[0],
            "排障与调优" => AREAS[6],
            _ => AREAS[5],
        }
        .into(),
        level: if card.project.is_some() { 3 } else { 2 },
        goal: format!("围绕“{}”解释原理并完成本题要求", card.title),
        deliverable: "依据题干提交思路、代码或命令，说明边界与验证方法；参照本题评分点。".into(),
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Filter {
    #[serde(default)]
    pub area: String,
    #[serde(default)]
    pub level: u8,
}
impl Filter {
    pub fn matches(&self, t: &Training) -> bool {
        (self.area.is_empty() || self.area == t.area) && (self.level == 0 || self.level == t.level)
    }
}
#[derive(Clone)]
pub struct Candidate {
    pub card: Card,
    pub training: Training,
    pub independent: u32,
    pub latest: Option<u8>,
    pub mastered: bool,
}
pub struct Choice {
    pub index: usize,
    pub reason: String,
}
#[derive(Clone, Default)]
pub struct AreaProgress {
    pub area: String,
    pub total: usize,
    pub practiced: usize,
    pub mastered: usize,
    pub diagnosed: bool,
    pub foundation: usize,
    pub application: usize,
    pub stage: u8,
}
pub fn route(items: &[Candidate], rules: &Rules) -> Vec<AreaProgress> {
    AREAS
        .iter()
        .map(|area| {
            let rows: Vec<_> = items.iter().filter(|r| r.training.area == *area).collect();
            let passed = |level| {
                rows.iter()
                    .filter(|r| {
                        r.training.level == level && r.latest.is_some_and(|s| s >= rules.threshold)
                    })
                    .count()
            };
            let foundation = passed(1);
            let application = passed(2);
            AreaProgress {
                area: (*area).into(),
                total: rows.len(),
                practiced: rows.iter().filter(|r| r.independent > 0).count(),
                mastered: rows.iter().filter(|r| r.mastered).count(),
                diagnosed: rows
                    .iter()
                    .any(|r| r.training.level == 1 && r.independent > 0),
                foundation,
                application,
                stage: if foundation < 2 {
                    1
                } else if application < 2 {
                    2
                } else {
                    3
                },
            }
        })
        .collect()
}
pub fn choose(
    items: &[Candidate],
    topic: &str,
    filter: &Filter,
    recent: &[String],
    rules: &Rules,
    roll: u64,
) -> Option<Choice> {
    let progress = route(items, rules);
    let guided = topic.is_empty();
    let mut eligible: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            filter.matches(&r.training)
                && (topic.is_empty()
                    || topic == MIXED
                    || category(&r.card.topic) == category(topic))
                && !(r.mastered && rules.mastered == MasteredMode::Stop)
        })
        .map(|(i, _)| i)
        .collect();
    let mut diagnostic = false;
    if guided {
        eligible.retain(|&i| {
            let r = &items[i];
            let stage = progress
                .iter()
                .find(|p| p.area == r.training.area)
                .map(|p| p.stage)
                .unwrap_or(1);
            let ready = r.training.level <= stage;
            let project_ready = r.card.project.is_none()
                || [AREAS[1], AREAS[2]]
                    .iter()
                    .all(|area| progress.iter().any(|p| p.area == *area && p.stage >= 2));
            ready && project_ready
        });
        // Diagnose one basic item in every direction before repeatedly focusing one weak area.
        if let Some(area) = progress
            .iter()
            .filter(|p| {
                !p.diagnosed
                    && eligible
                        .iter()
                        .any(|&i| items[i].training.area == p.area && items[i].training.level == 1)
            })
            .min_by_key(|p| {
                recent
                    .iter()
                    .filter(|id| {
                        items
                            .iter()
                            .any(|r| r.card.id == **id && r.training.area == p.area)
                    })
                    .count()
            })
        {
            eligible
                .retain(|&i| items[i].training.area == area.area && items[i].training.level == 1);
            diagnostic = true;
        }
    }
    if eligible
        .iter()
        .any(|&i| !recent.contains(&items[i].card.id))
    {
        eligible.retain(|&i| !recent.contains(&items[i].card.id));
    }
    if eligible.is_empty() {
        return None;
    }
    // Pick a direction before a card; a large SQL bank must not crowd out other abilities.
    let areas: Vec<_> = AREAS
        .iter()
        .filter(|area| eligible.iter().any(|&i| items[i].training.area == **area))
        .collect();
    let selected_area = areas[(roll as usize) % areas.len()];
    eligible.retain(|&i| items[i].training.area == *selected_area);
    let weights: Vec<u64> = eligible
        .iter()
        .map(|&i| {
            let r = &items[i];
            if r.mastered {
                1
            } else if r.latest.is_some_and(|s| s < rules.threshold) {
                150
            } else if r.independent == 0 {
                100
            } else {
                60
            }
        })
        .collect();
    let mut draw = roll.rotate_left(23) % weights.iter().sum::<u64>();
    for (index, weight) in eligible.into_iter().zip(weights) {
        if draw < weight {
            let r = &items[index];
            let reason = if !guided {
                "专项练习：按当前筛选抽取".into()
            } else if diagnostic {
                format!("基础摸底：先了解你的{}能力", r.training.area)
            } else if r.mastered {
                "低频复习：检查已掌握知识能否独立应用".into()
            } else if r.latest.is_some_and(|s| s < rules.threshold) {
                format!(
                    "薄弱项复习：上次独立作答 {}%，先补齐这一点",
                    r.latest.unwrap()
                )
            } else if r.independent == 0 {
                format!("学习新题：{}已满足当前阶段的练习条件", r.training.area)
            } else {
                "巩固练习：继续积累独立作答记录".into()
            };
            return Some(Choice { index, reason });
        }
        draw -= weight;
    }
    None
}

#[cfg(test)]
mod tests;
