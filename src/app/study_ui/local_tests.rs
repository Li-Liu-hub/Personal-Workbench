use super::*;
use crate::app::editor_tests::{input_frame, pointer};

#[test]
fn model_selection_pins_spark_q4_and_kokoro_start_never_launches_text_model() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().into());
    let old = Config {
        model: "D:/models/llm/spark-x2.5/4b/Spark-X2.5-4B.gguf".into(),
        ..Config::default()
    };
    app.db
        .as_mut()
        .unwrap()
        .set_setting("study.ai.config", &serde_json::to_string(&old).unwrap())
        .unwrap();
    app.initialize_study();
    assert_eq!(app.study.config.model, Config::default().model);
    app.section = Section::Words;
    app.study.speech_selected = true;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    position(&output, "模型管理");
    position(&output, "Kokoro · 朗读");
    let button = position(&output, "启动模型");
    input_frame(&mut app, &ctx, 0.2, pointer(button, true), vec![]);
    input_frame(&mut app, &ctx, 0.3, pointer(button, false), vec![]);
    assert!(app.study.audio.settings);
    assert!(!app.study.busy && !app.study.ai.is_loaded());
    // A relocated copy of the same quantization is retained on restart.
    let selected = Config {
        model: directory.path().join(Config::MODEL_NAME),
        ..Config::default()
    };
    app.db
        .as_mut()
        .unwrap()
        .set_setting(
            "study.ai.config",
            &serde_json::to_string(&selected).unwrap(),
        )
        .unwrap();
    app.initialize_study();
    assert_eq!(app.study.config.model, selected.model);
}

fn position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
    output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == label => {
                Some(t.pos + t.galley.rect.size() / 2.)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {label}"))
}
#[test]
fn dictation_hides_spelling_click_checks_without_ai_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.section = Section::Words;
    let base = app
        .db
        .as_ref()
        .unwrap()
        .study_card("ielts:benefit")
        .unwrap()
        .unwrap();
    let mut s = Session {
        word_mode: "听写".into(),
        ..Default::default()
    };
    app.prepare_english_base(&mut s, base, "听写".into());
    assert!(!app.study.busy && s.generating.is_none());
    app.study.words = s;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    assert!(!output.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().to_lowercase().contains("benefit"))));
    position(&output, "播放单词");
    app.study.words.answer = "  BENEFIT  ".into();
    let output = input_frame(&mut app, &ctx, 0.2, vec![], vec![]);
    let p = position(&output, "检查拼写");
    input_frame(&mut app, &ctx, 0.3, pointer(p, true), vec![]);
    input_frame(&mut app, &ctx, 0.4, pointer(p, false), vec![]);
    let attempt = app.study.words.result.as_ref().unwrap();
    assert_eq!(attempt.completion, 100);
    assert!(!attempt.assisted);
    assert!(!app.study.ai.is_loaded());
    let id = attempt.card_id.clone();
    drop(app);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(db.study_attempts(&id).unwrap()[0].completion, 100);
    let card = db.study_card(&id).unwrap().unwrap();
    assert_eq!(
        crate::english::dictation_grade(&card, "benfit")
            .completion(&card, "benfit")
            .unwrap(),
        0
    );
}
#[test]
fn dictionary_meaning_is_available_without_a_text_model_and_linked_coding_retains_drafts() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    let base = app
        .db
        .as_ref()
        .unwrap()
        .study_card("ielts:benefit")
        .unwrap()
        .unwrap();
    let mut s = Session::default();
    app.prepare_english_base(&mut s, base, "释义".into());
    assert!(s.card.is_some() && s.generating.is_none() && !app.study.busy);
    let card = crate::coding::cards()[0].clone();
    let p = pending(&card, "my saved source");
    app.db.as_mut().unwrap().save_study_pending(&p).unwrap();
    app.open_linked_practice(card.clone());
    assert_eq!(app.study.practice.answer, "my saved source");
    let mut s = std::mem::take(&mut app.study.practice);
    s.answer = card.answer.clone();
    app.submit_study("engineering", &mut s);
    assert!(s.result.is_none());
    assert!(app.study.error.contains("先运行"));
    assert!(!app.study.busy && !app.study.ai.is_loaded());
}
#[test]
fn dictation_and_coding_actions_fit_small_and_standard_windows() {
    for size in [[1000., 680.], [1280., 800.]] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        for word in [true, false] {
            app.section = if word {
                Section::Words
            } else {
                Section::Practice
            };
            let card = if word {
                let base = app
                    .db
                    .as_ref()
                    .unwrap()
                    .study_card("ielts:benefit")
                    .unwrap()
                    .unwrap();
                crate::english::exercise(&base, "听写", None).unwrap()
            } else {
                crate::coding::cards()[0].clone()
            };
            if word {
                app.db
                    .as_mut()
                    .unwrap()
                    .import_study_cards(std::slice::from_ref(&card))
                    .unwrap();
            }
            *app.study
                .session_mut(if word { "word" } else { "engineering" }) = Session {
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
            for label in if word {
                vec!["播放单词", "检查拼写", "启动模型", "模型管理"]
            } else {
                vec!["运行样例", "提交 · 样例判题", "启动模型", "模型管理"]
            } {
                let p = position(&output, label);
                assert!(
                    p.x > 0. && p.x < size[0] && p.y > 0. && p.y < size[1],
                    "{label}: {p:?}"
                );
            }
            if !word {
                let code = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Text(t) if t.galley.text().contains("import java.util.*;") => {
                            Some(t.pos)
                        }
                        _ => None,
                    })
                    .expect("code editor is visible");
                assert!(code.y + 50. < position(&output, "提交 · 样例判题").y);
            }
        }
    }
}
