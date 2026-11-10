use crate::error::{DeskError, Result};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
};

struct RemoteAssetCache {
    assets: BTreeMap<String, Vec<u8>>,
    dirty: bool,
}

static REMOTE_ASSETS: OnceLock<Mutex<BTreeMap<PathBuf, RemoteAssetCache>>> = OnceLock::new();

fn remote_assets() -> &'static Mutex<BTreeMap<PathBuf, RemoteAssetCache>> {
    REMOTE_ASSETS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn data_key(data: &Path) -> PathBuf {
    data.canonicalize().unwrap_or_else(|_| data.to_path_buf())
}

pub fn remote_assets_ready(data: &Path) -> bool {
    #[cfg(debug_assertions)]
    {
        let _ = data;
        true
    }
    #[cfg(not(debug_assertions))]
    {
        remote_assets()
            .lock()
            .ok()
            .and_then(|cache| cache.get(&data_key(data)).map(|_| ()))
            .is_some()
    }
}

#[cfg(not(debug_assertions))]
pub(crate) fn enable_remote_cache(data: &Path, assets: BTreeMap<String, Vec<u8>>) -> Result<()> {
    for path in assets.keys() {
        validate_remote_path(path)?;
    }
    remote_assets()
        .lock()
        .map_err(|_| DeskError::InvalidPath("远端图片缓存暂不可用".into()))?
        .insert(
            data_key(data),
            RemoteAssetCache {
                assets,
                dirty: false,
            },
        );
    Ok(())
}

#[cfg(not(debug_assertions))]
pub(crate) fn remote_assets_dirty(data: &Path) -> bool {
    remote_assets()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&data_key(data)).map(|cache| cache.dirty))
        .unwrap_or(false)
}

#[cfg(not(debug_assertions))]
pub(crate) fn remote_assets_snapshot(data: &Path) -> Option<BTreeMap<String, Vec<u8>>> {
    remote_assets()
        .lock()
        .ok()?
        .get(&data_key(data))
        .map(|cache| cache.assets.clone())
}

#[cfg(not(debug_assertions))]
pub(crate) fn mark_remote_assets_synced(data: &Path) {
    if let Ok(mut cache) = remote_assets().lock()
        && let Some(cache) = cache.get_mut(&data_key(data))
    {
        cache.dirty = false;
    }
}

fn cache_remote_asset(data: &Path, path: String, bytes: &[u8]) -> Result<Option<String>> {
    let mut cache = remote_assets()
        .lock()
        .map_err(|_| DeskError::InvalidPath("远端图片缓存暂不可用".into()))?;
    let Some(cache) = cache.get_mut(&data_key(data)) else {
        return Ok(None);
    };
    validate_remote_path(&path)?;
    cache.assets.insert(path.clone(), bytes.to_vec());
    cache.dirty = true;
    Ok(Some(path))
}

fn cached_remote_asset(data: &Path, path: &str) -> Result<Option<Option<Vec<u8>>>> {
    let cache = remote_assets()
        .lock()
        .map_err(|_| DeskError::InvalidPath("远端图片缓存暂不可用".into()))?;
    Ok(cache
        .get(&data_key(data))
        .map(|cache| cache.assets.get(path).cloned()))
}

fn validate_remote_path(path: &str) -> Result<()> {
    if !(path.starts_with("writing-assets/") || path.starts_with("data/picture/"))
        || path.contains(':')
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(DeskError::InvalidPath("文档图片路径无效".into()));
    }
    Ok(())
}

fn assets_directory(data: &Path) -> Result<PathBuf> {
    let root = data.canonicalize()?;
    let dir = data.join("writing-assets");
    fs::create_dir_all(&dir)?;
    let canonical = dir.canonicalize()?;
    if !canonical.starts_with(&root) {
        return Err(DeskError::InvalidPath("图片目录不在数据目录内".into()));
    }
    Ok(canonical)
}

fn canonical_document_id(document_id: &str) -> Result<String> {
    uuid::Uuid::parse_str(document_id)
        .map(|id| id.to_string())
        .map_err(|_| DeskError::InvalidPath("文档编号无效".into()))
}

fn picture_root(data: &Path, create: bool) -> Result<PathBuf> {
    let root = data.canonicalize()?;
    let picture = data.join("data").join("picture");
    if create {
        fs::create_dir_all(&picture)?;
    }
    let picture = picture.canonicalize()?;
    if !picture.starts_with(&root) {
        return Err(DeskError::InvalidPath("文档图片目录不在数据目录内".into()));
    }
    Ok(picture)
}

fn picture_document_directory(data: &Path, document_id: &str) -> Result<(String, PathBuf)> {
    let id = canonical_document_id(document_id)?;
    let picture = picture_root(data, true)?;
    let directory = picture.join(&id);
    fs::create_dir_all(&directory)?;
    let directory = directory.canonicalize()?;
    if !directory.starts_with(&picture) {
        return Err(DeskError::InvalidPath("文档图片目录无效".into()));
    }
    Ok((id, directory))
}

