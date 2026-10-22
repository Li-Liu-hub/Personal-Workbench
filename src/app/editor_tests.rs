use super::*;
use std::time::Instant;

#[test]
fn contact_import_is_persisted_after_save_and_never_applied_to_a_different_contact() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("聊天.xlsx");
    fs::write(&path, contacts::tests::workbook_fixture()).unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
    app.section = Section::Contacts;
    app.add_current_item();
    assert!(app.contact_dialog_open);
    app.contact_draft.name = "示例联系人".into();
    app.start_contact_import(vec![path.clone(), path.clone()]);
    assert!(!app.save_contact_draft());
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.contact_import_busy && Instant::now() < deadline {
        app.poll_background();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!app.contact_import_busy);
    assert_eq!(app.contact_draft.chats.len(), 1);
    assert!(app.save_contact_draft());
    assert_eq!(app.contact_items.len(), 1);
    assert!(app.contact_items[0].matches("下周一起看书"));
    app.start_contact_import(vec![path]);
    app.contact_dialog_open = false;
    app.add_current_item();
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.contact_import_busy && Instant::now() < deadline {
        app.poll_background();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!app.contact_import_busy);
    assert!(app.contact_draft.chats.is_empty());
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    app.contact_dialog_open = false;
    app.search_query = "下周一起看书".into();
    let output = input_frame(&mut app, &ctx, 1.0, vec![], vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="示例联系人"))
    );
    let restored = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
    assert_eq!(restored.contact_items.len(), 1);
    assert_eq!(restored.contact_items[0].chats.len(), 1);
}

pub(super) fn input_frame(
    app: &mut PersonalDeskApp,
    ctx: &egui::Context,
    time: f64,
    events: Vec<egui::Event>,
    dropped_files: Vec<egui::DroppedFileHandle>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            time: Some(time),
            events,
            dropped_files,
            ..Default::default()
        },
        |ui| app.render_workspace(ui),
    );
    output.textures_delta.clear();
    output
}

pub(super) fn pointer(position: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

#[test]
fn tag_filters_combine_with_search_and_manager_edits_keep_pending_selections_in_sync() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
    let mut account = Credential::new();
    account.title = "work-card".into();
    account.username = "alice".into();
    account.tags = vec!["工作".into()];
    app.db.as_mut().unwrap().save_credential(&account).unwrap();
    let mut other = Credential::new();
    other.title = "game-card".into();
    other.username = "bob".into();
    other.tags = vec!["游戏".into()];
    app.db.as_mut().unwrap().save_credential(&other).unwrap();
    app.load_summaries();
    app.load_tags();
    app.section = Section::Credentials;
    app.account_filter = TagFilter::Named("游戏".into());
    app.search_query = "bob".into();
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    let output = input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
    let texts = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(texts.contains(&"game-card"));
    assert!(!texts.contains(&"work-card"));
    app.credential_draft = other.clone();
    app.tag_assignment = Some(TagAssignment {
        scope: TagScope::Credentials,
        id: other.id,
        selected: vec!["游戏".into()],
    });
    app.db
        .as_mut()
        .unwrap()
        .rename_tag(TagScope::Credentials, "游戏", "休闲")
        .unwrap();
    app.sync_tag_edit(TagScope::Credentials, "游戏", Some("休闲"));
    assert_eq!(app.account_filter, TagFilter::Named("休闲".into()));
    assert_eq!(app.credential_draft.tags, vec!["休闲"]);
    assert_eq!(app.tag_assignment.as_ref().unwrap().selected, vec!["休闲"]);
    app.db
        .as_mut()
        .unwrap()
        .delete_tag(TagScope::Credentials, "休闲")
        .unwrap();
    app.sync_tag_edit(TagScope::Credentials, "休闲", None);
    assert_eq!(app.account_filter, TagFilter::All);
    assert!(app.credential_draft.tags.is_empty());
    assert_eq!(app.credentials.len(), 2);
}

