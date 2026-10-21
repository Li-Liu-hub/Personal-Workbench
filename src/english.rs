use crate::study::{Card, Rubric};
use serde::Deserialize;
pub const MODES: [&str; 5] = ["释义", "造句", "中译英", "英译中", "语法"];
pub fn slug(mode: &str) -> &str {
    match mode {
        "造句" => "sentence",
        "中译英" => "to-en",
        "英译中" => "to-cn",
        "语法" => "grammar",
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
    pub prompt: String,
    pub reference: String,
    pub explanation: String,
}
fn grammar(base: &Card) -> Draft {
    let word = &base.title;
    let index = base
        .id
        .bytes()
        .fold(0usize, |a, b| a.wrapping_add(b as usize))
        % 10;
    let (wrong, right, explanation) = match index {
        0 => (
            format!("These student are learning the word '{word}'."),
            format!("These students are learning the word '{word}'."),
            "these 修饰复数可数名词，所以 student 要改为 students。",
        ),
        1 => (
            format!("She don't know the meaning of '{word}'."),
            format!("She doesn't know the meaning of '{word}'."),
            "一般现在时主语为第三人称单数 she，否定句使用 doesn't，后面动词保持原形。",
        ),
        2 => (
            format!("I have learn how to use '{word}'."),
            format!("I have learned how to use '{word}'."),
            "现在完成时结构为 have/has 加过去分词；learn 要使用 learned（learnt 也可）。",
        ),
        3 => (
            format!("We can uses '{word}' in a sentence."),
            format!("We can use '{word}' in a sentence."),
            "情态动词 can 后接动词原形，不使用第三人称单数 uses。",
        ),
        4 => (
            format!("He explained me the meaning of '{word}'."),
            format!("He explained the meaning of '{word}' to me."),
            "explain 的常用结构为 explain something to someone，不能直接套用 explain someone something。",
        ),
        5 => (
            format!("I am interested to learning the word '{word}'."),
            format!("I am interested in learning the word '{word}'."),
            "固定搭配为 be interested in；in 是介词，后面可接动名词 learning。",
        ),
        6 => (
            format!("There is several examples of '{word}' in this book."),
            format!("There are several examples of '{word}' in this book."),
            "there be 结构中 several examples 是复数，be 动词应使用 are。",
        ),
        7 => (
            format!("If I will understand '{word}', I will write a sentence with it."),
            format!("If I understand '{word}', I will write a sentence with it."),
            "表示将来可能发生的条件时，if 条件从句通常用一般现在时，主句可用 will。",
        ),
        8 => (
            format!("The teacher asked me learn how to use '{word}'."),
            format!("The teacher asked me to learn how to use '{word}'."),
            "ask someone to do something 表示要求某人做某事，需要 to learn。",
        ),
        _ => (
            format!("I look forward to use '{word}' correctly."),
            format!("I look forward to using '{word}' correctly."),
            "look forward to 中的 to 是介词，后面应接动名词 using，而不是动词原形 use。",
        ),
    };
    Draft {
        prompt: format!("改正下面句子的语法错误，写出完整正确句子：\n{wrong}"),
        reference: right,
        explanation: explanation.into(),
    }
}
pub fn exercise(base: &Card, mode: &str, draft: Option<Draft>) -> Result<Card, String> {
    if !MODES.contains(&mode) {
        return Err("未知英语练习方式".into());
    }
    if mode == "释义" {
        return Ok(base.clone());
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
            ("目标词使用准确，含义与语境合适", 40),
            ("语法正确", 30),
            ("表达完整清楚", 20),
            ("拼写与搭配基本正确", 10),
        ]
    } else {
        let draft = if mode == "语法" {
            grammar(base)
        } else {
            draft.ok_or("这类练习需要先生成题目")?
        };
        if [&draft.prompt, &draft.reference, &draft.explanation]
            .iter()
            .any(|s| s.trim().is_empty() || s.chars().count() > 1500)
        {
            return Err("生成的练习不完整，请重试".into());
        }
        card.prompt = draft.prompt;
        card.answer = if mode == "语法" {
            format!(
                "参考解答：{}\n\n讲解：{}\n接受含义准确的不同表达，不要求逐字一致。",
                draft.reference, draft.explanation
            )
        } else {
            format!(
                "参考解答：{}\n\n词义参考：{}\n译文由本地 AI 生成，可复制核对；接受含义准确的不同表达，不要求逐字一致。",
                draft.reference, base.answer
            )
        };
        if mode == "语法" {
            [
                ("准确定位并修正语法问题", 40),
                ("改写后整句语法正确", 30),
                ("保留原句含义", 20),
                ("表达自然，拼写基本正确", 10),
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
    fn grammar_pairs_have_a_real_difference_and_every_rubric_totals_one_hundred() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::open(dir.path()).unwrap();
        let mut base = db.study_card("ielts:benefit").unwrap().unwrap();
        for i in 0..30 {
            base.id = format!("test:{i}");
            let draft = grammar(&base);
            assert_ne!(draft.prompt.lines().last().unwrap(), draft.reference);
            let card = exercise(&base, "语法", None).unwrap();
            assert_eq!(
                card.rubric.iter().map(|r| r.points as u32).sum::<u32>(),
                100
            );
        }
    }
}