fn format(bytes: &[u8]) -> Result<(&'static str, &'static str)> {
    match image::guess_format(bytes).map_err(|_| DeskError::UnsupportedFormat)? {
        image::ImageFormat::Png => Ok(("png", "image/png")),
        image::ImageFormat::Jpeg => Ok(("jpg", "image/jpeg")),
        image::ImageFormat::WebP => Ok(("webp", "image/webp")),
        image::ImageFormat::Bmp => Ok(("bmp", "image/bmp")),
        _ => Err(DeskError::UnsupportedFormat),
    }
}

fn validate_image(bytes: &[u8]) -> Result<&'static str> {
    if bytes.len() > crate::contacts::MAX_FILE_BYTES {
        return Err(DeskError::InvalidPath("图片不能超过 8 MiB".into()));
    }
    let (ext, _) = format(bytes)?;
    crate::theme::decode(bytes)?;
    Ok(ext)
}

fn store(bytes: &[u8], directory: &Path, ext: &str) -> Result<String> {
    let name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(&name))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(name)
}

/// Store an independent copy in the existing writing-assets export directory.
pub fn import(data: &Path, source: &Path) -> Result<String> {
    let bytes = crate::contacts::import_photo(source)?;
    import_bytes(data, &bytes)
}

/// Save validated clipboard image bytes as a managed legacy writing asset.
pub fn import_bytes(data: &Path, bytes: &[u8]) -> Result<String> {
    let ext = validate_image(bytes)?;
    let name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let reference = format!("writing-assets/{name}");
    if let Some(path) = cache_remote_asset(data, reference, bytes)? {
        return Ok(path);
    }
    let directory = assets_directory(data)?;
    let name = store(bytes, &directory, ext)?;
    Ok(format!("writing-assets/{name}"))
}

/// Store an inserted image under data/picture/<document-id>/.
pub fn import_document(data: &Path, document_id: &str, source: &Path) -> Result<String> {
    let bytes = crate::contacts::import_photo(source)?;
    import_document_bytes(data, document_id, &bytes)
}

/// Save validated clipboard image bytes in the document's own picture directory.
pub fn import_document_bytes(data: &Path, document_id: &str, bytes: &[u8]) -> Result<String> {
    let ext = validate_image(bytes)?;
    let id = canonical_document_id(document_id)?;
    let name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let reference = format!("data/picture/{id}/{name}");
    if let Some(path) = cache_remote_asset(data, reference, bytes)? {
        return Ok(path);
    }
    let (id, directory) = picture_document_directory(data, document_id)?;
    let name = store(bytes, &directory, ext)?;
    Ok(format!("data/picture/{id}/{name}"))
}

fn decode_percent_encoding(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let Some(pair) = bytes.get(index + 1..index + 3) else {
                return Err(DeskError::InvalidPath("文档图片路径编码无效".into()));
            };
            let hex = std::str::from_utf8(pair)
                .map_err(|_| DeskError::InvalidPath("文档图片路径编码无效".into()))?;
            decoded.push(
                u8::from_str_radix(hex, 16)
                    .map_err(|_| DeskError::InvalidPath("文档图片路径编码无效".into()))?,
            );
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| DeskError::InvalidPath("文档图片路径编码无效".into()))
}

