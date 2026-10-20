use super::{
    editor_tests::{input_frame, pointer},
    *,
};

#[test]
fn terminal_command_bar_keeps_focus_for_consecutive_commands() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
    app.open_terminal(directory.path().to_path_buf());
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.terminal_starting && Instant::now() < deadline {
        app.poll_background();
        thread::sleep(Duration::from_millis(10));
    }
    assert!(app.terminal.is_some(), "{}", app.terminal_error);
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    let mut time = 0.0;
    // Let the interactive shell initialize while the command bar gains focus.
    let ready = Instant::now() + Duration::from_secs(3);
    while Instant::now() < ready {
        input_frame(&mut app, &ctx, time, vec![], vec![]);
        time += 0.02;
        thread::sleep(Duration::from_millis(20));
    }
    for marker in ["FIRST_BAR", "SECOND_BAR"] {
        input_frame(
            &mut app,
            &ctx,
            time,
            vec![
                egui::Event::Text(format!("[Console]::WriteLine(('{marker}_'+(40+2)))")),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            vec![],
        );
        time += 0.02;
        input_frame(
            &mut app,
            &ctx,
            time,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![],
        );
        time += 0.02;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            input_frame(&mut app, &ctx, time, vec![], vec![]);
            time += 0.02;
            let contents = app.terminal.as_ref().unwrap().parser.screen().contents();
            if contents.contains(&format!("{marker}_42")) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "command did not execute: {contents:?}"
            );
            thread::sleep(Duration::from_millis(20));
        }
        assert!(app.terminal_command.is_empty());
    }
}

#[test]
fn terminal_canvas_accepts_focus_text_and_enter_through_real_egui_events() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
    app.open_terminal(directory.path().to_path_buf());
    let startup_deadline = Instant::now() + Duration::from_secs(10);
    while app.terminal_starting && Instant::now() < startup_deadline {
        app.poll_background();
        thread::sleep(Duration::from_millis(10));
    }
    assert!(app.terminal.is_some(), "{}", app.terminal_error);
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
    input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    input_frame(
        &mut app,
        &ctx,
        0.2,
        pointer(egui::pos2(700.0, 350.0), true),
        vec![],
    );
    input_frame(
        &mut app,
        &ctx,
        0.23,
        pointer(egui::pos2(700.0, 350.0), false),
        vec![],
    );
    assert!(app.terminal_focused, "terminal failed to receive focus");
    let ready = Instant::now() + Duration::from_secs(3);
    let mut time = 0.3;
    while Instant::now() < ready {
        input_frame(&mut app, &ctx, time, vec![], vec![]);
        time += 0.02;
        thread::sleep(Duration::from_millis(20));
    }
    input_frame(
        &mut app,
        &ctx,
        time,
        vec![
            egui::Event::Text("[Console]::WriteLine(('UI_RESULT_'+(40+2)))".into()),
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        vec![],
    );
    time += 0.1;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        input_frame(&mut app, &ctx, time, vec![], vec![]);
        time += 0.02;
        if app
            .terminal
            .as_ref()
            .unwrap()
            .parser
            .screen()
            .contents()
            .contains("UI_RESULT_42")
        {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "canvas command did not execute: {:?}",
        app.terminal.as_ref().unwrap().parser.screen().contents()
    );
}

fn position(output: &egui::FullOutput, value: &str) -> egui::Pos2 {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == value => Some(text.pos),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing visible text: {value}"))
}

#[test]
fn additional_module_label_assignment_saves_and_filters_with_search_in_both_views() {
    for (section, scope) in [
        (Section::Folders, TagScope::Shortcuts),
        (Section::Documents, TagScope::Documents),
        (Section::Websites, TagScope::Websites),
        (Section::Contacts, TagScope::Contacts),
        (Section::Tools, TagScope::Tools),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
        let mut ids = vec![];
        for name in ["label-match-one", "label-match-two"] {
            let path = directory.path().join(format!("{name}.exe"));
            fs::write(&path, b"MZ synthetic").unwrap();
            let db = app.db.as_mut().unwrap();
            let id = match scope {
                TagScope::Shortcuts => {
                    db.save_shortcut(name, &path, false).unwrap();
                    db.shortcuts()
                        .unwrap()
                        .into_iter()
                        .find(|s| s.name == name)
                        .unwrap()
                        .id
                }
                TagScope::Documents => {
                    db.set_document_title(&path, name).unwrap();
                    path.canonicalize().unwrap().to_string_lossy().into_owned()
                }
                TagScope::Websites => {
                    let mut item = Website::new();
                    item.title = name.into();
                    item.url = "https://example.test".into();
                    db.save_website(&item).unwrap();
                    item.id
                }
                TagScope::Contacts => {
                    let mut item = Contact::new();
                    item.name = name.into();
                    db.save_contact(&item).unwrap();
                    item.id
                }
                TagScope::Tools => {
                    let mut item = crate::launcher::AppTool::new(path);
                    item.name = name.into();
                    db.save_app_tool(&item).unwrap();
                    item.id
                }
                _ => unreachable!(),
            };
            ids.push(id);
        }
        app.load_websites();
        app.load_contacts();
        app.section = section;
        app.db.as_mut().unwrap().create_tag(scope, "工作").unwrap();
        app.tag_assignment = Some(TagAssignment {
            scope,
            id: ids[0].clone(),
            selected: vec!["工作".into()],
        });
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
        let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        let at = position(&output, "保存标签") + egui::vec2(8.0, 8.0);
        input_frame(&mut app, &ctx, 0.2, pointer(at, true), vec![]);
        input_frame(&mut app, &ctx, 0.23, pointer(at, false), vec![]);
        assert!(app.tag_assignment.is_none(), "{scope:?}");
        assert_eq!(app.record_labels(scope, &ids[0]), ["工作"]);
        app.module_filters
            .insert(scope.key().into(), TagFilter::Named("工作".into()));
        app.search_query = "label-match".into();
        for mode in [ViewMode::Grid, ViewMode::List] {
            let time = if mode == ViewMode::Grid { 1.0 } else { 3.0 };
            app.set_view_mode(mode);
            let output = input_frame(&mut app, &ctx, time, vec![], vec![]);
            position(&output, "label-match-one");
            assert!(!output.shapes.iter().any(
                |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="label-match-two")
            ));
            app.search_query = "missing-query".into();
            let output = input_frame(&mut app, &ctx, time + 1.0, vec![], vec![]);
            assert!(!output.shapes.iter().any(
                |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="label-match-one")
            ));
            app.search_query = "label-match".into();
        }
        app.db
            .as_mut()
            .unwrap()
            .rename_tag(scope, "工作", "学习")
            .unwrap();
        app.sync_tag_edit(scope, "工作", Some("学习"));
        assert_eq!(
            app.module_filters[scope.key()],
            TagFilter::Named("学习".into())
        );
        app.db.as_mut().unwrap().delete_tag(scope, "学习").unwrap();
        app.sync_tag_edit(scope, "学习", None);
        assert_eq!(app.module_filters[scope.key()], TagFilter::All);
    }
}

