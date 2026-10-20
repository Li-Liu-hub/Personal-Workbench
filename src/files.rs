use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::{
    error::{DeskError, Result},
    model::FileItem,
};

pub fn choose_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("选择快捷文件夹")
        .pick_folder()
}
pub fn choose_file() -> Option<PathBuf> {
    rfd::FileDialog::new().set_title("选择快捷文件").pick_file()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileFilter {
    #[default]
    All,
    Folders,
    Documents,
    Images,
    Video,
    Audio,
    Archives,
    Programs,
    Other,
}
impl FileFilter {
    pub const ALL: [Self; 9] = [
        Self::All,
        Self::Folders,
        Self::Documents,
        Self::Images,
        Self::Video,
        Self::Audio,
        Self::Archives,
        Self::Programs,
        Self::Other,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "全部",
            Self::Folders => "文件夹",
            Self::Documents => "文档",
            Self::Images => "图片",
            Self::Video => "视频",
            Self::Audio => "音频",
            Self::Archives => "压缩包",
            Self::Programs => "程序",
            Self::Other => "其他",
        }
    }
    pub fn matches(self, path: &Path, is_dir: bool) -> bool {
        if self == Self::All {
            return true;
        }
        if is_dir {
            return self == Self::Folders;
        }
        let ext = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        let kind = match ext.as_str() {
            "txt" | "md" | "markdown" | "doc" | "docx" | "pdf" | "xls" | "xlsx" | "csv" | "ppt"
            | "pptx" | "rtf" | "sql" | "json" | "xml" | "yaml" | "yml" => Self::Documents,
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "svg" | "ico" | "heic" => {
                Self::Images
            }
            "mp4" | "mkv" | "avi" | "mov" | "webm" | "wmv" => Self::Video,
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" => Self::Audio,
            "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" => Self::Archives,
            "exe" | "msi" | "bat" | "cmd" | "ps1" | "lnk" => Self::Programs,
            _ => Self::Other,
        };
        kind == self
    }
}

pub fn open_in_system(path: &Path) -> Result<()> {
    open::that_detached(shell_path(path))
        .map_err(|e| DeskError::InvalidPath(format!("无法打开此位置：{e}")))
}

pub fn open_with_target(path: &Path) -> Result<PathBuf> {
    let path = path.canonicalize()?;
    if !path.is_file() {
        return Err(DeskError::InvalidPath("请选择一个实际文件".into()));
    }
    Ok(shell_path(&path))
}

pub fn shell_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::{
            ffi::OsString,
            os::windows::ffi::{OsStrExt, OsStringExt},
        };
        let value = path.as_os_str().encode_wide().collect::<Vec<_>>();
        let prefix = |s: &str| s.encode_utf16().collect::<Vec<_>>();
        if value.starts_with(&prefix(r"\\?\UNC\")) {
            return OsString::from_wide(
                &prefix(r"\\")
                    .into_iter()
                    .chain(value[8..].iter().copied())
                    .collect::<Vec<_>>(),
            )
            .into();
        }
        if value.starts_with(&prefix(r"\\?\")) {
            return OsString::from_wide(&value[4..]).into();
        }
    }
    path.to_path_buf()
}

