use super::{
    editor_tests::{input_frame, pointer},
    *,
};
use sha2::{Digest, Sha256};

#[derive(Debug)]
struct ExternalFile(PathBuf);
impl egui::DroppedFile for ExternalFile {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> std::result::Result<Vec<u8>, String> {
        fs::read(&self.0).map_err(|e| e.to_string())
    }
}

#[test]
fn external_drop_and_ctrl_c_do_not_copy_or_import_files() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let file = temp.path().join("outside.exe");
    fs::write(&file, "Do not execute or copy").unwrap();
    let mut app = PersonalDeskApp::open_data_directory(data.clone());
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    assert!(files::open_in_system(&file).is_err());
    for (index, section) in [Section::Documents, Section::Folders]
        .into_iter()
        .enumerate()
    {
        let time = index as f64;
        app.section = section;
        input_frame(&mut app, &ctx, time, vec![], vec![]);
        input_frame(
            &mut app,
            &ctx,
            time + 0.1,
            vec![
                egui::Event::PointerMoved(egui::pos2(500., 250.)),
                egui::Event::Key {
                    key: egui::Key::C,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                },
            ],
            vec![std::sync::Arc::new(ExternalFile(file.clone()))],
        );
        assert!(
            app.db
                .as_ref()
                .unwrap()
                .document_records()
                .unwrap()
                .is_empty()
        );
        assert!(app.db.as_ref().unwrap().shortcuts().unwrap().is_empty());
        assert!(app.location_request.is_none());
    }
    assert!(!data.join("outside.exe").exists());
    assert_eq!(fs::read_to_string(file).unwrap(), "Do not execute or copy");
}

fn text_at(out: &egui::FullOutput, value: &str) -> egui::Pos2 {
    out.shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == value => Some(t.pos),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {value}"))
}
fn click(app: &mut PersonalDeskApp, ctx: &egui::Context, at: egui::Pos2, t: f64) {
    input_frame(app, ctx, t, pointer(at, true), vec![]);
    input_frame(app, ctx, t + 0.02, pointer(at, false), vec![]);
}

#[test]
fn removing_and_restoring_indices_after_restart_never_changes_the_file_or_folder() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let folder = temp.path().join("external");
    fs::create_dir(&folder).unwrap();
    let file = folder.join("original.md");
    fs::write(&file, b"Original bytes\n").unwrap();
    let before = Sha256::digest(fs::read(&file).unwrap());
    let modified = fs::metadata(&file).unwrap().modified().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(data.clone());
    app.add_document_indices(std::slice::from_ref(&file));
    app.add_shortcut(folder.clone());
    let path = app.db.as_ref().unwrap().document_records().unwrap()[0]
        .path
        .clone();
    let key = path.to_string_lossy().to_string();
    app.db
        .as_mut()
        .unwrap()
        .set_record_tags(TagScope::Documents, &key, &["工作".into()])
        .unwrap();
    app.db
        .as_mut()
        .unwrap()
        .set_favorite("documents", &key, true)
        .unwrap();
    let shortcut = app.db.as_ref().unwrap().shortcuts().unwrap()[0].clone();
    app.remove_index("documents", &key);
    app.remove_index("shortcuts", &shortcut.id);
    assert!(
        app.db
            .as_ref()
            .unwrap()
            .document_records()
            .unwrap()
            .is_empty()
    );
    assert!(app.db.as_ref().unwrap().shortcuts().unwrap().is_empty());
    drop(app);
    let mut app = PersonalDeskApp::open_data_directory(data);
    app.undo_index_removal();
    app.undo_index_removal();
    assert_eq!(
        app.db.as_ref().unwrap().document_records().unwrap()[0].path,
        path
    );
    assert_eq!(app.record_labels(TagScope::Documents, &key), ["工作"]);
    assert!(
        app.db
            .as_ref()
            .unwrap()
            .favorites()
            .unwrap()
            .contains(&("documents".into(), key))
    );
    assert_eq!(
        app.db.as_ref().unwrap().shortcuts().unwrap()[0].id,
        shortcut.id
    );
    assert_eq!(Sha256::digest(fs::read(&file).unwrap()), before);
    assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
    assert_eq!(fs::read_dir(folder).unwrap().count(), 1);
    assert!(!app.index_undo_available());
}