#[test]
fn file_copy_buttons_and_outbound_drag_work_in_grid_and_list_without_opening_the_file() {
    for mode in [ViewMode::Grid, ViewMode::List] {
        for source in ["browser", "shortcut", "document"] {
            let directory = tempfile::tempdir().unwrap();
            let folder = directory.path().join("files");
            fs::create_dir(&folder).unwrap();
            let path = folder.join("file-to-copy.txt");
            fs::write(&path, "synthetic").unwrap();
            let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
            match source {
                "browser" => app.navigate(folder.clone(), folder),
                "shortcut" => app
                    .db
                    .as_mut()
                    .unwrap()
                    .save_shortcut("file-to-copy.txt", &path, false)
                    .unwrap(),
                _ => {
                    app.db
                        .as_mut()
                        .unwrap()
                        .set_document_title(&path, "file-to-copy.txt")
                        .unwrap();
                    app.section = Section::Documents;
                }
            }
            app.set_view_mode(mode);
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            let mut output = input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
            for time in [0.1, 0.2] {
                output = input_frame(&mut app, &ctx, time, vec![], vec![]);
            }
            let copy = position(&output, "复制") + egui::vec2(5.0, 5.0);
            input_frame(&mut app, &ctx, 1.0, pointer(copy, true), vec![]);
            input_frame(&mut app, &ctx, 1.03, pointer(copy, false), vec![]);
            assert_eq!(
                app.copy_file_request
                    .take()
                    .unwrap()
                    .canonicalize()
                    .unwrap(),
                path.canonicalize().unwrap()
            );
            assert!(!app.editor_visible);
            assert!(app.system_open_request.is_none());
            let output = input_frame(&mut app, &ctx, 2.0, vec![], vec![]);
            let at = position(&output, "file-to-copy.txt") + egui::vec2(8.0, 8.0);
            input_frame(&mut app, &ctx, 2.1, pointer(at, true), vec![]);
            input_frame(
                &mut app,
                &ctx,
                2.2,
                vec![egui::Event::PointerMoved(at + egui::vec2(36.0, 8.0))],
                vec![],
            );
            assert_eq!(
                app.drag_file_request
                    .take()
                    .unwrap()
                    .canonicalize()
                    .unwrap(),
                path.canonicalize().unwrap(),
                "{source} {mode:?}"
            );
            assert!(!app.editor_visible);
            assert!(app.system_open_request.is_none());
            assert_eq!(fs::read_to_string(&path).unwrap(), "synthetic");
        }
    }
}

#[test]
fn file_type_filter_combines_with_search_and_view_switching() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("files");
    fs::create_dir(&folder).unwrap();
    for name in ["match-note.MD", "match-image.PNG", "other-note.txt"] {
        fs::write(folder.join(name), "synthetic").unwrap();
    }
    fs::create_dir(folder.join("match-folder")).unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
    app.navigate(folder.clone(), folder);
    app.search_query = "match".into();
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    for mode in [ViewMode::Grid, ViewMode::List] {
        app.set_view_mode(mode);
        for (filter, expected) in [
            (files::FileFilter::Documents, "match-note.MD"),
            (files::FileFilter::Images, "match-image.PNG"),
            (files::FileFilter::Folders, "match-folder"),
        ] {
            app.file_filter = filter;
            let output = input_frame(&mut app, &ctx, 1.0, vec![], vec![]);
            position(&output, expected);
            for shape in &output.shapes {
                if let egui::Shape::Text(text) = &shape.shape {
                    let name = text.galley.text();
                    if [
                        "match-note.MD",
                        "match-image.PNG",
                        "other-note.txt",
                        "match-folder",
                    ]
                    .contains(&name)
                    {
                        assert_eq!(name, expected);
                    }
                }
            }
        }
    }
}

#[test]
fn unified_toolbar_stays_on_one_line_without_instruction_panels_at_supported_sizes() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    for size in [[1000.0, 680.0], [1280.0, 800.0], [1920.0, 1080.0]] {
        for section in [
            Section::Folders,
            Section::Credentials,
            Section::Documents,
            Section::Inventory,
            Section::Contacts,
            Section::Websites,
            Section::Tools,
            Section::Settings,
        ] {
            app.section = section;
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
            let title = app.page_title();
            let positions = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some((t.galley.text(), t.pos)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let heading = positions
                .iter()
                .find(|(value, at)| *value == title && at.x > 204.0)
                .unwrap()
                .1;
            for label in ["导出", "宫格", "列表"] {
                let at = position(&output, label);
                assert!(
                    (at.y - heading.y).abs() < 24.0,
                    "{section:?} {size:?} {label}"
                );
                assert!(at.x >= 204.0 && at.x < size[0] - 20.0);
            }
            assert_eq!(
                positions
                    .iter()
                    .filter(|(value, at)| *value == title && at.x > 204.0)
                    .count(),
                1
            );
            for unwanted in [
                "我的空间",
                "我的办公",
                "视图",
                "只属于你的空间",
                "让日常井然有序",
                "办公台已就绪",
            ] {
                assert!(!positions.iter().any(|(text, _)| *text == unwanted));
            }
            assert!(!positions.iter().any(
                |(text, _)| text.starts_with("点击条目") || text.starts_with("点击文件夹浏览")
            ));
        }
    }
}

