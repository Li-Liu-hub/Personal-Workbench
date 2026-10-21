use super::*;

#[test]
fn account_grid_fills_width_aligns_rows_and_only_draws_visible_cards_after_resize_and_zoom() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    app.section = Section::Credentials;
    app.view_modes.set(Section::Credentials, ViewMode::Grid);
    app.credentials = (0..1000)
        .map(|i| CredentialSummary {
            id: format!("fixture-{i}"),
            title: format!("Account {i}"),
            username: format!("user{i}@example.test"),
            password: "synthetic-long-value".repeat(i % 7 + 1),
            ..Default::default()
        })
        .collect();
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    for size in [[1000., 680.], [1280., 800.], [1920., 1080.], [1000., 680.]] {
        for scale in [1., 1.25, 1.5] {
            ctx.set_pixels_per_point(scale);
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
            let mut cards = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(r)
                        if r.rect.width() > 180.
                            && r.rect.height() > 200.
                            && r.fill == surface() =>
                    {
                        Some(r.rect)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            cards.sort_by(|a, b| a.left().total_cmp(&b.left()));
            let first_y = cards.iter().map(|r| r.top()).fold(f32::INFINITY, f32::min);
            let first_row = cards
                .iter()
                .filter(|r| (r.top() - first_y).abs() < 1.)
                .collect::<Vec<_>>();
            assert!(first_row.len() >= 3, "{size:?}, {scale}");
            assert!((first_row[0].left() - 208.).abs() < 2.);
            let right = first_row.last().unwrap().right();
            assert!(
                size[0] - right <= 40.,
                "unused right space: {size:?} {right}"
            );
            let height = first_row[0].height();
            assert!(first_row.iter().all(|r| (r.height() - height).abs() < 1.));
            assert!(
                cards.len() < 40,
                "offscreen accounts should not create widgets"
            );
        }
    }
    // Changes to search, labels and favorites must invalidate the cached order/geometry.
    app.search_query = "Account 999".into();
    let output = editor_tests::input_frame(&mut app, &ctx, 1., vec![], vec![]);
    let titles = |out: &egui::FullOutput| {
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t)
                    if t.pos.y > 110. && t.galley.text().starts_with("Account ") =>
                {
                    Some(t.galley.text().to_owned())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(titles(&output), vec!["Account 999"]);
    app.search_query.clear();
    app.account_filter = TagFilter::Named("test".into());
    app.credentials[998].tags = vec!["test".into()];
    let output = editor_tests::input_frame(&mut app, &ctx, 2., vec![], vec![]);
    assert_eq!(titles(&output), vec!["Account 998"]);
}

#[test]
fn contact_overviews_omit_binary_originals_and_details_save_and_export_preserve_them() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    let mut contact = Contact::new();
    contact.name = "Synthetic memory fixture".into();
    let path = dir.path().join("chat.txt");
    fs::write(&path, "searchable chat text").unwrap();
    let mut chat = contacts::import_chat(&path).unwrap();
    chat.original = vec![b'x'; 8 * 1024 * 1024];
    contact.chats.push(chat);
    app.db.as_mut().unwrap().save_contact(&contact).unwrap();
    app.load_contacts();
    assert!(app.contact_items[0].matches("searchable chat"));
    assert!(app.contact_items[0].chats[0].original.is_empty());
    app.open_contact_details(&contact.id);
    assert_eq!(
        app.contact_draft.chats[0].original,
        contact.chats[0].original
    );
    app.contact_draft.school = "Updated school".into();
    assert!(app.save_contact_draft());
    assert!(app.contact_items[0].chats[0].original.is_empty());
    let saved = app.db.as_ref().unwrap().contacts().unwrap().remove(0);
    assert_eq!(saved.chats[0].original, contact.chats[0].original);
    let zip_path = dir.path().join("contacts.zip");
    export::write(&app.data_dir, &zip_path, export::Scope::Contacts).unwrap();
    let mut archive = zip::ZipArchive::new(fs::File::open(zip_path).unwrap()).unwrap();
    let mut found = false;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        if file.name().ends_with(".txt") {
            use std::io::Read;
            let mut bytes = vec![];
            file.read_to_end(&mut bytes).unwrap();
            if bytes == contact.chats[0].original {
                found = true;
            }
        }
    }
    assert!(found);
}

