use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use unicode_normalization::{UnicodeNormalization, is_nfkc};

use super::{Database, read_tags};
use crate::{
    error::{DeskError, Result},
    library::Book,
};

#[derive(Serialize)]
pub struct WritingEntry {
    id: String,
    book_id: String,
    title: String,
    tags: Vec<String>,
    updated_at: String,
}

#[derive(Serialize)]
pub struct WritingDirectory {
    pages: Vec<WritingEntry>,
    books: Vec<Book>,
}

#[derive(Serialize)]
pub struct WritingRecent {
    id: String,
    title: String,
    updated_at: String,
    is_essay: bool,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WritingScope {
    Writing,
    Essays,
}

#[derive(Serialize)]
pub struct WritingHit {
    id: String,
    title: String,
    excerpt: String,
    is_essay: bool,
    score: f64,
}

#[derive(Serialize, Default)]
pub struct WritingResults {
    hits: Vec<WritingHit>,
    total: usize,
}

// Reuse normalized text across queries while bounding the native memory cost.
const BODY_CACHE_LIMIT: usize = 24 * 1024 * 1024;
struct WritingSearchCache {
    directory: PathBuf,
    connection: Connection,
    version: i64,
    bodies: HashMap<String, String>,
    bytes: usize,
}
static SEARCH_CACHE: Mutex<Option<WritingSearchCache>> = Mutex::new(None);

impl Database {
    pub fn writing_directory(&self) -> Result<WritingDirectory> {
        let mut statement = self.desk.prepare_cached(
            "SELECT p.id,p.book_id,p.title,p.tags_json,p.updated_at
             FROM book_pages p JOIN live_books b ON b.id=p.book_id
             WHERE p.deleted=0 ORDER BY p.updated_at DESC,p.rowid DESC",
        )?;
        let pages = statement
            .query_map([], |row| {
                Ok(WritingEntry {
                    id: row.get(0)?,
                    book_id: row.get(1)?,
                    title: row.get(2)?,
                    tags: read_tags(row, 3)?,
                    updated_at: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(WritingDirectory {
            pages,
            books: self.books()?,
        })
    }

    pub fn writing_recent(&self, per_kind: u32) -> Result<Vec<WritingRecent>> {
        let mut statement = self.desk.prepare_cached(
            "WITH ranked AS (
                SELECT p.id,p.title,p.updated_at,p.rowid AS page_order,
                       e.page_id IS NOT NULL AS is_essay, julianday(p.updated_at) AS edited,
                       ROW_NUMBER() OVER (
                           PARTITION BY e.page_id IS NOT NULL
                           ORDER BY julianday(p.updated_at) DESC,p.rowid DESC
                       ) AS position
                FROM book_pages p JOIN live_books b ON b.id=p.book_id
                LEFT JOIN daily_essays e ON e.page_id=p.id
                WHERE p.deleted=0
             )
             SELECT id,title,updated_at,is_essay FROM ranked
             WHERE position<=?1 ORDER BY edited DESC,page_order DESC",
        )?;
        let rows = statement.query_map([per_kind], |row| {
            Ok(WritingRecent {
                id: row.get(0)?,
                title: row.get(1)?,
                updated_at: row.get(2)?,
                is_essay: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

// A search can scan the library without holding the connection that saves drafts.
fn read_connection(data: &Path) -> Result<Connection> {
    let connection =
        Connection::open_with_flags(data.join("desk.db"), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_secs(3))?;
    Ok(connection)
}

pub(crate) fn matching_ids(data: &Path, query: &str) -> Result<Vec<String>> {
    let term = lowercase(query.trim());
    if term.is_empty() {
        return Ok(Vec::new());
    }
    let connection = read_connection(data)?;
    let mut statement = connection.prepare(
        "SELECT p.id,p.title,p.tags_json,p.body FROM book_pages p
         JOIN live_books b ON b.id=p.book_id WHERE p.deleted=0",
    )?;
    let mut rows = statement.query([])?;
    let mut matches = Vec::new();
    while let Some(row) = rows.next()? {
        let title: String = row.get(1)?;
        let tags = read_tags(row, 2)?;
        let mut matched = lowercase(&title).contains(term.as_ref())
            || tags
                .iter()
                .any(|tag| lowercase(tag).contains(term.as_ref()));
        if !matched {
            let body: String = row.get(3)?;
            matched = lowercase(&body).contains(term.as_ref());
        }
        if matched {
            matches.push(row.get(0)?);
        }
    }
    Ok(matches)
}

pub(crate) fn search(
    data: &Path,
    query: &str,
    scope: Option<WritingScope>,
) -> Result<WritingResults> {
    const LIMIT: usize = 24;
    let normalized = normalize(query);
    let terms = normalized.split_whitespace().collect::<Vec<_>>();
    if terms.is_empty() {
        return Ok(WritingResults::default());
    }
    let mut cache = SEARCH_CACHE
        .lock()
        .map_err(|_| DeskError::InvalidInput("文档搜索暂时不可用".into()))?;
    if cache.as_ref().is_none_or(|cache| cache.directory != data) {
        *cache = Some(WritingSearchCache {
            directory: data.to_path_buf(),
            connection: read_connection(data)?,
            version: -1,
            bodies: HashMap::new(),
            bytes: 0,
        });
    }
    let WritingSearchCache {
        connection,
        version,
        bodies,
        bytes,
        ..
    } = cache.as_mut().unwrap();
    let snapshot = connection.transaction()?;
    // Establish the read snapshot before checking for writes from other connections.
    snapshot.query_row("SELECT COUNT(*) FROM sqlite_schema", [], |_| Ok(()))?;
    let current_version = snapshot.pragma_query_value(None, "data_version", |row| row.get(0))?;
    if *version != current_version {
        bodies.clear();
        *bytes = 0;
        *version = current_version;
    }
    let is_essay = scope.map(|scope| matches!(scope, WritingScope::Essays));
    let mut statement = snapshot.prepare_cached(
        "SELECT p.id,p.title,p.tags_json,e.page_id IS NOT NULL,
                CASE WHEN json_valid(e.topic_json) THEN json_extract(e.topic_json,'$.title') END
         FROM book_pages p JOIN live_books b ON b.id=p.book_id
         LEFT JOIN daily_essays e ON e.page_id=p.id
         WHERE p.deleted=0 AND (?1 IS NULL OR (e.page_id IS NOT NULL)=?1)
         ORDER BY p.updated_at DESC,p.rowid DESC",
    )?;
    let mut read_body = snapshot.prepare_cached("SELECT body FROM book_pages WHERE id=?1")?;
    let mut rows = statement.query([is_essay])?;
    let mut result = WritingResults::default();
    let mut scores = vec![0.0; terms.len()];
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let title = if title.is_empty() {
            "未命名文档"
        } else {
            &title
        };
        let tags = read_tags(row, 2)?;
        let topic: Option<String> = row.get(4)?;
        scores.fill(0.0);
        score_field(&terms, &mut scores, title, 3.0);
        score_field(&terms, &mut scores, &tags.join(" "), 1.2);
        if let Some(topic) = &topic {
            score_field(&terms, &mut scores, topic, 1.0);
        }
        let mut body: Option<String> = None;
        if scores.iter().any(|&score| score < 550.0) {
            if let Some(normalized) = bodies.get(&id) {
                score_normalized(&terms, &mut scores, normalized, 0.55);
            } else {
                let raw: String = read_body.query_row([&id], |row| row.get(0))?;
                let normalized = normalize(&raw);
                score_normalized(&terms, &mut scores, &normalized, 0.55);
                let capacity = match &normalized {
                    Cow::Borrowed(text) => text.len(),
                    Cow::Owned(text) => text.capacity(),
                };
                let size = capacity + id.len() + 64;
                if *bytes + size <= BODY_CACHE_LIMIT {
                    *bytes += size;
                    bodies.insert(id.clone(), normalized.into_owned());
                }
                body = Some(raw);
            }
        }
        if scores.contains(&0.0) {
            continue;
        }
        result.total += 1;
        let score = scores.iter().sum();
        let position = result.hits.partition_point(|hit| hit.score >= score);
        if position >= LIMIT {
            continue;
        }
        let body = match body {
            Some(body) => body,
            None => read_body.query_row([&id], |row| row.get(0))?,
        };
        let excerpt = excerpt(&body);
        let excerpt = if !excerpt.is_empty() {
            excerpt
        } else if !tags.is_empty() {
            tags.join(" / ")
        } else {
            topic.unwrap_or_else(|| "未填写正文".into())
        };
        result.hits.insert(
            position,
            WritingHit {
                id,
                title: title.into(),
                excerpt,
                is_essay: row.get(3)?,
                score,
            },
        );
        result.hits.truncate(LIMIT);
    }
    Ok(result)
}

fn lowercase(value: &str) -> Cow<'_, str> {
    if value.is_ascii() {
        if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Cow::Owned(value.to_ascii_lowercase());
        }
        return Cow::Borrowed(value);
    }
    Cow::Owned(value.to_lowercase())
}

fn normalize(value: &str) -> Cow<'_, str> {
    if value.is_ascii() {
        return lowercase(value);
    }
    if let Some(normalized) = normalize_common(value) {
        return normalized;
    }
    if is_nfkc(value) {
        lowercase(value)
    } else {
        Cow::Owned(value.nfkc().collect::<String>().to_lowercase())
    }
}

// The common character set cannot compose across characters. Other Unicode
// text goes through the full normalizer, including any combining marks.
fn normalize_common(value: &str) -> Option<Cow<'_, str>> {
    let mut normalized = None;
    let mut previous = 0;
    for (index, ch) in value.char_indices() {
        let replacement = match ch {
            'A'..='Z' => ch.to_ascii_lowercase(),
            '\u{ff01}'..='\u{ff5e}' => ((ch as u32 - 0xfee0) as u8).to_ascii_lowercase() as char,
            '\u{3000}' => ' ',
            '\0'..='\x7f'
            | '\u{4e00}'..='\u{9fff}'
            | '、'
            | '。'
            | '“'
            | '”'
            | '‘'
            | '’'
            | '《'
            | '》'
            | '【'
            | '】'
            | '·'
            | '—' => continue,
            _ => return None,
        };
        let buffer = normalized.get_or_insert_with(|| String::with_capacity(value.len()));
        buffer.push_str(&value[previous..index]);
        buffer.push(replacement);
        previous = index + ch.len_utf8();
    }
    Some(match normalized {
        Some(mut buffer) => {
            buffer.push_str(&value[previous..]);
            Cow::Owned(buffer)
        }
        None => Cow::Borrowed(value),
    })
}

fn score_field(terms: &[&str], scores: &mut [f64], field: &str, weight: f64) {
    if field.is_empty() || scores.iter().all(|&score| score >= 1000.0 * weight) {
        return;
    }
    score_normalized(terms, scores, &normalize(field), weight);
}

fn score_normalized(terms: &[&str], scores: &mut [f64], field: &str, weight: f64) {
    for (term, score) in terms.iter().zip(scores) {
        if *score >= 1000.0 * weight {
            continue;
        }
        if let Some(index) = field.find(term) {
            let base = if field == *term {
                1000.0
            } else if index == 0 {
                700.0
            } else if field[..index].ends_with(|ch: char| {
                ch.is_whitespace()
                    || "-_/\\.,;:!?()[]{}<>\"'`~@#$%^&*+=|，。；：！？、（）【】《》“”‘’·"
                        .contains(ch)
            }) {
                550.0
            } else {
                300.0
            };
            *score = score.max(base * weight);
        }
    }
}

fn excerpt(body: &str) -> String {
    let mut preview = String::new();
    let mut chars = 0;
    for word in body.split_whitespace() {
        if chars != 0 {
            preview.push(' ');
            chars += 1;
        }
        for ch in word.chars().take(80 - chars) {
            preview.push(ch);
            chars += 1;
        }
        if chars == 80 {
            break;
        }
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightweight_reads_keep_full_text_matches_and_hide_deleted_pages() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::open(dir.path()).unwrap();
        let mut document = db.create_writing("项目进度", None, None).unwrap();
        document.body = format!("{} ＲＡＲＥ词", "普通正文 ".repeat(100));
        db.finish_writing_save("test", &mut document).unwrap();
        let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let assignment = db.ensure_essay_day(day).unwrap();
        let mut essay = db.start_essay(&assignment.day).unwrap();
        essay.body = document.body.clone();
        db.finish_writing_save("test", &mut essay).unwrap();
        let deleted = db.create_writing("rare词", None, None).unwrap();
        db.set_library_deleted(&deleted.id, true, true).unwrap();

        let directory = db.writing_directory().unwrap();
        assert_eq!(directory.pages.len(), 2);
        assert!(
            !serde_json::to_string(&directory)
                .unwrap()
                .contains("普通正文")
        );
        assert_eq!(
            db.writing_recent(1)
                .unwrap()
                .iter()
                .filter(|row| row.is_essay)
                .count(),
            1
        );
        assert_eq!(db.writing_recent(1).unwrap().len(), 2);
        assert_eq!(matching_ids(dir.path(), "ＲＡＲＥ词").unwrap().len(), 2);
        let result = search(dir.path(), "项目 rare词", None).unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.hits[0].id, document.id);
        let result = search(dir.path(), "rare词", Some(WritingScope::Essays)).unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.hits[0].id, essay.id);

        document.body = "刚刚更新后的正文".into();
        db.finish_writing_save("test", &mut document).unwrap();
        assert_eq!(
            search(dir.path(), "rare词", Some(WritingScope::Writing))
                .unwrap()
                .total,
            0
        );
        assert_eq!(
            search(dir.path(), "更新后", None).unwrap().hits[0].id,
            document.id
        );
        *SEARCH_CACHE.lock().unwrap() = None;
    }
}