#[test]
fn every_module_switches_between_grid_and_rows_without_losing_search_or_tags() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
    let folder = directory.path().join("files");
    fs::create_dir(&folder).unwrap();
    for name in ["zz-view-one", "zz-view-two"] {
        fs::write(folder.join(format!("{name}.txt")), "synthetic").unwrap();
        let program = folder.join(format!("{name}.exe"));
        fs::write(&program, b"MZ synthetic").unwrap();
        app.db
            .as_mut()
            .unwrap()
            .save_app_tool(&crate::launcher::AppTool::new(program))
            .unwrap();
        let mut book = crate::library::Book::new();
        book.title = name.into();
        app.db.as_mut().unwrap().save_book(&book).unwrap();
        let mut account = Credential::new();
        account.title = name.into();
        account.tags = vec!["视图标签".into()];
        app.db.as_mut().unwrap().save_credential(&account).unwrap();
        let mut item = InventoryItem::new();
        item.name = name.into();
        item.tags = vec!["视图标签".into()];
        app.db.as_mut().unwrap().save_inventory_item(&item).unwrap();
        let mut site = Website::new();
        site.title = name.into();
        site.url = format!("https://example.test/{name}");
        app.db.as_mut().unwrap().save_website(&site).unwrap();
        let mut contact = Contact::new();
        contact.name = name.into();
        app.db.as_mut().unwrap().save_contact(&contact).unwrap();
        app.create_internal_document();
        app.editor_title = name.into();
        app.editor_text = "synthetic".into();
        app.editor_dirty = true;
        assert!(app.save_editor());
        app.editor_visible = false;
    }
    app.load_summaries();
    app.load_inventory();
    app.load_websites();
    app.load_contacts();
    app.load_tags();
    app.account_filter = TagFilter::Named("视图标签".into());
    app.inventory_filter = TagFilter::Named("视图标签".into());
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    let mut time = 0.0;
    for section in views::SECTIONS.into_iter().filter(|s| {
        !matches!(
            s,
            Section::Games | Section::Codex | Section::Browser | Section::Terminal
        )
    }) {
        app.section = section;
        if section == Section::Folders {
            app.navigate(folder.clone(), folder.clone());
        }
        app.search_query = if section == Section::Settings {
            String::new()
        } else {
            "zz-view".into()
        };
        let names = match section {
            Section::Folders => ["zz-view-one.exe", "zz-view-two.exe"],
            Section::Settings => ["导出全部数据", "本地数据"],
            _ => ["zz-view-one", "zz-view-two"],
        };
        app.set_view_mode(ViewMode::Grid);
        time += 1.0;
        input_frame(&mut app, &ctx, time, vec![], vec![]);
        time += 0.1;
        let output = input_frame(&mut app, &ctx, time, vec![], vec![]);
        assert!(
            (position(&output, names[0]).y - position(&output, names[1]).y).abs() < 1.0,
            "{section:?} grid"
        );
        for (label, mode) in [
            ("列表", ViewMode::List),
            ("宫格", ViewMode::Grid),
            ("列表", ViewMode::List),
        ] {
            time += 1.0;
            let output = input_frame(&mut app, &ctx, time, vec![], vec![]);
            let at = position(&output, label) + egui::vec2(7.0, 7.0);
            time += 0.1;
            input_frame(&mut app, &ctx, time, pointer(at, true), vec![]);
            time += 0.03;
            input_frame(&mut app, &ctx, time, pointer(at, false), vec![]);
            time += 0.1;
            let output = input_frame(&mut app, &ctx, time, vec![], vec![]);
            assert_eq!(app.view_mode(), mode, "{section:?}");
            let first = position(&output, names[0]);
            let second = position(&output, names[1]);
            if mode == ViewMode::List {
                assert!(
                    (first.y - second.y).abs() > 45.0,
                    "{section:?} needs separate rows"
                );
                assert!(
                    (first.x - second.x).abs() < 1.0,
                    "{section:?} row names align"
                );
            } else {
                assert!((first.y - second.y).abs() < 1.0, "{section:?} grid");
            }
            assert_eq!(
                app.search_query,
                if section == Section::Settings {
                    ""
                } else {
                    "zz-view"
                }
            );
            assert_eq!(app.account_filter, TagFilter::Named("视图标签".into()));
            assert_eq!(app.inventory_filter, TagFilter::Named("视图标签".into()));
        }
    }
    let restored = PersonalDeskApp::open_data_directory(directory.path().join("data"));
    for section in views::SECTIONS.into_iter().filter(|s| {
        !matches!(
            s,
            Section::Games | Section::Codex | Section::Browser | Section::Terminal
        )
    }) {
        assert_eq!(restored.view_modes.mode(section), ViewMode::List);
    }
}

fn key_event(key: egui::Key) -> Vec<egui::Event> {
    vec![egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]
}

#[test]
fn account_values_are_visible_in_grid_and_list_without_opening_a_dialog() {
    for mode in [ViewMode::Grid, ViewMode::List] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        let mut credential = Credential::new();
        credential.title = "测试账号".into();
        credential.username = "synthetic-visible-account@example.test".into();
        credential.password = "Synthetic-visible-password-0123456789".repeat(3);
        app.db
            .as_mut()
            .unwrap()
            .save_credential(&credential)
            .unwrap();
        app.load_summaries();
        app.section = Section::Credentials;
        app.set_view_mode(mode);
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let output = input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
        for value in [&credential.username, &credential.password] {
            position(&output, value);
        }
        assert!(!app.credential_dialog_open);
        assert_eq!(
            app.db
                .as_ref()
                .unwrap()
                .credential(&credential.id)
                .unwrap()
                .unwrap()
                .password,
            credential.password
        );
    }
}

#[test]
fn choosing_an_inventory_classification_saves_updates_summary_and_filters_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.section = Section::Inventory;
    let mut item = InventoryItem::new();
    item.name = "测试笔记本电脑".into();
    app.db.as_mut().unwrap().save_inventory_item(&item).unwrap();
    app.db
        .as_mut()
        .unwrap()
        .create_tag(TagScope::Inventory, "已买")
        .unwrap();
    app.load_tags();
    app.load_inventory();
    app.inventory_draft = item.clone();
    app.selected_inventory = Some(item.id.clone());
    app.inventory_dialog_open = true;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    let mut output = input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
    for t in [0.1, 0.2] {
        output = input_frame(&mut app, &ctx, t, vec![], vec![]);
    }
    let area = ctx
        .memory(|m| m.area_rect(egui::Id::unique("inventory-details")))
        .unwrap();
    let at = output
        .shapes
        .iter()
        .rev()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == "已买" && area.contains(t.pos) => {
                Some(t.pos + egui::vec2(4.0, 5.0))
            }
            _ => None,
        })
        .unwrap();
    input_frame(&mut app, &ctx, 1.0, pointer(at, true), vec![]);
    output = input_frame(&mut app, &ctx, 1.03, pointer(at, false), vec![]);
    assert_eq!(app.inventory_draft.tags, vec!["已买"]);
    let at = position(&output, "保存物品") + egui::vec2(4.0, 5.0);
    input_frame(&mut app, &ctx, 2.0, pointer(at, true), vec![]);
    input_frame(&mut app, &ctx, 2.03, pointer(at, false), vec![]);
    assert!(!app.inventory_dialog_open);
    assert_eq!(app.inventory_items[0].category, "已买");
    for (index, mode) in [ViewMode::Grid, ViewMode::List].into_iter().enumerate() {
        app.set_view_mode(mode);
        app.inventory_filter = TagFilter::Named("已买".into());
        output = input_frame(&mut app, &ctx, 3.0 + index as f64, vec![], vec![]);
        position(&output, "测试笔记本电脑");
        if mode == ViewMode::Grid {
            position(&output, "1 件 · 已买");
        } else {
            position(&output, "已买");
        }
        assert!(
            !output.shapes.iter().any(
                |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="1 件 · 未分类")
            )
        );
    }
    let db = app.db.as_mut().unwrap();
    db.rename_tag(TagScope::Inventory, "已买", "已拥有")
        .unwrap();
    assert_eq!(db.inventory_items().unwrap()[0].category, "已拥有");
    db.set_record_tags(TagScope::Inventory, &item.id, &[])
        .unwrap();
    drop(app);
    let restored = PersonalDeskApp::open_data_directory(dir.path().into());
    assert!(restored.inventory_items[0].tags.is_empty());
    assert!(restored.inventory_items[0].category.is_empty());
}

