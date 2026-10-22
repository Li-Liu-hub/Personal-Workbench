use super::*;
use crate::essays::{Assignment, Record, Summary, Topic};

impl Database {
    pub(super) fn prepare_essays(&self) -> Result<()> {
        self.desk.execute_batch(
            "CREATE TABLE IF NOT EXISTS daily_essays(
            day TEXT PRIMARY KEY, topic_id TEXT NOT NULL, topic_json TEXT NOT NULL,
            page_id TEXT UNIQUE REFERENCES book_pages(id));",
        )?;
        Ok(())
    }

    pub fn essay_assignment(&self, day: &str) -> Result<Option<Assignment>> {
        let row = self
            .desk
            .query_row(
                "SELECT topic_json,page_id FROM daily_essays WHERE day=?1",
                [day],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        row.map(|(json, page_id)| {
            Ok(Assignment {
                day: day.into(),
                topic: serde_json::from_str(&json)?,
                page_id,
            })
        })
        .transpose()
    }

    fn next_essay_topic(&self) -> Result<Topic> {
        let recent = self
            .desk
            .prepare("SELECT topic_id FROM daily_essays ORDER BY day DESC LIMIT 24")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<std::collections::HashSet<_>, _>>()?;
        let choices = crate::essays::topics()
            .into_iter()
            .filter(|t| !recent.contains(&t.id))
            .collect::<Vec<_>>();
        let entropy = u128::from_le_bytes(*Uuid::new_v4().as_bytes());
        Ok(choices[(entropy % choices.len() as u128) as usize].clone())
    }

    pub fn ensure_essay_day(&self, day: chrono::NaiveDate) -> Result<Assignment> {
        let day = day.format("%Y-%m-%d").to_string();
        if let Some(existing) = self.essay_assignment(&day)? {
            return Ok(existing);
        }
        let topic = self.next_essay_topic()?;
        self.desk.execute(
            "INSERT OR IGNORE INTO daily_essays(day,topic_id,topic_json) VALUES(?1,?2,?3)",
            params![day, topic.id, serde_json::to_string(&topic)?],
        )?;
        self.essay_assignment(&day)?
            .ok_or_else(|| DeskError::InvalidInput("无法生成随笔主题".into()))
    }

    pub fn reroll_essay(&self, day: &str, expected: &str) -> Result<Assignment> {
        let topic = self.next_essay_topic()?;
        let changed = self.desk.execute(
            "UPDATE daily_essays SET topic_id=?1,topic_json=?2
            WHERE day=?3 AND topic_id=?4 AND page_id IS NULL",
            params![topic.id, serde_json::to_string(&topic)?, day, expected],
        )?;
        if changed != 1 {
            return Err(DeskError::InvalidInput(
                "主题已更新或已开始写作，请重新打开".into(),
            ));
        }
        self.essay_assignment(day)?
            .ok_or(DeskError::UnsupportedFormat)
    }

    /// Page creation and daily linkage commit together. Repeated clicks reuse the same page.
    pub fn start_essay(&mut self, day: &str) -> Result<crate::library::Page> {
        let tx = self.desk.transaction()?;
        let (json, existing) = tx.query_row(
            "SELECT topic_json,page_id FROM daily_essays WHERE day=?1",
            [day],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )?;
        if let Some(id) = existing {
            tx.commit()?;
            return self
                .writing_page(&id)?
                .ok_or_else(|| DeskError::InvalidInput("这篇随笔已在文档中删除".into()));
        }
        let topic: Topic = serde_json::from_str(&json)?;
        let mut page = crate::library::Page::new(WRITING_ROOT.into());
        page.title = format!("{day} · {}", topic.title);
        page.revision = 1;
        tx.execute(
            "INSERT OR IGNORE INTO books(id,title) VALUES(?1,'文档')",
            [WRITING_ROOT],
        )?;
        tx.execute(
            "INSERT INTO book_pages(id,book_id,title,body,revision) VALUES(?1,?2,?3,'',1)",
            params![page.id, page.book_id, page.title],
        )?;
        tx.execute(
            "UPDATE daily_essays SET page_id=?1 WHERE day=?2",
            params![page.id, day],
        )?;
        tx.commit()?;
        Ok(page)
    }

    pub fn essay_summaries(&self, query: &str) -> Result<Vec<Summary>> {
        let mut s = self.desk.prepare("SELECT e.day,p.title,e.topic_json,p.id FROM daily_essays e
            JOIN book_pages p ON p.id=e.page_id JOIN books b ON b.id=p.book_id
            WHERE p.deleted=0 AND b.deleted=0 AND instr(lower(e.day||p.title||p.body||e.topic_json),lower(?1))>0
            ORDER BY e.day DESC")?;
        let rows = s
            .query_map([query.trim()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(day, title, json, page_id)| {
                Ok(Summary {
                    day,
                    title,
                    topic: serde_json::from_str::<Topic>(&json)?.title,
                    page_id,
                })
            })
            .collect()
    }

    pub fn essay_records(&self) -> Result<Vec<Record>> {
        let days = self
            .desk
            .prepare("SELECT day FROM daily_essays ORDER BY day DESC")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        days.into_iter()
            .map(|day| {
                let assignment = self
                    .essay_assignment(&day)?
                    .ok_or(DeskError::UnsupportedFormat)?;
                let page = assignment
                    .page_id
                    .as_deref()
                    .map(|id| self.writing_page(id))
                    .transpose()?
                    .flatten();
                Ok(Record { assignment, page })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
