//! Publish one local Markdown document and its managed images to MongoDB.
//!
//! 发布顺序（任何一步失败都不会让已发布正文指向缺失图片）：
//! 1. 先在本地校验标题/正文/编号/图片引用与大小上限，并读出图片字节；
//! 2. 分批 `insert_many` 写入本批次图片（带 `batch_id`），旧正文此时仍指向旧图片；
//! 3. `replace_one(upsert)` 更新正文并记录本批次号，此时新图片已经就绪；
//! 4. 最后删除同一文档的旧批次图片；失败只留下可重试清理的旧资产，不影响正文。
//!
//! 不依赖 MongoDB 服务器的纯逻辑（入参校验、Markdown 图片引用解析、分批）都有单元测试。

use std::path::Path;

use mongodb::bson::{Binary, Document, doc, spec::BinarySubtype};

use crate::{
    error::{DeskError, Result},
    library::Page,
    mongo_config::MongoConfig,
    writing_images,
};

/// 正文文档图片附件所在的集合（图片按 `document_id` 归属）。
pub const DEFAULT_ASSET_COLLECTION: &str = "document_assets";
/// 标题上限（字节）：留足索引与跨语言余量。
const MAX_TITLE_BYTES: usize = 1024;
/// 正文上限：MongoDB 单文档 16 MiB，正文留 4 MiB 给其它字段与索引。
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
/// 单次发布最多引用的图片数量。
const MAX_ASSET_COUNT: usize = 128;
/// 单张图片读取上限（与 `writing_images` 的 8 MiB 限制一致，兜底校验）。
const MAX_ASSET_BYTES: usize = crate::contacts::MAX_FILE_BYTES;
/// 单批 `insert_many` 的图片总字节与条数上限，避免超过 MongoDB 协议/文档大小限制。
const ASSET_BATCH_BYTES: usize = 8 * 1024 * 1024;
const ASSET_BATCH_COUNT: usize = 20;

/// 已读取并校验过的受管图片。
#[derive(Debug)]
struct ManagedAsset {
    path: String,
    content_type: String,
    bytes: Vec<u8>,
}

/// 把一篇本地文档及其受管图片发布到 MongoDB 文档库，返回 `库.集合.文档编号`。
pub async fn publish(data_dir: &Path, page: &Page) -> Result<String> {
    // 1. 纯本地校验与读取；失败时还没有任何网络写操作。
    let assets = prepare_assets(data_dir, page)?;
    let config = MongoConfig::load()?.ok_or_else(|| {
        publish_error("尚未配置 MongoDB 文档库（请先在设置里填写服务器、用户名和密码）")
    })?;
    let client = config.client().await?;
    let database = client.database(config.database());
    let documents = database.collection::<Document>(config.collection());
    let asset_collection = database.collection::<Document>(DEFAULT_ASSET_COLLECTION);

    let batch_id = uuid::Uuid::new_v4().to_string();
    let now = mongodb::bson::DateTime::now();
    let asset_paths = assets
        .iter()
        .map(|asset| asset.path.clone())
        .collect::<Vec<_>>();

    // 2. 先写新批次图片（分批插入）；失败时旧正文和旧图片都还在。
    let batches = asset_batches(&assets);
    for (index, batch) in batches.iter().enumerate() {
        let batch_documents = batch
            .iter()
            .map(|asset| build_asset_document(page, &batch_id, asset, now))
            .collect::<Vec<_>>();
        if let Err(error) = asset_collection.insert_many(batch_documents).await {
            return Err(publish_error(format!(
                "写入第 {}/{} 批图片失败（共 {} 张；已发布正文未改动）：{error}",
                index + 1,
                batches.len(),
                assets.len()
            )));
        }
    }

    // 3. 正文总是最后写入：此时正文引用的图片已经全部存在。
    let document = build_document(page, asset_paths, &batch_id, now);
    documents
        .replace_one(doc! { "_id": &page.id }, document)
        .upsert(true)
        .await
        .map_err(|error| {
            publish_error(format!(
                "写入正文文档 {} 失败（新图片已写入，下次发布会覆盖）：{error}",
                page.id
            ))
        })?;

    // 4. 最后清理旧批次图片；失败只留下需重试清理的旧资产。
    let cleanup = if batches.is_empty() {
        asset_collection
            .delete_many(doc! { "document_id": &page.id })
            .await
    } else {
        asset_collection
            .delete_many(doc! {
                "document_id": &page.id,
                "batch_id": doc! { "$ne": &batch_id },
            })
            .await
    };
    if let Err(error) = cleanup {
        return Err(publish_error(format!(
            "正文文档 {} 已发布，但旧图片清理失败（下次发布会重试清理）：{error}",
            page.id
        )));
    }

    Ok(format!(
        "{}.{}.{}",
        config.database(),
        config.collection(),
        page.id
    ))
}

