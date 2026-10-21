use super::*;
use crate::study::{self, Attempt, Card, Grade, NotebookEntry, Pending, Progress, Rules};
impl Database {
    pub(super) fn prepare_study(&mut self) -> Result<()> {
        self.desk.execute_batch("CREATE TABLE IF NOT EXISTS study_cards(id TEXT PRIMARY KEY,kind TEXT NOT NULL,topic TEXT NOT NULL,card_json TEXT NOT NULL);
          CREATE INDEX IF NOT EXISTS study_cards_kind_topic ON study_cards(kind,topic);
          CREATE TABLE IF NOT EXISTS study_attempts(id TEXT PRIMARY KEY,card_id TEXT NOT NULL REFERENCES study_cards(id),number INTEGER NOT NULL,at TEXT NOT NULL,completion INTEGER NOT NULL CHECK(completion BETWEEN 0 AND 100),attempt_json TEXT NOT NULL,UNIQUE(card_id,number));
          CREATE INDEX IF NOT EXISTS study_attempts_card ON study_attempts(card_id,number);
          CREATE TABLE IF NOT EXISTS study_progress(card_id TEXT PRIMARY KEY REFERENCES study_cards(id),attempts INTEGER NOT NULL,streak INTEGER NOT NULL,mastered INTEGER NOT NULL,latest INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS study_pending(id TEXT PRIMARY KEY,card_id TEXT NOT NULL REFERENCES study_cards(id),answer TEXT NOT NULL,at TEXT NOT NULL);")?;
        if self
            .settings()?
            .get("study.seed.version")
            .is_some_and(|v| v == "1")
        {
            return self.prepare_sql_study();
        }
        let mut cards: Vec<Card> =
            serde_json::from_str(include_str!("../../assets/engineering.json"))?;
        #[derive(serde::Deserialize)]
        struct Word {
            word: String,
            phonetic: String,
            meaning: String,
        }
        #[derive(serde::Deserialize)]
        struct Words {
            words: Vec<Word>,
        }
        let words: Words = serde_json::from_str(include_str!("../../assets/ielts.json"))?;
        cards.extend(words.words.into_iter().map(|w| Card {
            id: format!("ielts:{}", w.word.to_lowercase()),
            kind: "word".into(),
            title: w.word.clone(),
            topic: "IELTS".into(),
            prompt: format!("写出 {} 的中文含义。", w.word),
            answer: w.meaning,
            rubric: vec![study::Rubric {
                id: "meaning".into(),
                text: "准确表达至少一个常用核心含义；不要求列出全部义项，也不要求造句。".into(),
                points: 100,
            }],
            source: "https://github.com/skywind3000/ECDICT".into(),
            phonetic: w.phonetic,
            sql: None,
        }));
        self.import_study_cards(&cards)?;
        self.set_setting("study.seed.version", "1")?;
        self.prepare_sql_study()
    }
    fn prepare_sql_study(&mut self) -> Result<()> {
        if self
            .settings()?
            .get("study.sqlite.seed.version")
            .is_some_and(|v| v == "1")
        {
            return Ok(());
        }
        let previous: Vec<Card> =
            serde_json::from_str(include_str!("../../assets/engineering.json"))?;
        let cards: Vec<Card> =
            serde_json::from_str(include_str!("../../assets/sql_practice.json"))?;
        for card in &cards {
            card.validate()?;
        }
        let tx = self.desk.transaction()?;
        for card in &cards {
            let existing: Option<String> = tx
                .query_row(
                    "SELECT card_json FROM study_cards WHERE id=?1",
                    [&card.id],
                    |r| r.get(0),
                )
                .optional()?;
            match existing {
                None => {
                    tx.execute(
                        "INSERT INTO study_cards(id,kind,topic,card_json) VALUES(?1,?2,?3,?4)",
                        params![card.id, card.kind, card.topic, serde_json::to_string(card)?],
                    )?;
                }
                Some(json) => {
                    // Upgrade only untouched built-ins. Custom banks and immutable attempt snapshots remain intact.
                    let existing: Card = serde_json::from_str(&json)?;
                    if previous.iter().any(|old| {
                        old.id == card.id
                            && serde_json::to_value(old).ok()
                                == serde_json::to_value(&existing).ok()
                    }) {
                        tx.execute(
                            "UPDATE study_cards SET topic=?2,card_json=?3 WHERE id=?1",
                            params![card.id, card.topic, serde_json::to_string(card)?],
                        )?;
                    }
                }
            }
        }
        tx.execute("INSERT INTO settings(key,value) VALUES('study.sqlite.seed.version','1') ON CONFLICT(key) DO UPDATE SET value='1'", [])?;
        tx.commit()?;
        Ok(())
    }
    pub fn import_study_cards(&mut self, cards: &[Card]) -> Result<usize> {
        for card in cards {
            card.validate()?;
        }
        let tx = self.desk.transaction()?;
        let mut added = 0;
        for card in cards {
            added += tx.execute(
                "INSERT OR IGNORE INTO study_cards(id,kind,topic,card_json) VALUES(?1,?2,?3,?4)",
                params![card.id, card.kind, card.topic, serde_json::to_string(card)?],
            )?;
        }
        tx.commit()?;
        Ok(added)
    }
    pub fn study_rules(&self) -> Rules {
        self.settings()
            .ok()
            .and_then(|s| {
                s.get("study.rules")
                    .and_then(|v| serde_json::from_str(v).ok())
            })
            .unwrap_or_default()
    }
    pub fn save_study_rules(&mut self, rules: &Rules) -> Result<()> {
        rules.validate()?;
        let tx = self.desk.transaction()?;
        let records = tx
            .prepare("SELECT card_id,completion FROM study_attempts ORDER BY card_id,number")?
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u8>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut groups = std::collections::BTreeMap::<String, Vec<u8>>::new();
        for (id, score) in records {
            groups.entry(id).or_default().push(score);
        }
        for (id, scores) in groups {
            let p = Progress::from_scores(id, &scores, rules);
            write_progress(&tx, &p)?;
        }
        tx.execute("INSERT INTO settings(key,value) VALUES('study.rules',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(rules)?])?;
        tx.commit()?;
        Ok(())
    }
    pub fn study_card(&self, id: &str) -> Result<Option<Card>> {
        let json = self
            .desk
            .query_row("SELECT card_json FROM study_cards WHERE id=?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()?;
        json.map(|v| serde_json::from_str(&v).map_err(DeskError::from))
            .transpose()
    }
    pub fn study_cards(&self, kind: &str) -> Result<Vec<Card>> {
        self.desk
            .prepare("SELECT card_json FROM study_cards WHERE kind=?1 ORDER BY id")?
            .query_map([kind], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(|v| Ok(serde_json::from_str(&v)?))
            .collect()
    }
    #[cfg(test)]
    pub fn study_progress(&self, id: &str) -> Result<Progress> {
        Ok(self.desk.query_row("SELECT card_id,attempts,streak,mastered,latest FROM study_progress WHERE card_id=?1",[id],|r|Ok(Progress{card_id:r.get(0)?,attempts:r.get(1)?,streak:r.get(2)?,mastered:r.get(3)?,latest:r.get(4)?})).optional()?.unwrap_or_else(||Progress{card_id:id.into(),..Default::default()}))
    }
    pub fn draw_study(
        &self,
        kind: &str,
        topic: &str,
        recent: &[String],
        roll: u64,
    ) -> Result<Option<Card>> {
        let rules = self.study_rules();
        let rows=self.desk.prepare("SELECT c.id,c.topic,COALESCE(p.mastered,0),COALESCE(p.attempts,0),COALESCE(p.latest,0) FROM study_cards c LEFT JOIN study_progress p ON p.card_id=c.id WHERE c.kind=?1 AND (?2='' OR c.topic=?2)")?.query_map(params![kind,topic],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,bool>(2)?,r.get::<_,u32>(3)?,r.get::<_,u8>(4)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let mut items = rows
            .into_iter()
            .filter(|r| !r.2 || rules.mastered == study::MasteredMode::Rare)
            .collect::<Vec<_>>();
        if items.iter().any(|r| !recent.contains(&r.0)) {
            items.retain(|r| !recent.contains(&r.0));
        }
        // Topic selection is uniform in mixed training; within each topic weak/new items get priority.
        if kind == "engineering" && topic.is_empty() && !items.is_empty() {
            let topics = items
                .iter()
                .map(|r| r.1.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let topic_weights = topics
                .iter()
                .map(|t| {
                    if items.iter().any(|r| &r.1 == t && !r.2) {
                        100u64
                    } else {
                        1
                    }
                })
                .collect::<Vec<_>>();
            let mut draw = roll % topic_weights.iter().sum::<u64>();
            let mut selected = &topics[0];
            for (t, w) in topics.iter().zip(topic_weights) {
                if draw < w {
                    selected = t;
                    break;
                }
                draw -= w;
            }
            items.retain(|r| &r.1 == selected);
        }
        let weights = items
            .iter()
            .map(|r| {
                if r.2 {
                    1
                } else if r.3 == 0 {
                    100
                } else if r.4 < rules.threshold {
                    150
                } else {
                    60
                }
            })
            .collect::<Vec<u64>>();
        let total: u64 = weights.iter().sum();
        if total == 0 {
            return Ok(None);
        }
        let mut chosen = roll.rotate_left(23) % total;
        for (item, weight) in items.iter().zip(weights) {
            if chosen < weight {
                return self.study_card(&item.0);
            }
            chosen -= weight;
        }
        Ok(None)
    }
    pub fn save_study_pending(&mut self, p: &Pending) -> Result<()> {
        self.desk.execute("INSERT INTO study_pending(id,card_id,answer,at) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET answer=excluded.answer",params![p.id,p.card_id,p.answer,p.at])?;
        Ok(())
    }
    pub fn study_pending(&self, kind: &str) -> Result<Vec<Pending>> {
        Ok(self.desk.prepare("SELECT p.id,p.card_id,p.answer,p.at FROM study_pending p JOIN study_cards c ON c.id=p.card_id WHERE c.kind=?1 ORDER BY p.at DESC")?.query_map([kind],|r|Ok(Pending{id:r.get(0)?,card_id:r.get(1)?,answer:r.get(2)?,at:r.get(3)?}))?.collect::<std::result::Result<Vec<_>,_>>()?)
    }
    pub fn record_study_attempt(
        &mut self,
        p: &Pending,
        feedback: Grade,
        model: &str,
        elapsed_ms: u64,
    ) -> Result<Attempt> {
        if let Some(existing) = self
            .desk
            .query_row(
                "SELECT attempt_json FROM study_attempts WHERE id=?1",
                [&p.id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(serde_json::from_str(&existing)?);
        }
        let card = self
            .study_card(&p.card_id)?
            .ok_or(DeskError::UnsupportedFormat)?;
        let completion = feedback.completion(&card, &p.answer)?;
        let rules = self.study_rules();
        let tx = self.desk.transaction()?;
        let mut scores = tx
            .prepare("SELECT completion FROM study_attempts WHERE card_id=?1 ORDER BY number")?
            .query_map([&p.card_id], |r| r.get::<_, u8>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let attempt = Attempt {
            id: p.id.clone(),
            card_id: p.card_id.clone(),
            number: scores.len() as u32 + 1,
            at: chrono::Local::now().to_rfc3339(),
            answer: p.answer.clone(),
            completion,
            feedback,
            model: model.into(),
            elapsed_ms,
            question: card,
            rules: rules.clone(),
        };
        tx.execute("INSERT INTO study_attempts(id,card_id,number,at,completion,attempt_json) VALUES(?1,?2,?3,?4,?5,?6)",params![attempt.id,attempt.card_id,attempt.number,attempt.at,attempt.completion,serde_json::to_string(&attempt)?])?;
        scores.push(completion);
        write_progress(
            &tx,
            &Progress::from_scores(p.card_id.clone(), &scores, &rules),
        )?;
        tx.execute("DELETE FROM study_pending WHERE id=?1", [&p.id])?;
        tx.commit()?;
        Ok(attempt)
    }
    pub fn study_all_attempts(&self, kind: &str) -> Result<Vec<Attempt>> {
        self.desk.prepare("SELECT a.attempt_json FROM study_attempts a JOIN study_cards c ON c.id=a.card_id WHERE c.kind=?1 ORDER BY a.at")?.query_map([kind],|r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?.into_iter().map(|v|Ok(serde_json::from_str(&v)?)).collect()
    }
    pub fn study_attempts(&self, id: &str) -> Result<Vec<Attempt>> {
        self.desk
            .prepare("SELECT attempt_json FROM study_attempts WHERE card_id=?1 ORDER BY number")?
            .query_map([id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(|v| Ok(serde_json::from_str(&v)?))
            .collect()
    }
    pub fn study_notebook(&self, kind: &str, query: &str) -> Result<Vec<NotebookEntry>> {
        let records=self.desk.prepare("SELECT c.card_json,p.card_id,p.attempts,p.streak,p.mastered,p.latest FROM study_progress p JOIN study_cards c ON c.id=p.card_id WHERE c.kind=?1 AND instr(lower(c.card_json),lower(?2))>0 ORDER BY p.mastered,p.latest,p.attempts DESC")?.query_map(params![kind,query],|r|Ok((r.get::<_,String>(0)?,Progress{card_id:r.get(1)?,attempts:r.get(2)?,streak:r.get(3)?,mastered:r.get(4)?,latest:r.get(5)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
        records
            .into_iter()
            .map(|(json, progress)| {
                Ok(NotebookEntry {
                    card: serde_json::from_str(&json)?,
                    progress,
                })
            })
            .collect()
    }
    pub fn study_today(&self, kind: &str) -> Result<u32> {
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        Ok(self.desk.query_row("SELECT COUNT(DISTINCT a.card_id) FROM study_attempts a JOIN study_cards c ON c.id=a.card_id WHERE c.kind=?1 AND substr(a.at,1,10)=?2",params![kind,day],|r|r.get(0))?)
    }
    pub fn study_count(&self, kind: &str) -> Result<u32> {
        Ok(self.desk.query_row(
            "SELECT COUNT(*) FROM study_cards WHERE kind=?1",
            [kind],
            |r| r.get(0),
        )?)
    }
}
fn write_progress(tx: &rusqlite::Transaction<'_>, p: &Progress) -> Result<()> {
    tx.execute("INSERT INTO study_progress(card_id,attempts,streak,mastered,latest) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(card_id) DO UPDATE SET attempts=excluded.attempts,streak=excluded.streak,mastered=excluded.mastered,latest=excluded.latest",params![p.card_id,p.attempts,p.streak,p.mastered,p.latest])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sqlite_seed_upgrade_preserves_attempts_pending_and_custom_questions() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::open(dir.path()).unwrap();
        let old: Vec<Card> =
            serde_json::from_str(include_str!("../../assets/engineering.json")).unwrap();
        for id in ["eng:3:01", "eng:3:02"] {
            let mut card = old.iter().find(|c| c.id == id).unwrap().clone();
            if id == "eng:3:02" {
                card.prompt.push_str(" 自定义题干保留");
            }
            db.desk
                .execute(
                    "UPDATE study_cards SET card_json=?2 WHERE id=?1",
                    params![id, serde_json::to_string(&card).unwrap()],
                )
                .unwrap();
        }
        let card = db.study_card("eng:3:01").unwrap().unwrap();
        let p = Pending {
            id: "previous-attempt".into(),
            card_id: card.id.clone(),
            answer: "历史答案".into(),
            at: "2026-10-21T12:00:00+08:00".into(),
        };
        db.record_study_attempt(&p, Grade::unknown(&card), "old-model", 12)
            .unwrap();
        let saved: String = db
            .desk
            .query_row(
                "SELECT attempt_json FROM study_attempts WHERE id=?1",
                [&p.id],
                |r| r.get(0),
            )
            .unwrap();
        let pending = Pending {
            id: "previous-pending".into(),
            ..p
        };
        db.save_study_pending(&pending).unwrap();
        db.desk
            .execute(
                "DELETE FROM settings WHERE key='study.sqlite.seed.version'",
                [],
            )
            .unwrap();
        drop(db);
        let db = Database::open(dir.path()).unwrap();
        assert!(db.study_card("eng:3:01").unwrap().unwrap().sql.is_some());
        let custom = db.study_card("eng:3:02").unwrap().unwrap();
        assert!(custom.sql.is_none() && custom.prompt.ends_with("自定义题干保留"));
        let after: String = db
            .desk
            .query_row(
                "SELECT attempt_json FROM study_attempts WHERE id='previous-attempt'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(saved, after);
        assert!(
            db.study_attempts(&card.id).unwrap()[0]
                .question
                .sql
                .is_none()
        );
        assert_eq!(db.study_progress(&card.id).unwrap().attempts, 1);
        assert_eq!(
            db.study_pending("engineering").unwrap()[0].answer,
            "历史答案"
        );
        assert_eq!(db.study_count("engineering").unwrap(), 37);
        let integrity: String = db
            .desk
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
    }
}