pub fn open_with_dialog(path: &Path) -> Result<()> {
    let path = open_with_target(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::{
            System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
            UI::{
                Shell::{OAIF_EXEC, OPENASINFO, SHOpenWithDialog},
                WindowsAndMessaging::GetForegroundWindow,
            },
        };
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // The GUI thread normally already owns an apartment through native drag/drop.
        let initialized =
            unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        let info = OPENASINFO {
            pcszFile: wide.as_ptr(),
            pcszClass: std::ptr::null(),
            oaifInFlags: OAIF_EXEC,
        };
        let result = unsafe { SHOpenWithDialog(GetForegroundWindow(), &info) };
        if initialized >= 0 {
            unsafe { CoUninitialize() };
        }
        if result < 0 && result as u32 != 0x800704c7 {
            return Err(DeskError::InvalidPath(format!(
                "无法显示打开方式：0x{:08X}",
                result as u32
            )));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(DeskError::UnsupportedFormat)
    }
}

pub fn list_directory(path: &Path) -> Result<Vec<FileItem>> {
    let canonical = path.canonicalize()?;
    if !canonical.is_dir() {
        return Err(DeskError::InvalidPath("所选路径不是文件夹".into()));
    }
    let mut items = Vec::new();
    for entry in fs::read_dir(&canonical)? {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => continue,
        };
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(".personal-desk-deleted-")
        {
            continue;
        }
        let kind = match entry.file_type() {
            Ok(value) if !value.is_symlink() => value,
            _ => continue,
        };
        let metadata = match entry.metadata() {
            Ok(value) => value,
            Err(_) => continue,
        };
        let modified = metadata
            .modified()
            .ok()
            .map(|time| {
                let dt: chrono::DateTime<chrono::Local> = time.into();
                dt.format("%Y-%m-%d %H:%M").to_string()
            })
            .unwrap_or_default();
        items.push(FileItem {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: entry.path(),
            is_dir: kind.is_dir(),
            size: metadata.len(),
            modified,
        });
    }
    items.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(items)
}

pub fn create_folder(parent: &Path, name: &str) -> Result<PathBuf> {
    validate_name(name)?;
    let parent = parent.canonicalize()?;
    if !parent.is_dir() {
        return Err(DeskError::InvalidPath("当前路径不可用".into()));
    }
    let target = parent.join(name.trim());
    if target.exists() {
        return Err(DeskError::AlreadyExists);
    }
    fs::create_dir(&target)?;
    Ok(target)
}

pub fn rename_path(path: &Path, new_name: &str) -> Result<PathBuf> {
    validate_name(new_name)?;
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(DeskError::InvalidPath("不对符号链接或联接重命名".into()));
    }
    let source = path.canonicalize()?;
    let parent = source
        .parent()
        .ok_or_else(|| DeskError::InvalidPath("不能重命名磁盘根目录".into()))?;
    let target = parent.join(new_name.trim());
    if target.exists() && target != source {
        return Err(DeskError::AlreadyExists);
    }
    fs::rename(&source, &target)?;
    Ok(target)
}

pub fn copy_path(source: &Path, destination_folder: &Path) -> Result<PathBuf> {
    copy_path_as(source, destination_folder, None)
}

fn copy_path_as(
    source: &Path,
    destination_folder: &Path,
    name: Option<&std::ffi::OsStr>,
) -> Result<PathBuf> {
    if fs::symlink_metadata(source)?.file_type().is_symlink() {
        return Err(DeskError::InvalidPath("不复制符号链接或联接".into()));
    }
    let source = source.canonicalize()?;
    let destination_folder = destination_folder.canonicalize()?;
    if destination_folder.starts_with(&source) {
        return Err(DeskError::InvalidPath(
            "不能把文件夹复制到自身或子目录".into(),
        ));
    }
    let original_name = source
        .file_name()
        .ok_or_else(|| DeskError::InvalidPath("无法读取文件名".into()))?;
    let target = destination_folder.join(name.unwrap_or(original_name));
    if target.exists() {
        return Err(DeskError::AlreadyExists);
    }
    let temp = target.with_file_name(format!(".personaldesk-{}.tmp", uuid::Uuid::new_v4()));
    if let Err(error) = copy_no_follow(&source, &temp) {
        if temp.is_dir() {
            let _ = fs::remove_dir_all(&temp);
        } else {
            let _ = fs::remove_file(&temp);
        }
        return Err(error);
    }
    if let Err(error) = publish_copy(&temp, &target) {
        if temp.is_dir() {
            let _ = fs::remove_dir_all(&temp);
        } else {
            let _ = fs::remove_file(&temp);
        }
        return Err(error.into());
    }
    Ok(target)
}

