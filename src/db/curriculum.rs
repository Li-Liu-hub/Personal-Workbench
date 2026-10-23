use super::*;
use crate::curriculum::{self, Candidate, Filter};
use crate::study::Card;
impl Database {
    pub(crate) fn knowledge_totals(&self, id: &str) -> Result<(u32, u32, u32, u32)> {
        Ok(self.desk.prepare_cached("SELECT COUNT(*),COALESCE(SUM(p.attempts>0),0),COALESCE(SUM(p.mastered=1),0),COALESCE(SUM(p.attempts>0 AND p.latest<?2 AND p.mastered=0),0) FROM study_knowledge k LEFT JOIN study_progress p ON p.card_id=k.card_id WHERE k.knowledge_id=?1")?
            .query_row(params![id,self.study_rules().threshold],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?)
    }
    pub(crate) fn practice_candidates(&self, topic: &str) -> Result<Vec<Candidate>> {
        self.practice_candidates_for(topic, "")
    }
    pub(crate) fn practice_candidates_for(
        &self,
        topic: &str,
        knowledge: &str,
    ) -> Result<Vec<Candidate>> {
        let guided = topic.is_empty() && knowledge.is_empty();
        let topic = if topic == curriculum::MIXED {
            ""
        } else {
            curriculum::category(topic)
        };
        let mut statement=self.desk.prepare("SELECT c.card_json,COALESCE(p.mastered,0),COALESCE(a.total,0),last.completion
            FROM study_cards c LEFT JOIN study_progress p ON p.card_id=c.id
            LEFT JOIN (SELECT card_id,COUNT(*) total,MAX(number) last_no FROM study_attempts
                WHERE COALESCE(json_extract(attempt_json,'$.assisted'),0)=0 GROUP BY card_id) a ON a.card_id=c.id
            LEFT JOIN study_attempts last ON last.card_id=c.id AND last.number=a.last_no
            WHERE c.kind='engineering' AND (?1='' OR (CASE c.topic
                WHEN '场景题' THEN '工程设计' WHEN 'SQL 命令' THEN 'SQL 训练'
                WHEN 'Bug 排查' THEN '排障与调优' WHEN '性能调优' THEN '排障与调优'
                WHEN '项目修 Bug' THEN '项目实战' ELSE c.topic END)=?1) AND (?2='' OR EXISTS(SELECT 1 FROM study_knowledge k WHERE k.card_id=c.id AND k.knowledge_id=?2)) AND (?3=0 OR NOT EXISTS(SELECT 1 FROM study_knowledge k WHERE k.card_id=c.id)) ORDER BY c.id")?;
        let rows = statement
            .query_map(params![topic, knowledge, guided], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, u32>(2)?,
                    r.get::<_, Option<u8>>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(json, mastered, independent, latest)| {
                let card: Card = curriculum::with_profile(serde_json::from_str(&json)?);
                Ok(Candidate {
                    training: curriculum::profile(&card),
                    card,
                    mastered,
                    independent,
                    latest,
                })
            })
            .collect()
    }
    #[cfg(test)]
    pub(crate) fn practice_choice(
        &self,
        topic: &str,
        filter: &Filter,
        recent: &[String],
        roll: u64,
    ) -> Result<Option<(Card, String)>> {
        self.practice_choice_for(topic, filter, recent, roll, "")
    }
    pub(crate) fn practice_choice_for(
        &self,
        topic: &str,
        filter: &Filter,
        recent: &[String],
        roll: u64,
        knowledge: &str,
    ) -> Result<Option<(Card, String)>> {
        let candidates = self.practice_candidates_for(topic, knowledge)?;
        Ok(curriculum::choose(
            &candidates,
            topic,
            filter,
            recent,
            &self.study_rules(),
            roll,
        )
        .map(|c| (candidates[c.index].card.clone(), c.reason)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_export_without_rewriting_legacy_cards_and_invalid_metadata_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::open(dir.path()).unwrap();
        let raw: String = db
            .desk
            .query_row(
                "SELECT card_json FROM study_cards WHERE id='eng:2:01'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            serde_json::from_str::<serde_json::Value>(&raw)
                .unwrap()
                .get("training")
                .is_none()
        );
        let mut bad = db.study_card("eng:2:01").unwrap().unwrap();
        assert!(bad.training.is_some());
        bad.id = "bad-training".into();
        bad.training.as_mut().unwrap().level = 4;
        assert!(db.import_study_cards(&[bad]).is_err());
        let zip_path = dir.path().join("export.zip");
        crate::export::write(dir.path(), &zip_path, crate::export::Scope::All).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path).unwrap()).unwrap();
        let bundle: crate::study::Bundle =
            serde_json::from_reader(zip.by_name("刷题王.json").unwrap()).unwrap();
        assert_eq!(bundle.cards.len(), curriculum::BUILTIN_COUNT as usize);
        assert!(bundle.cards.iter().all(|c| c.training.is_some()));
        let after: String = db
            .desk
            .query_row(
                "SELECT card_json FROM study_cards WHERE id='eng:2:01'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(raw, after);
    }
    #[test]
    fn assisted_attempts_never_unlock_later_stages_and_legacy_snapshots_survive() {
        use crate::study::{Check, Grade, Pending};
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::open(dir.path()).unwrap();
        let cards = db
            .practice_candidates("")
            .unwrap()
            .into_iter()
            .filter(|r| r.training.area == curriculum::AREAS[0] && r.training.level == 1)
            .take(2)
            .map(|r| r.card)
            .collect::<Vec<_>>();
        let mut saved = Vec::new();
        for (i, card) in cards.iter().enumerate() {
            let p = Pending {
                id: format!("assisted-{i}"),
                card_id: card.id.clone(),
                answer: "fixture".into(),
                at: "2026-10-22T10:00:00+08:00".into(),
            };
            db.mark_study_help(&p.id).unwrap();
            let g = Grade {
                checks: card
                    .rubric
                    .iter()
                    .map(|r| Check {
                        id: r.id.clone(),
                        earned: r.points,
                        reason: "fixture".into(),
                        evidence: "fixture".into(),
                    })
                    .collect(),
                summary: "fixture".into(),
                weak_points: vec![],
                sql: None,
                project: None,
            };
            let attempt = db
                .record_study_attempt(&p, g.clone(), "fixture", 1)
                .unwrap();
            assert!(attempt.assisted);
            let rows = db.practice_candidates("").unwrap();
            assert_eq!(curriculum::route(&rows, &db.study_rules())[0].stage, 1);
            assert_eq!(
                rows.iter()
                    .find(|r| r.card.id == card.id)
                    .unwrap()
                    .independent,
                0
            );
            let independent = Pending {
                id: format!("independent-{i}"),
                ..p
            };
            db.record_study_attempt(&independent, g, "fixture", 1)
                .unwrap();
            saved.push(
                db.desk
                    .query_row(
                        "SELECT attempt_json FROM study_attempts WHERE id=?1",
                        [independent.id],
                        |r| r.get::<_, String>(0),
                    )
                    .unwrap(),
            );
        }
        assert_eq!(
            curriculum::route(&db.practice_candidates("").unwrap(), &db.study_rules())[0].stage,
            2
        );
        let mut custom = cards[0].clone();
        custom.id = "route:java:references".into();
        custom.title = "自定义题目不覆盖".into();
        db.desk
            .execute(
                "UPDATE study_cards SET card_json=?2 WHERE id=?1",
                params![custom.id, serde_json::to_string(&custom).unwrap()],
            )
            .unwrap();
        drop(db);
        let db = Database::open(dir.path()).unwrap();
        assert_eq!(
            db.study_card(&custom.id).unwrap().unwrap().title,
            custom.title
        );
        for (i, json) in saved.iter().enumerate() {
            let raw: String = db
                .desk
                .query_row(
                    "SELECT attempt_json FROM study_attempts WHERE id=?1",
                    [format!("independent-{i}")],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(&raw, json);
        }
        assert_eq!(
            db.study_count("engineering").unwrap(),
            curriculum::BUILTIN_COUNT
        );
        let sql = db.practice_candidates("SQL 命令").unwrap();
        assert_eq!(sql.len(), 88);
        assert!(
            sql.iter()
                .all(|r| curriculum::category(&r.card.topic) == "SQL 训练")
        );
    }
}
