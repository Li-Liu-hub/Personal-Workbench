use super::*;

#[test]
fn saves_reopens_and_detects_external_edits_without_losing_original() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store {
        directory: dir.path().join("docs/简历"),
    };
    let mut document = store.load().unwrap().document;
    document.name = "示例姓名".into();
    let revision = store.save(&document, None).unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded.document, document);
    assert_eq!(loaded.revision.as_deref(), Some(revision.as_slice()));
    document.name = "修改姓名".into();
    store.save(&document, Some(&revision)).unwrap();
    let previous: Resume =
        serde_json::from_slice(&fs::read(store.directory.join("resume.previous.json")).unwrap())
            .unwrap();
    assert_eq!(previous.name, "示例姓名");
    let before = fs::read(store.directory.join("resume.json")).unwrap();
    assert!(matches!(
        store.save(&Resume::default(), Some(&revision)),
        Err(DeskError::ConcurrentModification)
    ));
    assert_eq!(
        before,
        fs::read(store.directory.join("resume.json")).unwrap()
    );
    fs::write(store.directory.join("resume.json"), "invalid json").unwrap();
    assert!(store.load().is_err());
    assert!(store.save(&document, Some(&revision)).is_err());
    assert_eq!(
        fs::read_to_string(store.directory.join("resume.json")).unwrap(),
        "invalid json"
    );
}

#[test]
fn validates_version_photo_bounds_and_preserves_incomplete_drafts() {
    let mut document = Resume::default();
    assert!(document.validate().is_ok());
    for photo in [
        "../secret.png",
        "C:\\secret.png",
        "photo-/../../secret",
        "photo-x/secret",
    ] {
        document.photo = photo.into();
        assert!(document.validate().is_err());
    }
    document.photo = "photo-local.png".into();
    assert!(document.validate().is_ok());
    document.style.font_pt = f32::NAN;
    assert!(document.validate().is_err());
    document.style.font_pt = 9.;
    document.version = 99;
    assert!(document.validate().is_err());
}

#[test]
fn conflict_resolution_keeps_external_backup_across_later_saves() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store {
        directory: dir.path().to_path_buf(),
    };
    let mut external = Resume {
        name: "外部版本".into(),
        ..Default::default()
    };
    store.save(&external, None).unwrap();
    let external_bytes = fs::read(dir.path().join("resume.json")).unwrap();
    let draft = Resume {
        name: "当前编辑".into(),
        ..external.clone()
    };
    let (revision, backup) = store.save_preserving_external(&draft).unwrap();
    assert_eq!(fs::read(&backup).unwrap(), external_bytes);
    assert_eq!(store.load().unwrap().document, draft);
    external.name = "后续修改".into();
    store.save(&external, Some(&revision)).unwrap();
    assert_eq!(fs::read(backup).unwrap(), external_bytes);
}

#[test]
fn original_format_recovers_legacy_metadata_and_respects_explicit_user_changes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store {
        directory: dir.path().to_path_buf(),
    };
    let mut doc = Resume::default();
    doc.sections[0].entries.push(Entry {
        id: "original".into(),
        summary: "成果：已完成交付".into(),
        highlights: vec!["已完成交付".into()],
        bullets: vec![Bullet {
            title: "工程".into(),
            body: "保留用户新写的内容，使用 Spring Boot 完成服务".into(),
            highlights: vec!["Spring Boot".into()],
        }],
        ..Default::default()
    });
    let mut legacy = serde_json::to_value(&doc).unwrap();
    legacy["style"].as_object_mut().unwrap().remove("line_pt");
    legacy["style"]["font_pt"] = serde_json::json!(10.0);
    let old_entry = &mut legacy["sections"][0]["entries"][0];
    old_entry.as_object_mut().unwrap().remove("highlights");
    old_entry["bullets"][0]
        .as_object_mut()
        .unwrap()
        .remove("highlights");
    fs::write(
        dir.path().join("resume.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .unwrap();
    fs::write(dir.path().join("resume-format.json"), serde_json::to_vec(&serde_json::json!({ "version": 1, "style": Style::default(), "entries": { "original": {"highlights": ["已完成交付"], "bullets": {"工程": ["SpringBoot", "已被删除的关键词"]}}} })).unwrap()).unwrap();
    let mut loaded = store.load().unwrap();
    assert_eq!(loaded.document.style.font_pt, 10.5);
    assert_eq!(loaded.document.style.project_gap_pt, 7.);
    assert_eq!(
        loaded.document.sections[0].entries[0].bullets[0].highlights,
        ["Spring Boot"]
    );
    assert_eq!(
        loaded.document.sections[0].entries[0].bullets[0].body,
        doc.sections[0].entries[0].bullets[0].body
    );
    loaded.document.style.project_gap_pt = 11.;
    loaded.document.sections[0].entries[0].bullets[0]
        .highlights
        .clear();
    store
        .save(&loaded.document, loaded.revision.as_deref())
        .unwrap();
    let reopened = store.load().unwrap().document;
    assert_eq!(reopened.style.project_gap_pt, 11.);
    assert!(
        reopened.sections[0].entries[0].bullets[0]
            .highlights
            .is_empty()
    );
    assert_eq!(reopened, loaded.document);
}

#[test]
fn module_and_full_export_include_docs_and_keep_other_scopes_separate() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let folder = dir.path().join("docs/简历");
    let mut db = crate::db::Database::open(&data).unwrap();
    db.set_setting("resume.directory", &folder.to_string_lossy())
        .unwrap();
    let store = Store { directory: folder };
    store.save(&Resume::default(), None).unwrap();
    fs::write(store.directory.join("original.pdf"), b"original").unwrap();
    for scope in [
        crate::export::Scope::Resume,
        crate::export::Scope::All,
        crate::export::Scope::Folders,
    ] {
        let output = dir.path().join(format!("{}.zip", scope.label()));
        crate::export::write(&data, &output, scope).unwrap();
        let mut zip = zip::ZipArchive::new(fs::File::open(output).unwrap()).unwrap();
        assert_eq!(
            zip.by_name("docs/简历/resume.json").is_ok(),
            scope != crate::export::Scope::Folders
        );
        assert_eq!(
            zip.by_name("docs/简历/original.pdf").is_ok(),
            scope != crate::export::Scope::Folders
        );
    }
}

