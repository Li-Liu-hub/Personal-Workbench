use super::*;
use crate::app::editor_tests::pointer;

fn frame(
    app: &mut PersonalDeskApp,
    ctx: &egui::Context,
    size: [f32; 2],
    time: &mut f64,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    *time += 0.2;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
            time: Some(*time),
            events,
            ..Default::default()
        },
        |ui| app.render_workspace(ui),
    );
    output.textures_delta.clear();
    output
}

fn text_rect(output: &egui::FullOutput, value: &str) -> egui::Rect {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == value => {
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                shape.clip_rect.intersects(rect).then_some(rect)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing visible text: {value}"))
}

#[test]
fn history_opens_from_both_settings_views_and_older_releases_are_reachable() {
    for size in [[1000., 680.], [1280., 800.]] {
        for mode in [ViewMode::Grid, ViewMode::List] {
            let directory = tempfile::tempdir().unwrap();
            let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
            app.section = Section::Settings;
            app.set_view_mode(mode);
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            let mut time = 0.;
            frame(&mut app, &ctx, size, &mut time, vec![]);
            let output = frame(&mut app, &ctx, size, &mut time, vec![]);
            let at = text_rect(&output, "版本历史").center();
            frame(&mut app, &ctx, size, &mut time, pointer(at, true));
            frame(&mut app, &ctx, size, &mut time, pointer(at, false));
            assert_eq!(app.settings_detail.as_deref(), Some("版本历史"));
            let output = frame(&mut app, &ctx, size, &mut time, vec![]);
            let current = format!("当前版本  v{}", crate::updater::VERSION);
            let rect = text_rect(&output, &current);
            assert!(rect.min.x >= 0. && rect.max.x <= size[0]);
            text_rect(&output, &history::releases()[0].changes[0]);

            let previous = &history::releases()[1];
            let title = format!("v{} · {}", previous.version, previous.title);
            let at = text_rect(&output, &title).center();
            frame(&mut app, &ctx, size, &mut time, pointer(at, true));
            frame(&mut app, &ctx, size, &mut time, pointer(at, false));
            let output = frame(&mut app, &ctx, size, &mut time, vec![]);
            text_rect(&output, &previous.changes[0]);

            for _ in 0..10 {
                frame(
                    &mut app,
                    &ctx,
                    size,
                    &mut time,
                    vec![
                        egui::Event::PointerMoved(egui::pos2(size[0] / 2., size[1] / 2.)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0., -600.),
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Unknown,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            let output = frame(&mut app, &ctx, size, &mut time, vec![]);
            let oldest = history::releases().last().unwrap();
            let at =
                text_rect(&output, &format!("v{} · {}", oldest.version, oldest.title)).center();
            frame(&mut app, &ctx, size, &mut time, pointer(at, true));
            frame(&mut app, &ctx, size, &mut time, pointer(at, false));
            frame(
                &mut app,
                &ctx,
                size,
                &mut time,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert!(app.settings_detail.is_none());
        }
    }
}
