//! Release chain version consistency.
//!
//! The desktop client, its Tauri configuration, the npm package and the release
//! history feed are published together; a stale copy would ship an update that
//! `scripts/publish-update.ps1` cannot warn about. Keep every version in sync.

use serde_json::Value;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    let path = workspace_root().join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("无法读取 {}：{error}", path.display()));
    text.trim_start_matches('\u{feff}').to_owned()
}

/// Minimal TOML `[package] version = "…"` reader.
///
/// The release chain only ever uses a plain string version, so a full TOML
/// parser would be an unnecessary dependency here.
fn package_version(manifest: &str) -> String {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package || line.starts_with('#') {
            continue;
        }
        let Some(rest) = line.strip_prefix("version") else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let Some(rest) = rest.trim().strip_prefix('"') else {
            continue;
        };
        let end = rest.find('"').expect("Cargo.toml 的 version 缺少结尾引号");
        assert!(!rest[..end].is_empty(), "Cargo.toml 的 version 不能为空");
        return rest[..end].to_owned();
    }
    panic!("Cargo.toml 缺少 [package] version");
}

fn json_version(relative: &str) -> String {
    let value: Value = serde_json::from_str(&read(relative))
        .unwrap_or_else(|error| panic!("{relative} 不是合法 JSON：{error}"));
    value
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{relative} 缺少字符串 version 字段"))
        .to_owned()
}

fn release_history() -> Vec<Value> {
    let value: Value = serde_json::from_str(&read("assets/release_history.json"))
        .expect("assets/release_history.json 不是合法 JSON");
    value
        .as_array()
        .expect("assets/release_history.json 顶层必须是版本数组")
        .clone()
}

fn version_of(release: &Value, index: usize) -> String {
    release
        .get("version")
        .and_then(Value::as_str)
        .filter(|version| !version.trim().is_empty())
        .unwrap_or_else(|| panic!("版本历史第 {index} 条缺少 version"))
        .to_owned()
}

#[test]
fn package_versions_are_identical_across_the_release_chain() {
    let root = package_version(&read("Cargo.toml"));
    let desktop = package_version(&read("desktop/src-tauri/Cargo.toml"));
    let tauri = json_version("desktop/src-tauri/tauri.conf.json");
    let frontend = json_version("desktop/package.json");
    let history = release_history();
    let newest = version_of(history.first().expect("版本历史不能为空"), 0);

    assert_eq!(root, desktop, "根 crate 与 desktop/src-tauri 版本不一致");
    assert_eq!(root, tauri, "根 crate 与 tauri.conf.json 版本不一致");
    assert_eq!(
        root, frontend,
        "根 crate 与 desktop/package.json 版本不一致"
    );
    assert_eq!(root, newest, "根 crate 与版本历史首项不一致");
    assert_eq!(
        root,
        personal_desk::updater::VERSION,
        "根 crate 编译版本与 Cargo.toml 不一致"
    );
}

#[test]
fn release_history_versions_are_unique_and_strictly_descending() {
    let history = release_history();
    assert!(!history.is_empty(), "版本历史不能为空");

    let mut seen = std::collections::BTreeSet::new();
    let mut previous: Option<semver::Version> = None;
    for (index, release) in history.iter().enumerate() {
        let text = version_of(release, index);
        let version = semver::Version::parse(&text)
            .unwrap_or_else(|error| panic!("版本历史第 {index} 条 {text} 不是合法版本号：{error}"));
        assert!(seen.insert(text.clone()), "版本历史中 {text} 重复出现");
        if let Some(previous) = &previous {
            assert!(
                version < *previous,
                "版本历史必须严格降序：{text} 未小于 {previous}"
            );
        }
        previous = Some(version);

        let title = release
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(!title.trim().is_empty(), "版本 {text} 缺少标题");

        let changes = release
            .get("changes")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("版本 {text} 缺少 changes 数组"));
        assert!(!changes.is_empty(), "版本 {text} 的更新说明为空");
        for change in changes {
            let change = change
                .as_str()
                .unwrap_or_else(|| panic!("版本 {text} 的更新说明必须是字符串"));
            assert!(
                !change.trim().is_empty(),
                "版本 {text} 的更新说明不能为空行"
            );
        }
    }
}
