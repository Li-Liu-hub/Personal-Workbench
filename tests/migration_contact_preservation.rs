use personal_desk::{
    contacts::{ChatRecord, Contact},
    db::Database,
};
#[test]
fn batch_chat_import_deduplicates_and_rolls_back_on_conflicting_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut c = Contact::new();
    c.name = "测试联系人".into();
    c.photo = vec![7];
    c.chat_notes = "不能丢失的备注".into();
    db.save_contact_with_tags(&c, &["同学".into()]).unwrap();
    let chat = |id: &str, bytes: Vec<u8>| ChatRecord {
        id: id.into(),
        title: "记录".into(),
        format: "txt".into(),
        text: "全文".into(),
        sheets: vec![],
        original: bytes,
    };
    assert_eq!(
        db.append_contact_chats(
            &c.id,
            vec![
                chat("first", vec![1]),
                chat("duplicate", vec![1]),
                chat("second", vec![2])
            ]
        )
        .unwrap(),
        2
    );
    let before = db.contact(&c.id).unwrap().unwrap();
    assert_eq!(before.photo, c.photo);
    assert_eq!(before.chat_notes, c.chat_notes);
    assert!(
        db.append_contact_chats(&c.id, vec![chat("third", vec![3]), chat("first", vec![4])])
            .is_err()
    );
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(db.contact(&c.id).unwrap().unwrap(), before);
    assert_eq!(
        db.tagged_records(personal_desk::tags::Scope::Contacts)
            .unwrap()[0]
            .tags,
        vec!["同学"]
    );
}
#[test]
fn updating_contact_fields_preserves_photo_notes_and_original_chat() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut c = Contact::new();
    c.name = "示例".into();
    c.photo = vec![1, 2, 3];
    c.chat_notes = "备注".into();
    c.chats.push(ChatRecord {
        id: "chat".into(),
        title: "对话".into(),
        format: "txt".into(),
        text: "内容".into(),
        sheets: vec![],
        original: vec![4, 5, 6],
    });
    db.save_contact_with_tags(&c, &[]).unwrap();
    let mut edited = c.clone();
    edited.name = "新姓名".into();
    edited.photo.clear();
    edited.chats.clear();
    edited.chat_notes.clear();
    db.update_contact_fields(&edited, &["分类".into()]).unwrap();
    let saved = db.contact(&c.id).unwrap().unwrap();
    assert_eq!(saved.name, "新姓名");
    assert_eq!(saved.photo, c.photo);
    assert_eq!(saved.chat_notes, c.chat_notes);
    assert_eq!(saved.chats, c.chats);
}
#[test]
fn adding_photo_and_chat_preserves_contact_and_deduplicates() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut c = Contact::new();
    c.name = "测试".into();
    c.chat_notes = "保留".into();
    db.save_contact_with_tags(&c, &["分类".into()]).unwrap();
    let chat = ChatRecord {
        id: "chat-one".into(),
        title: "对话".into(),
        format: "txt".into(),
        text: "内容".into(),
        sheets: vec![],
        original: vec![1, 2],
    };
    db.set_contact_photo(&c.id, &[7, 8]).unwrap();
    assert!(db.append_contact_chat(&c.id, chat.clone()).unwrap());
    let mut dup = chat.clone();
    dup.id = "chat-two".into();
    assert!(!db.append_contact_chat(&c.id, dup).unwrap());
    let saved = db.contact(&c.id).unwrap().unwrap();
    assert_eq!(saved.photo, vec![7, 8]);
    assert_eq!(saved.chat_notes, "保留");
    assert_eq!(saved.chats, vec![chat]);
}
#[test]
fn editing_contact_notes_keeps_binary_attachments() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    let mut c = Contact::new();
    c.name = "联系人".into();
    c.photo = vec![8, 9];
    c.chat_notes = "原备注".into();
    c.chats.push(ChatRecord {
        id: "note-chat".into(),
        title: "聊天".into(),
        format: "txt".into(),
        text: "内容".into(),
        sheets: vec![],
        original: vec![1, 2, 3],
    });
    db.save_contact_with_tags(&c, &[]).unwrap();
    let mut edit = c.clone();
    edit.chat_notes = "新备注".into();
    edit.photo.clear();
    edit.chats.clear();
    db.update_contact_details(&edit, &["亲友".into()]).unwrap();
    let saved = db.contact(&c.id).unwrap().unwrap();
    assert_eq!(saved.chat_notes, "新备注");
    assert_eq!(saved.photo, c.photo);
    assert_eq!(saved.chats, c.chats);
}