/// Only serve managed assets, disallow path traversal and links escaping the asset directory.
pub fn read(data: &Path, reference: &str) -> Result<(Vec<u8>, &'static str)> {
    let decoded = decode_percent_encoding(reference)?;
    if let Some(asset) = cached_remote_asset(data, &decoded)? {
        let bytes = asset.ok_or_else(|| DeskError::InvalidPath("文档图片文件无效".into()))?;
        let (_, mime) = format(&bytes)?;
        crate::theme::decode(&bytes)?;
        return Ok((bytes, mime));
    }
    let path = Path::new(&decoded);
    let mut parts = path.components();
    let first = parts.next();
    let file = if first == Some(Component::Normal("writing-assets".as_ref())) {
        let rest = parts.collect::<Vec<_>>();
        if rest.is_empty() || !rest.iter().all(|part| matches!(part, Component::Normal(_))) {
            return Err(DeskError::InvalidPath("文档图片路径无效".into()));
        }
        let directory = assets_directory(data)?;
        let file = data.join(path).canonicalize()?;
        if !file.starts_with(directory) {
            return Err(DeskError::InvalidPath("文档图片不在资源目录内".into()));
        }
        file
    } else if first == Some(Component::Normal("data".as_ref())) {
        if parts.next() != Some(Component::Normal("picture".as_ref())) {
            return Err(DeskError::InvalidPath("文档图片路径无效".into()));
        }
        let Some(Component::Normal(id)) = parts.next() else {
            return Err(DeskError::InvalidPath("文档图片路径无效".into()));
        };
        let Some(Component::Normal(_name)) = parts.next() else {
            return Err(DeskError::InvalidPath("文档图片路径无效".into()));
        };
        if parts.next().is_some() {
            return Err(DeskError::InvalidPath("文档图片路径无效".into()));
        }
        let canonical_id = canonical_document_id(id.to_str().unwrap_or_default())?;
        if canonical_id.as_str() != id.to_string_lossy().as_ref() {
            return Err(DeskError::InvalidPath("文档编号无效".into()));
        }
        let picture = picture_root(data, false)?;
        let directory = picture.join(canonical_id).canonicalize()?;
        if !directory.starts_with(&picture) {
            return Err(DeskError::InvalidPath("文档图片目录无效".into()));
        }
        let file = data.join(path).canonicalize()?;
        if !file.starts_with(directory) {
            return Err(DeskError::InvalidPath("文档图片不在所属目录内".into()));
        }
        file
    } else {
        return Err(DeskError::InvalidPath("文档图片路径无效".into()));
    };
    if !file.is_file() {
        return Err(DeskError::InvalidPath("文档图片文件无效".into()));
    }
    let bytes = crate::contacts::import_photo(&file)?;
    let (_, mime) = format(&bytes)?;
    Ok((bytes, mime))
}
/// Export one Markdown document with a unique sibling folder for any managed images.
/// Unmanaged links remain unchanged; failures leave the original destination untouched.
pub fn export_markdown(data: &Path, page: &crate::library::Page, destination: &Path) -> Result<()> {
    use std::collections::HashMap;
    let parent = destination
        .parent()
        .filter(|p| p.is_dir())
        .ok_or_else(|| DeskError::InvalidPath("导出目录不可用".into()))?;
    let mut output = String::new();
    let mut rest = page.body.as_str();
    let mut references: HashMap<String, String> = HashMap::new();
    let asset_name = format!("personal-desk-images-{}", uuid::Uuid::new_v4());
    let temp_assets = tempfile::Builder::new()
        .prefix(".personaldesk-images-")
        .tempdir_in(parent)?;
    while let Some((start, marker_len)) =
        [("](writing-assets/", 2usize), ("](data/picture/", 2usize)]
            .into_iter()
            .filter_map(|(marker, length)| rest.find(marker).map(|position| (position, length)))
            .min_by_key(|(position, _)| *position)
    {
        let value_start = start + marker_len;
        let Some(end_offset) = rest[value_start..].find(')') else {
            break;
        };
        let end = value_start + end_offset;
        let reference = &rest[value_start..end];
        let replacement = if let Some(name) = references.get(reference) {
            name.clone()
        } else {
            let (bytes, mime) = read(data, reference)?;
            let ext = match mime {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                "image/webp" => "webp",
                "image/bmp" => "bmp",
                _ => return Err(DeskError::UnsupportedFormat),
            };
            let name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
            let mut out = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(temp_assets.path().join(&name))?;
            out.write_all(&bytes)?;
            out.sync_all()?;
            let relative = format!("{asset_name}/{name}");
            references.insert(reference.to_string(), relative.clone());
            relative
        };
        output.push_str(&rest[..value_start]);
        output.push_str(&replacement);
        output.push(')');
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    write!(file, "# {}\n\n{}", page.title, output)?;
    file.as_file().sync_all()?;
    let final_assets = parent.join(&asset_name);
    let mut moved_assets = false;
    if !references.is_empty() {
        fs::rename(temp_assets.path(), &final_assets)?;
        moved_assets = true;
    }
    if let Err(error) = file.persist(destination) {
        if moved_assets {
            for value in references.values() {
                if let Some(name) = Path::new(value).file_name() {
                    let _ = fs::remove_file(final_assets.join(name));
                }
            }
            let _ = fs::remove_dir(&final_assets);
        }
        return Err(error.error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    fn tiny_png() -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba([20, 40, 60, 255])));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn reads_legacy_asset_paths_with_percent_encoded_spaces() {
        let data = tempfile::tempdir().unwrap();
        let asset = data
            .path()
            .join("writing-assets/obsidian-test/733aa4970b160f72");
        fs::create_dir_all(&asset).unwrap();
        let expected = tiny_png();
        fs::write(asset.join("Pasted image 20261016160000.png"), &expected).unwrap();
        let (actual, mime) = read(
            data.path(),
            "writing-assets/obsidian-test/733aa4970b160f72/Pasted%20image%2020261016160000.png",
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(mime, "image/png");
    }

    #[test]
    fn rejects_percent_encoded_path_traversal() {
        let data = tempfile::tempdir().unwrap();
        assert!(read(data.path(), "writing-assets/%2e%2e/outside.png").is_err());
        assert!(read(data.path(), "writing-assets/bad%XX.png").is_err());
    }
}
