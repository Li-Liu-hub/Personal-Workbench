use crate::error::{DeskError, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

/// 主题背景图的填充方式：Cover、Contain 或 Stretch。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFit {
    #[default]
    Cover,
    Contain,
    Stretch,
}

/// A decoded image in a UI-agnostic layout: tightly packed RGBA8 rows.
///
/// The desktop client (Tauri/React) consumes theme images as data URLs, so the
/// core library only needs pixels and dimensions, not a windowing toolkit type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// 外观主题配置；包含配色、透明度、背景图、动画和缩放。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeSettings {
    pub accent: [u8; 3],
    pub text: [u8; 3],
    pub muted: [u8; 3],
    pub canvas: [u8; 3],
    pub sidebar: [u8; 3],
    pub surface: [u8; 3],
    pub input: [u8; 3],
    pub border: [u8; 3],
    pub button_text: [u8; 3],
    pub sidebar_text: [u8; 3],
    pub surface_opacity: f32,
    pub sidebar_opacity: f32,
    pub image_opacity: f32,
    pub content_overlay: f32,
    pub zoom: f32,
    pub animations: bool,
    pub background: Option<String>,
    pub fit: ImageFit,
}
impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            accent: [49, 89, 74],
            text: [37, 42, 38],
            muted: [108, 116, 108],
            canvas: [247, 244, 236],
            sidebar: [27, 45, 37],
            surface: [255, 253, 248],
            input: [240, 235, 223],
            border: [226, 220, 207],
            button_text: [255, 253, 248],
            sidebar_text: [201, 211, 202],
            surface_opacity: 1.0,
            sidebar_opacity: 1.0,
            image_opacity: 0.7,
            content_overlay: 0.12,
            zoom: 1.0,
            animations: true,
            background: None,
            fit: ImageFit::Cover,
        }
    }
}
impl ThemeSettings {
    pub fn load(settings: &BTreeMap<String, String>) -> Self {
        let mut theme: Self = settings
            .get("appearance.theme")
            .and_then(|s| serde_json::from_str(s).ok())
            .filter(|theme: &Self| theme.validate().is_ok())
            .unwrap_or_default();
        // Refresh only untouched stock palettes, keeping custom colors and backgrounds.
        if (theme.accent == [58, 101, 89]
            && theme.text == [40, 51, 47]
            && theme.muted == [123, 133, 128]
            && theme.canvas == [248, 249, 248]
            && theme.sidebar == [244, 246, 244]
            && theme.surface == [255, 255, 255]
            && theme.input == [247, 248, 247]
            && theme.border == [227, 231, 228]
            && theme.sidebar_text == [98, 109, 103])
            || (theme.accent == [101, 88, 211]
                && theme.text == [32, 40, 59]
                && theme.canvas == [247, 248, 252])
            || (theme.accent == [0, 122, 255]
                && theme.text == [29, 29, 31]
                && theme.canvas == [250, 250, 252]
                && theme.sidebar == [240, 240, 244]
                && theme.input == [245, 245, 247]
                && theme.border == [224, 225, 230]
                && theme.muted == [112, 112, 119]
                && theme.sidebar_text == [88, 88, 96]
                && theme.surface == [255, 255, 255])
            || (theme.accent == [0, 113, 227]
                && theme.text == [31, 35, 42]
                && theme.muted == [118, 126, 139]
                && theme.canvas == [248, 249, 251]
                && theme.sidebar == [241, 244, 248]
                && theme.surface == [255, 255, 255]
                && theme.input == [246, 248, 251]
                && theme.border == [227, 232, 239]
                && theme.button_text == [255, 255, 255]
                && theme.sidebar_text == [94, 103, 117])
        {
            let fresh = Self::default();
            theme.accent = fresh.accent;
            theme.text = fresh.text;
            theme.muted = fresh.muted;
            theme.canvas = fresh.canvas;
            theme.sidebar = fresh.sidebar;
            theme.input = fresh.input;
            theme.border = fresh.border;
            theme.sidebar_text = fresh.sidebar_text;
        }
        theme
    }
    pub fn validate(&self) -> Result<()> {
        if ![
            self.surface_opacity,
            self.sidebar_opacity,
            self.image_opacity,
            self.content_overlay,
        ]
        .iter()
        .all(|n| n.is_finite() && (0.0..=1.0).contains(n))
            || !self.zoom.is_finite()
            || !(0.8..=1.5).contains(&self.zoom)
        {
            return Err(DeskError::InvalidPath("主题透明度或缩放数值无效".into()));
        }
        Ok(())
    }
}

pub fn background_path(data: &Path, value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let parts = path.components().collect::<Vec<_>>();
    if parts.len() != 2
        || parts[0].as_os_str() != "theme-assets"
        || !matches!(parts[1], std::path::Component::Normal(_))
    {
        return Err(DeskError::InvalidPath("主题图片路径无效".into()));
    }
    Ok(data.join(path))
}
fn image_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    std::fs::File::open(path)?
        .take(20 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err(DeskError::InvalidPath(
            "背景图片请控制在 20 MiB 以内".into(),
        ));
    }
    Ok(bytes)
}
fn decode_limited(bytes: &[u8]) -> Result<image::DynamicImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| DeskError::InvalidPath(format!("图片无法读取：{e}")))
}

