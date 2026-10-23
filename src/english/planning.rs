//! A task blueprint is independent of a word. Rotate skills, forms and contexts,
//! then persist the generated card, so retries/history never regenerate a question.
use super::{Card, Draft};

const SKILLS: [(&str, [&str; 3]); 24] = [
    (
        "主谓一致",
        ["改错并解释", "语境填空并解释", "比较两句并说明选择"],
    ),
    (
        "可数与不可数名词",
        ["改错并解释", "语境填空并解释", "比较两句并说明选择"],
    ),
    (
        "冠词与限定词",
        ["语境填空并解释", "改错并解释", "比较两句并说明选择"],
    ),
    (
        "一般过去与现在完成",
        ["比较两句并说明选择", "语境填空并解释", "改错并解释"],
    ),
    (
        "现在完成与完成进行",
        ["语境填空并解释", "比较两句并说明选择", "按要求改写并解释"],
    ),
    (
        "过去完成与叙事顺序",
        ["按要求改写并解释", "语境填空并解释", "改错并解释"],
    ),
    (
        "将来时间与安排",
        ["语境填空并解释", "比较两句并说明选择", "改错并解释"],
    ),
    (
        "被动语态与施事",
        ["按要求改写并解释", "改错并解释", "语境填空并解释"],
    ),
    (
        "情态动词与推测强度",
        ["比较两句并说明选择", "语境填空并解释", "按要求改写并解释"],
    ),
    (
        "动名词与不定式",
        ["语境填空并解释", "改错并解释", "比较两句并说明选择"],
    ),
    (
        "介词与动词搭配",
        ["改错并解释", "语境填空并解释", "比较两句并说明选择"],
    ),
    (
        "比较级与数量比较",
        ["按要求改写并解释", "语境填空并解释", "改错并解释"],
    ),
    (
        "限定性关系从句",
        ["合并两句并解释", "改错并解释", "语境填空并解释"],
    ),
    (
        "非限定性关系从句",
        ["合并两句并解释", "比较两句并说明选择", "改错并解释"],
    ),
    (
        "名词性从句与语序",
        ["改错并解释", "合并两句并解释", "语境填空并解释"],
    ),
    (
        "真实条件与将来条件",
        ["语境填空并解释", "按要求改写并解释", "改错并解释"],
    ),
    (
        "虚拟条件与过去遗憾",
        ["按要求改写并解释", "比较两句并说明选择", "语境填空并解释"],
    ),
    (
        "间接引语与时间变化",
        ["按要求改写并解释", "改错并解释", "比较两句并说明选择"],
    ),
    (
        "让步原因与连接词",
        ["合并两句并解释", "语境填空并解释", "改错并解释"],
    ),
    (
        "平行结构与并列",
        ["改错并解释", "合并两句并解释", "按要求改写并解释"],
    ),
    (
        "代词指代与歧义",
        ["按要求改写并解释", "比较两句并说明选择", "改错并解释"],
    ),
    (
        "分词与修饰对象",
        ["改错并解释", "合并两句并解释", "比较两句并说明选择"],
    ),
    (
        "疑问句与否定范围",
        ["比较两句并说明选择", "按要求改写并解释", "改错并解释"],
    ),
    (
        "句子衔接与信息组织",
        ["合并两句并解释", "按要求改写并解释", "比较两句并说明选择"],
    ),
];
const CONTEXTS: [&str; 8] = [
    "教育与学习",
    "环境与城市",
    "健康与生活",
    "工作与职业",
    "科技与社会",
    "旅行与文化",
    "软件开发与协作",
    "日常交流",
];