#[test]
fn photo_import_resizes_and_keeps_source_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.png");
    image::RgbaImage::new(1200, 1600).save(&source).unwrap();
    let before = fs::read(&source).unwrap();
    let store = Store {
        directory: dir.path().join("docs/简历"),
    };
    let name = store.import_photo(&source).unwrap();
    let image = image::open(store.directory.join(name)).unwrap();
    assert!(image.width() <= 600 && image.height() <= 800);
    assert_eq!(before, fs::read(source).unwrap());
}

#[test]
#[ignore = "requires local Typst and Windows fonts"]
fn actual_typst_renders_literal_user_content_and_multiple_pages() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store {
        directory: dir.path().join("docs/简历"),
    };
    let mut document = Resume {
        name: "中文简历 #read(\"secret\") $ \\ [测试]".into(),
        ..Default::default()
    };
    document.sections[0].entries.push(Entry {
        id: "test".into(),
        heading: "不会执行用户内容".into(),
        bullets: (0..30)
            .map(|_| Bullet {
                title: "工程实践".into(),
                body: "这是测试内容：保存、重启、预览、导出。".repeat(15),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    });
    let rendered = TypstRenderer::discover().render(&document, &store).unwrap();
    assert!(rendered.pdf.starts_with(b"%PDF-"));
    assert!(rendered.pages.len() >= 2);
    assert_eq!(rendered.revision, document.revision().unwrap());
    assert!(!dir.path().join("secret").exists());
    let page = image::load_from_memory(&rendered.pages[0]).unwrap();
    assert!(
        page.width() >= 2480,
        "300 PPI A4 preview must preserve text detail"
    );
}

#[test]
#[ignore = "requires local Typst and Windows fonts"]
fn actual_typst_spacing_changes_rendered_bullet_positions() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store {
        directory: dir.path().to_path_buf(),
    };
    let mut document = Resume {
        sections: vec![ResumeSection {
            id: "s".into(),
            title: "Spacing".into(),
            entries: vec![Entry {
                id: "e".into(),
                bullets: ["First", "Second", "Third"]
                    .map(|title| Bullet {
                        title: title.into(),
                        body: "content".into(),
                        ..Default::default()
                    })
                    .to_vec(),
                ..Default::default()
            }],
        }],
        ..Default::default()
    };
    let renderer = TypstRenderer::discover();
    let positions = |document: &Resume| {
        let rendered = renderer.render(document, &store).unwrap();
        let image = image::load_from_memory(&rendered.pages[0])
            .unwrap()
            .to_rgb8();
        let mut starts = vec![];
        let mut previous = false;
        for y in 0..image.height() {
            let blue = (0..image.width() / 2)
                .filter(|x| {
                    let p = image.get_pixel(*x, y);
                    p[0] < 100 && u16::from(p[2]) > u16::from(p[0]) + 45
                })
                .count()
                > 2;
            if blue && !previous {
                starts.push(y as f32 * (210. / 25.4 * 72.) / image.width() as f32);
            }
            previous = blue;
        }
        starts
    };
    let before = positions(&document);
    document.style.project_gap_pt += 10.;
    let after = positions(&document);
    assert_eq!(before.len(), 4, "{before:?}");
    assert_eq!(after.len(), 4, "{after:?}");
    assert!((after[1] - before[1]).abs() <= 0.7);
    assert!((9.0..=11.0).contains(&(after[2] - before[2])));
    assert!((19.0..=21.0).contains(&(after[3] - before[3])));
}