#[test]
fn file_context_menus_queue_open_with_for_all_real_file_entry_points_in_both_views() {
    for mode in [ViewMode::Grid, ViewMode::List] {
        for target in ["folder", "shortcut", "indexed", "document"] {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("open-with.txt");
            fs::write(&file, "synthetic file").unwrap();
            let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
            match target {
                "folder" => app.navigate(dir.path().into(), dir.path().into()),
                "shortcut" => app
                    .db
                    .as_mut()
                    .unwrap()
                    .save_shortcut("open-with.txt", &file, false)
                    .unwrap(),
                "indexed" => app.search_results.push(SearchResult {
                    category: "已索引文件".into(),
                    title: "open-with.txt".into(),
                    detail: "text".into(),
                    path: Some(file.to_string_lossy().into()),
                }),
                _ => {
                    app.create_internal_document();
                    app.editor_title = "open-with.txt".into();
                    app.editor_text = "text".into();
                    app.editor_dirty = true;
                    assert!(app.save_editor());
                    app.editor_visible = false;
                    app.section = Section::Documents;
                }
            }
            app.set_view_mode(mode);
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
            let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
            let at = position(&output, "open-with.txt") + egui::vec2(7.0, 7.0);
            for (time, pressed) in [(1.0, true), (1.1, false)] {
                input_frame(
                    &mut app,
                    &ctx,
                    time,
                    vec![
                        egui::Event::PointerMoved(at),
                        egui::Event::PointerButton {
                            pos: at,
                            button: egui::PointerButton::Secondary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    vec![],
                );
            }
            let output = input_frame(&mut app, &ctx, 1.2, vec![], vec![]);
            assert!(
                output.shapes.iter().any(
                    |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="选择程序打开…")
                ),
                "{mode:?} {target} context menu missing"
            );
            let at = position(&output, "选择程序打开…") + egui::vec2(7.0, 7.0);
            input_frame(&mut app, &ctx, 2.0, pointer(at, true), vec![]);
            input_frame(&mut app, &ctx, 2.1, pointer(at, false), vec![]);
            assert!(app.open_with_request.is_some(), "{mode:?} {target}");
            assert!(app.system_open_request.is_none());
        }
    }
    let dir = tempfile::tempdir().unwrap();
    assert!(files::open_with_target(dir.path()).is_err());
    assert!(files::open_with_target(&dir.path().join("missing")).is_err());
}

#[test]
fn snake_keyboard_pause_high_score_and_games_export_work_through_the_app() {
    use crate::games::{Cell, Direction, Status};
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.section = Section::Games;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    for mode in [ViewMode::Grid, ViewMode::List] {
        app.set_view_mode(mode);
        let output = input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
        position(&output, "无尽贪吃蛇");
    }
    app.snake_open = true;
    input_frame(&mut app, &ctx, 1.0, vec![], vec![]);
    let original = app.snake.body[0];
    input_frame(&mut app, &ctx, 1.1, key_event(egui::Key::Space), vec![]);
    assert_eq!(app.snake.status, Status::Running);
    input_frame(&mut app, &ctx, 1.3, vec![], vec![]);
    assert_ne!(app.snake.body[0], original);
    input_frame(&mut app, &ctx, 1.5, key_event(egui::Key::W), vec![]);
    assert_eq!(app.snake.direction, Direction::Up);
    input_frame(&mut app, &ctx, 1.6, key_event(egui::Key::Space), vec![]);
    assert_eq!(app.snake.status, Status::Paused);
    let paused = app.snake.body.clone();
    input_frame(&mut app, &ctx, 20.0, vec![], vec![]);
    assert_eq!(app.snake.body, paused);
    app.snake = crate::games::Snake::new(2);
    app.snake.toggle();
    app.snake.food = Some(Cell { x: 9, y: 10 });
    app.snake_last_time = None;
    input_frame(&mut app, &ctx, 21.0, vec![], vec![]);
    input_frame(&mut app, &ctx, 21.2, vec![], vec![]);
    assert_eq!(app.snake_best, 10);
    let id = app.search_field.unwrap();
    ctx.memory_mut(|m| m.request_focus(id));
    input_frame(&mut app, &ctx, 21.4, key_event(egui::Key::W), vec![]);
    assert_eq!(app.snake.status, Status::Paused);
    let restored = PersonalDeskApp::open_data_directory(dir.path().into());
    assert_eq!(restored.snake_best, 10);
    let path = dir.path().join("game.zip");
    export::write(dir.path(), &path, export::Scope::Games).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let value: serde_json::Value =
        serde_json::from_reader(zip.by_name("小游戏.json").unwrap()).unwrap();
    assert_eq!(value["snake"]["best_score"], 10);
    assert!(zip.by_name("账号密码.json").is_err());
}
#[test]
fn quick_copy_buttons_copy_exact_values_in_both_views_without_opening_details() {
    for mode in [ViewMode::Grid, ViewMode::List] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        let mut item = Credential::new();
        item.title = "复制测试".into();
        item.username = "synthetic-account@example.test".into();
        item.password = "Synthetic-Only-Password".into();
        app.db.as_mut().unwrap().save_credential(&item).unwrap();
        app.load_summaries();
        app.section = Section::Credentials;
        app.set_view_mode(mode);
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let mut time = 1.0;
        for (index, expected) in [&item.username, &item.password].into_iter().enumerate() {
            input_frame(&mut app, &ctx, time, vec![], vec![]);
            time += 0.1;
            let output = input_frame(&mut app, &ctx, time, vec![], vec![]);
            let buttons = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if matches!(text.galley.text(), "复制" | "已复制") => {
                        Some(text.pos + egui::vec2(7.0, 7.0))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(buttons.len(), 2);
            time += 0.1;
            input_frame(&mut app, &ctx, time, pointer(buttons[index], true), vec![]);
            time += 0.02;
            let result = input_frame(&mut app, &ctx, time, pointer(buttons[index], false), vec![]);
            assert!(result.platform_output.commands.iter().any(
                |command| matches!(command,egui::OutputCommand::CopyText(value) if value==expected)
            ));
            assert!(!app.credential_dialog_open);
            time += 2.0;
        }
    }
}
#[test]
fn website_buttons_queue_direct_open_and_tools_double_click_launch_in_both_views() {
    for mode in [ViewMode::Grid, ViewMode::List] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
        let mut site = Website::new();
        site.title = "测试网站".into();
        site.url = "https://example.test/path".into();
        app.db.as_mut().unwrap().save_website(&site).unwrap();
        app.load_websites();
        app.section = Section::Websites;
        app.set_view_mode(mode);
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        input_frame(&mut app, &ctx, 1.0, vec![], vec![]);
        let output = input_frame(&mut app, &ctx, 1.1, vec![], vec![]);
        let at = position(&output, "打开") + egui::vec2(8.0, 7.0);
        input_frame(&mut app, &ctx, 1.2, pointer(at, true), vec![]);
        input_frame(&mut app, &ctx, 1.22, pointer(at, false), vec![]);
        assert_eq!(app.website_open_request.as_ref().unwrap().url, site.url);
        assert!(!app.website_dialog_open);
        let path = dir.path().join("Synthetic.exe");
        fs::write(&path, b"MZ synthetic").unwrap();
        let mut tool = crate::launcher::AppTool::new(path.clone());
        tool.name = "测试工具".into();
        app.db.as_mut().unwrap().save_app_tool(&tool).unwrap();
        app.section = Section::Tools;
        app.set_view_mode(mode);
        let output = input_frame(&mut app, &ctx, 2.0, vec![], vec![]);
        let at = position(&output, "测试工具") + egui::vec2(12.0, 8.0);
        for (time, pressed) in [(2.1, true), (2.12, false), (2.2, true), (2.22, false)] {
            input_frame(&mut app, &ctx, time, pointer(at, pressed), vec![]);
            if time == 2.12 {
                assert!(app.tool_launch_request.is_none());
            }
        }
        assert_eq!(app.tool_launch_request, Some(path.canonicalize().unwrap()));
        assert!(app.tool_draft.is_none());
    }
}
#[test]
fn theme_image_is_copied_persists_after_source_removal_and_exports_with_chats_and_tools() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("data");
    let mut app = PersonalDeskApp::open_data_directory(base.clone());
    let source = dir.path().join("background.png");
    image::RgbaImage::from_pixel(40, 20, image::Rgba([12, 140, 170, 255]))
        .save(&source)
        .unwrap();
    app.theme.accent = [20, 160, 140];
    app.theme.background = Some(source.to_string_lossy().into_owned());
    app.theme.surface_opacity = 0.8;
    app.theme.fit = crate::theme::ImageFit::Contain;
    assert!(app.save_theme());
    let relative = app.theme.background.clone().unwrap();
    assert!(relative.starts_with("theme-assets/"));
    fs::remove_file(source).unwrap();
    assert!(crate::theme::load_image(&base.join(&relative)).is_ok());
    let executable = dir.path().join("Tool.exe");
    fs::write(&executable, b"MZ synthetic").unwrap();
    app.db
        .as_mut()
        .unwrap()
        .save_app_tool(&crate::launcher::AppTool::new(executable.clone()))
        .unwrap();
    app.codex
        .current
        .messages
        .push(crate::codex_client::Message {
            role: "user".into(),
            text: "Synthetic chat".into(),
            item_id: String::new(),
        });
    app.codex.save();
    let zip = dir.path().join("all.zip");
    crate::export::write(&base, &zip, export::Scope::All).unwrap();
    let mut archive = zip::ZipArchive::new(fs::File::open(zip).unwrap()).unwrap();
    assert!(archive.by_name("主题.json").is_ok());
    assert!(archive.by_name(&format!("data/{relative}")).is_ok());
    assert!(archive.by_name("小工具.json").is_ok());
    assert!(
        archive
            .file_names()
            .any(|name| name.starts_with("data/codex/") && name.ends_with(".json"))
    );
    assert!(
        !archive
            .file_names()
            .any(|name| name.ends_with("Tool.exe") || name.contains("auth.json"))
    );
    drop(app);
    let restored = PersonalDeskApp::open_data_directory(base);
    assert_eq!(restored.theme.accent, [20, 160, 140]);
    assert_eq!(restored.theme.fit, crate::theme::ImageFit::Contain);
    assert_eq!(restored.db.as_ref().unwrap().app_tools().unwrap().len(), 1);
    assert_eq!(restored.codex.history.len(), 1);
}

