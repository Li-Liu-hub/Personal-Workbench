use super::*;
use crate::{english, local_ai::Dialogue, study::Check};

fn full(card: &Card, answer: &str) -> Grade {
    Grade {
        checks: card
            .rubric
            .iter()
            .map(|r| Check {
                id: r.id.clone(),
                earned: r.points,
                reason: "fixture".into(),
                evidence: answer.into(),
            })
            .collect(),
        summary: "fixture feedback".into(),
        weak_points: vec![],
        sql: None,
        project: None,
    }
}
fn wait(app: &mut PersonalDeskApp) {
    let start = std::time::Instant::now();
    while app.study.busy && start.elapsed() < Duration::from_secs(5) {
        app.poll_background();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!app.study.busy);
}
#[test]
fn one_word_has_independent_modes_and_assisted_attempts_do_not_master_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let base = db.study_card("ielts:benefit").unwrap().unwrap();
    let sentence = english::exercise(&base, "造句", None).unwrap();
    db.import_study_cards(std::slice::from_ref(&sentence))
        .unwrap();
    assert_eq!(db.study_count("word").unwrap(), 5040);
    assert_ne!(base.id, sentence.id);
    let answer = "Regular exercise can benefit our health.";
    for _ in 0..5 {
        let p = pending(&sentence, answer);
        db.mark_study_help(&p.id).unwrap();
        let attempt = db
            .record_study_attempt(&p, full(&sentence, answer), "fixture", 1)
            .unwrap();
        assert!(attempt.assisted);
    }
    assert!(!db.study_progress(&sentence.id).unwrap().mastered);
    assert_eq!(db.study_progress(&sentence.id).unwrap().attempts, 5);
    for _ in 0..5 {
        db.record_study_attempt(
            &pending(&sentence, answer),
            full(&sentence, answer),
            "fixture",
            1,
        )
        .unwrap();
    }
    assert!(db.study_progress(&sentence.id).unwrap().mastered);
    assert_eq!(db.study_progress(&sentence.id).unwrap().attempts, 10);
    assert_eq!(db.study_progress(&base.id).unwrap().attempts, 0);
    db.record_study_attempt(&pending(&base, "益处"), full(&base, "益处"), "fixture", 1)
        .unwrap();
    assert_eq!(db.study_today("word").unwrap(), 1);
    let rules = Rules {
        threshold: 90,
        ..Default::default()
    };
    db.save_study_rules(&rules).unwrap();
    assert_eq!(db.study_progress(&sentence.id).unwrap().attempts, 10);
    assert_eq!(db.study_progress(&sentence.id).unwrap().streak, 5);
}
#[test]
fn changing_to_sentence_practice_keeps_the_selected_word_and_its_previous_answer() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    let card = app
        .db
        .as_ref()
        .unwrap()
        .study_card("ielts:benefit")
        .unwrap()
        .unwrap();
    let mut s = Session {
        card: Some(card.clone()),
        answer: "益处".into(),
        pending: Some(pending(&card, "益处")),
        word_mode: "造句".into(),
        ..Default::default()
    };
    app.switch_english_mode(&mut s);
    assert_eq!(s.card.as_ref().unwrap().title, "benefit");
    assert_eq!(s.card.as_ref().unwrap().topic, "造句");
    assert_eq!(
        s.card.as_ref().unwrap().origin.as_deref(),
        Some(card.id.as_str())
    );
    assert!(
        app.db
            .as_ref()
            .unwrap()
            .study_pending("word")
            .unwrap()
            .iter()
            .any(|p| p.card_id == card.id && p.answer == "益处")
    );
    assert!(!app.study.busy);
}
#[test]
fn failed_tutor_keeps_question_and_answer_and_success_is_scoped_persistent_and_exported() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.study.config.model = dir.path().join("missing.gguf");
    let card = app
        .db
        .as_ref()
        .unwrap()
        .study_card("eng:3:04")
        .unwrap()
        .unwrap();
    let mut session = Session {
        card: Some(card.clone()),
        answer: "my answer".into(),
        pending: Some(pending(&card, "my answer")),
        ..Default::default()
    };
    app.ask_tutor(
        "engineering",
        &mut session,
        "为什么 NULL 会影响查询？".into(),
    );
    app.study.practice = session;
    wait(&mut app);
    assert_eq!(app.study.practice.answer, "my answer");
    assert_eq!(app.study.practice.chat.len(), 1);
    assert!(app.study.error.contains("未找到"));
    app.accept_learning(LearningMessage::Replied {
        kind: "engineering".into(),
        card_id: card.id.clone(),
        result: Ok("NULL 表示未知，WHERE 只保留 TRUE。".into()),
    });
    assert_eq!(app.study.practice.chat.len(), 2);
    assert_eq!(
        app.db
            .as_ref()
            .unwrap()
            .study_dialogue(&card.id)
            .unwrap()
            .len(),
        2
    );
    let p = app.study.practice.pending.clone().unwrap();
    let attempt = app
        .db
        .as_mut()
        .unwrap()
        .record_study_attempt(&p, full(&card, "my answer"), "fixture", 1)
        .unwrap();
    assert!(attempt.assisted);
    drop(app);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(db.study_dialogue(&card.id).unwrap().len(), 2);
    let target = dir.path().join("all.zip");
    export::write(dir.path(), &target, export::Scope::All).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(target).unwrap()).unwrap();
    let bundle: study::Bundle =
        serde_json::from_reader(zip.by_name("刷题王.json").unwrap()).unwrap();
    assert_eq!(bundle.dialogue.len(), 2);
    assert!(bundle.attempts[0].assisted);
}
#[test]
fn english_generation_fields_are_valid_and_cached_without_overwriting_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let base = db.study_card("ielts:benefit").unwrap().unwrap();
    for mode in ["中译英", "英译中", "语法"] {
        let exercise = english::exercise(
            &base,
            mode,
            Some(english::Draft {
                prompt: "请翻译或改正：Regular exercise can benefit our health.".into(),
                reference: "Regular exercise can benefit our health.".into(),
                explanation: "benefit 可以用作及物动词。".into(),
            }),
        )
        .unwrap();
        db.import_study_cards(std::slice::from_ref(&exercise))
            .unwrap();
        let mut changed = exercise.clone();
        changed.prompt = "new prompt must not overwrite".into();
        assert_eq!(db.import_study_cards(&[changed]).unwrap(), 0);
        assert_eq!(
            db.study_card(&exercise.id).unwrap().unwrap().prompt,
            exercise.prompt
        );
    }
    assert_eq!(db.study_count("word").unwrap(), 5040);
}
#[test]
fn project_sources_test_results_and_dialogue_survive_restart_and_export() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let saved = crate::challenges::Saved {
        card_id: crate::challenges::ID.into(),
        source: crate::challenges::original(),
        report: Some(crate::challenges::Report {
            compiled: true,
            passed: 2,
            total: 9,
            output: "fixture report".into(),
            elapsed_ms: 1,
        }),
    };
    db.save_study_project(&saved).unwrap();
    db.append_study_dialogue(
        &saved.card_id,
        &Dialogue::new("user", "怎样修复重复扣库存？"),
    )
    .unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.study_project(&saved.card_id)
            .unwrap()
            .unwrap()
            .source
            .files,
        saved.source.files
    );
    let target = dir.path().join("all.zip");
    export::write(dir.path(), &target, export::Scope::All).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(target).unwrap()).unwrap();
    let bundle: study::Bundle =
        serde_json::from_reader(zip.by_name("刷题王.json").unwrap()).unwrap();
    assert_eq!(bundle.projects.len(), 1);
    assert_eq!(bundle.projects[0].report.as_ref().unwrap().passed, 2);
}
#[test]
fn learning_workspace_keeps_editor_actions_and_tutor_inside_supported_windows() {
    for size in [[1000., 680.], [1280., 800.], [1920., 1080.]] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        app.section = Section::Practice;
        let card = app
            .db
            .as_ref()
            .unwrap()
            .study_card("eng:3:05")
            .unwrap()
            .unwrap();
        app.study.practice = Session {
            pending: Some(pending(&card, "")),
            card: Some(card),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let mut output = egui::FullOutput::default();
        for _ in 0..3 {
            output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                    ..Default::default()
                },
                |ui| app.render_workspace(ui),
            );
            output.textures_delta.clear();
        }
        for label in [
            "运行 SQL",
            "提交 · AI 批改",
            "AI 解答",
            "发送追问",
            "启动模型",
        ] {
            let shape = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == label => {
                        Some((t.pos, t.galley.size()))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing {label} {size:?}"));
            assert!(
                shape.0.x >= 184.
                    && shape.0.x + shape.1.x < size[0]
                    && shape.0.y + shape.1.y < size[1],
                "{label}: {shape:?} {size:?}"
            );
        }
    }
}
#[test]
#[ignore = "requires installed local model and GPU runtime"]
fn live_learning_tutor_english_modes_and_real_token_metrics() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let ai = Arc::new(LocalAi::default());
    let config = Config::default();
    let log = dir.path().join("model.log");
    ai.warmup(&config, &log).unwrap();
    assert_eq!(ai.snapshot().tokens, None);
    let card = db.study_card("eng:3:04").unwrap().unwrap();
    let reply = ai
        .discuss(
            &config,
            &log,
            &card,
            "",
            "",
            &[Dialogue::new(
                "user",
                "用三句话讲清为什么 NOT IN 遇到 NULL 会出问题，给一个例子。",
            )],
        )
        .unwrap();
    println!("TUTOR {reply}");
    assert!(reply.chars().count() > 20);
    let stats = ai.snapshot();
    println!("METRICS tokens={:?} speed={:?}", stats.tokens, stats.speed);
    assert!(stats.tokens.is_some_and(|n| n > 0));
    assert!(stats.speed.is_some_and(|n| n > 0.));
    let base = db.study_card("ielts:benefit").unwrap().unwrap();
    for mode in ["中译英", "英译中", "语法"] {
        let exercise = ai.english_exercise(&config, &log, &base, mode).unwrap();
        println!("ENGLISH {mode}: {} | {}", exercise.prompt, exercise.answer);
        db.import_study_cards(std::slice::from_ref(&exercise))
            .unwrap();
        let answer = exercise
            .answer
            .lines()
            .next()
            .unwrap()
            .trim_start_matches("参考解答：");
        if mode == "语法" {
            assert_ne!(
                exercise.prompt.lines().last().unwrap(),
                answer,
                "grammar input must contain a real error"
            );
        }
        let grade = ai.grade(&config, &log, &exercise, answer).unwrap();
        let score = grade.completion(&exercise, answer).unwrap();
        println!("GRADE {mode}: {score} {}", grade.summary);
        assert!(score >= 75);
        if mode == "语法" {
            let wrong = exercise.prompt.lines().last().unwrap();
            let wrong_grade = ai.grade(&config, &log, &exercise, wrong).unwrap();
            let wrong_score = wrong_grade.completion(&exercise, wrong).unwrap();
            println!("GRAMMAR unchanged-error score={wrong_score}");
            assert!(wrong_score < 85);
        }
        db.record_study_attempt(&pending(&exercise, answer), grade, "live", 1)
            .unwrap();
    }
    let sentence = english::exercise(&base, "造句", None).unwrap();
    let answer = "Regular exercise can benefit our physical and mental health.";
    let grade = ai.grade(&config, &log, &sentence, answer).unwrap();
    assert!(grade.completion(&sentence, answer).unwrap() >= 75);
    let running = ai.clone();
    let request_config = config.clone();
    let request_log = log.clone();
    let request_card = card.clone();
    let worker = thread::spawn(move || {
        running.discuss(
            &request_config,
            &request_log,
            &request_card,
            "",
            "",
            &[Dialogue::new("user", "请详细讲解所有步骤并给出多个例子。")],
        )
    });
    let started = std::time::Instant::now();
    while !ai.busy.load(std::sync::atomic::Ordering::Acquire)
        && started.elapsed() < Duration::from_secs(2)
    {
        thread::sleep(Duration::from_millis(1));
    }
    ai.stop();
    assert!(worker.join().unwrap().is_err());
    ai.warmup(&config, &log).unwrap();
    ai.stop();
}