#[test]
fn account_password_stays_readable_without_a_visibility_switch_at_display_scales() {
    for scale in [1.0, 1.25, 1.5] {
        let directory = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
        app.credential_draft.title = "测试账号".into();
        app.credential_draft.password = "SYNTHETIC-visible-password".into();
        assert!(app.save_credential_draft());
        app.selected_credential = Some(app.credential_draft.id.clone());
        app.credential_dialog_open = true;
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        ctx.set_pixels_per_point(scale);
        for time in [0.0, 1.0, 31.0, 61.0] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 680.0),
                    )),
                    time: Some(time),
                    ..Default::default()
                },
                |ui| app.render_workspace(ui),
            );
            output.textures_delta.clear();
            if time == 0.0 {
                continue;
            }
            let mut password_visible = false;
            for shape in output.shapes {
                if let egui::epaint::Shape::Text(text) = shape.shape {
                    assert!(!["显示密码", "隐藏密码"].contains(&text.galley.text()));
                    if text.galley.text() == "SYNTHETIC-visible-password" {
                        password_visible = true;
                        assert!(
                            text.galley.job.sections.iter().all(|section| section
                                .format
                                .font_id
                                .size
                                >= 16.0)
                        );
                    }
                }
            }
            assert!(
                password_visible,
                "password must stay visible at {scale}x after {time} seconds"
            );
        }
    }
}

#[test]
fn every_application_dialog_is_centered_and_fits_after_resizing() {
    for id in [
        "credential-details",
        "inventory-details",
        "shortcut-details",
        "settings-details",
        "website-details",
        "export-details",
        "manage-tags",
        "assign-tags",
        "contact-details",
        "contact-chat",
        "theme-editor",
        "tool-details",
    ] {
        let directory = tempfile::tempdir().expect("isolated data");
        let mut app = PersonalDeskApp::open_data_directory(directory.path().to_path_buf());
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        match id {
            "credential-details" => app.credential_dialog_open = true,
            "inventory-details" => app.inventory_dialog_open = true,
            "shortcut-details" => app.shortcut_dialog_open = true,
            "website-details" => app.website_dialog_open = true,
            "export-details" => app.export_dialog_open = true,
            "theme-editor" => app.theme_editor_open = true,
            "tool-details" => {
                app.tool_draft = Some(crate::launcher::AppTool::new(
                    directory.path().join("test.exe"),
                ))
            }

            "contact-details" => app.contact_dialog_open = true,
            "contact-chat" => {
                let path = directory.path().join("chat.xlsx");
                fs::write(&path, contacts::tests::workbook_fixture()).unwrap();
                let mut chat = contacts::import_chat(&path).unwrap();
                chat.title = "很长的聊天记录文件名称".repeat(30);
                app.contact_chat_open = Some(chat.id.clone());
                app.contact_draft.chats.push(chat);
            }
            "manage-tags" => app.open_tag_manager(TagScope::Credentials),
            "assign-tags" => {
                app.tag_assignment = Some(TagAssignment {
                    scope: TagScope::Credentials,
                    id: "synthetic".into(),
                    selected: vec![],
                })
            }
            _ => app.settings_detail = Some("本地数据".into()),
        }
        for size in [
            [1280.0, 800.0],
            [1000.0, 680.0],
            [1440.0, 900.0],
            [1000.0, 680.0],
        ] {
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size.into());
            for _ in 0..8 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(viewport),
                        ..Default::default()
                    },
                    |ui| app.render_workspace(ui),
                );
                output.textures_delta.clear();
            }
            let rect = ctx
                .memory(|memory| memory.area_rect(egui::Id::unique(id)))
                .expect("dialog exists");
            if matches!(id, "credential-details" | "inventory-details") {
                assert!(
                    rect.height() > 400.0,
                    "{id} must not collapse its scrolling content: {rect:?}"
                );
            }
            assert!(
                viewport.contains_rect(rect),
                "{id} overflows {size:?}: {rect:?}"
            );
            assert!(
                (rect.center() - viewport.center()).length() < 2.0,
                "{id} off center in {size:?}: {rect:?}"
            );
        }
    }
}
