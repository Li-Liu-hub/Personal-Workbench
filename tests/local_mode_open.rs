//! debug 回归：显式本地入口与普通打开读取同一份本机数据。
//!
//! release 构建的 `Database::open` 会连接私网加密库，所以本文件只在 debug 下编译；
//! release 的降级行为由 `tests/local_only_export.rs` 用本地模式覆盖。
#![cfg(debug_assertions)]

use std::path::Path;

use personal_desk::{
    db::Database,
    model::{InventoryItem, Website},
};

const ESSAY_DAY: &str = "2026-11-10";

fn seed(data: &Path) -> String {
    let mut db = Database::open(data).unwrap();
    let mut page = db.create_writing("本地模式文档", None, None).unwrap();
    page.body = "只在本地保存的正文".into();
    db.save_book_page(&mut page).unwrap();
    db.ensure_essay_day(chrono::NaiveDate::from_ymd_opt(2026, 11, 10).unwrap())
        .unwrap();
    db.save_inventory_item(&InventoryItem {
        id: "item-1".into(),
        name: "键盘".into(),
        quantity: 2,
        ..InventoryItem::default()
    })
    .unwrap();
    db.save_website_classified(
        &Website {
            id: "site-1".into(),
            title: "Rust".into(),
            url: "https://www.rust-lang.org".into(),
            ..Website::default()
        },
        &[],
    )
    .unwrap();
    db.set_setting("view.folders", "grid").unwrap();
    page.id
}

fn assert_same_records(expected: &Database, actual: &Database, page_id: &str) {
    assert_eq!(
        expected.inventory_items().unwrap(),
        actual.inventory_items().unwrap()
    );
    assert_eq!(expected.websites().unwrap(), actual.websites().unwrap());
    assert_eq!(expected.settings().unwrap(), actual.settings().unwrap());
    assert_eq!(
        serde_json::to_value(expected.essay_assignment(ESSAY_DAY).unwrap()).unwrap(),
        serde_json::to_value(actual.essay_assignment(ESSAY_DAY).unwrap()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(expected.writing_page(page_id).unwrap()).unwrap(),
        serde_json::to_value(actual.writing_page(page_id).unwrap()).unwrap()
    );
}

#[test]
fn local_open_reads_the_same_local_records_as_open() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let page_id = seed(&data);

    let opened = Database::open(&data).unwrap();
    let local = Database::open_local(&data).unwrap();

    assert_same_records(&opened, &local, &page_id);
    assert_eq!(
        local.writing_page(&page_id).unwrap().unwrap().body,
        "只在本地保存的正文"
    );
}

#[test]
fn injectable_local_flag_marks_degraded_mode_and_stays_local() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let page_id = seed(&data);
    let reference = Database::open_local(&data).unwrap();

    let (forced, degraded) = Database::open_or_local_with(&data, true).unwrap();
    assert!(degraded, "强制本地模式必须标记为降级");
    assert_same_records(&reference, &forced, &page_id);

    // debug 构建没有私网加密库，正常打开不会降级。
    let (normal, degraded) = Database::open_or_local_with(&data, false).unwrap();
    assert!(!degraded);
    assert_same_records(&reference, &normal, &page_id);
}
