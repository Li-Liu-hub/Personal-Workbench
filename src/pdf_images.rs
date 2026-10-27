//! Local PDF rendering with the Windows PDF engine; no external renderer required.
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use windows::{
    Data::Pdf::{PdfDocument, PdfPageRenderOptions},
    Storage::{
        StorageFile,
        Streams::{DataReader, InMemoryRandomAccessStream},
    },
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    core::HSTRING,
};

type Result<T> = std::result::Result<T, String>;

// windows-core 0.52 may dereference a null message buffer for unmapped HRESULTs.
// Preserve the code without requesting the OS message, including for damaged PDFs.
fn win_error(error: windows::core::Error) -> String {
    format!("Windows 错误 0x{:08X}", error.code().0 as u32)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Png,
    Jpg,
    WebP,
    Bmp,
}

impl Format {
    pub const ALL: [Self; 4] = [Self::Png, Self::Jpg, Self::WebP, Self::Bmp];
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpg => "JPG",
            Self::WebP => "WebP",
            Self::Bmp => "BMP",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::WebP => "webp",
            Self::Bmp => "bmp",
        }
    }
}

#[derive(Clone)]
pub struct Options {
    pub input: PathBuf,
    pub output: PathBuf,
    pub format: Format,
    pub dpi: u32,
    pub quality: u8,
    pub pages: String,
}