#[derive(Clone, Debug)]
pub struct Plan {
    pub sequence: u64,
    pub skill: &'static str,
    pub form: &'static str,
    pub context: &'static str,
    pub level: &'static str,
}
pub fn plan(mode: &str, sequence: u64) -> Plan {
    let n = sequence as usize;
    let cycle = n / SKILLS.len();
    let (skill, forms) = SKILLS[n % SKILLS.len()];
    Plan {
        sequence,
        skill: match mode {
            "语法" | "句式改写" => skill,
            "搭配填空" => [
                "动词与名词搭配",
                "介词搭配",
                "词形与词性",
                "近义词的语境区别",
                "学术与日常语域",
                "固定表达",
            ][n % 6],
            "短文理解" => [
                "主旨与细节",
                "根据证据推断",
                "态度与目的",
                "因果与转折",
                "上下文猜词",
                "概括与信息整合",
            ][n % 6],
            _ => [
                "具体描述",
                "解释原因",
                "对比观点",
                "提出建议",
                "举例论证",
                "表达让步",
            ][n % 6],
        },
        form: if mode == "语法" {
            forms[cycle % forms.len()]
        } else {
            match mode {
                "句式改写" => "按要求改写并解释",
                "搭配填空" => "语境填空并解释",
                "短文理解" => "阅读短文并回答",
                "中译英" => "中译英",
                "英译中" => "英译中",
                _ => "结合语境表达",
            }
        },
        context: CONTEXTS[(n + cycle * 3) % CONTEXTS.len()],
        level: if cycle % 3 == 2 { "B2" } else { "B1" },
    }
}
pub fn variant_id(base: &str, mode: &str, sequence: u64) -> String {
    if mode == "释义" {
        super::id(base, mode)
    } else {
        format!("drill:{}:v2:{sequence}:{base}", super::slug(mode))
    }
}
impl Plan {
    pub fn instructions(&self) -> String {
        if matches!(self.form, "中译英" | "英译中") {
            return format!(
                "本题只能是{}，不能变成填空、选择或回答原文问题。语境：{}；难度：{}。题干给完整待译句子；reference给对应目标语言的完整译文，explanation具体说明译法和目标词的用法。",
                self.form, self.context, self.level
            );
        }
        let requirement = match self.form {
            "改错并解释" => {
                "a_prompt只给一条有真实语法错误的原句，要求改正并解释；不得给选项，也不得包含正确句。b_reference只给改正后的完整句子，c_explanation具体指出原句哪个词错以及怎么改。"
            }
            "语境填空并解释" => {
                "a_prompt给完整语境和一至两处空格，提供原形或候选词；要求填写并解释。b_reference给正确填写和完整句子，c_explanation解释选择依据。"
            }
            "比较两句并说明选择" => {
                "a_prompt给两条有关键差异的完整句子和明确语境，让学生判断哪句符合语境并解释。b_reference明确结论，c_explanation解释含义或语法的差别。"
            }
            "合并两句并解释" => {
                "a_prompt给两条标号的英文原句，要求用指定知识点合并为一句。b_reference给合并后的句子，c_explanation说明结构和含义如何保留。"
            }
            "按要求改写并解释" => {
                "a_prompt给英文原句及明确改写要求，要求使用指定知识点保留核心含义。b_reference给改写，c_explanation说明转换的具体规则。"
            }
            "阅读短文并回答" => {
                "a_prompt给完整短文及明确问题，不给答案；b_reference逐问回答并引用短文依据，c_explanation解释推断和目标词在文中的含义。"
            }
            _ => "明确本题的表达要求。",
        };
        format!(
            "知识点：{}；唯一题目形式：{}；语境：{}；难度：{}。{}只考一个主要知识点，目标词应自然出现在实际语境中，不要仅把它放在引号里谈论拼写。",
            self.skill, self.form, self.context, self.level, requirement
        )
    }

