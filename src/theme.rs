use crate::error::{DeskError, Result};
use eframe::egui::{self, Color32};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFit {
    #[default]
    Cover,
    Contain,
    Stretch,
}

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
    pub background: Option<String>,
    pub fit: ImageFit,
}
impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            accent: [0, 122, 255],
            text: [29, 29, 31],
            muted: [112, 112, 119],
            canvas: [250, 250, 252],
            sidebar: [240, 240, 244],
            surface: [255, 255, 255],
            input: [245, 245, 247],
            border: [224, 225, 230],
            button_text: [255, 255, 255],
            sidebar_text: [88, 88, 96],
            surface_opacity: 1.0,
            sidebar_opacity: 1.0,
            image_opacity: 0.7,
            content_overlay: 0.12,
            zoom: 1.0,
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
        // Migrate only the old stock palette, retaining custom images and sizing.
        if theme.accent == [101, 88, 211]
            && theme.text == [32, 40, 59]
            && theme.canvas == [247, 248, 252]
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
    pub fn activate(&self) {
        ACTIVE.with(|state| *state.borrow_mut() = self.clone());
    }
    pub fn apply(&self, ctx: &egui::Context) {
        self.activate();
        ctx.set_zoom_factor(self.zoom);
        ctx.style_mut_of(egui::Theme::Light, |style| {
            let v = &mut style.visuals;
            v.panel_fill = canvas();
            v.window_fill = surface();
            v.extreme_bg_color = input();
            v.text_edit_bg_color = Some(input());
            v.faint_bg_color = input();
            v.override_text_color = Some(ink());
            v.weak_text_color = Some(muted());
            v.hyperlink_color = accent();
            v.window_stroke = egui::Stroke::new(1.0, border());
            v.selection.bg_fill = accent().gamma_multiply(0.22);
            v.selection.stroke = egui::Stroke::new(1.0, accent());
            for widget in [
                &mut v.widgets.noninteractive,
                &mut v.widgets.inactive,
                &mut v.widgets.hovered,
                &mut v.widgets.active,
                &mut v.widgets.open,
            ] {
                widget.fg_stroke = egui::Stroke::new(1.0, ink());
                widget.bg_stroke = egui::Stroke::new(1.0, border());
                widget.bg_fill = surface();
                widget.weak_bg_fill = surface();
            }
            v.widgets.noninteractive.bg_fill = border();
            v.widgets.inactive.weak_bg_fill = input();
            v.widgets.hovered.bg_fill = input();
            v.widgets.hovered.weak_bg_fill = input();
            v.widgets.active.bg_fill = surface().lerp_to_gamma(accent(), 0.08);
            v.widgets.active.weak_bg_fill = v.widgets.active.bg_fill;
            v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent());
            v.widgets.active.bg_stroke = egui::Stroke::new(1.0, accent());
        });
    }
}

thread_local! {static ACTIVE:RefCell<ThemeSettings>=RefCell::new(ThemeSettings::default());}
fn rgb(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}
fn rgba(color: [u8; 3], alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(color[0], color[1], color[2], (alpha * 255.0).round() as u8)
}
pub fn accent() -> Color32 {
    ACTIVE.with(|t| rgb(t.borrow().accent))
}
pub fn ink() -> Color32 {
    ACTIVE.with(|t| rgb(t.borrow().text))
}
pub fn muted() -> Color32 {
    ACTIVE.with(|t| rgb(t.borrow().muted))
}
pub fn canvas() -> Color32 {
    ACTIVE.with(|t| rgb(t.borrow().canvas))
}
pub fn sidebar() -> Color32 {
    ACTIVE.with(|t| {
        let t = t.borrow();
        rgba(t.sidebar, t.sidebar_opacity)
    })
}
pub fn surface() -> Color32 {
    ACTIVE.with(|t| {
        let t = t.borrow();
        rgba(t.surface, t.surface_opacity)
    })
}
pub fn input() -> Color32 {
    ACTIVE.with(|t| rgb(t.borrow().input))
}
pub fn border() -> Color32 {
    ACTIVE.with(|t| rgb(t.borrow().border))
}
pub fn button_text() -> Color32 {
    ACTIVE.with(|t| rgb(t.borrow().button_text))
}
pub fn sidebar_text() -> Color32 {
    ACTIVE.with(|t| rgb(t.borrow().sidebar_text))
}
pub fn content_fill() -> Color32 {
    ACTIVE.with(|t| {
        let t = t.borrow();
        rgba(
            t.canvas,
            if t.background.is_some() {
                t.content_overlay
            } else {
                1.0
            },
        )
    })
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
pub fn decode(bytes: &[u8]) -> Result<egui::ColorImage> {
    decode_sized(bytes, 2048)
}
fn decode_sized(bytes: &[u8], max_side: usize) -> Result<egui::ColorImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| DeskError::InvalidPath(format!("图片无法读取：{e}")))?;
    let limit = max_side.clamp(1, 2048) as u32;
    let image = if image.width() > limit || image.height() > limit {
        image.thumbnail(limit, limit)
    } else {
        image
    }
    .to_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}
pub fn load_image(path: &Path) -> Result<egui::ColorImage> {
    decode(&image_bytes(path)?)
}
pub fn load_image_with_limit(path: &Path, max_side: usize) -> Result<egui::ColorImage> {
    decode_sized(&image_bytes(path)?, max_side)
}
pub fn import_image(data: &Path, source: &Path) -> Result<String> {
    let bytes = image_bytes(source)?;
    decode(&bytes)?;
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

pub fn image_rects(rect: egui::Rect, size: egui::Vec2, fit: ImageFit) -> (egui::Rect, egui::Rect) {
    let full = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
    if fit == ImageFit::Stretch {
        return (rect, full);
    }
    if fit == ImageFit::Contain {
        let scale = (rect.width() / size.x).min(rect.height() / size.y);
        return (
            egui::Rect::from_center_size(rect.center(), size * scale),
            full,
        );
    }
    let scale = (rect.width() / size.x).max(rect.height() / size.y);
    let uv_size = rect.size() / (size * scale);
    (
        rect,
        egui::Rect::from_center_size(egui::pos2(0.5, 0.5), uv_size),
    )
}
