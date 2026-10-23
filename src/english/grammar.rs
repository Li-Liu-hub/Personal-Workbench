//! Reviewed foundations are local: a small model grades, rather than inventing rules.
use super::{Card, Draft};
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Skill {
    skill: String,
    tasks: Vec<[String; 4]>,
}
fn bank() -> &'static [Skill] {
    static BANK: OnceLock<Vec<Skill>> = OnceLock::new();
    BANK.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/english_grammar.json"))
            .expect("validated grammar bank")
    })
}
pub fn grammar_card(sequence: u64) -> Result<Card, String> {
    let skills = bank();
    let index = sequence as usize % skills.len();
    let skill = &skills[index];
    let variant = sequence as usize / skills.len() % skill.tasks.len();
    let [form, prompt, reference, explanation] = &skill.tasks[variant];
    let base = Card {
        id: format!("grammar:skill:{index}"),
        kind: "word".into(),
        title: skill.skill.clone(),
        topic: "语法".into(),
        prompt: prompt.clone(),
        answer: reference.clone(),
        rubric: vec![],
        source: "内置基础语法题库 v1 · 题目与讲解本地保存".into(),
        knowledge_ids: vec![],
        tags: vec![],
        phonetic: String::new(),
        sql: None,
        origin: None,
        project: None,
        training: None,
    };
    let mut card = super::exercise(
        &base,
        "语法",
        Some(Draft {
            prompt: prompt.clone(),
            reference: reference.clone(),
            explanation: explanation.clone(),
        }),
    )?;
    card.id = format!("english:grammar:v3:{index}:{variant}");
    card.tags = vec![
        skill.skill.clone(),
        form.clone(),
        "基础语法".into(),
        format!("练习序号:{sequence}"),
    ];
    card.validate().map_err(|e| e.to_string())?;
    Ok(card)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reviewed_grammar_has_48_distinct_tasks_and_24_skills() {
        let cards: Vec<_> = (0..48).map(|n| grammar_card(n).unwrap()).collect();
        let distinct = |items: Vec<String>| {
            items
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
        };
        assert_eq!(distinct(cards.iter().map(|c| c.id.clone()).collect()), 48);
        assert_eq!(
            distinct(cards.iter().map(|c| c.title.clone()).collect()),
            24
        );
        assert_eq!(
            distinct(cards.iter().map(|c| c.prompt.clone()).collect()),
            48
        );
        assert!(distinct(cards.iter().map(|c| c.tags[1].clone()).collect()) >= 5);
        for (n, card) in cards.iter().enumerate() {
            let repeat = grammar_card(n as u64 + 48).unwrap();
            assert_eq!(card.id, repeat.id);
            assert_eq!(card.answer, repeat.answer);
            assert!(!card.source.contains("AI"));
        }
        assert_eq!(
            grammar_card(12).unwrap().origin,
            grammar_card(36).unwrap().origin
        );
        assert_ne!(
            grammar_card(12).unwrap().prompt,
            grammar_card(36).unwrap().prompt
        );
    }
}
