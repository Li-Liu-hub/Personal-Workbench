use super::*;
use crate::app::editor_tests::{input_frame, pointer};
fn pos(out: &egui::FullOutput, label: &str) -> egui::Pos2 {
    out.shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text().starts_with(label) => {
                Some(t.galley.rect.translate(t.pos.to_vec2()).center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {label}"))
}
fn press(key: egui::Key) -> Vec<egui::Event> {
    [true, false]
        .into_iter()
        .map(|pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Default::default(),
        })
        .collect()
}
#[test]
fn gallery_stage_controls_checkpoint_resume_and_unlock_work_in_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.section = Section::Games;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    let p = pos(&out, "俄罗斯方块");
    input_frame(&mut app, &ctx, 0.2, pointer(p, true), vec![]);
    input_frame(&mut app, &ctx, 0.3, pointer(p, false), vec![]);
    let out = input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
    let p = pos(&out, "第 1 关");
    input_frame(&mut app, &ctx, 0.5, pointer(p, true), vec![]);
    input_frame(&mut app, &ctx, 0.6, pointer(p, false), vec![]);
    assert!(app.games.tetris_open);
    assert_eq!(app.games.tetris.stage, 1);
    input_frame(&mut app, &ctx, 0.7, press(egui::Key::Enter), vec![]);
    assert_eq!(app.games.tetris.status, Status::Running);
    input_frame(&mut app, &ctx, 0.8, press(egui::Key::Space), vec![]);
    assert!(app.games.tetris.board.iter().flatten().any(|v| *v > 0));
    input_frame(&mut app, &ctx, 0.9, press(egui::Key::P), vec![]);
    assert_eq!(app.games.tetris.status, Status::Paused);
    assert!(app.flush_games());
    let board = app.games.tetris.board.clone();
    drop(app);
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    assert_eq!(app.games.tetris.board, board);
    assert_eq!(app.games.tetris.status, Status::Paused);
    app.section = Section::Games;
    app.games.selected = "tetris".into();
    let out = input_frame(&mut app, &ctx, 1., vec![], vec![]);
    let p = pos(&out, "继续存档");
    input_frame(&mut app, &ctx, 1.1, pointer(p, true), vec![]);
    input_frame(&mut app, &ctx, 1.2, pointer(p, false), vec![]);
    assert!(app.games.tetris_open);
    app.games.tetris.status = Status::Cleared;
    app.games.tetris.lines = app.games.tetris.goal();
    app.games.dirty = true;
    input_frame(&mut app, &ctx, 3., vec![], vec![]);
    assert_eq!(
        app.db
            .as_ref()
            .unwrap()
            .game_library()
            .unwrap()
            .tetris_unlocked,
        2
    );
    input_frame(&mut app, &ctx, 4., vec![], vec![]);
    assert_eq!(app.db.as_ref().unwrap().game_records(100).unwrap().len(), 1);
}
