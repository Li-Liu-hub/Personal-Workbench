//! `PERSONAL_DESK_LOCAL_ONLY=1` 时导出降级为纯本地数据，并把降级写进导出警告。
//!
//! 本文件只有一个测试用例：它在进程内设置环境变量，整个测试二进制因此不会与
//! 其他用例并发读写进程环境；测试只使用临时目录，不联网、不碰真实数据目录。
#![cfg(debug_assertions)]

use std::io::Read;

use personal_desk::{
    db::{Database, LOCAL_ONLY_ENV},
    export::{self, Scope},
    model::InventoryItem,
};

#[test]
fn local_only_environment_degrades_export_with_a_visible_warning() {
    assert_eq!(LOCAL_ONLY_ENV, "PERSONAL_DESK_LOCAL_ONLY");

    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    {
        let mut db = Database::open_local(&data).unwrap();
        db.save_inventory_item(&InventoryItem {
            id: "item-1".into(),
            name: "键盘".into(),
            quantity: 2,
            ..InventoryItem::default()
        })
        .unwrap();
    }

    let baseline = export::write(&data, &temp.path().join("baseline.zip"), Scope::All).unwrap();
    assert!(
        baseline.warnings == 0,
        "本地数据正常导出不应产生警告，实际：{}",
        baseline.warnings
    );

    // SAFETY: 该测试二进制只有这一个用例，且没有其他线程读取进程环境变量。
    unsafe { std::env::set_var(LOCAL_ONLY_ENV, "1") };
    let degraded = export::write(&data, &temp.path().join("degraded.zip"), Scope::All);
    unsafe { std::env::remove_var(LOCAL_ONLY_ENV) };
    let degraded = degraded.expect("本地模式导出必须成功");

    assert_eq!(
        degraded.warnings,
        baseline.warnings + 1,
        "降级导出必须恰好增加一条警告"
    );
    assert!(degraded.path.is_file(), "降级导出仍然必须生成 ZIP");

    let mut archive = zip::ZipArchive::new(std::fs::File::open(&degraded.path).unwrap()).unwrap();
    let mut manifest = String::new();
    archive
        .by_name("导出清单.json")
        .unwrap()
        .read_to_string(&mut manifest)
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    let warnings = manifest["warnings"]
        .as_array()
        .expect("导出清单必须带 warnings 数组");
    println!(
        "降级导出警告数 {}（基线 {}）；导出清单 warnings={warnings:?}",
        degraded.warnings, baseline.warnings
    );
    assert!(
        warnings.iter().any(|warning| warning
            .as_str()
            .is_some_and(|text| text.contains("私网加密库不可用，本次导出只包含本机数据"))),
        "导出清单必须包含降级说明：{warnings:?}"
    );
}