#[test]
fn codex_composer_stays_visible_in_small_windows_and_theme_cancel_restores_saved_state() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().into());
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    app.section = Section::Codex;
    app.desktop.basic_chat = true;
    for size in [[1280.0, 800.0], [1000.0, 680.0]] {
        for frame in 0..4 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                    time: Some(frame as f64),
                    ..Default::default()
                },
                |ui| app.render_workspace(ui),
            );
            if frame == 3 {
                let pos = position(&output, "发送消息");
                assert!(
                    pos.y + 20.0 < size[1] - 24.0,
                    "Composer is clipped at {size:?}: {pos:?}"
                );
            }
            output.textures_delta.clear();
        }
    }
    app.section = Section::Settings;
    app.theme_editor_open = true;
    app.theme.accent = [200, 50, 70];
    let image_path = directory.path().join("draft.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([100, 180, 200, 255]))
        .save(&image_path)
        .unwrap();
    app.theme.background = Some(image_path.to_string_lossy().into_owned());
    input_frame(&mut app, &ctx, 10.0, vec![], vec![]);
    assert!(app.theme_background.is_some());
    let events = vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }];
    input_frame(&mut app, &ctx, 10.1, events, vec![]);
    assert!(!app.theme_editor_open);
    assert_eq!(app.theme, app.theme_saved);
    assert!(app.theme_background.is_none());
    assert!(app.theme.background.is_none());
}
#[test]
fn codex_page_defaults_to_the_original_desktop_view_and_keeps_basic_chat_as_an_explicit_fallback() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().into());
    assert!(!app.desktop.basic_chat);
    app.section = Section::Codex;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 1.0, vec![], vec![]);
    let output = input_frame(&mut app, &ctx, 1.1, vec![], vec![]);
    position(&output, "Codex / ChatGPT 原版界面");
    position(&output, "嵌入现有窗口");
    let area = app.desktop.area.unwrap();
    assert!(area[2] > 900 && area[3] > 650);
    assert!(app.desktop.attach_request.is_none());
    let at = position(&output, "基础聊天") + egui::vec2(8.0, 7.0);
    input_frame(&mut app, &ctx, 1.2, pointer(at, true), vec![]);
    input_frame(&mut app, &ctx, 1.22, pointer(at, false), vec![]);
    assert!(app.desktop.basic_chat);
    let output = input_frame(&mut app, &ctx, 1.4, vec![], vec![]);
    let at = position(&output, "返回原版应用界面") + egui::vec2(8.0, 7.0);
    input_frame(&mut app, &ctx, 1.5, pointer(at, true), vec![]);
    input_frame(&mut app, &ctx, 1.52, pointer(at, false), vec![]);
    assert!(!app.desktop.basic_chat);
}

fn replace_focused_text(value: &str) -> Vec<egui::Event> {
    vec![
        egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            },
        },
        egui::Event::Text(value.into()),
    ]
}

