use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::error::{DeskError, Result};

pub trait DocumentFormatHandler: Send + Sync {
    fn extension(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn body_search(&self) -> bool;
    fn create_blank(&self, path: &Path) -> Result<()>;
}

pub fn default_documents_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("documents")
}

/// New in-app documents get an internal identity; users never choose a storage path.
pub fn create_managed_document(data_dir: &Path) -> Result<PathBuf> {
    let handler = PlainTextHandler { markdown: true };
    debug_assert!(handler.body_search());
    let directory = default_documents_dir(data_dir);
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{}.{}", uuid::Uuid::new_v4(), handler.extension()));
    create_document(&handler, &path)
}

pub struct PlainTextHandler {
    pub markdown: bool,
}
impl DocumentFormatHandler for PlainTextHandler {
    fn extension(&self) -> &'static str {
        if self.markdown { "md" } else { "txt" }
    }
    fn label(&self) -> &'static str {
        if self.markdown { "Markdown" } else { "TXT" }
    }
    fn body_search(&self) -> bool {
        true
    }
    fn create_blank(&self, path: &Path) -> Result<()> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?
            .sync_all()?;
        Ok(())
    }
}

pub struct WordHandler;
impl DocumentFormatHandler for WordHandler {
    fn extension(&self) -> &'static str {
        "docx"
    }
    fn label(&self) -> &'static str {
        "Word 文档"
    }
    fn body_search(&self) -> bool {
        false
    }
    fn create_blank(&self, path: &Path) -> Result<()> {
        self.export_text(path, "")
    }
}

impl WordHandler {
    pub fn export_text(&self, path: &Path, text: &str) -> Result<()> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let paragraphs = text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .map(|line| {
                let escaped = line
                    .chars()
                    .map(|ch| match ch {
                        '&' => "&amp;".into(),
                        '<' => "&lt;".into(),
                        '>' => "&gt;".into(),
                        '\t'
                        | '\u{20}'..='\u{D7FF}'
                        | '\u{E000}'..='\u{FFFD}'
                        | '\u{10000}'..='\u{10FFFF}' => ch.to_string(),
                        _ => "\u{FFFD}".into(),
                    })
                    .collect::<String>();
                format!("<w:p><w:r><w:t xml:space=\"preserve\">{escaped}</w:t></w:r></w:p>")
            })
            .collect::<String>();
        let document_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{paragraphs}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>"#
        );
        let parts: [(&str, &str); 4] = [
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
            ),
            ("word/document.xml", &document_xml),
            (
                "word/_rels/document.xml.rels",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#,
            ),
        ];
        for (name, body) in parts {
            zip.start_file(name, options)
                .map_err(|e| DeskError::InvalidPath(e.to_string()))?;
            zip.write_all(body.as_bytes())?;
        }
        zip.finish()
            .map_err(|e| DeskError::InvalidPath(e.to_string()))?
            .sync_all()?;
        Ok(())
    }
}

pub fn create_document(handler: &dyn DocumentFormatHandler, path: &Path) -> Result<PathBuf> {
    handler.create_blank(path)?;
    Ok(path.to_path_buf())
}

pub fn read_text_with_fingerprint(path: &Path) -> Result<(String, [u8; 32])> {
    let bytes = fs::read(path)?;
    let fingerprint = fingerprint_bytes(&bytes);
    let text = String::from_utf8(bytes)
        .map_err(|error| DeskError::InvalidPath(format!("文本不是有效的 UTF-8：{error}")))?;
    Ok((text.trim_start_matches('\u{feff}').to_string(), fingerprint))
}

pub fn fingerprint(path: &Path) -> Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

pub fn fingerprint_bytes(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn save_text_atomic_checked(
    path: &Path,
    text: &str,
    expected_fingerprint: Option<[u8; 32]>,
) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| DeskError::InvalidPath("文档目录无效".into()))?;
    fs::create_dir_all(parent)?;
    verify_fingerprint(path, expected_fingerprint)?;
    let temp = parent.join(format!(".personaldesk-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        verify_fingerprint(path, expected_fingerprint)?;
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{
                MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
            };
            let src = temp
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>();
            let dst = path
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>();
            let ok = unsafe {
                MoveFileExW(
                    src.as_ptr(),
                    dst.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            };
            if ok == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        #[cfg(not(windows))]
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub fn save_text_new(path: &Path, text: &str) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn verify_fingerprint(path: &Path, expected: Option<[u8; 32]>) -> Result<()> {
    let actual = if path.exists() {
        Some(fingerprint(path)?)
    } else {
        None
    };
    if actual != expected {
        return Err(DeskError::ConcurrentModification);
    }
    Ok(())
}
