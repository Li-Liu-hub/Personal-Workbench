//! File-based résumé model and storage; rendering is a replaceable local process adapter.
use crate::error::{DeskError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

mod format;
mod render;
pub use render::{Rendered, TypstRenderer};
#[cfg(test)]
mod tests;

pub const TEMPLATE: &str = include_str!("../assets/resume.typ");

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Resume {
    pub version: u32,
    pub name: String,
    pub tagline: String,
    pub contact: String,
    pub location: String,
    pub photo: String,
    pub style: Style,
    pub sections: Vec<ResumeSection>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Style {
    pub font_pt: f32,
    pub margin_mm: f32,
    pub show_photo: bool,
    pub line_pt: f32,
    pub project_gap_pt: f32,
    pub skills_gap_pt: f32,
    pub top_mm: f32,
    pub bottom_mm: f32,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            font_pt: 10.5,
            margin_mm: 8.5,
            show_photo: true,
            line_pt: 14.72,
            project_gap_pt: 7.,
            skills_gap_pt: 3.,
            top_mm: 6.5,
            bottom_mm: 3.,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ResumeSection {
    pub id: String,
    pub title: String,
    pub entries: Vec<Entry>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub id: String,
    pub kind: String,
    pub heading: String,
    pub meta: String,
    pub summary: String,
    pub bullets: Vec<Bullet>,
    #[serde(default)]
    pub highlights: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Bullet {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub highlights: Vec<String>,
}

impl Default for Resume {
    fn default() -> Self {
        Self {
            version: 1,
            name: String::new(),
            tagline: String::new(),
            contact: String::new(),
            location: String::new(),
            photo: String::new(),
            style: Style::default(),
            sections: ["教育背景", "实习经历", "专业技能"]
                .map(|title| ResumeSection {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: title.into(),
                    entries: vec![],
                })
                .to_vec(),
        }
    }
}
impl Resume {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err(DeskError::InvalidInput(
                "此简历版本暂不支持，请保留源文件".into(),
            ));
        }
        if !(7.0..=14.0).contains(&self.style.font_pt)
            || !(6.0..=25.0).contains(&self.style.margin_mm)
            || !(8.0..=28.0).contains(&self.style.line_pt)
            || !(0.0..=20.0).contains(&self.style.project_gap_pt)
            || !(0.0..=20.0).contains(&self.style.skills_gap_pt)
            || !(3.0..=25.0).contains(&self.style.top_mm)
            || !(2.0..=25.0).contains(&self.style.bottom_mm)
        {
            return Err(DeskError::InvalidInput("字号或页边距超出支持范围".into()));
        }
        if !self.photo.is_empty()
            && (Path::new(&self.photo).components().count() != 1
                || !self.photo.starts_with("photo-")
                || self.photo.contains(['/', '\\']))
        {
            return Err(DeskError::InvalidInput(
                "照片必须是简历目录中的 photo- 文件".into(),
            ));
        }
        if self.sections.len() > 30
            || self
                .sections
                .iter()
                .any(|s| s.entries.len() > 100 || s.entries.iter().any(|e| e.bullets.len() > 100))
            || serde_json::to_vec(self)?.len() > 1_000_000
        {
            return Err(DeskError::InvalidInput("简历内容过多，请精简后保存".into()));
        }
        Ok(())
    }
    pub fn revision(&self) -> Result<Vec<u8>> {
        Ok(Sha256::digest(serde_json::to_vec_pretty(self)?).to_vec())
    }
}

pub struct Store {
    pub directory: PathBuf,
}
pub struct Loaded {
    pub document: Resume,
    pub revision: Option<Vec<u8>>,
}
impl Store {
    pub fn load(&self) -> Result<Loaded> {
        let path = self.directory.join("resume.json");
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Loaded {
                    document: Resume::default(),
                    revision: None,
                });
            }
            Err(e) => return Err(e.into()),
        };
        if bytes.len() > 1_000_000 {
            return Err(DeskError::InvalidInput("简历文件超过 1 MB".into()));
        }
        let raw: serde_json::Value = serde_json::from_slice(&bytes)?;
        let mut document: Resume = serde_json::from_value(raw.clone())?;
        format::restore_legacy(&mut document, &raw, &self.directory)?;
        document.validate()?;
        Ok(Loaded {
            document,
            revision: Some(Sha256::digest(&bytes).to_vec()),
        })
    }
    pub fn save(&self, document: &Resume, expected: Option<&[u8]>) -> Result<Vec<u8>> {
        document.validate()?;
        fs::create_dir_all(&self.directory)?;
        let target = self.directory.join("resume.json");
        let previous = match fs::read(&target) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let revision = previous.as_ref().map(|b| Sha256::digest(b).to_vec());
        if revision.as_deref() != expected {
            return Err(DeskError::ConcurrentModification);
        }
        let bytes = serde_json::to_vec_pretty(document)?;
        if let Some(previous) = previous {
            atomic_write(&self.directory.join("resume.previous.json"), &previous)?;
        }
        atomic_write(&target, &bytes)?;
        Ok(Sha256::digest(&bytes).to_vec())
    }
    pub fn import_photo(&self, source: &Path) -> Result<String> {
        if fs::metadata(source)?.len() > 10_000_000 {
            return Err(DeskError::InvalidInput("照片请控制在 10 MB 以内".into()));
        }
        let bytes = fs::read(source)?;
        let image = image::load_from_memory(&bytes)
            .map_err(|e| DeskError::InvalidInput(format!("无法读取照片：{e}")))?;
        let image = image.thumbnail(600, 800);
        fs::create_dir_all(&self.directory)?;
        let name = format!("photo-{}.png", uuid::Uuid::new_v4());
        image
            .save(self.directory.join(&name))
            .map_err(|e| DeskError::InvalidInput(e.to_string()))?;
        Ok(name)
    }
    /// User-selected conflict resolution. Keep an independent copy of the external file.
    pub fn save_preserving_external(&self, document: &Resume) -> Result<(Vec<u8>, PathBuf)> {
        document.validate()?;
        let bytes = fs::read(self.directory.join("resume.json"))?;
        let expected = Sha256::digest(&bytes).to_vec();
        let backup = self
            .directory
            .join(format!("resume.external-{}.json", uuid::Uuid::new_v4()));
        atomic_write(&backup, &bytes)?;
        // Recheck after backing up, so another intervening edit still blocks the save.
        let revision = self.save(document, Some(&expected))?;
        Ok((revision, backup))
    }
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| DeskError::InvalidPath("缺少父目录".into()))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Explicit configuration wins. Installed clients keep documents beside their data.
pub fn directory(data_dir: &Path, configured: Option<&str>) -> PathBuf {
    if let Some(path) = configured.filter(|s| !s.trim().is_empty()) {
        return PathBuf::from(path);
    }
    data_dir.join("docs/简历")
}