impl Options {
    pub fn validate(&self) -> Result<()> {
        if !self.input.is_file()
            || !self
                .input
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        {
            return Err("请选择一个有效的 PDF 文件".into());
        }
        if !self.output.is_dir() {
            return Err("请选择已存在的输出文件夹".into());
        }
        if !(72..=600).contains(&self.dpi) {
            return Err("清晰度请输入 72–600 DPI".into());
        }
        if !(1..=100).contains(&self.quality) {
            return Err("JPG 质量请输入 1–100".into());
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Summary {
    pub folder: PathBuf,
    pub pages: usize,
}

/// Blank means all pages. Deduplicate and sort so filenames retain original page numbers.
fn page_numbers(text: &str, count: u32) -> Result<Vec<u32>> {
    if count == 0 {
        return Err("PDF 中没有页面".into());
    }
    if text.trim().is_empty() {
        return Ok((1..=count).collect());
    }
    let invalid = || "页码格式无效，请输入如 1,3-5，且页码不得超过 PDF 总页数".to_owned();
    let mut pages = BTreeSet::new();
    for part in text.replace('，', ",").split(',') {
        let bounds: Vec<_> = part.trim().split('-').map(str::trim).collect();
        if bounds.is_empty() || bounds.len() > 2 {
            return Err(invalid());
        }
        let start: u32 = bounds[0].parse().map_err(|_| invalid())?;
        let end = if bounds.len() == 2 {
            bounds[1].parse().map_err(|_| invalid())?
        } else {
            start
        };
        if start == 0 || end < start || end > count {
            return Err(invalid());
        }
        pages.extend(start..=end);
    }
    Ok(pages.into_iter().collect())
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("已取消转换，未生成结果文件夹".into())
    } else {
        Ok(())
    }
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}

fn render(doc: &PdfDocument, number: u32, dpi: u32) -> Result<image::DynamicImage> {
    let page = doc.GetPage(number - 1).map_err(win_error)?;
    let result = (|| {
        let size = page.Size().map_err(win_error)?;
        let width = (size.Width as f64 * dpi as f64 / 96.).round();
        let height = (size.Height as f64 * dpi as f64 / 96.).round();
        if width < 1.
            || height < 1.
            || !width.is_finite()
            || !height.is_finite()
            || width * height > 60_000_000.
            || width > 32767.
            || height > 32767.
        {
            return Err(format!(
                "第 {number} 页在当前清晰度下过大，请降低 DPI（单页上限 6000 万像素）"
            ));
        }
        let bytes = (|| -> windows::core::Result<Vec<u8>> {
            let options = PdfPageRenderOptions::new()?;
            // WinRT render destinations are DIPs and use the primary display scale.
            // Compensate before rendering and normalize rounding differences below.
            let scale = unsafe {
                windows_sys::Win32::UI::Shell::GetScaleFactorForDevice(
                    windows_sys::Win32::UI::Shell::DEVICE_PRIMARY,
                )
            }
            .clamp(100, 500) as f64
                / 100.;
            options.SetDestinationWidth((width / scale).ceil() as u32)?;
            options.SetDestinationHeight((height / scale).ceil() as u32)?;
            options.SetBackgroundColor(windows::UI::Color {
                A: 255,
                R: 255,
                G: 255,
                B: 255,
            })?;
            let stream = InMemoryRandomAccessStream::new()?;
            page.RenderWithOptionsToStreamAsync(&stream, &options)?
                .get()?;
            let length = stream.Size()?;
            if length > 256 * 1024 * 1024 {
                return Err(windows::core::Error::new(
                    windows::Win32::Foundation::E_FAIL,
                    "PDF image stream is invalid".into(),
                ));
            }
            let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
            let loaded = reader.LoadAsync(length as u32)?.get()?;
            if loaded as u64 != length {
                return Err(windows::core::Error::new(
                    windows::Win32::Foundation::E_FAIL,
                    "PDF image stream is invalid".into(),
                ));
            }
            let mut bytes = vec![0; loaded as usize];
            reader.ReadBytes(&mut bytes)?;
            Ok(bytes)
        })()
        .map_err(|e| format!("第 {number} 页渲染失败：{}", win_error(e)))?;
        let image = image::load_from_memory(&bytes)
            .map_err(|e| format!("第 {number} 页图片读取失败：{e}"))?;
        if image.width() == width as u32 && image.height() == height as u32 {
            Ok(image)
        } else {
            Ok(image.resize_exact(
                width as u32,
                height as u32,
                image::imageops::FilterType::Lanczos3,
            ))
        }
    })();
    let _ = page.Close();
    result
}

fn save(image: &image::DynamicImage, path: &Path, format: Format, quality: u8) -> Result<()> {
    let rgb = image.to_rgb8();
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    if format == Format::Jpg {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, quality)
            .encode_image(&rgb)
            .map_err(|e| e.to_string())?;
    } else {
        let format = match format {
            Format::Png => image::ImageFormat::Png,
            Format::WebP => image::ImageFormat::WebP,
            _ => image::ImageFormat::Bmp,
        };
        image::DynamicImage::ImageRgb8(rgb)
            .write_to(&mut file, format)
            .map_err(|e| e.to_string())?;
    }
    use std::io::Write;
    file.flush().map_err(|e| e.to_string())?;
    file.get_ref().sync_all().map_err(|e| e.to_string())
}

/// Stage every page in a temporary folder, then publish the whole batch on success.
pub fn convert(
    opts: &Options,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize, usize, u32),
) -> Result<Summary> {
    opts.validate()?;
    check_cancel(cancel)?;
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)
            .map_err(|e| format!("无法初始化 PDF 引擎：{}", win_error(e)))?;
    }
    let _apartment = Apartment;
    let input = std::fs::canonicalize(&opts.input).map_err(|e| e.to_string())?;
    // StorageFile expects a normal absolute Windows path, rather than the extended prefix.
    let input = input.to_string_lossy();
    let input = input
        .strip_prefix(r"\\?\UNC\")
        .map(|p| format!(r"\\{p}"))
        .unwrap_or_else(|| input.strip_prefix(r"\\?\").unwrap_or(&input).to_owned());
    let doc = (|| -> windows::core::Result<PdfDocument> {
        let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(input))?.get()?;
        PdfDocument::LoadFromFileAsync(&file)?.get()
    })()
    .map_err(|e| {
        format!(
            "无法打开 PDF，请检查文件是否损坏或需要密码：{}",
            win_error(e)
        )
    })?;
    let pages = page_numbers(&opts.pages, doc.PageCount().map_err(win_error)?)?;
    check_cancel(cancel)?;
    let staging = tempfile::Builder::new()
        .prefix(".pdf-images-")
        .tempdir_in(&opts.output)
        .map_err(|e| e.to_string())?;
    let stem: String = opts
        .input
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .take(60)
        .collect();
    progress(0, pages.len(), pages[0]);
    for (index, number) in pages.iter().copied().enumerate() {
        check_cancel(cancel)?;
        let image = render(&doc, number, opts.dpi)?;
        check_cancel(cancel)?;
        save(
            &image,
            &staging
                .path()
                .join(format!("{stem}_{number:04}.{}", opts.format.extension())),
            opts.format,
            opts.quality,
        )?;
        progress(index + 1, pages.len(), number);
    }
    check_cancel(cancel)?;
    let folder = opts
        .output
        .join(format!("{stem}_图片_{}", uuid::Uuid::new_v4()));
    std::fs::rename(staging.path(), &folder).map_err(|e| format!("保存结果失败：{e}"))?;
    Ok(Summary {
        folder,
        pages: pages.len(),
    })
}

#[cfg(test)]
pub(crate) mod tests;