fn publish_error(message: impl Into<String>) -> DeskError {
    DeskError::InvalidInput(format!("MongoDB 文档发布：{}", message.into()))
}

/// 校验发布入参并读出正文引用的受管图片；任何失败都不产生远端写入。
fn prepare_assets(data_dir: &Path, page: &Page) -> Result<Vec<ManagedAsset>> {
    validate_page(page)?;
    let references = parse_asset_references(&page.body);
    if references.len() > MAX_ASSET_COUNT {
        return Err(publish_error(format!(
            "正文引用的图片过多（{} 张，上限 {MAX_ASSET_COUNT} 张）",
            references.len()
        )));
    }
    let mut assets = Vec::with_capacity(references.len());
    for reference in references {
        let (bytes, mime) = writing_images::read(data_dir, &reference)
            .map_err(|error| publish_error(format!("图片 {reference} 读取失败：{error}")))?;
        if bytes.is_empty() {
            return Err(publish_error(format!("图片 {reference} 内容为空")));
        }
        if bytes.len() > MAX_ASSET_BYTES {
            return Err(publish_error(format!(
                "图片 {reference} 超过 {} MiB 上限",
                MAX_ASSET_BYTES / (1024 * 1024)
            )));
        }
        assets.push(ManagedAsset {
            path: reference,
            content_type: mime.to_owned(),
            bytes,
        });
    }
    Ok(assets)
}

fn validate_page(page: &Page) -> Result<()> {
    if page.title.trim().is_empty() {
        return Err(publish_error("标题不能为空"));
    }
    if page.title.len() > MAX_TITLE_BYTES {
        return Err(publish_error(format!(
            "标题超过 {MAX_TITLE_BYTES} 字节上限，请先精简"
        )));
    }
    if page.body.trim().is_empty() {
        return Err(publish_error("正文不能为空，未发布的空文档不会上传"));
    }
    if page.body.len() > MAX_BODY_BYTES {
        return Err(publish_error(format!(
            "正文超过 {} MiB 上限，请拆分文档后再发布",
            MAX_BODY_BYTES / (1024 * 1024)
        )));
    }
    if uuid::Uuid::parse_str(&page.id).is_err() {
        return Err(publish_error(format!(
            "文档编号 {} 不是有效的 UUID",
            page.id
        )));
    }
    Ok(())
}

/// 从 Markdown 正文中解析受管本地图片引用，按出现顺序去重。
///
/// 只识别 `](writing-assets/...` 与 `](data/picture/...` 两种受管路径；
/// 外部链接、普通文本里的路径都不会被当成附件上传。
fn parse_asset_references(body: &str) -> Vec<String> {
    let mut references = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut rest = body;
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
        let reference = rest[value_start..end].trim().to_owned();
        if !reference.is_empty() && seen.insert(reference.clone()) {
            references.push(reference);
        }
        rest = &rest[end + 1..];
    }
    references
}

/// 按条数与总字节把图片拆成多批，保证单次 `insert_many` 的载荷不超限。
fn asset_batches(assets: &[ManagedAsset]) -> Vec<&[ManagedAsset]> {
    let mut batches = Vec::new();
    let mut start = 0;
    let mut bytes = 0usize;
    for (index, asset) in assets.iter().enumerate() {
        let size = asset.bytes.len() + asset.path.len() + asset.content_type.len();
        let count_full = index - start >= ASSET_BATCH_COUNT;
        let bytes_full = index > start && bytes + size > ASSET_BATCH_BYTES;
        if count_full || bytes_full {
            batches.push(&assets[start..index]);
            start = index;
            bytes = 0;
        }
        bytes += size;
    }
    if start < assets.len() {
        batches.push(&assets[start..]);
    }
    batches
}

fn build_asset_document(
    page: &Page,
    batch_id: &str,
    asset: &ManagedAsset,
    now: mongodb::bson::DateTime,
) -> Document {
    doc! {
        "document_id": &page.id,
        "batch_id": batch_id,
        "path": &asset.path,
        "content_type": &asset.content_type,
        "data": Binary { subtype: BinarySubtype::Generic, bytes: asset.bytes.clone() },
        "updated_at": now,
    }
}

