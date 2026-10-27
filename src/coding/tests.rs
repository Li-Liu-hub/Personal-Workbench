use super::*;

#[test]
fn grading_rechecks_compilation_task_identity_and_actual_output() {
    let card = cards().remove(0);
    let lab = card.coding.as_ref().unwrap();
    let report = Report {
        compiled: true,
        total: lab.cases.len(),
        cases: lab
            .cases
            .iter()
            .map(|case| CaseReport {
                input: case.input.clone(),
                expected: case.expected.clone(),
                actual: case.expected.clone(),
                error: String::new(),
                passed: true,
            })
            .collect(),
        ..Default::default()
    };
    assert_eq!(
        report
            .grade(&card, "source")
            .completion(&card, "source")
            .unwrap(),
        100
    );
    for field in ["compiled", "total", "input", "expected", "actual", "extra"] {
        let mut invalid = report.clone();
        match field {
            "compiled" => invalid.compiled = false,
            "total" => invalid.total += 1,
            "input" => invalid.cases[0].input.push_str("different"),
            "expected" => invalid.cases[0].expected.push_str("different"),
            "actual" => invalid.cases[0].actual = "incorrect result".into(),
            _ => invalid.cases.push(invalid.cases[0].clone()),
        }
        assert!(
            invalid
                .grade(&card, "source")
                .completion(&card, "source")
                .unwrap()
                < 100,
            "{field}"
        );
    }
}

#[test]
fn coding_lab_rejects_unsupported_languages_and_unbounded_inputs() {
    let valid = cards().remove(0).coding.unwrap();
    for field in [
        "language",
        "starter",
        "no-cases",
        "many-cases",
        "input",
        "expected",
    ] {
        let mut lab = valid.clone();
        match field {
            "language" => lab.language = "Shell".into(),
            "starter" => lab.starter = "x".repeat(65_537),
            "no-cases" => lab.cases.clear(),
            "many-cases" => lab.cases = vec![lab.cases[0].clone(); 13],
            "input" => lab.cases[0].input = "x".repeat(8193),
            _ => lab.cases[0].expected = "x".repeat(8193),
        }
        assert!(lab.validate().is_err(), "{field}");
    }
}

#[test]
fn output_comparison_normalizes_line_endings_but_preserves_meaningful_spacing() {
    assert_eq!(normalized("a  \r\nb\r\n"), "a\nb");
    assert_ne!(normalized(" a\nb"), normalized("a\nb"));
    assert_ne!(normalized("a b"), normalized("ab"));
}
use crate::{db::Database, study::Pending};

fn runtime() -> PathBuf {
    PathBuf::from(
        std::env::var_os("PERSONAL_DESK_LEARNING_RUNTIME")
            .expect("set PERSONAL_DESK_LEARNING_RUNTIME to the installed learning-runtime folder"),
    )
}
#[test]
fn coding_seeds_link_exact_knowledge_and_export_real_evidence_without_replacing_edits() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let cards = cards();
    assert_eq!(cards.len(), 12);
    for card in &cards {
        card.validate().unwrap();
        for id in &card.knowledge_ids {
            assert!(db.reference_entry(id).unwrap().is_some());
            assert!(
                db.knowledge_cards(id)
                    .unwrap()
                    .iter()
                    .any(|c| c.id == card.id)
            );
        }
    }
    let card = &cards[0];
    let pending = Pending {
        id: "code-attempt".into(),
        card_id: card.id.clone(),
        answer: card.answer.clone(),
        at: "2026-10-23T20:00:00+08:00".into(),
    };
    let report = Report {
        compiled: true,
        total: card.coding.as_ref().unwrap().cases.len(),
        cases: card
            .coding
            .as_ref()
            .unwrap()
            .cases
            .iter()
            .map(|c| CaseReport {
                input: c.input.clone(),
                expected: c.expected.clone(),
                actual: c.expected.clone(),
                error: String::new(),
                passed: true,
            })
            .collect(),
        ..Default::default()
    };
    db.save_study_pending(&pending).unwrap();
    let attempt = db
        .record_study_attempt(&pending, report.grade(card, &pending.answer), "fixture", 3)
        .unwrap();
    assert_eq!(attempt.completion, 100);
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.study_attempts(&card.id).unwrap()[0]
            .feedback
            .coding
            .as_ref()
            .unwrap()
            .passed(),
        3
    );
    let path = dir.path().join("coding-export.zip");
    crate::export::write(dir.path(), &path, crate::export::Scope::Practice).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut json = String::new();
    use std::io::Read;
    zip.by_name("刷题王.json")
        .unwrap()
        .read_to_string(&mut json)
        .unwrap();
    let bundle: crate::study::Bundle = serde_json::from_str(&json).unwrap();
    assert_eq!(
        bundle.attempts[0]
            .feedback
            .coding
            .as_ref()
            .unwrap()
            .passed(),
        3
    );
    assert!(bundle.cards.iter().any(|c| c.coding.is_some()));
}
#[test]
#[ignore = "requires local JDK 17 and installed TinyCC; set PERSONAL_DESK_LEARNING_RUNTIME"]
fn every_reference_program_really_compiles_and_passes_all_samples() {
    for card in cards() {
        let lab = card.coding.as_ref().unwrap();
        let report = run(lab, &card.answer, &runtime(), &AtomicBool::new(false));
        assert!(report.compiled, "{}: {report:?}", card.id);
        assert_eq!(report.passed(), lab.cases.len(), "{}: {report:?}", card.id);
        assert_eq!(
            report
                .grade(&card, &card.answer)
                .completion(&card, &card.answer)
                .unwrap(),
            100
        );
        println!("{} {}/{}", card.id, report.passed(), lab.cases.len());
    }
}
#[test]
#[ignore = "requires installed TinyCC; set PERSONAL_DESK_LEARNING_RUNTIME"]
fn compile_errors_wrong_output_crash_timeout_and_cancellation_never_pass() {
    let card = cards()
        .into_iter()
        .find(|c| c.coding.as_ref().unwrap().language == "C")
        .unwrap();
    let lab = card.coding.as_ref().unwrap();
    let root = runtime();
    let report = run(lab, "int main( {", &root, &AtomicBool::new(false));
    assert!(!report.compiled && report.passed() == 0);
    for code in [
        "#include <stdio.h>\nint main(void){puts(\"wrong\");return 0;}",
        "int main(void){return 1;}",
        "int main(void){for(;;){}}",
        "#include <stdio.h>\nint main(void){for(;;) puts(\"spam\");}",
    ] {
        let report = run(lab, code, &root, &AtomicBool::new(false));
        assert!(report.compiled && report.passed() == 0, "{report:?}");
    }
    let cancelled = run(lab, &card.answer, &root, &AtomicBool::new(true));
    assert_eq!(cancelled.passed(), 0);
    assert!(cancelled.output.contains("停止"));
}