#[test]
fn inline_account_edits_save_cancel_copy_and_open_details_in_both_views() {
    for mode in [ViewMode::Grid, ViewMode::List] {
        let directory = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
        let mut original = Credential::new();
        original.title = "inline-record".into();
        original.username = "synthetic-original-account".into();
        original.password = "synthetic-original-password".into();
        original.website = "https://example.test".into();
        original.notes = "notes stay intact".into();
        original.tags = vec!["工作".into()];
        app.db.as_mut().unwrap().save_credential(&original).unwrap();
        app.load_summaries();
        app.load_tags();
        app.section = Section::Credentials;
        app.set_view_mode(mode);
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
        let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        let at = position(&output, &original.username) + egui::vec2(4.0, 6.0);
        input_frame(&mut app, &ctx, 0.2, pointer(at, true), vec![]);
        input_frame(&mut app, &ctx, 0.23, pointer(at, false), vec![]);
        assert!(ctx.memory(|m| m.has_focus(egui::Id::unique((
            "credential-inline",
            &original.id,
            "username"
        )))));
        input_frame(
            &mut app,
            &ctx,
            0.3,
            replace_focused_text("edited-account"),
            vec![],
        );
        assert!(!app.credential_dialog_open);
        assert_eq!(
            app.inline_credentials[&original.id].username,
            "edited-account"
        );
        assert_eq!(
            app.db
                .as_ref()
                .unwrap()
                .credential(&original.id)
                .unwrap()
                .unwrap()
                .username,
            original.username
        );
        ctx.memory_mut(|m| {
            m.request_focus(egui::Id::unique((
                "credential-inline",
                &original.id,
                "password",
            )))
        });
        input_frame(
            &mut app,
            &ctx,
            0.4,
            replace_focused_text("edited-password"),
            vec![],
        );
        let output = input_frame(&mut app, &ctx, 0.5, vec![], vec![]);
        let copies = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "复制" => {
                    Some(text.pos + egui::vec2(5.0, 5.0))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        input_frame(&mut app, &ctx, 0.6, pointer(copies[1], true), vec![]);
        let copied = input_frame(&mut app, &ctx, 0.63, pointer(copies[1], false), vec![]);
        assert!(
            copied
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c,egui::OutputCommand::CopyText(v) if v=="edited-password"))
        );
        // Saving merges only the three inline fields, retaining separately edited metadata.
        let mut latest = original.clone();
        latest.notes = "updated notes".into();
        latest.tags.push("学习".into());
        app.db.as_mut().unwrap().save_credential(&latest).unwrap();
        let at = position(&output, "保存") + egui::vec2(5.0, 5.0);
        input_frame(&mut app, &ctx, 0.7, pointer(at, true), vec![]);
        input_frame(&mut app, &ctx, 0.73, pointer(at, false), vec![]);
        assert!(app.inline_credentials.is_empty());
        let saved = app
            .db
            .as_ref()
            .unwrap()
            .credential(&original.id)
            .unwrap()
            .unwrap();
        assert_eq!(saved.username, "edited-account");
        assert_eq!(saved.password, "edited-password");
        assert_eq!(saved.notes, latest.notes);
        assert_eq!(saved.tags, latest.tags);
        assert_eq!(saved.website, original.website);
        ctx.memory_mut(|m| {
            m.request_focus(egui::Id::unique((
                "credential-inline",
                &original.id,
                "username",
            )))
        });
        input_frame(
            &mut app,
            &ctx,
            0.8,
            replace_focused_text("discard-this"),
            vec![],
        );
        let output = input_frame(&mut app, &ctx, 0.9, vec![], vec![]);
        let at = position(&output, "取消") + egui::vec2(5.0, 5.0);
        input_frame(&mut app, &ctx, 1.0, pointer(at, true), vec![]);
        input_frame(&mut app, &ctx, 1.03, pointer(at, false), vec![]);
        assert!(app.inline_credentials.is_empty());
        let output = input_frame(&mut app, &ctx, 1.1, vec![], vec![]);
        position(&output, "edited-account");
        let at = position(&output, "详情") + egui::vec2(5.0, 5.0);
        input_frame(&mut app, &ctx, 1.2, pointer(at, true), vec![]);
        input_frame(&mut app, &ctx, 1.23, pointer(at, false), vec![]);
        assert!(app.credential_dialog_open);
        assert_eq!(app.credential_draft.username, "edited-account");
        let restored = PersonalDeskApp::open_data_directory(directory.path().join("data"));
        assert_eq!(restored.credentials[0].password, "edited-password");
    }
}

#[test]
fn visible_search_types_filters_clears_and_automatically_loads_indexed_content() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("files");
    fs::create_dir(&folder).unwrap();
    let path = folder.join("body-only-match.txt");
    fs::write(&path, "unique-body-needle").unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
    app.db
        .as_mut()
        .unwrap()
        .save_shortcut("files", &folder, true)
        .unwrap();
    let root_id = app.db.as_ref().unwrap().shortcuts().unwrap()[0].id.clone();
    app.db
        .as_mut()
        .unwrap()
        .index_file(&root_id, &path, "body-only-match.txt", "unique-body-needle")
        .unwrap();
    app.navigate(folder.clone(), folder);
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
    let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    position(&output, "搜索");
    position(&output, "清空");
    ctx.memory_mut(|m| m.request_focus(app.search_field.unwrap()));
    input_frame(
        &mut app,
        &ctx,
        0.2,
        vec![egui::Event::Text("unique-body-needle".into())],
        vec![],
    );
    assert!(app.search_results.is_empty());
    let output = input_frame(&mut app, &ctx, 0.6, vec![], vec![]);
    position(&output, "body-only-match.txt");
    assert_eq!(
        app.search_results
            .iter()
            .filter(|r| r.category == "已索引文件")
            .count(),
        1
    );
    app.set_view_mode(ViewMode::Grid);
    let output = input_frame(&mut app, &ctx, 0.7, vec![], vec![]);
    position(&output, "body-only-match.txt");
    let at = position(&output, "清空") + egui::vec2(5.0, 5.0);
    input_frame(&mut app, &ctx, 0.8, pointer(at, true), vec![]);
    input_frame(&mut app, &ctx, 0.83, pointer(at, false), vec![]);
    assert!(app.search_query.is_empty());
    assert!(app.search_results.is_empty());
    let mut account = Credential::new();
    account.title = "search-account".into();
    account.password = "synthetic-search-password".into();
    account.notes = "search-notes-needle".into();
    account.tags = vec!["工作".into()];
    app.db.as_mut().unwrap().save_credential(&account).unwrap();
    app.load_summaries();
    app.load_tags();
    app.section = Section::Credentials;
    for (query, tag, visible) in [
        ("search-password", "工作", true),
        ("notes-needle", "工作", true),
        ("notes-needle", "学习", false),
    ] {
        app.search_query = query.into();
        app.account_filter = TagFilter::Named(tag.into());
        let output = input_frame(&mut app, &ctx, 1.0, vec![], vec![]);
        assert_eq!(
            output.shapes.iter().any(
                |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="search-account")
            ),
            visible
        );
    }
}

#[test]
fn classification_dropdown_saves_from_account_cards_and_editable_rows_without_opening_details() {
    for mode in [ViewMode::Grid, ViewMode::List] {
        for section in [Section::Credentials, Section::Websites, Section::Inventory] {
            if mode == ViewMode::Grid && section != Section::Credentials {
                continue;
            }
            let directory = tempfile::tempdir().unwrap();
            let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
            let (id, scope) = match section {
                Section::Credentials => {
                    let mut item = Credential::new();
                    item.title = "dropdown-item".into();
                    app.db.as_mut().unwrap().save_credential(&item).unwrap();
                    (item.id, TagScope::Credentials)
                }
                Section::Websites => {
                    let mut item = Website::new();
                    item.title = "dropdown-item".into();
                    item.url = "https://example.test".into();
                    app.db.as_mut().unwrap().save_website(&item).unwrap();
                    (item.id, TagScope::Websites)
                }
                _ => {
                    let mut item = InventoryItem::new();
                    item.name = "dropdown-item".into();
                    app.db.as_mut().unwrap().save_inventory_item(&item).unwrap();
                    (item.id, TagScope::Inventory)
                }
            };
            app.db
                .as_mut()
                .unwrap()
                .create_tag(scope, "dropdown-choice")
                .unwrap();
            app.load_summaries();
            app.load_websites();
            app.load_inventory();
            app.load_tags();
            app.section = section;
            app.set_view_mode(mode);
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
            let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
            let at = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == "未分类" => {
                        Some(t.pos + egui::vec2(4.0, 5.0))
                    }
                    _ => None,
                })
                .next_back()
                .unwrap();
            input_frame(&mut app, &ctx, 0.2, pointer(at, true), vec![]);
            input_frame(&mut app, &ctx, 0.23, pointer(at, false), vec![]);
            let output = input_frame(&mut app, &ctx, 0.3, vec![], vec![]);
            let at = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == "dropdown-choice" => {
                        Some(t.pos + egui::vec2(4.0, 5.0))
                    }
                    _ => None,
                })
                .next_back()
                .unwrap();
            input_frame(&mut app, &ctx, 0.4, pointer(at, true), vec![]);
            input_frame(&mut app, &ctx, 0.43, pointer(at, false), vec![]);
            if section == Section::Inventory {
                assert!(app.save_inline_inventory(&id));
            }
            assert_eq!(
                app.record_labels(scope, &id),
                ["dropdown-choice"],
                "{section:?} {mode:?}"
            );
            assert!(
                !app.credential_dialog_open
                    && !app.website_dialog_open
                    && !app.inventory_dialog_open
            );
            if section == Section::Websites {
                assert_eq!(
                    app.db.as_ref().unwrap().websites().unwrap()[0].category,
                    "dropdown-choice"
                );
            }
            let restored = PersonalDeskApp::open_data_directory(directory.path().join("data"));
            assert_eq!(restored.record_labels(scope, &id), ["dropdown-choice"]);
            app.save_record_classification(scope, &id, &[]);
            drop(app);
            let restored = PersonalDeskApp::open_data_directory(directory.path().join("data"));
            assert!(restored.record_labels(scope, &id).is_empty());
        }
    }
}

