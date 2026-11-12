use rusqlite::{OptionalExtension, params};

use super::Database;
use crate::{
    contacts::{self, ChatRecord, Contact, ContactSearchItem},
    error::Result,
};

impl Database {
    /// Update list fields without replacing photos or imported chat attachments.
    pub fn update_contact_fields(&mut self, contact: &Contact, tags: &[String]) -> Result<()> {
        self.update_contact_fields_inner(contact, tags, false)
    }
    /// Update contact details and notes without replacing the stored photo or chat originals.
    pub fn update_contact_details(&mut self, contact: &Contact, tags: &[String]) -> Result<()> {
        self.update_contact_fields_inner(contact, tags, true)
    }
    fn update_contact_fields_inner(
        &mut self,
        contact: &Contact,
        tags: &[String],
        include_notes: bool,
    ) -> Result<()> {
        if contact.name.trim().is_empty() {
            return Err(crate::error::DeskError::InvalidPath("请填写姓名".into()));
        }
        let birthday = crate::model::normalize_date(&contact.birthday)?;
        let selected = crate::tags::normalize(tags)?;
        let tx = self.desk.transaction()?;
        super::ensure_tags(&tx, crate::tags::Scope::Contacts, &selected)?;
        let changed = tx.execute("UPDATE contacts SET name=?1,school=?2,major=?3,work=?4,phone=?5,national_id=?6,address=?7,gender=?8,birthday=?9,hobbies=?10,tags_json=?11,updated_at=CURRENT_TIMESTAMP WHERE id=?12",
            params![contact.name.trim(),contact.school,contact.major,contact.work,contact.phone,contact.national_id,contact.address,contact.gender,birthday,contact.hobbies,serde_json::to_string(&selected)?,contact.id])?;
        if changed != 1 {
            return Err(crate::error::DeskError::InvalidPath(
                "联系人已不存在".into(),
            ));
        }
        if include_notes {
            tx.execute(
                "UPDATE contacts SET chat_notes=?1 WHERE id=?2",
                params![contact.chat_notes, contact.id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn contacts(&self) -> Result<Vec<Contact>> {
        self.read_contacts(None, true)
    }

    /// List preview retains searchable chat text. Attachments are read on demand.
    pub fn contact_overviews(&self) -> Result<Vec<Contact>> {
        self.read_contacts(None, false)
    }

    pub fn contact_search_items(&self) -> Result<Vec<ContactSearchItem>> {
        let mut statement = self.desk.prepare_cached(
            "SELECT id,name,school,major,work,phone FROM contacts ORDER BY updated_at DESC,rowid DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ContactSearchItem {
                id: row.get(0)?,
                name: row.get(1)?,
                school: row.get(2)?,
                major: row.get(3)?,
                work: row.get(4)?,
                phone: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn contact_exists(&self, id: &str) -> Result<bool> {
        Ok(self.desk.query_row(
            "SELECT EXISTS(SELECT 1 FROM contacts WHERE id=?1)",
            [id],
            |row| row.get(0),
        )?)
    }

    pub fn contact_photo(&self, id: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .desk
            .query_row("SELECT photo FROM contacts WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn contact_chat(&self, contact_id: &str, chat_id: &str) -> Result<Option<ChatRecord>> {
        let row = self
            .desk
            .query_row(
                "SELECT preview_json,original FROM contact_chats WHERE id=?1 AND contact_id=?2",
                [chat_id, contact_id],
                |row| Ok((row.get::<_, String>(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(preview, original)| {
            let mut record: ChatRecord = serde_json::from_str(&preview)?;
            record.original = original;
            Ok(record)
        })
        .transpose()
    }

    pub fn contact(&self, id: &str) -> Result<Option<Contact>> {
        Ok(self.read_contacts(Some(id), true)?.pop())
    }

    fn read_contacts(&self, id: Option<&str>, originals: bool) -> Result<Vec<Contact>> {
        Self::read_contacts_from(&self.desk, id, originals)
    }

    fn read_contacts_from(
        connection: &rusqlite::Connection,
        id: Option<&str>,
        originals: bool,
    ) -> Result<Vec<Contact>> {
        let mut statement = connection.prepare(
            "SELECT id,name,school,work,phone,national_id,address,gender,hobbies,chat_notes,birthday,major,
                    CASE WHEN ?2 THEN photo ELSE x'' END
             FROM contacts WHERE (?1 IS NULL OR id=?1) ORDER BY updated_at DESC,rowid DESC",
        )?;
        let mut contacts = statement
            .query_map(params![id, originals], |row| {
                Ok(Contact {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    school: row.get(2)?,
                    work: row.get(3)?,
                    phone: row.get(4)?,
                    national_id: row.get(5)?,
                    address: row.get(6)?,
                    gender: row.get(7)?,
                    hobbies: row.get(8)?,
                    chat_notes: row.get(9)?,
                    chats: vec![],
                    birthday: row.get(10)?,
                    major: row.get(11)?,
                    photo: row.get(12)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if !originals {
            let mut statement = connection.prepare(
                "SELECT contact_id,preview_json FROM contact_chats
                 WHERE (?1 IS NULL OR contact_id=?1) ORDER BY contact_id,position",
            )?;
            let mut previews: std::collections::HashMap<String, Vec<ChatRecord>> =
                std::collections::HashMap::new();
            let mut rows = statement.query([id])?;
            while let Some(row) = rows.next()? {
                let preview: String = row.get(1)?;
                previews
                    .entry(row.get(0)?)
                    .or_default()
                    .push(serde_json::from_str(&preview)?);
            }
            for contact in &mut contacts {
                contact.chats = previews.remove(&contact.id).unwrap_or_default();
            }
            return Ok(contacts);
        }
        let mut statement = connection.prepare(
            "SELECT preview_json,original FROM contact_chats WHERE contact_id=?1 ORDER BY position",
        )?;
        for contact in &mut contacts {
            let records = statement.query_map([&contact.id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?;
            for record in records {
                let (json, original) = record?;
                let mut chat: ChatRecord = serde_json::from_str(&json)?;
                chat.original = original;
                contact.chats.push(chat);
            }
        }
        Ok(contacts)
    }

    #[cfg(test)]
    pub fn save_contact(&mut self, contact: &Contact) -> Result<()> {
        self.save_contact_inner(contact, None)
    }
    pub fn save_contact_with_tags(&mut self, contact: &Contact, tags: &[String]) -> Result<()> {
        self.save_contact_inner(contact, Some(tags))
    }
    fn save_contact_inner(&mut self, contact: &Contact, selected: Option<&[String]>) -> Result<()> {
        contacts::validate(contact)?;
        let birthday = crate::model::normalize_date(&contact.birthday)?;
        let tx = self.desk.transaction()?;
        tx.execute("INSERT INTO contacts(id,name,school,work,phone,national_id,address,gender,hobbies,chat_notes,birthday,major,photo)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
            ON CONFLICT(id) DO UPDATE SET name=excluded.name,school=excluded.school,work=excluded.work,
            phone=excluded.phone,national_id=excluded.national_id,address=excluded.address,gender=excluded.gender,
            hobbies=excluded.hobbies,chat_notes=excluded.chat_notes,birthday=excluded.birthday,major=excluded.major,photo=excluded.photo,updated_at=CURRENT_TIMESTAMP",
            params![contact.id,contact.name.trim(),contact.school,contact.work,contact.phone,
                contact.national_id,contact.address,contact.gender,contact.hobbies,contact.chat_notes,birthday,contact.major,contact.photo])?;
        tx.execute(
            "DELETE FROM contact_chats WHERE contact_id=?1",
            [&contact.id],
        )?;
        for (position, chat) in contact.chats.iter().enumerate() {
            tx.execute("INSERT INTO contact_chats(id,contact_id,position,preview_json,original) VALUES(?1,?2,?3,?4,?5)",
                params![chat.id,contact.id,position as i64,serde_json::to_string(chat)?,chat.original])?;
        }
        if let Some(selected) = selected {
            let selected = crate::tags::normalize(selected)?;
            super::ensure_tags(&tx, crate::tags::Scope::Contacts, &selected)?;
            tx.execute(
                "UPDATE contacts SET tags_json=?1 WHERE id=?2",
                params![serde_json::to_string(&selected)?, contact.id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Update only the photo; leave chats and other contact fields untouched.
    pub fn set_contact_photo(&mut self, id: &str, photo: &[u8]) -> Result<()> {
        if photo.len() > contacts::MAX_FILE_BYTES {
            return Err(crate::error::DeskError::InvalidPath(
                "照片不能超过 8 MiB".into(),
            ));
        }
        let changed = self.desk.execute(
            "UPDATE contacts SET photo=?1,updated_at=CURRENT_TIMESTAMP WHERE id=?2",
            params![photo, id],
        )?;
        if changed != 1 {
            return Err(crate::error::DeskError::InvalidPath(
                "联系人已不存在".into(),
            ));
        }
        Ok(())
    }
    /// Append an imported chat while retaining the stored originals and contact details.
    pub fn append_contact_chat(&mut self, id: &str, chat: ChatRecord) -> Result<bool> {
        self.append_contact_chats(id, vec![chat])
            .map(|added| added > 0)
    }
    /// Validate and append the entire selection atomically; existing records remain untouched.
    pub fn append_contact_chats(&mut self, id: &str, chats: Vec<ChatRecord>) -> Result<usize> {
        let tx = self
            .desk
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut contact = Self::read_contacts_from(&tx, Some(id), true)?
            .pop()
            .ok_or_else(|| crate::error::DeskError::InvalidPath("联系人已不存在".into()))?;
        let before = contact.chats.len();
        let added = contacts::append_chats(&mut contact, chats)?;
        for chat in &contact.chats[before..] {
            tx.execute("INSERT INTO contact_chats(id,contact_id,position,preview_json,original) VALUES(?1,?2,(SELECT COALESCE(MAX(position),-1)+1 FROM contact_chats WHERE contact_id=?2),?3,?4)", params![chat.id,id,serde_json::to_string(chat)?,chat.original])?;
        }
        tx.commit()?;
        Ok(added)
    }
    pub fn remove_contact_chat(&mut self, id: &str, chat_id: &str) -> Result<()> {
        let changed = self.desk.execute(
            "DELETE FROM contact_chats WHERE contact_id=?1 AND id=?2",
            params![id, chat_id],
        )?;
        if changed != 1 {
            return Err(crate::error::DeskError::InvalidPath(
                "聊天记录已不存在".into(),
            ));
        }
        Ok(())
    }
    pub fn delete_contact(&mut self, id: &str) -> Result<()> {
        self.desk
            .execute("DELETE FROM contacts WHERE id=?1", [id])?;
        Ok(())
    }
}
