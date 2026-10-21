use rusqlite::params;

use super::Database;
use crate::{
    contacts::{self, ChatRecord, Contact},
    error::Result,
};

impl Database {
    /// Update list fields without replacing photos or imported chat attachments.
    pub fn update_contact_fields(&mut self, contact: &Contact, tags: &[String]) -> Result<()> {
        if contact.name.trim().is_empty() {
            return Err(crate::error::DeskError::InvalidPath("请填写姓名".into()));
        }
        crate::model::validate_date(&contact.birthday)?;
        let selected = crate::tags::normalize(tags)?;
        let tx = self.desk.transaction()?;
        super::ensure_tags(&tx, crate::tags::Scope::Contacts, &selected)?;
        let changed = tx.execute("UPDATE contacts SET name=?1,school=?2,major=?3,work=?4,phone=?5,national_id=?6,address=?7,gender=?8,birthday=?9,hobbies=?10,tags_json=?11,updated_at=CURRENT_TIMESTAMP WHERE id=?12",
            params![contact.name.trim(),contact.school,contact.major,contact.work,contact.phone,contact.national_id,contact.address,contact.gender,contact.birthday,contact.hobbies,serde_json::to_string(&selected)?,contact.id])?;
        if changed != 1 {
            return Err(crate::error::DeskError::InvalidPath(
                "联系人已不存在".into(),
            ));
        }
        tx.commit()?;
        Ok(())
    }
    pub fn contacts(&self) -> Result<Vec<Contact>> {
        let mut statement = self.desk.prepare(
            "SELECT id,name,school,work,phone,national_id,address,gender,hobbies,chat_notes,birthday,major,photo
             FROM contacts ORDER BY updated_at DESC,rowid DESC",
        )?;
        let mut contacts = statement
            .query_map([], |row| {
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
        let mut statement = self.desk.prepare(
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
        let tx = self.desk.transaction()?;
        tx.execute("INSERT INTO contacts(id,name,school,work,phone,national_id,address,gender,hobbies,chat_notes,birthday,major,photo)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
            ON CONFLICT(id) DO UPDATE SET name=excluded.name,school=excluded.school,work=excluded.work,
            phone=excluded.phone,national_id=excluded.national_id,address=excluded.address,gender=excluded.gender,
            hobbies=excluded.hobbies,chat_notes=excluded.chat_notes,birthday=excluded.birthday,major=excluded.major,photo=excluded.photo,updated_at=CURRENT_TIMESTAMP",
            params![contact.id,contact.name.trim(),contact.school,contact.work,contact.phone,
                contact.national_id,contact.address,contact.gender,contact.hobbies,contact.chat_notes,contact.birthday,contact.major,contact.photo])?;
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

    pub fn delete_contact(&mut self, id: &str) -> Result<()> {
        self.desk
            .execute("DELETE FROM contacts WHERE id=?1", [id])?;
        Ok(())
    }
}
