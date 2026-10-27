use crate::study::{Card, Rubric};
use serde::Deserialize;
pub const MODES: [&str; 9] = [
    "释义",
    "造句",
    "中译英",
    "英译中",
    "语法",
    "搭配填空",
    "句式改写",
    "短文理解",
    "听写",
];
mod grammar;
mod planning;
pub use grammar::grammar_card;
pub use planning::{Plan, plan, variant_id};
pub fn dictation_grade(card: &Card, answer: &str) -> crate::study::Grade {
    let correct = answer.trim().eq_ignore_ascii_case(card.title.trim());
    let mut grade = crate::study::Grade::unknown(card);
    for (check, rubric) in grade.checks.iter_mut().zip(&card.rubric) {
        check.earned = if correct { rubric.points } else { 0 };
        check.reason = format!("正确拼写：{}。忽略大小写及首尾空白。", card.title);
        check.evidence = answer.trim().into();
    }
    grade.summary = format!(
        "{}。正确拼写：{}\n{}",
        if correct {
            "拼写正确"
        } else {
            "继续听音练习"
        },
        card.title,
        card.answer
    );
    grade
}
pub fn slug(mode: &str) -> &str {
    match mode {
        "造句" => "sentence",
        "听写" => "dictation",
        "中译英" => "to-en",
        "英译中" => "to-cn",
        "语法" => "grammar",
        "搭配填空" => "collocation",
        "句式改写" => "rewrite",
        "短文理解" => "reading",
        _ => "meaning",
    }
}
pub fn id(base: &str, mode: &str) -> String {
    if mode == "释义" {
        base.into()
    } else {
        format!("drill:{}:{base}", slug(mode))
    }
}
#[derive(Deserialize)]
pub struct Draft {
    #[serde(alias = "a_prompt")]
    pub prompt: String,
    #[serde(alias = "b_reference")]
    pub reference: String,
    #[serde(alias = "c_explanation")]
    pub explanation: String,
}
pub fn exercise(base: &Card, mode: &str, draft: Option<Draft>) -> Result<Card, String> {
    if !MODES.contains(&mode) {
        return Err("未知英语练习方式".into());
    }
    if mode == "释义" {
        return Ok(base.clone());
    }
    if mode == "听写" {
        let mut card = base.clone();
        card.id = id(&base.id, mode);
        card.origin = Some(base.id.clone());
        card.topic = mode.into();
        card.prompt =
            "点击播放，听英文并写出单词。可反复播放或调慢语速；提交后显示词义与正确拼写。".into();
        card.rubric = vec![Rubric {
            id: "spelling".into(),
            text: "拼写与所听单词一致，忽略大小写和首尾空白".into(),
            points: 100,
        }];
        return Ok(card);
    }
    let mut card = base.clone();
    card.id = id(&base.id, mode);
    card.origin = Some(base.id.clone());
    card.topic = mode.into();
    card.sql = None;
    let criteria = if mode == "造句" {
        card.prompt = format!(
            "用 {} 写一个完整的英文句子。可以使用合理的词形变化，优先表达真实生活、雅思话题或技术场景中的意思。",
            base.title
        );
        card.answer = format!(
            "词义参考：{}。学生需正确使用目标词或词形变化造句；不存在唯一标准句子。接受语义合理的生活、雅思和技术场景。",
            base.answer
        );
        [
            (
                "正确使用目标词或合理词形，词义、词性、搭配与语境相符；仅出现该词不得分",
                40,
            ),
            ("主谓、时态、单复数、冠词及动词后接结构正确", 30),
            (
                "传达具体且完整的信息，不是孤立单词或片段；词义误用不在此重复扣分",
                20,
            ),
            ("单词拼写基本正确；词义、搭配和语法错误不在此重复扣分", 10),
        ]
    } else {
        let draft = draft.ok_or("这类练习需要先生成题目")?;
        if [&draft.prompt, &draft.reference, &draft.explanation]
            .iter()
            .any(|s| s.trim().is_empty() || s.chars().count() > 1500)
        {
            return Err("生成的练习不完整，请重试".into());
        }
        card.prompt = match mode {
            "英译中" => format!(
                "英译中：将下面英文翻译为中文；如果原句是问句，只需翻译，不用回答问题。\n{}",
                draft.prompt
            ),
            "中译英" => format!(
                "中译英：将下面中文翻译为英文，合理使用目标词或词形变化。\n{}",
                draft.prompt
            ),
            _ => draft.prompt,
        };
        card.answer = if mode == "语法" {
            format!(
                "参考解答：{}\n\n讲解：{}\n接受含义准确的不同表达，不要求逐字一致。",
                draft.reference, draft.explanation
            )
        } else {
            format!(
                "参考解答：{}\n\n讲解：{}\n\n词义参考：{}\n练习与参考由本地 AI 生成，可复制核对；接受含义准确的不同表达，不要求逐字一致。",
                draft.reference, draft.explanation, base.answer
            )
        };
        if mode == "语法" {
            [
                ("准确完成本题的语法任务（填空、改错、合句或辨析）", 40),
                ("改写后整句语法正确", 30),
                ("解释所用语法规则，接受简明说明", 20),
                ("表达自然，拼写基本正确", 10),
            ]
        } else if mode == "搭配填空" {
            [
                ("填入的词语或词形适合上下文", 40),
                ("正确解释搭配或词形选择", 30),
                ("句意准确，接受合理替代", 20),
                ("拼写基本正确", 10),
            ]
        } else if mode == "句式改写" {
            [
                ("保留原句核心含义", 40),
                ("完成题目要求的句式转换", 30),
                ("改写后语法正确", 20),
                ("表达自然完整", 10),
            ]
        } else if mode == "短文理解" {
            [
                ("准确回答短文的主要问题", 40),
                ("解释有原文信息支持", 30),
                ("理解目标词在文中的含义", 20),
                ("表达清楚，接受中文或英文作答", 10),
            ]
        } else {
            [
                ("核心含义准确，没有关键遗漏或歧义", 40),
                ("目标语言语法正确", 30),
                ("词语及搭配恰当", 20),
                ("表达完整自然，接受合理不同译法", 10),
            ]
        }
    };
    card.rubric = criteria
        .into_iter()
        .enumerate()
        .map(|(i, (text, points))| Rubric {
            id: format!("r{}", i + 1),
            text: text.into(),
            points,
        })
        .collect();
    card.validate().map_err(|e| e.to_string())?;
    Ok(card)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dictation_grading_uses_rubric_limits_and_tolerates_a_missing_rubric() {
        let mut base = crate::coding::cards().remove(0);
        base.kind = "word".into();
        base.coding = None;
        base.title = "Benefit".into();
        let mut card = exercise(&base, "听写", None).unwrap();
        assert_eq!(
            dictation_grade(&card, " BENEFIT ")
                .completion(&card, " BENEFIT ")
                .unwrap(),
            100
        );
        assert_eq!(
            dictation_grade(&card, "benifit")
                .completion(&card, "benifit")
                .unwrap(),
            0
        );
        card.rubric.clear();
        assert!(dictation_grade(&card, "benefit").checks.is_empty());
    }

    #[test]
    fn generated_exercises_reject_incomplete_oversized_or_unknown_modes() {
        let base = crate::coding::cards().remove(0);
        assert!(exercise(&base, "unknown", None).is_err());
        for (prompt, reference, explanation) in [
            ("", "reference", "explanation"),
            ("prompt", " ", "explanation"),
            ("prompt", "reference", ""),
        ] {
            assert!(
                exercise(
                    &base,
                    "英译中",
                    Some(Draft {
                        prompt: prompt.into(),
                        reference: reference.into(),
                        explanation: explanation.into()
                    })
                )
                .is_err()
            );
        }
        assert!(
            exercise(
                &base,
                "英译中",
                Some(Draft {
                    prompt: "中".repeat(1501),
                    reference: "r".into(),
                    explanation: "e".into()
                })
            )
            .is_err()
        );
    }
    #[test]
    fn generated_grammar_is_not_discarded_and_new_modes_have_distinct_valid_rubrics() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::open(dir.path()).unwrap();
        let base = db.study_card("ielts:benefit").unwrap().unwrap();
        for mode in MODES {
            let draft = Draft {
                prompt: "Explain the benefits of public transport.".into(),
                reference: "It reduces congestion.".into(),
                explanation: "用具体原因支撑观点。".into(),
            };
            let card = exercise(&base, mode, Some(draft)).unwrap();
            card.validate().unwrap();
            if !matches!(mode, "释义" | "造句" | "听写") {
                assert!(card.prompt.contains("public transport"));
            }
        }
        assert!(exercise(&base, "语法", None).is_err());
    }
}
