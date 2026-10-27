//! Additive catalog expansion. Stable IDs and tombstones preserve personal work.
use super::*;
use crate::{reference::Entry, study::Card};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const DEEP_ENTRY_FIELDS: [&str; 9] = [
    "id",
    "category",
    "title",
    "summary",
    "syntax",
    "example",
    "notes",
    "source",
    "entry_type",
];
const DEEP_CARD_FIELDS: [&str; 10] = [
    "id",
    "kind",
    "topic",
    "title",
    "prompt",
    "answer",
    "rubric",
    "source",
    "knowledge_ids",
    "training",
];

fn deep_hash(value: &serde_json::Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

/// The subset and key order must match scripts/encyclopedia-deep/build.py.
fn deep_entry_hash(entry: &Entry) -> Result<String> {
    let value = serde_json::to_value(entry)?;
    let source = value
        .as_object()
        .ok_or_else(|| DeskError::InvalidInput("百科全书条目格式无效".into()))?;
    let mut subset = serde_json::Map::new();
    for field in DEEP_ENTRY_FIELDS {
        subset.insert(field.into(), source.get(field).cloned().unwrap_or_default());
    }
    deep_hash(&serde_json::Value::Object(subset))
}

fn deep_card_hash(card: &Card) -> Result<String> {
    let value = serde_json::to_value(card)?;
    let source = value
        .as_object()
        .ok_or_else(|| DeskError::InvalidInput("练习题目格式无效".into()))?;
    let mut subset = serde_json::Map::new();
    for field in DEEP_CARD_FIELDS {
        subset.insert(field.into(), source.get(field).cloned().unwrap_or_default());
    }
    deep_hash(&serde_json::Value::Object(subset))
}

/// Tags are merged because the 408 split renamed categories and retagged rows.
fn merge_tags(old: &[String], new: &[String]) -> Vec<String> {
    let mut merged = old.to_vec();
    for tag in new {
        if !merged.contains(tag) && merged.len() < 24 {
            merged.push(tag.clone());
        }
    }
    merged
}

impl Database {
    pub(super) fn prepare_encyclopedia(&mut self) -> Result<()> {
        self.prepare_encyclopedia_pack(
            "encyclopedia.ecosystem.version",
            include_str!("../../assets/java_ecosystem.json"),
            include_str!("../../assets/java_ecosystem_practice.json"),
        )?;
        self.prepare_encyclopedia_pack(
            "encyclopedia.other_languages.version",
            include_str!("../../assets/other_languages.json"),
            include_str!("../../assets/other_languages_practice.json"),
        )?;
        self.prepare_encyclopedia_pack(
            "encyclopedia.data_structures.version",
            include_str!("../../assets/data_structures.json"),
            include_str!("../../assets/data_structures_practice.json"),
        )?;
        self.prepare_encyclopedia_pack(
            "encyclopedia.computer_organization.version",
            include_str!("../../assets/computer_organization.json"),
            include_str!("../../assets/computer_organization_practice.json"),
        )?;
        self.prepare_encyclopedia_pack(
            "encyclopedia.operating_systems.version",
            include_str!("../../assets/operating_systems.json"),
            include_str!("../../assets/operating_systems_practice.json"),
        )?;
        self.prepare_encyclopedia_pack(
            "encyclopedia.computer_networks.version",
            include_str!("../../assets/computer_networks.json"),
            include_str!("../../assets/computer_networks_practice.json"),
        )?;
        self.prepare_encyclopedia_pack(
            "encyclopedia.mysql_internals.version",
            include_str!("../../assets/mysql_internals.json"),
            include_str!("../../assets/mysql_internals_practice.json"),
        )?;
        self.prepare_encyclopedia_pack(
            "encyclopedia.coding_standards.version",
            include_str!("../../assets/coding_standards.json"),
            include_str!("../../assets/coding_standards_practice.json"),
        )?;
        self.prepare_encyclopedia_pack(
            "encyclopedia.backend_interview.version",
            include_str!("../../assets/backend_interview.json"),
            include_str!("../../assets/backend_interview_practice.json"),
        )?;
        self.prepare_encyclopedia_pack(
            "encyclopedia.python.version",
            include_str!("../../assets/python.json"),
            include_str!("../../assets/python_practice.json"),
        )?;
        self.prepare_encyclopedia_pack(
            "encyclopedia.more_languages.version",
            include_str!("../../assets/more_languages.json"),
            include_str!("../../assets/more_languages_practice.json"),
        )?;
        self.prepare_encyclopedia_pack(
            "encyclopedia.deep_readings.version",
            include_str!("../../assets/deep_readings.json"),
            include_str!("../../assets/deep_readings_practice.json"),
        )?;
        self.prepare_cs408_subject_split()?;
        self.prepare_deep_upgrades()?;
        self.prepare_coding()
    }

    /// Rewrites only untouched built-in seeds. User edits, tombstones and attempt
    /// snapshots keep their current content, and the marker is set after success
    /// so a crash replays the exact same checks.
    fn prepare_deep_upgrades(&mut self) -> Result<()> {
        const MARKER: &str = "encyclopedia.deep_upgrades.version";
        if self.setting(MARKER)?.as_deref() == Some("1") {
            return Ok(());
        }
        let entries: Vec<Entry> =
            serde_json::from_str(include_str!("../../assets/deep_upgrades.json"))?;
        let cards: Vec<Card> =
            serde_json::from_str(include_str!("../../assets/deep_upgrades_practice.json"))?;
        let entry_hashes: HashMap<String, Vec<String>> =
            serde_json::from_str(include_str!("../../assets/deep_upgrade_entry_hashes.json"))?;
        let card_hashes: HashMap<String, Vec<String>> =
            serde_json::from_str(include_str!("../../assets/deep_upgrade_card_hashes.json"))?;
        for entry in &entries {
            entry.validate()?;
        }
        for card in &cards {
            card.validate()?;
        }
        let expected: HashMap<&str, &Entry> = entries
            .iter()
            .map(|entry| (entry.id.as_str(), entry))
            .collect();
        for seed in &entries {
            let Some(current) = self.reference_entry(&seed.id)? else {
                continue;
            };
            let current_hash = deep_entry_hash(&current)?;
            if !entry_hashes
                .get(&seed.id)
                .is_some_and(|hashes| hashes.contains(&current_hash))
            {
                continue;
            }
            let mut updated = current.clone();
            updated.summary = seed.summary.clone();
            updated.syntax = seed.syntax.clone();
            updated.example = seed.example.clone();
            updated.notes = seed.notes.clone();
            updated.source = seed.source.clone();
            updated.entry_type = seed.entry_type.clone();
            updated.tags = merge_tags(&current.tags, &seed.tags);
            self.save_reference(&updated)?;
        }
        // Only upgrade a card when its linked entry already carries the deep
        // content, so a partially failed run and user-edited entries stay safe.
        let mut entry_ready = HashMap::new();
        for seed in &cards {
            let Some(entry_id) = seed.knowledge_ids.first() else {
                continue;
            };
            let ready = match (
                expected.get(entry_id.as_str()),
                self.reference_entry(entry_id)?,
            ) {
                (Some(seed_entry), Some(current)) => {
                    deep_entry_hash(&current)? == deep_entry_hash(seed_entry)?
                }
                _ => false,
            };
            entry_ready.insert(entry_id.clone(), ready);
        }
        let tx = self.desk.transaction()?;
        for seed in &cards {
            let Some(entry_id) = seed.knowledge_ids.first() else {
                continue;
            };
            if !entry_ready.get(entry_id).copied().unwrap_or(false) {
                continue;
            }
            let old: Option<String> = tx
                .query_row(
                    "SELECT card_json FROM study_cards WHERE id=?1",
                    [&seed.id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(old) = old else {
                continue;
            };
            let existing: Card = serde_json::from_str(&old)?;
            let existing_hash = deep_card_hash(&existing)?;
            if !card_hashes
                .get(&seed.id)
                .is_some_and(|hashes| hashes.contains(&existing_hash))
            {
                continue;
            }
            let mut updated = existing;
            updated.title = seed.title.clone();
            updated.topic = seed.topic.clone();
            updated.prompt = seed.prompt.clone();
            updated.answer = seed.answer.clone();
            updated.rubric = seed.rubric.clone();
            updated.source = seed.source.clone();
            updated.knowledge_ids = seed.knowledge_ids.clone();
            updated.training = seed.training.clone();
            updated.tags = merge_tags(&updated.tags, &seed.tags);
            updated.validate()?;
            tx.execute(
                "UPDATE study_cards SET topic=?2,card_json=?3 WHERE id=?1 AND card_json=?4",
                params![
                    updated.id,
                    updated.topic,
                    serde_json::to_string(&updated)?,
                    old
                ],
            )?;
            for id in &updated.knowledge_ids {
                tx.execute(
                    "INSERT OR IGNORE INTO study_knowledge(card_id,knowledge_id) VALUES(?1,?2)",
                    params![updated.id, id],
                )?;
            }
        }
        tx.execute("INSERT INTO settings(key,value) VALUES('encyclopedia.deep_upgrades.version','1') ON CONFLICT(key) DO UPDATE SET value='1'", [])?;
        tx.commit()?;
        Ok(())
    }

    fn prepare_encyclopedia_pack(
        &mut self,
        marker: &str,
        entries_json: &str,
        cards_json: &str,
    ) -> Result<()> {
        if self.setting(marker)?.as_deref() == Some("1") {
            return Ok(());
        }
        let entries: Vec<Entry> = serde_json::from_str(entries_json)?;
        let cards: Vec<Card> = serde_json::from_str(cards_json)?;
        for entry in &entries {
            entry.validate()?;
        }
        for card in &cards {
            card.validate()?;
        }
        let tx = self.desk.transaction()?;
        for item in entries {
            tx.execute("INSERT OR IGNORE INTO command_entries(id,language,category,title,summary,syntax,example,notes,source,search_text,entry_type,tags_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![item.id,item.language,item.category,item.title,item.summary,item.syntax,item.example,item.notes,item.source,item.indexed_text(),item.entry_type,serde_json::to_string(&item.tags)?])?;
        }
        tx.commit()?;
        // Import and marker are replayable: INSERT OR IGNORE never rewrites edits or history.
        self.import_study_cards(&cards)?;
        self.set_setting(marker, "1")?;
        Ok(())
    }
    /// 旧版 408 聚合包按“学科=语言”拆分：只迁移语言字段，条目内容、编辑与历史保持原样。
    fn prepare_cs408_subject_split(&mut self) -> Result<()> {
        const MARKER: &str = "encyclopedia.cs408.subjects.version";
        if self.setting(MARKER)?.as_deref() == Some("1") {
            return Ok(());
        }
        let ids = self
            .desk
            .prepare("SELECT id FROM command_entries WHERE language='408' AND deleted=0")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for id in ids {
            if let Some(mut entry) = self.reference_entry(&id)?
                && entry.language == "408"
            {
                entry.language = entry.category.clone();
                self.save_reference(&entry)?;
            }
        }
        self.set_setting(MARKER, "1")?;
        Ok(())
    }
    pub fn reference_count(&self, language: &str) -> Result<usize> {
        Ok(self.desk.query_row(
            "SELECT COUNT(*) FROM command_entries WHERE deleted=0 AND (?1='' OR language=?1)",
            [language],
            |r| r.get::<_, i64>(0),
        )? as usize)
    }
}