fn publish_copy(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
        let source = source
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let destination = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // Omitting REPLACE_EXISTING makes publication fail if another file appeared.
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        if destination.exists() {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        fs::rename(source, destination)
    }
}

#[derive(Default)]
pub struct DropReport {
    pub copied: Vec<PathBuf>,
    pub failures: Vec<(PathBuf, String)>,
}

/// External drops copy to the chosen directory without removing any source.
pub fn copy_dropped(sources: &[PathBuf], destination: &Path) -> DropReport {
    let mut report = DropReport::default();
    let mut seen = BTreeSet::new();
    for source in sources {
        let key = source.canonicalize().unwrap_or_else(|_| source.clone());
        if !seen.insert(key) {
            continue;
        }
        let result = (|| {
            let base = source
                .file_name()
                .ok_or_else(|| DeskError::InvalidPath("拖入项目没有可用文件名".into()))?;
            for index in 0..10_000 {
                let name = if index == 0 {
                    base.to_os_string()
                } else {
                    let stem = if source.is_dir() {
                        base.to_string_lossy()
                    } else {
                        source.file_stem().unwrap_or(base).to_string_lossy()
                    };
                    let suffix = if index == 1 {
                        " (副本)".into()
                    } else {
                        format!(" (副本 {index})")
                    };
                    let extension = if source.is_file() {
                        source
                            .extension()
                            .map(|value| format!(".{}", value.to_string_lossy()))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    format!("{stem}{suffix}{extension}").into()
                };
                if destination.join(&name).exists() {
                    continue;
                }
                match copy_path_as(source, destination, Some(&name)) {
                    Err(DeskError::AlreadyExists) => continue,
                    Err(DeskError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists => {
                        continue;
                    }
                    other => return other,
                }
            }
            Err(DeskError::InvalidPath(
                "同名副本过多，请先整理目标文件夹".into(),
            ))
        })();
        match result {
            Ok(path) => report.copied.push(path),
            Err(error) => report.failures.push((source.clone(), error.to_string())),
        }
    }
    report
}

fn copy_no_follow(source: &Path, target: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        return Err(DeskError::InvalidPath("不复制符号链接".into()));
    }
    if metadata.is_file() {
        let mut from = fs::File::open(source)?;
        let mut to = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        if let Err(error) = io::copy(&mut from, &mut to) {
            let _ = fs::remove_file(target);
            return Err(error.into());
        }
        to.sync_all()?;
    } else if metadata.is_dir() {
        fs::create_dir(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            if entry.file_type()?.is_symlink() {
                return Err(DeskError::InvalidPath(
                    "目录包含符号链接或联接，已停止复制以避免遗漏文件".into(),
                ));
            }
            copy_no_follow(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else {
        return Err(DeskError::InvalidPath("不支持此文件类型".into()));
    }
    Ok(())
}

pub fn move_path(source: &Path, destination_folder: &Path) -> Result<PathBuf> {
    if fs::symlink_metadata(source)?.file_type().is_symlink() {
        return Err(DeskError::InvalidPath("不移动符号链接或联接".into()));
    }
    let source = source.canonicalize()?;
    let destination_folder = destination_folder.canonicalize()?;
    if destination_folder.starts_with(&source) {
        return Err(DeskError::InvalidPath(
            "不能把文件夹移到自身或子目录".into(),
        ));
    }
    let target = destination_folder.join(
        source
            .file_name()
            .ok_or_else(|| DeskError::InvalidPath("无法读取文件名".into()))?,
    );
    if target.exists() {
        return Err(DeskError::AlreadyExists);
    }
    if same_volume(&source, &destination_folder) {
        fs::rename(&source, &target)?;
        return Ok(target);
    }

    let copied = copy_path(&source, &destination_folder)?;
    if let Err(error) = verify_same_tree(&source, &copied) {
        let _ = remove_no_follow(&copied);
        return Err(error);
    }
    if let Err(error) = remove_no_follow(&source) {
        return Err(DeskError::InvalidPath(format!(
            "文件已复制并校验到 {}，但原位置无法移除：{error}",
            copied.display()
        )));
    }
    Ok(copied)
}

fn same_volume(source: &Path, destination: &Path) -> bool {
    #[cfg(windows)]
    {
        source.components().next() == destination.components().next()
    }
    #[cfg(not(windows))]
    {
        let _ = (source, destination);
        true
    }
}

fn verify_same_tree(source: &Path, copy: &Path) -> Result<()> {
    let source_meta = fs::symlink_metadata(source)?;
    let copy_meta = fs::symlink_metadata(copy)?;
    if source_meta.file_type().is_symlink() || copy_meta.file_type().is_symlink() {
        return Err(DeskError::InvalidPath("移动校验发现符号链接".into()));
    }
    if source_meta.is_file() && copy_meta.is_file() {
        if source_meta.len() != copy_meta.len() || file_digest(source)? != file_digest(copy)? {
            return Err(DeskError::InvalidPath("移动校验失败，原文件已保留".into()));
        }
        return Ok(());
    }
    if !source_meta.is_dir() || !copy_meta.is_dir() {
        return Err(DeskError::InvalidPath("移动校验发现文件类型变化".into()));
    }
    let names = |path: &Path| -> Result<BTreeSet<std::ffi::OsString>> {
        fs::read_dir(path)?
            .map(|entry| entry.map(|item| item.file_name()).map_err(Into::into))
            .collect()
    };
    if names(source)? != names(copy)? {
        return Err(DeskError::InvalidPath("移动校验发现目录内容变化".into()));
    }
    for name in names(source)? {
        verify_same_tree(&source.join(&name), &copy.join(name))?;
    }
    Ok(())
}

fn file_digest(path: &Path) -> Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = io::Read::read(&mut file, &mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

fn remove_no_follow(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to remove a symbolic link",
        ));
    }
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

pub fn scan_root(
    root: &Path,
    limit: usize,
    max_size: u64,
    max_total_text_bytes: u64,
) -> Result<Vec<IndexedFile>> {
    let root = root.canonicalize()?;
    let mut indexed = Vec::new();
    let mut indexed_text_bytes = 0_u64;
    for entry in walkdir::WalkDir::new(&root)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if indexed.len() >= limit {
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let is_text = matches!(ext.as_str(), "txt" | "md" | "markdown");
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let remaining_budget = max_total_text_bytes.saturating_sub(indexed_text_bytes);
        let body = if is_text && metadata.len() <= max_size && metadata.len() <= remaining_budget {
            let mut contents = String::new();
            match fs::File::open(path)
                .and_then(|file| file.take(remaining_budget).read_to_string(&mut contents))
            {
                Ok(_) => {
                    indexed_text_bytes = indexed_text_bytes.saturating_add(contents.len() as u64);
                    contents
                }
                Err(_) => String::new(),
            }
        } else {
            String::new()
        };
        indexed.push(IndexedFile {
            path: path.to_path_buf(),
            title: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            body,
        });
    }
    Ok(indexed)
}

#[derive(Debug)]
pub struct IndexedFile {
    pub path: PathBuf,
    pub title: String,
    pub body: String,
}

fn validate_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.chars().any(|c| {
            c.is_control() || ['<', '>', ';', ':', '"', '/', '\\', '|', '?', '*'].contains(&c)
        })
    {
        return Err(DeskError::InvalidPath(
            "名称为空或包含系统不允许的字符".into(),
        ));
    }
    if trimmed != trimmed.trim_end_matches([' ', '.']) {
        return Err(DeskError::InvalidPath("名称不能以空格或句点结尾".into()));
    }
    let stem = trimmed
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && ["COM", "LPT"].iter().any(|p| stem.starts_with(p))
            && stem
                .as_bytes()
                .get(3)
                .is_some_and(|b| (b'1'..=b'9').contains(b)))
    {
        return Err(DeskError::InvalidPath(
            "名称与 Windows 保留设备名冲突".into(),
        ));
    }
    Ok(())
}