#[test]
fn virtual_account_list_reaches_last_record_copies_values_and_preserves_its_edit_draft() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
    app.section = Section::Credentials;
    app.view_modes.set(Section::Credentials, ViewMode::List);
    app.credentials = (0..1000)
        .map(|i| CredentialSummary {
            id: format!("scroll-{i}"),
            title: format!("Row {i}"),
            username: format!("user{i}"),
            password: "synthetic-value".into(),
            ..Default::default()
        })
        .collect();
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    for i in 0..3 {
        editor_tests::input_frame(&mut app, &ctx, i as f64 * 0.1, vec![], vec![]);
    }
    editor_tests::input_frame(
        &mut app,
        &ctx,
        0.4,
        vec![
            egui::Event::PointerMoved(egui::pos2(800., 500.)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0., -1_000_000.),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        vec![],
    );
    let mut output = egui::FullOutput::default();
    for i in 5..15 {
        output = editor_tests::input_frame(&mut app, &ctx, i as f64 * 0.1, vec![], vec![]);
    }
    let at = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == "Row 999" => Some(t.pos),
            _ => None,
        })
        .expect("the last record remains reachable");
    assert!(at.y > 100. && at.y < 750.);
    let copy = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == "复制" && (t.pos.y - at.y).abs() < 10. => {
                Some(t.pos + egui::vec2(3., 3.))
            }
            _ => None,
        })
        .unwrap();
    editor_tests::input_frame(
        &mut app,
        &ctx,
        1.6,
        editor_tests::pointer(copy, true),
        vec![],
    );
    let output = editor_tests::input_frame(
        &mut app,
        &ctx,
        1.7,
        editor_tests::pointer(copy, false),
        vec![],
    );
    assert!(
        output
            .platform_output
            .commands
            .iter()
            .any(|c| matches!(c,egui::OutputCommand::CopyText(v) if v=="user999"))
    );
    let mut draft = app.credentials[999].clone();
    draft.username = "unsaved inline value".into();
    app.inline_credentials.insert(draft.id.clone(), draft);
    editor_tests::input_frame(&mut app, &ctx, 1.8, vec![], vec![]);
    assert_eq!(
        app.inline_credentials["scroll-999"].username,
        "unsaved inline value"
    );
}

/// Synthetic, isolated rendering measurement. No user's records or database are used.
#[test]
#[ignore = "manual before/after rendering measurement"]
fn credential_render_measurement() {
    for count in [45, 1000] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
        app.section = Section::Credentials;
        app.credentials = (0..count)
            .map(|i| crate::model::CredentialSummary {
                id: format!("synthetic-{i}"),
                title: format!("测试账号 {i}"),
                username: format!("user{i}@example.test"),
                password: "synthetic-value-".repeat(1 + i % 5),
                website: String::new(),
                notes: "synthetic notes ".repeat(20),
                tags: vec![],
            })
            .collect();
        for mode in [ViewMode::Grid, ViewMode::List] {
            app.view_modes.set(Section::Credentials, mode);
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            let mut shapes = 0;
            for _ in 0..5 {
                let _ = editor_tests::input_frame(&mut app, &ctx, 0., vec![], vec![]);
            }
            let start = std::time::Instant::now();
            for i in 0..60 {
                let output = editor_tests::input_frame(&mut app, &ctx, i as f64, vec![], vec![]);
                shapes = output.shapes.len();
            }
            println!(
                "render count={count} mode={mode:?} mean_us={} shapes={shapes}",
                start.elapsed().as_micros() / 60
            );
        }
    }
}