fn build_document(
    page: &Page,
    asset_paths: Vec<String>,
    batch_id: &str,
    now: mongodb::bson::DateTime,
) -> Document {
    doc! {
        "_id": &page.id,
        "title": &page.title,
        "body": &page.body,
        "tags": &page.tags,
        "book_id": &page.book_id,
        "revision": page.revision,
        "source": "personal-desk",
        "asset_paths": asset_paths,
        "asset_batch": batch_id,
        "published_at": now,
        "updated_at": now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Page {
        Page {
            id: uuid::Uuid::new_v4().to_string(),
            book_id: uuid::Uuid::new_v4().to_string(),
            title: "测试文档".into(),
            body: "正文".into(),
            tags: vec!["写".into()],
            revision: 3,
            updated_at: "2026-01-01".into(),
        }
    }

    fn asset(path: &str, size: usize) -> ManagedAsset {
        ManagedAsset {
            path: path.to_owned(),
            content_type: "image/png".into(),
            bytes: vec![0u8; size],
        }
    }

    #[test]
    fn rejects_empty_or_oversized_publish_input() {
        let mut value = page();
        value.title = "   ".into();
        assert!(validate_page(&value).is_err());

        let mut value = page();
        value.title = "x".repeat(MAX_TITLE_BYTES + 1);
        assert!(validate_page(&value).is_err());

        let mut value = page();
        value.body = " \n ".into();
        assert!(validate_page(&value).is_err());

        let mut value = page();
        value.body = "x".repeat(MAX_BODY_BYTES + 1);
        assert!(validate_page(&value).is_err());

        let mut value = page();
        value.id = "not-a-uuid".into();
        assert!(validate_page(&value).is_err());

        assert!(validate_page(&page()).is_ok());
    }

    #[test]
    fn parses_managed_image_references_in_order_without_duplicates() {
        let body = concat!(
            "开头 ![图一](writing-assets/one.png)\n",
            "外链 ![外](https://example.com/a.png) 不算\n",
            "再次 ![图一](writing-assets/one.png)\n",
            "文档图 ![图二](data/picture/00000000-0000-4000-8000-000000000000/two.png)\n",
            "纯文本 writing-assets/not-a-link.png 不算\n",
        );
        assert_eq!(
            parse_asset_references(body),
            vec![
                "writing-assets/one.png".to_owned(),
                "data/picture/00000000-0000-4000-8000-000000000000/two.png".to_owned(),
            ]
        );
        assert!(parse_asset_references("没有图片的正文").is_empty());
    }

    #[test]
    fn splits_asset_batches_by_count_and_size() {
        let many = (0..45)
            .map(|index| asset(&format!("writing-assets/{index}.png"), 1))
            .collect::<Vec<_>>();
        let batches = asset_batches(&many);
        assert_eq!(
            batches.iter().map(|batch| batch.len()).collect::<Vec<_>>(),
            vec![20, 20, 5]
        );

        // 单张接近上限时仍允许两张同批（留出路径与类型开销）；再多就分批。
        let big = (0..4)
            .map(|index| {
                asset(
                    &format!("writing-assets/{index}.png"),
                    ASSET_BATCH_BYTES / 2 - 64,
                )
            })
            .collect::<Vec<_>>();
        let batches = asset_batches(&big);
        assert_eq!(
            batches.iter().map(|batch| batch.len()).collect::<Vec<_>>(),
            vec![2, 2]
        );

        assert!(asset_batches(&[]).is_empty());
    }

    #[test]
    fn prepare_assets_fails_before_network_when_image_missing() {
        let directory = tempfile::tempdir().unwrap();
        let mut value = page();
        value.body = "![缺失](writing-assets/missing.png)".into();
        let error = prepare_assets(directory.path(), &value).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("writing-assets/missing.png"), "{message}");
        assert!(message.contains("读取失败"), "{message}");
    }

    #[test]
    fn builds_document_with_asset_batch_and_paths() {
        let value = page();
        let document = build_document(
            &value,
            vec!["writing-assets/one.png".into()],
            "batch-1",
            mongodb::bson::DateTime::now(),
        );
        assert_eq!(document.get_str("title").unwrap(), value.title);
        assert_eq!(document.get_str("asset_batch").unwrap(), "batch-1");
        assert_eq!(document.get_i64("revision").unwrap(), 3);
        let paths = document.get_array("asset_paths").unwrap();
        assert_eq!(paths.len(), 1);
    }
}