#[test]
fn update_entry_opens_centered_dialog_and_does_not_restart_with_unsaved_accounts() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
    let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    let at =
        position(&output, &format!("更新  v{}", crate::updater::VERSION)) + egui::vec2(5.0, 5.0);
    input_frame(&mut app, &ctx, 0.2, pointer(at, true), vec![]);
    input_frame(&mut app, &ctx, 0.23, pointer(at, false), vec![]);
    assert!(app.updates.open);
    let output = input_frame(&mut app, &ctx, 0.3, vec![], vec![]);
    position(&output, "检查更新");
    input_frame(&mut app, &ctx, 0.31, vec![], vec![]);
    input_frame(&mut app, &ctx, 0.32, vec![], vec![]);
    let area = ctx
        .memory(|m| m.area_rect(egui::Id::unique("app-update")))
        .unwrap();
    assert!(
        (area.center() - egui::pos2(640.0, 400.0)).length() < 2.0,
        "{area:?}"
    );
    let payload = directory.path().join("payload.exe");
    fs::write(&payload, b"MZ synthetic").unwrap();
    app.updates.set_test_release(crate::updater::Release {
        manifest: crate::updater::Manifest {
            app: "personal-desk".into(),
            version: "99.0.0".into(),
            executable: "payload.exe".into(),
            sha256: crate::updater::digest(&payload).unwrap(),
            size: 12,
            notes: "fixture".into(),
        },
        executable: payload,
    });
    app.inline_credentials
        .insert("unsaved".into(), CredentialSummary::default());
    let output = input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
    let at = position(&output, "安装更新并重启") + egui::vec2(5.0, 5.0);
    input_frame(&mut app, &ctx, 0.5, pointer(at, true), vec![]);
    input_frame(&mut app, &ctx, 0.53, pointer(at, false), vec![]);
    let output = input_frame(&mut app, &ctx, 0.6, vec![], vec![]);
    position(&output, "请先保存或取消账号修改");
    assert!(app.inline_credentials.contains_key("unsaved"));
    app.inline_credentials.clear();
    app.file_operations = 1;
    let output = input_frame(&mut app, &ctx, 0.7, vec![], vec![]);
    let at = position(&output, "安装更新并重启") + egui::vec2(5.0, 5.0);
    input_frame(&mut app, &ctx, 0.8, pointer(at, true), vec![]);
    input_frame(&mut app, &ctx, 0.83, pointer(at, false), vec![]);
    let output = input_frame(&mut app, &ctx, 0.9, vec![], vec![]);
    position(&output, "当前操作完成后即可更新");
}

#[test]
fn ctrl_z_restores_deleted_document_body_title_tags_and_search_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let mut app = PersonalDeskApp::open_data_directory(data.clone());
    let path = data.join("documents/restore.md");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "unique-restored-body").unwrap();
    let db = app.db.as_mut().unwrap();
    db.set_document_title(&path, "Restored title").unwrap();
    let key = db.document_record_key(&path).unwrap();
    db.set_record_tags(TagScope::Documents, &key, &["keep-tag".into()])
        .unwrap();
    db.index_file("documents", &path, "Restored title", "unique-restored-body")
        .unwrap();
    app.delete_document(&path);
    assert!(!path.exists());
    drop(app);
    let mut app = PersonalDeskApp::open_data_directory(data);
    app.section = Section::Documents;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
    input_frame(
        &mut app,
        &ctx,
        0.1,
        vec![egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        }],
        vec![],
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "unique-restored-body");
    let db = app.db.as_mut().unwrap();
    let key = db.document_record_key(&path).unwrap();
    assert_eq!(
        db.record_tags(TagScope::Documents, &key).unwrap(),
        ["keep-tag"]
    );
    assert_eq!(db.document_records().unwrap()[0].title, "Restored title");
    assert_eq!(db.search_files("unique-restored-body").unwrap().len(), 1);
    assert!(!app.undo.available());
}