#[test]
fn index_cards_and_rows_only_queue_opening_location_and_offer_metadata_removal() {
    for section in [Section::Folders] {
        for mode in [ViewMode::Grid, ViewMode::List] {
            let temp = tempfile::tempdir().unwrap();
            let file = temp.path().join("index-only.md");
            fs::write(&file, "Original").unwrap();
            let mut app = PersonalDeskApp::open_data_directory(temp.path().join("data"));
            if section == Section::Documents {
                app.add_document_indices(std::slice::from_ref(&file));
            } else {
                app.add_shortcut(file.clone());
            }
            app.section = section;
            app.set_view_mode(mode);
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            input_frame(&mut app, &ctx, 0., vec![], vec![]);
            let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
            let at = text_at(&out, "位置") + egui::vec2(3., 3.);
            click(&mut app, &ctx, at, 0.2);
            assert_eq!(
                app.location_request.take().unwrap().canonicalize().unwrap(),
                file.canonicalize().unwrap()
            );
            for unwanted in [
                "PowerShell",
                "保存文档",
                "复制",
                "移动到…",
                "新建文件夹",
                "重命名文件",
            ] {
                assert!(
                    !out.shapes.iter().any(
                        |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==unwanted)
                    )
                );
            }
            let at = text_at(
                &out,
                if section == Section::Documents {
                    "index-only"
                } else {
                    "index-only.md"
                },
            ) + egui::vec2(3., 3.);
            let secondary = |pressed| {
                vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            input_frame(&mut app, &ctx, 0.4, secondary(true), vec![]);
            input_frame(&mut app, &ctx, 0.42, secondary(false), vec![]);
            let menu = input_frame(&mut app, &ctx, 0.45, vec![], vec![]);
            let remove = text_at(&menu, "删除索引") + egui::vec2(3., 3.);
            click(&mut app, &ctx, remove, 0.5);
            assert!(app.index_undo_available());
            assert_eq!(fs::read_to_string(&file).unwrap(), "Original");
            if section == Section::Documents {
                assert!(
                    app.db
                        .as_ref()
                        .unwrap()
                        .document_records()
                        .unwrap()
                        .is_empty()
                );
            } else {
                assert!(app.db.as_ref().unwrap().shortcuts().unwrap().is_empty());
            }
        }
    }
}

#[test]
fn deleted_missing_index_can_be_restored_without_creating_a_replacement_file() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("missing.md");
    fs::write(&file, "Original").unwrap();
    let mut app = PersonalDeskApp::open_data_directory(temp.path().join("data"));
    app.add_document_indices(std::slice::from_ref(&file));
    let key = app.db.as_ref().unwrap().document_records().unwrap()[0]
        .path
        .to_string_lossy()
        .to_string();
    fs::remove_file(&file).unwrap();
    app.remove_index("documents", &key);
    app.undo_index_removal();
    assert!(!file.exists());
    assert_eq!(
        app.db.as_ref().unwrap().document_records().unwrap().len(),
        1
    );
    assert_eq!(files::location_target(&file).unwrap(), temp.path());
}

#[test]
fn book_grouping_and_external_document_export_store_references_without_touching_sources() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let mut app = PersonalDeskApp::open_data_directory(data.clone());
    let paths = [temp.path().join("a.md"), temp.path().join("b.md")];
    for p in &paths {
        fs::write(p, "Unchanged").unwrap();
    }
    app.add_document_indices(&paths);
    assert!(app.group_document_paths(&paths, None));
    let book = app.db.as_ref().unwrap().books().unwrap().remove(0);
    let key = paths[0]
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    app.remove_index("documents", &key);
    assert_eq!(
        app.db
            .as_ref()
            .unwrap()
            .book_documents(&book.id)
            .unwrap()
            .len(),
        1
    );
    app.undo_index_removal();
    assert_eq!(
        app.db
            .as_ref()
            .unwrap()
            .book_documents(&book.id)
            .unwrap()
            .len(),
        2
    );
    let archive = temp.path().join("export.zip");
    export::write(&data, &archive, export::Scope::Documents).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(archive).unwrap()).unwrap();
    assert!(
        !zip.file_names()
            .any(|n| n.starts_with("external_documents/"))
    );
    let records: serde_json::Value =
        serde_json::from_reader(zip.by_name("文档清单.json").unwrap()).unwrap();
    assert_eq!(records.as_array().unwrap().len(), 2);
    assert!(
        records
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["archive_path"].is_null())
    );
    for p in paths {
        assert_eq!(fs::read_to_string(p).unwrap(), "Unchanged");
    }
}
