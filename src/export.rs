use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use serde::Serialize;
use zip::{ZipWriter, write::SimpleFileOptions};

use crate::{
    db::Database,
    error::{DeskError, Result},
    model::{DocumentRecord, Section},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Practice,
    Words,
    All,
    Credentials,
    Folders,
    Documents,
    Inventory,
    Websites,
    Contacts,
    Games,
    Tools,
    LegacyChats,
    Theme,
    Books,
}

impl Scope {
    pub fn for_section(section: Section) -> Self {
        match section {
            Section::Books => Self::Books,
            Section::Practice => Self::Practice,
            Section::Words => Self::Words,
            Section::Credentials => Self::Credentials,
            Section::Folders => Self::Folders,
            Section::Documents => Self::Documents,
            Section::Inventory => Self::Inventory,
            Section::Websites => Self::Websites,
            Section::Contacts => Self::Contacts,
            Section::Games => Self::Games,
            Section::Tools => Self::Tools,
            Section::Settings => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Books => "图书",
            Self::Practice => "刷题王",
            Self::Words => "每日单词",
            Self::All => "全部数据",
            Self::Credentials => "账号密码",
            Self::Folders => "快捷路径",
            Self::Documents => "文档",
            Self::Inventory => "物品清单",
            Self::Websites => "网站收藏",
            Self::Contacts => "联系人",
            Self::Games => "小游戏",
            Self::Tools => "小工具",
            Self::LegacyChats => "历史聊天",
            Self::Theme => "主题",
        }
    }

    fn includes(self, other: Self) -> bool {
        self == Self::All || self == other || (self == Self::Documents && other == Self::Books)
    }

    fn includes_tags(self, scope: crate::tags::Scope) -> bool {
        use crate::tags::Scope as T;
        self.includes(match scope {
            T::Books | T::BookPages => Self::Books,
            T::Credentials => Self::Credentials,
            T::Inventory => Self::Inventory,
            T::Shortcuts => Self::Folders,
            T::Documents => Self::Documents,
            T::Websites => Self::Websites,
            T::Contacts => Self::Contacts,
            T::Tools => Self::Tools,
        })
    }
}

pub struct Report {
    pub path: PathBuf,
    pub warnings: usize,
}

#[derive(Serialize)]
struct ExportedDocument {
    #[serde(flatten)]
    record: DocumentRecord,
    archive_path: Option<String>,
}

struct Package<'a> {
    zip: ZipWriter<&'a mut File>,
    members: HashSet<String>,
    exclusions: HashSet<PathBuf>,
    warnings: Vec<String>,
}

