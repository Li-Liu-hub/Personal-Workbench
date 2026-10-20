use std::{
    fs::File,
    io::{Cursor, Read, Write},
    path::Path,
};

use calamine::{Reader, open_workbook_auto_from_rs};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{DeskError, Result};

#[cfg(test)]
pub(crate) mod tests;

pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CONTACT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub school: String,
    pub work: String,
    pub phone: String,
    pub national_id: String,
    pub address: String,
    pub gender: String,
    pub hobbies: String,
    pub chat_notes: String,
    #[serde(default)]
    pub birthday: String,
    #[serde(default)]
    pub major: String,
    #[serde(skip)]
    pub photo: Vec<u8>,
    #[serde(default)]
    pub chats: Vec<ChatRecord>,
}

impl Contact {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            ..Self::default()
        }
    }

    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        [
            &self.name,
            &self.school,
            &self.work,
            &self.phone,
            &self.national_id,
            &self.address,
            &self.gender,
            &self.hobbies,
            &self.chat_notes,
            &self.birthday,
            &self.major,
        ]
        .iter()
        .any(|value| value.to_lowercase().contains(&query))
            || self.chats.iter().any(|chat| {
                chat.title.to_lowercase().contains(&query)
                    || chat.text.to_lowercase().contains(&query)
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatRecord {
    pub id: String,
    pub title: String,
    pub format: String,
    pub text: String,
    pub sheets: Vec<ChatSheet>,
    // Binary originals live in a SQLite BLOB and separate ZIP members, not JSON arrays.
    #[serde(skip)]
    pub original: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatSheet {
    pub name: String,
    pub rows: Vec<Vec<String>>,
}

pub fn import_chat(path: &Path) -> Result<ChatRecord> {
    let format = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if !matches!(
        format.as_str(),
        "txt" | "md" | "markdown" | "json" | "xlsx" | "xls"
    ) {
        return Err(DeskError::UnsupportedFormat);
    }
    let mut original = Vec::new();
    File::open(path)?
        .take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut original)?;
    if original.len() > MAX_FILE_BYTES {
        return Err(DeskError::InvalidPath("单份聊天记录不能超过 8 MiB".into()));
    }
    let mut sheets = vec![];
    let text = if matches!(format.as_str(), "xlsx" | "xls") {
        if format == "xlsx" {
            let mut archive = zip::ZipArchive::new(Cursor::new(&original))?;
            let mut total = 0_u64;
            for index in 0..archive.len() {
                total = total.saturating_add(archive.by_index(index)?.size());
                if total > 64 * 1024 * 1024 {
                    return Err(DeskError::InvalidPath(
                        "Excel 解压后超过 64 MiB，请拆分导入".into(),
                    ));
                }
            }
        }
        let mut workbook = open_workbook_auto_from_rs(Cursor::new(&original))
            .map_err(|e| DeskError::InvalidPath(format!("无法读取 Excel：{e}")))?;
        let mut cells = 0;
        let mut preview = String::new();
        for name in workbook.sheet_names().to_vec() {
            let range = workbook
                .worksheet_range(&name)
                .map_err(|e| DeskError::InvalidPath(format!("无法读取工作表 {name}：{e}")))?;
            if range.width() > 256 {
                return Err(DeskError::InvalidPath(
                    "工作表超过 256 列，请拆分导入".into(),
                ));
            }
            cells += range.height().saturating_mul(range.width());
            if cells > 100_000 {
                return Err(DeskError::InvalidPath(
                    "Excel 超过 100,000 个单元格，请拆分导入".into(),
                ));
            }
            let rows: Vec<Vec<String>> = range
                .rows()
                .map(|row| row.iter().map(ToString::to_string).collect())
                .collect();
            preview.push_str(&format!("工作表：{name}\n"));
            for row in &rows {
                preview.push_str(&row.join("\t"));
                preview.push('\n');
            }
            if preview.len() > MAX_FILE_BYTES {
                return Err(DeskError::InvalidPath(
                    "Excel 文本超过 8 MiB，请拆分导入".into(),
                ));
            }
            sheets.push(ChatSheet { name, rows });
        }
        preview
    } else {
        let text = decode_text(&original)?;
        if format == "json" {
            // Accept arbitrary external message schemas; retain every field and original byte.
            serde_json::to_string_pretty(&serde_json::from_str::<serde_json::Value>(&text)?)?
        } else {
            text
        }
    };
    Ok(ChatRecord {
        id: Uuid::new_v4().to_string(),
        title: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        format,
        text,
        sheets,
        original,
    })
}

fn decode_text(bytes: &[u8]) -> Result<String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little = bytes[0] == 0xff;
        if !(bytes.len() - 2).is_multiple_of(2) {
            return Err(DeskError::InvalidPath("UTF-16 文本长度无效".into()));
        }
        let units = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| {
                if little {
                    u16::from_le_bytes([b[0], b[1]])
                } else {
                    u16::from_be_bytes([b[0], b[1]])
                }
            })
            .collect::<Vec<_>>();
        String::from_utf16(&units).map_err(|_| DeskError::InvalidPath("UTF-16 文本无法解码".into()))
    } else {
        String::from_utf8(
            bytes
                .strip_prefix(&[0xef, 0xbb, 0xbf])
                .unwrap_or(bytes)
                .to_vec(),
        )
        .map_err(|_| {
            DeskError::InvalidPath("请将文本保存为 UTF-8 或带 BOM 的 UTF-16 后导入".into())
        })
    }
}

