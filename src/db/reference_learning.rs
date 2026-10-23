use super::*;
use crate::study::Card;
use sha2::{Digest, Sha256};

impl Database {
    pub(super) fn prepare_learning_quality(&mut self) -> Result<()> {
        if self.setting("reference.learning.version")?.as_deref() == Some("1") {
            return Ok(());
        }
        let additions: Vec<Card> =
            serde_json::from_str(include_str!("../../assets/learning_variants.json"))?;
        self.import_study_cards(&additions)?;
        let fingerprints: std::collections::HashMap<String, String> = serde_json::from_str(
            include_str!("../../assets/reference_practice_legacy_hashes.json"),
        )?;
        let revised: Vec<Card> =
            serde_json::from_str(include_str!("../../assets/reference_practice.json"))?;
        let tx = self.desk.transaction()?;
        for card in revised {
            let Some(expected) = fingerprints.get(&card.id) else {
                continue;
            };
            let old: Option<String> = tx
                .query_row(
                    "SELECT card_json FROM study_cards WHERE id=?1",
                    [&card.id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(old) = old {
                let value: serde_json::Value = serde_json::from_str(&old)?;
                let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&value)?));
                // Only exact unmodified v0.12 seeds; user changes and attempt snapshots are never rewritten.
                if &digest == expected {
                    tx.execute(
                        "UPDATE study_cards SET card_json=?2 WHERE id=?1 AND card_json=?3",
                        params![card.id, serde_json::to_string(&card)?, old],
                    )?;
                }
            }
        }
        tx.execute("INSERT INTO settings(key,value) VALUES('reference.learning.version','1') ON CONFLICT(key) DO UPDATE SET value='1'", [])?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concrete_exercises_are_linked_and_sql_accepts_reference_but_rejects_wrong_results() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let cards: Vec<Card> =
            serde_json::from_str(include_str!("../../assets/learning_variants.json")).unwrap();
        assert_eq!(cards.len(), 24);
        for card in cards {
            card.validate().unwrap();
            for id in &card.knowledge_ids {
                assert!(db.reference_entry(id).unwrap().is_some(), "{}", card.id);
            }
            assert!(db.study_card(&card.id).unwrap().is_some());
            if let Some(lab) = &card.sql {
                let correct = crate::sql_practice::run(lab, &lab.reference);
                assert_eq!(correct.matches, Some(true), "{}: {correct:?}", card.id);
                assert!(correct.rolled_back);
                let wrong = crate::sql_practice::run(lab, "SELECT 1;");
                assert_eq!(wrong.matches, Some(false));
            }
        }
    }
    #[test]
    fn quality_upgrade_preserves_custom_cards_and_old_attempts() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::open(dir.path()).unwrap();
        let fingerprints: std::collections::HashMap<String, String> = serde_json::from_str(
            include_str!("../../assets/reference_practice_legacy_hashes.json"),
        )
        .unwrap();
        // Restore the exact prior rubric shape for one untouched seed.
        let mut original = db.study_card("knowledge:ref:js:01").unwrap().unwrap();
        original.rubric = vec![
            crate::study::Rubric {
                id: "k1".into(),
                text: "const 限制绑定重新赋值，不会冻结对象。".into(),
                points: 60,
            },
            crate::study::Rubric {
                id: "k2".into(),
                text: "避免依赖 var 的函数作用域和提升行为。".into(),
                points: 40,
            },
        ];
        original.answer = original
            .answer
            .split("\n\n评分边界：")
            .next()
            .unwrap()
            .into();
        let serialized = serde_json::to_string(&original).unwrap();
        let canonical: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&canonical).unwrap())
            ),
            fingerprints[&original.id]
        );
        db.desk
            .execute(
                "UPDATE study_cards SET card_json=?2 WHERE id=?1",
                params![original.id, serialized],
            )
            .unwrap();
        let pending = crate::study::Pending {
            id: "old-evidence".into(),
            card_id: original.id.clone(),
            answer: "previous answer".into(),
            at: "2026-10-23T12:00:00+08:00".into(),
        };
        let old = db
            .record_study_attempt(&pending, crate::study::Grade::unknown(&original), "old", 1)
            .unwrap();
        let mut custom = db.study_card("knowledge:ref:js:02").unwrap().unwrap();
        custom.prompt.push_str(" Personal custom requirement");
        db.desk
            .execute(
                "UPDATE study_cards SET card_json=?2 WHERE id=?1",
                params![custom.id, serde_json::to_string(&custom).unwrap()],
            )
            .unwrap();
        db.desk
            .execute(
                "DELETE FROM settings WHERE key='reference.learning.version'",
                [],
            )
            .unwrap();
        db.prepare_learning_quality().unwrap();
        assert_eq!(
            db.study_card(&original.id).unwrap().unwrap().rubric.len(),
            3
        );
        assert_eq!(
            db.study_card(&custom.id).unwrap().unwrap().prompt,
            custom.prompt
        );
        assert_eq!(
            serde_json::to_value(&db.study_attempts(&original.id).unwrap()[0]).unwrap(),
            serde_json::to_value(old).unwrap()
        );
    }
}