#[test]
fn reopening_a_contact_resets_scroll_to_name_and_keeps_new_fields() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
    app.section = Section::Contacts;
    app.contact_draft = Contact::new();
    app.contact_draft.name = "Visible person".into();
    app.contact_draft.birthday = "2000-02-29".into();
    app.contact_draft.major = "Computer Science".into();
    app.contact_dialog_open = true;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    for n in 0..4 {
        input_frame(&mut app, &ctx, n as f64 * 0.1, vec![], vec![]);
    }
    input_frame(
        &mut app,
        &ctx,
        0.5,
        vec![
            egui::Event::PointerMoved(egui::pos2(700.0, 500.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -1500.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        vec![],
    );
    app.contact_dialog_open = false;
    input_frame(&mut app, &ctx, 0.6, vec![], vec![]);
    app.contact_dialog_open = true;
    for n in 7..11 {
        input_frame(&mut app, &ctx, n as f64 * 0.1, vec![], vec![]);
    }
    let output = input_frame(&mut app, &ctx, 1.2, vec![], vec![]);
    let name = position(&output, "姓名");
    assert!(name.y > 0.0 && name.y < 400.0, "{name:?}");
    assert!(position(&output, "学校").y > name.y);
    assert!(app.save_contact_draft());
    let saved = &app.db.as_ref().unwrap().contacts().unwrap()[0];
    assert_eq!(saved.birthday, "2000-02-29");
    assert_eq!(saved.major, "Computer Science");
}

#[test]
fn stale_book_draft_recovers_as_a_copy_and_failure_retains_the_original_draft() {
    use crate::library::{Book, Page};
    let directory = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
    let db = app.db.as_mut().unwrap();
    let book = Book::new();
    db.save_book(&book).unwrap();
    let mut page = Page::new(book.id.clone());
    page.body = "old".into();
    db.save_book_page(&mut page).unwrap();
    let mut draft = page.clone();
    draft.body = "unsaved draft".into();
    db.save_book_draft("old-writer", &draft).unwrap();
    page.body = "newer saved body".into();
    db.save_book_page(&mut page).unwrap();
    let key = format!("old-writer:{}", draft.id);
    assert!(app.recover_book_draft(&key, &draft));
    let db = app.db.as_mut().unwrap();
    let pages = db.book_pages(&book.id).unwrap();
    assert!(
        pages
            .iter()
            .any(|p| p.id == page.id && p.body == "newer saved body")
    );
    assert!(
        pages
            .iter()
            .any(|p| p.id != page.id && p.body == "unsaved draft")
    );
    assert!(
        db.book_drafts(&book.id, "another-writer")
            .unwrap()
            .is_empty()
    );
    draft.title = " ".into();
    db.save_book_draft("old-writer", &draft).unwrap();
    // A deliberately invalid parent cannot receive a recovered page.
    draft.book_id = "missing-book".into();
    assert!(!app.recover_book_draft(&key, &draft));
    assert_eq!(
        app.db
            .as_ref()
            .unwrap()
            .book_drafts(&book.id, "another-writer")
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn favorites_pin_records_in_both_views_survive_restart_and_do_not_open_details() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let mut app = PersonalDeskApp::open_data_directory(data.clone());
    app.section = Section::Contacts;
    let mut first = Contact::new();
    first.name = "pin-target".into();
    let mut second = Contact::new();
    second.name = "ordinary-contact".into();
    app.db.as_mut().unwrap().save_contact(&first).unwrap();
    app.db.as_mut().unwrap().save_contact(&second).unwrap();
    app.load_contacts();
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
    let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    let stars = output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == "☆" => Some(t.pos + egui::vec2(4.0, 6.0)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let at = *stars.last().unwrap();
    input_frame(&mut app, &ctx, 0.2, pointer(at, true), vec![]);
    input_frame(&mut app, &ctx, 0.23, pointer(at, false), vec![]);
    assert!(app.is_favorite("contacts", &first.id));
    assert!(!app.contact_dialog_open);
    for mode in [ViewMode::List, ViewMode::Grid] {
        app.set_view_mode(mode);
        let output = input_frame(&mut app, &ctx, 1.0, vec![], vec![]);
        let first = position(&output, "pin-target");
        let second = position(&output, "ordinary-contact");
        assert!(first.y < second.y || (first.y == second.y && first.x < second.x));
    }
    drop(app);
    let app = PersonalDeskApp::open_data_directory(data);
    assert!(app.is_favorite("contacts", &first.id));
}

#[test]
fn file_navigation_toolbar_remains_visible_when_contents_scroll() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("files");
    fs::create_dir(&folder).unwrap();
    for i in 0..80 {
        fs::write(folder.join(format!("file-{i:02}.txt")), "fixture").unwrap();
    }
    let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
    app.navigate(folder.clone(), folder);
    app.set_view_mode(ViewMode::List);
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
    let before = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    let at = position(&before, "所有快捷路径");
    input_frame(
        &mut app,
        &ctx,
        0.2,
        vec![
            egui::Event::PointerMoved(egui::pos2(800.0, 600.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -1300.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        vec![],
    );
    let after = input_frame(&mut app, &ctx, 0.6, vec![], vec![]);
    assert_eq!(position(&after, "所有快捷路径"), at);
    let first_after = position(&after, "file-00.txt");
    assert!(
        first_after.y < position(&before, "file-00.txt").y - 100.0,
        "{first_after:?}"
    );
}

#[test]
fn inventory_inline_dates_price_and_classification_save_without_dropping_other_fields() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let mut app = PersonalDeskApp::open_data_directory(data.clone());
    let mut item = InventoryItem::new();
    item.name = "owned item".into();
    item.notes = "keep notes".into();
    app.db.as_mut().unwrap().save_inventory_item(&item).unwrap();
    item.purchase_date = "2026-10-01".into();
    item.disposal_date = "2026-10-20".into();
    item.purchase_price_cents = Some(123456);
    item.tags = vec!["已丢弃".into()];
    app.inline_inventory.insert(item.id.clone(), item.clone());
    assert!(app.save_inline_inventory(&item.id));
    item.disposal_date = "2026-02-30".into();
    app.inline_inventory.insert(item.id.clone(), item.clone());
    assert!(!app.save_inline_inventory(&item.id));
    assert!(app.inline_inventory.contains_key(&item.id));
    drop(app);
    let mut app = PersonalDeskApp::open_data_directory(data);
    app.section = Section::Inventory;
    app.set_view_mode(ViewMode::List);
    let saved = &app.inventory_items[0];
    assert_eq!(saved.disposal_date, "2026-10-20");
    assert_eq!(saved.purchase_price_cents, Some(123456));
    assert_eq!(saved.notes, "keep notes");
    assert_eq!(saved.tags, ["已丢弃"]);
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
    let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    position(&output, "购买日期");
    position(&output, "丢弃日期");
    position(&output, "owned item");
}

#[test]
fn contact_classification_changes_in_detail_survive_rename_clear_and_restart() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let mut app = PersonalDeskApp::open_data_directory(data.clone());
    app.section = Section::Contacts;
    let mut contact = Contact::new();
    contact.name = "classified-contact".into();
    app.db.as_mut().unwrap().save_contact(&contact).unwrap();
    app.contact_draft = contact.clone();
    app.contact_dialog_open = true;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
    app.db
        .as_mut()
        .unwrap()
        .create_tag(TagScope::Contacts, "同学")
        .unwrap();
    app.contact_draft_tags = vec!["同学".into()];
    assert!(app.save_contact_draft());
    app.db
        .as_mut()
        .unwrap()
        .rename_tag(TagScope::Contacts, "同学", "朋友")
        .unwrap();
    app.sync_tag_edit(TagScope::Contacts, "同学", Some("朋友"));
    assert_eq!(app.contact_draft_tags, ["朋友"]);
    assert!(app.save_contact_draft());
    drop(app);
    let mut app = PersonalDeskApp::open_data_directory(data.clone());
    assert_eq!(app.record_labels(TagScope::Contacts, &contact.id), ["朋友"]);
    app.contact_draft = contact;
    app.contact_draft_tags.clear();
    assert!(app.save_contact_draft());
    drop(app);
    let app = PersonalDeskApp::open_data_directory(data);
    assert!(
        app.record_labels(TagScope::Contacts, &app.contact_items[0].id)
            .is_empty()
    );
}

#[test]
fn file_background_operations_are_counted_until_their_completion_messages() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.txt");
    fs::write(&source, "keep body").unwrap();
    let target = directory.path().join("destination");
    fs::create_dir(&target).unwrap();
    let mut app = PersonalDeskApp::open_data_directory(directory.path().join("data"));
    app.start_copy(source.clone(), target.clone());
    assert_eq!(app.file_operations, 1);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.file_operations > 0 && Instant::now() < deadline {
        app.poll_background();
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.file_operations, 0);
    assert_eq!(
        fs::read_to_string(target.join("source.txt")).unwrap(),
        "keep body"
    );
    app.start_move(source.clone(), directory.path().join("does-not-exist"));
    assert_eq!(app.file_operations, 1);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.file_operations > 0 && Instant::now() < deadline {
        app.poll_background();
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.file_operations, 0);
    assert!(source.exists());
}
