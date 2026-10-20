use super::*;
use crate::{db::Database, export};
use std::{
    fs,
    io::{Cursor, Read, Write},
};

pub(crate) fn workbook_fixture() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let files = [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="周一聊天" sheetId="1" r:id="rId1"/><sheet name="周二聊天" sheetId="2" r:id="rId2"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:C2"/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>时间</t></is></c><c r="B1" t="inlineStr"><is><t>发送人</t></is></c><c r="C1" t="inlineStr"><is><t>内容</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>09:00</t></is></c><c r="B2" t="inlineStr"><is><t>测试同学</t></is></c><c r="C2" t="inlineStr"><is><t>下周一起看书 &amp; 交流</t></is></c></row></sheetData></worksheet>"#,
        ),
        (
            "xl/worksheets/sheet2.xml",
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:B2"/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>备注</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>编号</t></is></c><c r="B2" t="inlineStr"><is><t>000123</t></is></c></row></sheetData></worksheet>"#,
        ),
    ];
    for (name, text) in files {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn sample_contact() -> Contact {
    let mut contact = Contact::new();
    contact.name = "测试同学".into();
    contact.school = "示例大学".into();
    contact.work = "软件工程师".into();
    contact.phone = "00123456789".into();
    contact.national_id = "00000000000000000X".into();
    contact.gender = "自定义".into();
    contact.address = "示例路一号".into();
    contact.hobbies = "读书、摄影".into();
    contact.chat_notes = "下周讨论项目".into();
    contact
}

#[test]
fn imports_text_json_utf16_and_multisheet_excel_without_changing_originals() {
    let temp = tempfile::tempdir().unwrap();
    for (name, bytes, expected) in [
        ("记录.txt", "你好\n明天见".as_bytes().to_vec(), "明天见"),
        ("记录.md", b"# Hello\nMarkdown record".to_vec(), "Markdown"),
        (
            "记录.json",
            br#"{"messages":[{"sender":"A","text":"hello","extra":42}]}"#.to_vec(),
            "extra",
        ),
        (
            "UTF16.txt",
            [
                vec![0xff, 0xfe],
                "中文聊天"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            ]
            .concat(),
            "中文聊天",
        ),
        ("记录.xlsx", workbook_fixture(), "下周一起看书"),
    ] {
        let path = temp.path().join(name);
        fs::write(&path, &bytes).unwrap();
        let record = import_chat(&path).unwrap();
        assert!(record.text.contains(expected));
        assert_eq!(record.original, bytes);
        let output = temp.path().join(format!("导出-{name}"));
        export_chat(&record, &output).unwrap();
        assert_eq!(fs::read(&output).unwrap(), bytes);
        if name.ends_with("xlsx") {
            assert_eq!(record.sheets.len(), 2);
            assert_eq!(record.sheets[0].rows[1][2], "下周一起看书 & 交流");
            assert_eq!(record.sheets[1].rows[1][1], "000123");
        }
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    fs::write(temp.path().join("坏.json"), "{broken").unwrap();
    assert!(import_chat(&temp.path().join("坏.json")).is_err());
    fs::write(temp.path().join("坏.xlsx"), "not a workbook").unwrap();
    assert!(import_chat(&temp.path().join("坏.xlsx")).is_err());
    let large = File::create(temp.path().join("大.txt")).unwrap();
    large.set_len(MAX_FILE_BYTES as u64 + 1).unwrap();
    assert!(import_chat(&temp.path().join("大.txt")).is_err());
}

#[test]
fn contacts_persist_all_fields_search_chat_content_and_delete_only_own_attachments() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("原记录.xlsx");
    fs::write(&path, workbook_fixture()).unwrap();
    let chat = import_chat(&path).unwrap();
    let mut contact = sample_contact();
    assert_eq!(
        append_chats(&mut contact, vec![chat.clone(), chat]).unwrap(),
        1
    );
    let mut other = sample_contact();
    other.name = "另一人".into();
    {
        let mut db = Database::open(temp.path()).unwrap();
        db.save_contact(&contact).unwrap();
        db.save_contact(&other).unwrap();
        assert_eq!(db.contacts().unwrap().len(), 2);
    }
    let mut db = Database::open(temp.path()).unwrap();
    let restored = db
        .contacts()
        .unwrap()
        .into_iter()
        .find(|c| c.id == contact.id)
        .unwrap();
    assert_eq!(restored, contact);
    for query in [
        "示例大学",
        "摄影",
        "00000000000000000X",
        "下周一起看书",
        "原记录.xlsx",
    ] {
        assert!(restored.matches(query));
    }
    assert!(!restored.matches("不存在关键词"));
    contact.work = "新工作".into();
    contact.chats.clear();
    db.save_contact(&contact).unwrap();
    assert!(
        db.contacts()
            .unwrap()
            .into_iter()
            .find(|c| c.id == contact.id)
            .unwrap()
            .chats
            .is_empty()
    );
    db.delete_contact(&contact.id).unwrap();
    assert_eq!(db.contacts().unwrap(), vec![other]);
    let connection = rusqlite::Connection::open(temp.path().join("desk.db")).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM contact_chats", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(fs::read(path).unwrap(), workbook_fixture());
}

#[test]
fn contact_chat_save_is_transactional_and_size_failures_preserve_pending_records() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("原件.txt");
    fs::write(&path, "原聊天内容").unwrap();
    let mut contact = sample_contact();
    contact.chats.push(import_chat(&path).unwrap());
    let mut db = Database::open(temp.path()).unwrap();
    db.save_contact(&contact).unwrap();
    let original = contact.clone();
    contact.name = "不应保存的修改".into();
    contact.chats.push(contact.chats[0].clone());
    assert!(db.save_contact(&contact).is_err());
    assert_eq!(db.contacts().unwrap(), vec![original.clone()]);
    contact = original;
    let mut large = import_chat(&path).unwrap();
    large.original = vec![0; MAX_CONTACT_BYTES + 1];
    assert!(append_chats(&mut contact, vec![large]).is_err());
    assert_eq!(contact.chats.len(), 1);
    db.delete_contact(&contact.id).unwrap();
    assert!(db.contacts().unwrap().is_empty());
    let connection = rusqlite::Connection::open(temp.path().join("desk.db")).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM contact_chats", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn module_and_full_exports_keep_contact_chat_originals_and_archive_links() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("聊天.xlsx");
    let bytes = workbook_fixture();
    fs::write(&path, &bytes).unwrap();
    let mut contact = sample_contact();
    contact.chats.push(import_chat(&path).unwrap());
    Database::open(temp.path())
        .unwrap()
        .save_contact(&contact)
        .unwrap();
    for scope in [
        export::Scope::Contacts,
        export::Scope::All,
        export::Scope::Websites,
    ] {
        let archive_path = temp.path().join(format!("{scope:?}.zip"));
        export::write(temp.path(), &archive_path, scope).unwrap();
        let mut zip = zip::ZipArchive::new(File::open(archive_path).unwrap()).unwrap();
        if scope == export::Scope::Websites {
            assert!(zip.by_name("联系人.json").is_err());
            assert!(
                !zip.file_names()
                    .any(|name| name.starts_with("contact_chats/"))
            );
            continue;
        }
        let data: serde_json::Value =
            serde_json::from_reader(zip.by_name("联系人.json").unwrap()).unwrap();
        assert_eq!(data[0]["phone"], contact.phone);
        assert_eq!(data[0]["national_id"], contact.national_id);
        assert!(data[0]["chats"][0].get("original").is_none());
        let member = data[0]["chats"][0]["archive_path"].as_str().unwrap();
        let mut exported = vec![];
        zip.by_name(member)
            .unwrap()
            .read_to_end(&mut exported)
            .unwrap();
        assert_eq!(exported, bytes);
        if scope == export::Scope::All {
            let snapshot = temp.path().join("snapshot.db");
            let mut data = vec![];
            zip.by_name("data/desk.db")
                .unwrap()
                .read_to_end(&mut data)
                .unwrap();
            fs::write(&snapshot, data).unwrap();
            let c = rusqlite::Connection::open(snapshot).unwrap();
            assert_eq!(
                c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                    .unwrap(),
                "ok"
            );
            assert_eq!(
                c.query_row("SELECT original FROM contact_chats", [], |r| r
                    .get::<_, Vec<u8>>(0))
                    .unwrap(),
                bytes
            );
        } else {
            assert!(zip.by_name("账号密码.json").is_err());
        }
    }
}