    pub fn exercise(&self, base: &Card, mode: &str, draft: Option<Draft>) -> Result<Card, String> {
        if let Some(draft) = &draft {
            if mode == "搭配填空" && !draft.prompt.contains("___") {
                return Err(
                    "搭配填空必须在题干句子中使用 ___ 留出真实空格；不要在题干公布填入的答案。"
                        .into(),
                );
            }
            let chinese = |text: &str| {
                text.chars()
                    .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
                    .count()
            };
            if mode == "英译中" && chinese(&draft.reference) < 2 {
                return Err(
                    "英译中必须给中文参考译文，不能变成英文填空或英文问答。请重做完整英译中题目。"
                        .into(),
                );
            }
            if mode == "中译英"
                && (chinese(&draft.prompt) < 2
                    || draft
                        .reference
                        .chars()
                        .filter(char::is_ascii_alphabetic)
                        .count()
                        < 10)
            {
                return Err("中译英需要中文待译原句和完整英文参考译文。".into());
            }
            if matches!(mode, "中译英" | "英译中")
                && (draft.prompt.contains("___") || draft.prompt.contains("填空"))
            {
                return Err("翻译题不能变成填空题，请给完整待译句子和对应译文。".into());
            }
        }
        if mode == "语法"
            && self.form == "改错并解释"
            && let Some(draft) = &draft
        {
            let clean = |text: &str| {
                text.chars()
                    .filter(|c| !c.is_whitespace())
                    .flat_map(char::to_lowercase)
                    .collect::<String>()
            };
            let reference = draft.reference.lines().next().unwrap_or("").trim();
            let reference = reference
                .split_once([':', '：'])
                .map(|(_, rest)| rest)
                .unwrap_or(reference);
            let reference = clean(reference);
            if reference.len() > 15 && clean(&draft.prompt).contains(&reference) {
                return Err("改错题的原句与正确句相同，缺少真实语法错误。请在原句中设置一处错误；reference只写改正后的完整句子，规则放到explanation。".into());
            }
        }
        let mut card = super::exercise(base, mode, draft)?;
        if mode == "释义" {
            return Ok(card);
        }
        card.id = variant_id(&base.id, mode, self.sequence);
        card.tags = vec![
            self.skill.into(),
            self.form.into(),
            self.context.into(),
            self.level.into(),
            format!("练习序号:{}", self.sequence),
        ];
        if mode == "造句" {
            card.prompt = format!(
                "用 {} 写一个完整英文句子，表达与“{}”有关的意思，尝试{}。可使用合理词形变化；若场景不适合这个词，可自选自然语境。",
                base.title, self.context, self.skill
            );
        }
        card.validate().map_err(|e| e.to_string())?;
        Ok(card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_collocation_task_must_leave_an_actual_gap() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::open(dir.path()).unwrap();
        let base = db.study_card("ielts:benefit").unwrap().unwrap();
        let make = |prompt: &str| Draft {
            prompt: prompt.into(),
            reference: "benefit; Regular exercise can benefit your health.".into(),
            explanation: "can 后用动词原形，benefit 直接接宾语。".into(),
        };
        let task = plan("搭配填空", 0);
        assert!(task.exercise(&base, "搭配填空", Some(make("Regular exercise can benefit your health. The word to fill in the blank is benefit."))).is_err());
        assert!(
            task.exercise(
                &base,
                "搭配填空",
                Some(make(
                    "用 benefit 的适当形式填空并解释：Regular exercise can ___ your health."
                ))
            )
            .is_ok()
        );
    }
    #[test]
    fn a_translation_plan_rejects_fill_in_the_blank_and_the_wrong_answer_language() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::open(dir.path()).unwrap();
        let base = db.study_card("ielts:benefit").unwrap().unwrap();
        let task = plan("英译中", 0);
        assert_eq!(task.form, "英译中");
        let wrong = Draft {
            prompt: "Complete: This policy has many ____.".into(),
            reference: "This policy has many benefits.".into(),
            explanation: "benefits 是名词复数。".into(),
        };
        assert!(task.exercise(&base, "英译中", Some(wrong)).is_err());
        let actual = Draft {
            prompt: "Who benefits from this policy?".into(),
            reference: "谁从这项政策中受益？".into(),
            explanation: "benefit from 表示从中受益；译文保留问句。".into(),
        };
        let card = task.exercise(&base, "英译中", Some(actual)).unwrap();
        assert!(card.prompt.contains("不用回答问题"));
        assert!(card.answer.contains("benefit from"));
    }
    #[test]
    fn a_correction_task_cannot_present_its_correct_answer_as_the_error() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::open(dir.path()).unwrap();
        let base = db.study_card("ielts:benefit").unwrap().unwrap();
        let task = plan("语法", 0);
        let same = Draft {
            prompt: "改错：This policy benefits everyone.".into(),
            reference: "This policy benefits everyone.".into(),
            explanation: "subject agreement".into(),
        };
        assert!(task.exercise(&base, "语法", Some(same)).is_err());
        let actual = Draft {
            prompt: "改错：This policy benefit everyone.".into(),
            reference: "This policy benefits everyone.".into(),
            explanation: "单数主语 policy 搭配第三人称单数 benefits。".into(),
        };
        assert!(task.exercise(&base, "语法", Some(actual)).is_ok());
    }
    #[test]
    fn full_rotation_covers_skills_forms_and_contexts_without_changing_history_ids() {
        let plans: Vec<_> = (0..72).map(|n| plan("语法", n)).collect();
        let unique = |values: Vec<&str>| {
            values
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
        };
        assert_eq!(unique(plans.iter().map(|p| p.skill).collect()), 24);
        assert!(unique(plans.iter().map(|p| p.form).collect()) >= 5);
        assert_eq!(unique(plans.iter().map(|p| p.context).collect()), 8);
        assert_ne!(
            variant_id("ielts:benefit", "语法", 0),
            super::super::id("ielts:benefit", "语法")
        );
        assert_ne!(
            variant_id("ielts:benefit", "语法", 0),
            variant_id("ielts:benefit", "语法", 1)
        );
    }
}