pub fn decode(bytes: &[u8]) -> Result<ImageData> {
    let image = decode_limited(bytes)?;
    let limit = 2048;
    let image = if image.width() > limit || image.height() > limit {
        image.thumbnail(limit, limit)
    } else {
        image
    }
    .to_rgba8();
    Ok(ImageData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}
pub fn load_image(path: &Path) -> Result<ImageData> {
    decode(&image_bytes(path)?)
}

pub(crate) fn validated_image_bytes(path: &Path) -> Result<Vec<u8>> {
    let bytes = image_bytes(path)?;
    decode_limited(&bytes)?;
    Ok(bytes)
}

pub fn import_image(data: &Path, source: &Path) -> Result<String> {
    let bytes = validated_image_bytes(source)?;
    let format = image::guess_format(&bytes).map_err(|e| DeskError::InvalidPath(e.to_string()))?;
    let ext = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::WebP => "webp",
        image::ImageFormat::Bmp => "bmp",
        _ => return Err(DeskError::UnsupportedFormat),
    };
    let directory = data.join("theme-assets");
    std::fs::create_dir_all(&directory)?;
    let name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(directory.join(&name))
        .map_err(|e| DeskError::Io(e.error))?;
    Ok(format!("theme-assets/{name}"))
}

#[cfg(test)]
mod palette_tests {
    use super::*;
    #[test]
    fn current_stock_palette_refresh_preserves_preferences_and_custom_palette() {
        let old = ThemeSettings {
            accent: [0, 113, 227],
            text: [31, 35, 42],
            muted: [118, 126, 139],
            canvas: [248, 249, 251],
            sidebar: [241, 244, 248],
            surface: [255, 255, 255],
            input: [246, 248, 251],
            border: [227, 232, 239],
            button_text: [255, 255, 255],
            sidebar_text: [94, 103, 117],
            zoom: 1.25,
            animations: false,
            background: Some("theme-assets/custom.png".into()),
            surface_opacity: 0.8,
            ..Default::default()
        };
        let load = |theme: &ThemeSettings| {
            ThemeSettings::load(&BTreeMap::from([(
                "appearance.theme".into(),
                serde_json::to_string(theme).unwrap(),
            )]))
        };
        let refreshed = load(&old);
        assert_eq!(refreshed.accent, ThemeSettings::default().accent);
        assert_eq!(refreshed.zoom, old.zoom);
        assert_eq!(refreshed.background, old.background);
        assert_eq!(refreshed.surface_opacity, old.surface_opacity);
        assert!(!refreshed.animations);
        let custom = ThemeSettings {
            input: [220, 230, 240],
            ..old
        };
        assert_eq!(load(&custom), custom);
        assert!(
            serde_json::from_str::<ThemeSettings>("{}")
                .unwrap()
                .animations
        );
    }

    #[test]
    fn theme_values_reject_nonfinite_or_out_of_range_numbers_and_load_defaults() {
        for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            let theme = ThemeSettings {
                surface_opacity: value,
                ..Default::default()
            };
            assert!(theme.validate().is_err());
        }
        for zoom in [0.79, 1.51, f32::NAN, f32::INFINITY] {
            assert!(
                ThemeSettings {
                    zoom,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        let settings = BTreeMap::from([("appearance.theme".into(), r#"{"zoom":0.0}"#.into())]);
        assert_eq!(ThemeSettings::load(&settings), ThemeSettings::default());
    }
    #[test]
    fn stock_palette_refresh_keeps_custom_colors_images_and_zoom() {
        let old = ThemeSettings {
            accent: [0, 122, 255],
            text: [29, 29, 31],
            muted: [112, 112, 119],
            canvas: [250, 250, 252],
            sidebar: [240, 240, 244],
            surface: [255, 255, 255],
            input: [245, 245, 247],
            border: [224, 225, 230],
            sidebar_text: [88, 88, 96],
            background: Some("backgrounds/example.png".into()),
            zoom: 1.2,
            image_opacity: 0.4,
            ..Default::default()
        };
        let load = |theme: &ThemeSettings| {
            ThemeSettings::load(&BTreeMap::from([(
                "appearance.theme".into(),
                serde_json::to_string(theme).unwrap(),
            )]))
        };
        let refreshed = load(&old);
        assert_eq!(refreshed.accent, ThemeSettings::default().accent);
        assert_eq!(refreshed.background, old.background);
        assert_eq!(refreshed.zoom, 1.2);
        assert_eq!(refreshed.image_opacity, 0.4);
        let custom = ThemeSettings {
            muted: [90, 110, 130],
            ..old.clone()
        };
        assert_eq!(load(&custom), custom);
        let custom = ThemeSettings {
            accent: [150, 70, 80],
            ..old
        };
        assert_eq!(load(&custom), custom);
    }

    #[test]
    fn previous_stock_sage_palette_refreshes_to_ink_palette() {
        let old = ThemeSettings {
            accent: [58, 101, 89],
            text: [40, 51, 47],
            muted: [123, 133, 128],
            canvas: [248, 249, 248],
            sidebar: [244, 246, 244],
            surface: [255, 255, 255],
            input: [247, 248, 247],
            border: [227, 231, 228],
            button_text: [255, 255, 255],
            sidebar_text: [98, 109, 103],
            ..Default::default()
        };
        let refreshed = ThemeSettings::load(&BTreeMap::from([(
            "appearance.theme".into(),
            serde_json::to_string(&old).unwrap(),
        )]));
        assert_eq!(refreshed.accent, ThemeSettings::default().accent);
        assert_eq!(refreshed.canvas, ThemeSettings::default().canvas);
        assert_eq!(refreshed.sidebar, ThemeSettings::default().sidebar);
    }
}