impl Package<'_> {
    fn json(&mut self, name: &str, data: &impl Serialize) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(data)?;
        self.bytes(name, &bytes)
    }

    fn bytes(&mut self, name: &str, data: &[u8]) -> Result<()> {
        if !self.members.insert(name.to_owned()) {
            return Ok(());
        }
        self.zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )?;
        self.zip.write_all(data)?;
        Ok(())
    }

    fn file(&mut self, source: &Path, name: &str) -> Result<bool> {
        if self.exclusions.contains(&canonical(source)) {
            return Ok(false);
        }
        if self.members.contains(name) {
            return Ok(true);
        }
        let mut file = match File::open(source) {
            Ok(file) if file.metadata()?.is_file() => file,
            _ => {
                self.warnings
                    .push(format!("未能读取：{}", source.display()));
                return Ok(false);
            }
        };
        let before = file.metadata()?;
        self.zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )?;
        // A read failure aborts the temporary archive; it never publishes a partial member.
        std::io::copy(&mut file, &mut self.zip)?;
        let after = file.metadata()?;
        if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
            self.warnings
                .push(format!("导出期间文件发生变化：{}", source.display()));
        }
        self.members.insert(name.to_owned());
        Ok(true)
    }

    fn tree(&mut self, directory: &Path, prefix: &str) -> Result<()> {
        if !directory.exists() {
            return Ok(());
        }
        for entry in walkdir::WalkDir::new(directory)
            .follow_links(false)
            .sort_by_file_name()
        {
            match entry {
                Ok(entry) if entry.file_type().is_file() => {
                    let relative = entry
                        .path()
                        .strip_prefix(directory)
                        .map_err(|_| DeskError::InvalidPath("文档相对路径不可用".into()))?;
                    self.file(entry.path(), &format!("{prefix}/{}", zip_path(relative)))?;
                }
                Ok(entry) if entry.file_type().is_symlink() => self
                    .warnings
                    .push(format!("未打包链接目标：{}", entry.path().display())),
                Err(error) => self.warnings.push(format!("读取目录失败：{error}")),
                _ => {}
            }
        }
        Ok(())
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
fn zip_path(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Export a database snapshot plus readable records and document originals.
/// Shortcut destinations are addresses only; no external folder is traversed.
pub fn write(data_dir: &Path, destination: &Path, scope: Scope) -> Result<Report> {
    if destination
        .extension()
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("zip"))
    {
        return Err(DeskError::InvalidPath(
            "数据导出需要保存为 .zip 文件".into(),
        ));
    }
    let parent = destination
        .parent()
        .filter(|path| path.is_dir())
        .ok_or_else(|| DeskError::InvalidPath("导出目录不可用".into()))?;
    let snapshot_dir = tempfile::tempdir()?;
    let source = Database::open(data_dir)
        .map_err(|error| DeskError::InvalidPath(format!("读取导出源失败：{error}")))?;
    source
        .snapshot(&snapshot_dir.path().join("desk.db"))
        .map_err(|error| DeskError::InvalidPath(format!("建立数据库快照失败：{error}")))?;
    drop(source);
    let snapshot = Database::open(snapshot_dir.path())
        .map_err(|error| DeskError::InvalidPath(format!("读取快照失败：{error}")))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let exclusions = HashSet::from([canonical(destination), canonical(temporary.path())]);
    let warnings = {
        let mut package = Package {
            zip: ZipWriter::new(temporary.as_file_mut()),
            members: HashSet::new(),
            exclusions,
            warnings: vec![],
        };
        let mut counts = BTreeMap::new();
        let favorites = snapshot
            .favorites()?
            .into_iter()
            .filter(|(key, _)| {
                scope == Scope::All
                    || match key.as_str() {
                        "files" | "shortcuts" => scope == Scope::Folders,
                        "credentials" => scope == Scope::Credentials,
                        "documents" => scope == Scope::Documents,
                        "inventory" => scope == Scope::Inventory,
                        "contacts" => scope == Scope::Contacts,
                        "websites" => scope == Scope::Websites,
                        "tools" => scope == Scope::Tools,
                        "books" | "book_pages" => scope.includes(Scope::Books),
                        _ => false,
                    }
            })
            .collect::<Vec<_>>();
        package.json("收藏.json", &favorites)?;
        if scope.includes(Scope::Theme) {
            let theme = crate::theme::ThemeSettings::load(&snapshot.settings()?);
            package.json("主题.json", &theme)?;
            if let Some(value) = &theme.background {
                if std::path::Path::new(value).is_relative() {
                    let path = crate::theme::background_path(data_dir, value)?;
                    package.file(&path, &format!("data/{value}"))?;
                } else {
                    package.warnings.push("主题图片尚未保存为本机副本。".into());
                }
            }
        }
        for (study_scope, kind, name) in [
            (Scope::Practice, "engineering", "刷题王"),
            (Scope::Words, "word", "每日单词"),
        ] {
            if scope.includes(study_scope) {
                let cards = snapshot.study_cards(kind)?;
                let attempts = snapshot.study_all_attempts(kind)?;
                counts.insert(name, attempts.len());
                package.json(
                    &format!("{name}.json"),
                    &crate::study::Bundle {
                        version: 1,
                        cards,
                        attempts,
                    },
                )?;
                package.json(
                    &format!("{name}待完成.json"),
                    &snapshot.study_pending(kind)?,
                )?;
                package.json("练习规则.json", &snapshot.study_rules())?;
            }
        }
        if scope.includes(Scope::Tools) {
            let records = snapshot.app_tools()?;
            counts.insert("小工具", records.len());
            package.json("小工具.json", &records)?;
        }
        if scope.includes(Scope::LegacyChats) {
            let settings = snapshot
                .settings()?
                .into_iter()
                .filter(|(key, _)| key.starts_with("codex."))
                .collect::<BTreeMap<_, _>>();
            package.json("Codex设置.json", &settings)?;
            package.tree(&data_dir.join("codex"), "data/codex")?;
        }
        if scope.includes(Scope::Games) {
            let settings = snapshot.settings()?;
            let best = settings
                .get("games.snake.best")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            package.json(
                "小游戏.json",
                &serde_json::json!({"snake":{"mode":"endless","best_score":best}}),
            )?;
            counts.insert("小游戏", 1);
        }
        if scope.includes(Scope::Contacts) {
            let records = snapshot.contacts()?;
            let mut exported = serde_json::to_value(&records)?;
            for (index, contact) in records.iter().enumerate() {
                if !contact.photo.is_empty() {
                    let extension = image::guess_format(&contact.photo)
                        .ok()
                        .and_then(|format| format.extensions_str().first().copied())
                        .unwrap_or("bin");
                    let member = format!("contact_photos/{index:05}.{extension}");
                    package.bytes(&member, &contact.photo)?;
                    exported[index]["photo_archive_path"] = serde_json::Value::String(member);
                }
                for (chat_index, chat) in contact.chats.iter().enumerate() {
                    let member =
                        format!("contact_chats/{index:05}/{chat_index:05}.{}", chat.format);
                    package.bytes(&member, &chat.original)?;
                    exported[index]["chats"][chat_index]["archive_path"] =
                        serde_json::Value::String(member);
                }
            }
            counts.insert("联系人", records.len());
            package.json("联系人.json", &exported)?;
        }
        if scope == Scope::All {
            let journal = crate::undo::Journal::open(data_dir);
            let entries = journal.sources();
            if !journal.error.is_empty() {
                package.warnings.push(journal.error.clone());
            }
            if data_dir.join("undo-journal.json").is_file() {
                package.file(
                    &data_dir.join("undo-journal.json"),
                    "data/undo-journal.json",
                )?;
            }
            for (index, (original, quarantine)) in entries.iter().enumerate() {
                if crate::files::shell_path(&canonical(original))
                    .starts_with(crate::files::shell_path(&canonical(data_dir)))
                    && quarantine.is_file()
                {
                    package.file(
                        quarantine,
                        &format!("deleted_documents/{index:05}.original"),
                    )?;
                }
            }
        }
        if scope.includes(Scope::Books) {
            let books = snapshot.books()?;
            let mut pages = vec![];
            for book in &books {
                for page in snapshot.book_pages(&book.id)? {
                    let member = format!("books/{}/{}.md", book.id, page.id);
                    package.bytes(&member, page.body.as_bytes())?;
                    pages.push(page);
                }
            }
            let references=books.iter().map(|book|Ok(serde_json::json!({"book_id":book.id,"documents":snapshot.book_documents(&book.id)?}))).collect::<Result<Vec<_>>>()?;
            package.json("图书文档.json", &references)?;
            package.json("图书.json", &books)?;
            package.json("图书页面.json", &pages)?;
            counts.insert("图书", books.len());
            let writing = snapshot.writing_pages()?;
            for page in &writing {
                package.bytes(
                    &format!("writing/{}.md", page.id),
                    format!("# {}\n\n{}", page.title, page.body).as_bytes(),
                )?;
            }
            package.json("写作文档.json", &writing)?;
        }
        if crate::tags::Scope::ALL
            .into_iter()
            .any(|tag_scope| scope.includes_tags(tag_scope))
        {
            let catalog = snapshot
                .tag_catalog()?
                .into_iter()
                .filter(|tag| scope.includes_tags(tag.scope))
                .collect::<Vec<_>>();
            package.json("自定义标签.json", &catalog)?;
            let mut assignments = Vec::new();
            for tag_scope in crate::tags::Scope::ALL {
                if scope.includes_tags(tag_scope) {
                    assignments.extend(snapshot.tagged_records(tag_scope)?);
                }
            }
            package.json("条目标签.json", &assignments)?;
        }

        if scope.includes(Scope::Credentials) {
            let records = snapshot
                .credentials()
                .map_err(|error| DeskError::InvalidPath(format!("读取账号记录失败：{error}")))?;
            counts.insert("账号密码", records.len());
            package.json("账号密码.json", &records)?;
        }
        if scope.includes(Scope::Websites) {
            let records = snapshot
                .websites()
                .map_err(|error| DeskError::InvalidPath(format!("读取网站记录失败：{error}")))?;
            counts.insert("网站收藏", records.len());
            package.json("网站收藏.json", &records)?;
        }
        if scope.includes(Scope::Inventory) {
            let records = snapshot
                .inventory_items()
                .map_err(|error| DeskError::InvalidPath(format!("读取物品记录失败：{error}")))?;
            counts.insert("物品清单", records.len());
            package.json("物品清单.json", &records)?;
        }
        if scope.includes(Scope::Folders) {
            let records = snapshot.shortcuts().map_err(|error| {
                DeskError::InvalidPath(format!("读取快捷路径记录失败：{error}"))
            })?;
            counts.insert("快捷路径", records.len());
            package.json("快捷路径.json", &records)?;
        }
        if scope.includes(Scope::Documents) {
            let root = crate::files::shell_path(&canonical(&data_dir.join("documents")));
            let mut records = vec![];
            for record in snapshot
                .document_records()
                .map_err(|error| DeskError::InvalidPath(format!("读取文档记录失败：{error}")))?
                .into_iter()
            {
                let path = crate::files::shell_path(&canonical(&record.path));
                let (archived, name) = if let Ok(relative) = path.strip_prefix(&root) {
                    let name = format!("data/documents/{}", zip_path(relative));
                    (package.file(&record.path, &name)?, name)
                } else {
                    (false, String::new())
                };
                records.push(ExportedDocument {
                    record,
                    archive_path: archived.then_some(name),
                });
            }
            counts.insert("文档", records.len());
            package.json("文档清单.json", &records)?;
            package.tree(&data_dir.join("documents"), "data/documents")?;
            package.tree(&data_dir.join("recovery"), "data/recovery")?;
        }
        if scope == Scope::All {
            package.json(
                "设置.json",
                &snapshot.settings().map_err(|error| {
                    DeskError::InvalidPath(format!("读取设置记录失败：{error}"))
                })?,
            )?;
            drop(snapshot);
            package.file(&snapshot_dir.path().join("desk.db"), "data/desk.db")?;
            let old = data_dir.join("vault.db");
            if old.exists() {
                let old_snapshot = snapshot_dir.path().join("vault.db");
                let copied = rusqlite::Connection::open_with_flags(
                    &old,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .map_err(DeskError::from)
                .and_then(|connection| Database::snapshot_connection(&connection, &old_snapshot));
                if copied.is_ok() {
                    package.file(&old_snapshot, "legacy/vault.db")?;
                } else {
                    package
                        .warnings
                        .push("旧版账号库不能生成快照，已按原文件保留在 legacy 目录。".into());
                    package.file(&old, "legacy/vault.db")?;
                    for suffix in ["-wal", "-shm"] {
                        let path = data_dir.join(format!("vault.db{suffix}"));
                        if path.exists() {
                            package.file(&path, &format!("legacy/vault.db{suffix}"))?;
                        }
                    }
                }
            }
        }
        package.bytes("导出说明.txt", "个人办公台数据导出\nJSON 文件为 UTF-8，账号密码和附加信息均按原文导出。\n文档清单中的 archive_path 对应压缩包内的正文文件，恢复草稿位于 data/recovery。\n联系人.json 包含联系人资料、聊天笔记及聊天文本，聊天记录的 archive_path 对应 contact_chats 内的原文件（含 Excel）。\n快捷路径只导出名称和目标地址，不打包其指向的外部文件夹。\n全部导出另含 data/desk.db 一致性快照（含索引及配置）；旧版账号库如存在则保留在 legacy。\n未能读取或导出期间变化的文件记录在导出清单.json。\n".as_bytes())?;
        let warnings = package.warnings.len();
        package.json("导出清单.json", &serde_json::json!({ "format": "PersonalDeskExport", "version": 1, "scope": scope.label(), "exported_at": chrono::Utc::now().to_rfc3339(), "counts": counts, "warnings": package.warnings, "shortcut_targets_included": false, "external_document_targets_included": false }))?;
        let Package { zip, .. } = package;
        zip.finish()?.sync_all()?;
        warnings
    };
    temporary
        .persist(destination)
        .map_err(|error| DeskError::Io(error.error))?;
    Ok(Report {
        path: destination.to_path_buf(),
        warnings,
    })
}
