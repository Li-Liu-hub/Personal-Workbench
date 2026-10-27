//! Recover formatting metadata dropped by pre-formatting clients, without replacing text.
use super::*;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct OriginalFormat {
    version: u32,
    style: Style,
    entries: BTreeMap<String, EntryFormat>,
}
#[derive(Deserialize)]
struct EntryFormat {
    highlights: Vec<String>,
    bullets: BTreeMap<String, Vec<String>>,
}

pub(super) fn restore_legacy(
    document: &mut Resume,
    raw: &serde_json::Value,
    directory: &Path,
) -> Result<()> {
    let path = directory.join("resume-format.json");
    if !path.is_file() {
        return Ok(());
    }
    if fs::metadata(&path)?.len() > 250_000 {
        return Err(DeskError::InvalidInput("简历样式备份文件过大".into()));
    }
    let original: OriginalFormat = serde_json::from_slice(&fs::read(path)?)?;
    if original.version != 1 {
        return Err(DeskError::InvalidInput("不支持此简历样式备份版本".into()));
    }
    if raw["style"].get("line_pt").is_none() && document.style.font_pt == 10. {
        document.style.font_pt = original.style.font_pt;
    }
    let Some(sections) = raw["sections"].as_array() else {
        return Ok(());
    };
    for (section, raw_section) in document.sections.iter_mut().zip(sections) {
        let Some(entries) = raw_section["entries"].as_array() else {
            continue;
        };
        for (entry, raw_entry) in section.entries.iter_mut().zip(entries) {
            let Some(saved) = original.entries.get(&entry.id) else {
                continue;
            };
            if raw_entry.get("highlights").is_none() {
                entry.highlights = matching(&entry.summary, &saved.highlights);
            }
            let Some(bullets) = raw_entry["bullets"].as_array() else {
                continue;
            };
            for (bullet, raw_bullet) in entry.bullets.iter_mut().zip(bullets) {
                if raw_bullet.get("highlights").is_none() {
                    let phrases = saved
                        .bullets
                        .values()
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>();
                    bullet.highlights = matching(&bullet.body, &phrases);
                }
            }
        }
    }
    Ok(())
}
fn matching(text: &str, phrases: &[String]) -> Vec<String> {
    let chars = text
        .char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .collect::<Vec<_>>();
    let normalized = chars.iter().map(|(_, c)| *c).collect::<String>();
    phrases
        .iter()
        .filter_map(|phrase| {
            let needle = phrase
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>();
            if needle.is_empty() {
                return None;
            }
            let at = normalized.find(&needle)?;
            let start = normalized[..at].chars().count();
            let last = start + needle.chars().count() - 1;
            Some(text[chars[start].0..chars[last].0 + chars[last].1.len_utf8()].to_owned())
        })
        .collect()
}
