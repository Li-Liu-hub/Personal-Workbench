#[test]
fn tools_export_snapshots_queue_profile_and_settings_without_live_browser_or_token() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let hiring = data.join("hiring");
    let mut store = super::store::Store::open(&hiring).unwrap();
    store
        .add(&super::store::Job {
            url: "https://www.zhipin.com/job_detail/a.html".into(),
            ..Default::default()
        })
        .unwrap();
    super::Settings::default().save(&hiring).unwrap();
    std::fs::write(hiring.join("profile.md"), "test profile").unwrap();
    std::fs::write(hiring.join("otp_token.txt"), "secret test token").unwrap();
    std::fs::create_dir_all(hiring.join("boss-browser")).unwrap();
    std::fs::write(hiring.join("boss-browser/Cookies"), "test login").unwrap();
    let second = store.create_account("账号2").unwrap();
    let profile = second.browser_directory(&hiring).join("boss-browser");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(profile.join("Cookies"), "second login").unwrap();
    let path = d.path().join("tools.zip");
    crate::export::write(&data, &path, crate::export::Scope::Tools).unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    for name in [
        "data/hiring/hiring.sqlite",
        "data/hiring/profile.md",
        "data/hiring/settings.json",
    ] {
        assert!(archive.by_name(name).is_ok());
    }
    assert!(
        !archive
            .file_names()
            .any(|n| n.contains("token") || n.contains("Cookies") || n.contains("-wal"))
    );
    let target = d.path().join("restored.sqlite");
    std::io::copy(
        &mut archive.by_name("data/hiring/hiring.sqlite").unwrap(),
        &mut std::fs::File::create(&target).unwrap(),
    )
    .unwrap();
    let connection = rusqlite::Connection::open(target).unwrap();
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM queue", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM hiring_accounts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM queue_identity", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