pub fn validate(contact: &Contact) -> Result<()> {
    crate::model::validate_date(&contact.birthday)?;
    if contact.photo.len() > MAX_FILE_BYTES {
        return Err(DeskError::InvalidPath("照片不能超过 8 MiB".into()));
    }
    if contact.id.is_empty() || contact.name.trim().is_empty() {
        return Err(DeskError::InvalidPath("请填写联系人姓名".into()));
    }
    if contact.chats.iter().any(|chat| {
        !matches!(
            chat.format.as_str(),
            "txt" | "md" | "markdown" | "json" | "xlsx" | "xls"
        )
    }) {
        return Err(DeskError::UnsupportedFormat);
    }
    let total: usize = contact.chats.iter().map(|chat| chat.original.len()).sum();
    if total > MAX_CONTACT_BYTES {
        return Err(DeskError::InvalidPath(
            "每位联系人的聊天原文件总量不能超过 32 MiB".into(),
        ));
    }
    Ok(())
}

pub fn import_photo(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    File::open(path)?
        .take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(DeskError::InvalidPath("照片不能超过 8 MiB".into()));
    }
    let mut reader = image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|_| DeskError::InvalidPath("照片格式不可用".into()))?;
    Ok(bytes)
}

pub fn append_chats(contact: &mut Contact, chats: Vec<ChatRecord>) -> Result<usize> {
    let mut candidate = contact.clone();
    let before = candidate.chats.len();
    for chat in chats {
        if !candidate
            .chats
            .iter()
            .any(|existing| existing.format == chat.format && existing.original == chat.original)
        {
            candidate.chats.push(chat);
        }
    }
    // A new contact may receive records before its name is entered.
    let name = candidate.name.clone();
    if candidate.name.trim().is_empty() {
        candidate.name = "待填写".into();
    }
    validate(&candidate)?;
    candidate.name = name;
    let added = candidate.chats.len() - before;
    *contact = candidate;
    Ok(added)
}

pub fn export_chat(chat: &ChatRecord, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .filter(|p| p.is_dir())
        .ok_or_else(|| DeskError::InvalidPath("导出目录不可用".into()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&chat.original)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(destination)
        .map_err(|e| DeskError::Io(e.error))?;
    Ok(())
}
