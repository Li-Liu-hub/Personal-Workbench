//! 外观主题相关命令。

use std::path::{Path, PathBuf};

use crate::theme::{self, ThemeSettings};
use base64::Engine;

use crate::state::{data_directory, development_db};

/// 读取当前主题配置。
#[tauri::command]
pub fn theme_get() -> Result<ThemeSettings, String> {
    let database = development_db()?;
    Ok(ThemeSettings::load(
        &database.settings().map_err(|error| error.to_string())?,
    ))
}

/// 保存主题配置，返回生效后的配置。
#[tauri::command]
pub fn theme_save(mut theme: ThemeSettings) -> Result<ThemeSettings, String> {
    theme.validate().map_err(|error| error.to_string())?;
    if let Some(path) = theme
        .background
        .clone()
        .filter(|path| Path::new(path).is_absolute())
    {
        let data = data_directory()?;
        theme.background =
            Some(theme::import_image(&data, Path::new(&path)).map_err(|error| error.to_string())?);
    }
    let json = serde_json::to_string(&theme).map_err(|error| error.to_string())?;
    development_db()?
        .set_setting("appearance.theme", &json)
        .map_err(|error| error.to_string())?;
    Ok(theme)
}

/// 设置界面缩放比例，返回更新后的主题。
#[tauri::command]
pub fn theme_set_zoom(zoom: f32) -> Result<ThemeSettings, String> {
    let mut database = development_db()?;
    let mut theme = ThemeSettings::load(&database.settings().map_err(|error| error.to_string())?);
    theme.zoom = zoom;
    theme.validate().map_err(|error| error.to_string())?;
    let json = serde_json::to_string(&theme).map_err(|error| error.to_string())?;
    database
        .set_setting("appearance.theme", &json)
        .map_err(|error| error.to_string())?;
    Ok(theme)
}

/// 选择主题背景图片，返回文件路径；取消时返回 null。
#[tauri::command]
pub async fn theme_pick_image() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("选择主题背景图片")
        .add_filter("图片", &["png", "jpg", "jpeg", "webp", "bmp"])
        .pick_file()
        .await
        .map(|file| file.path().to_owned())
}

/// 读取主题背景图片，返回 data URL。
#[tauri::command]
pub async fn theme_image(value: String) -> Result<String, String> {
    let data = data_directory()?;
    tauri::async_runtime::spawn_blocking(move || read_theme_image(&data, &value))
        .await
        .map_err(|error| error.to_string())?
}

fn read_theme_image(data: &Path, value: &str) -> Result<String, String> {
    let path = theme::background_path(data, value).map_err(|error| error.to_string())?;
    let bytes = theme::validated_image_bytes(&path).map_err(|error| error.to_string())?;
    let mime = match image::guess_format(&bytes).map_err(|error| error.to_string())? {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        image::ImageFormat::Bmp => "image/bmp",
        _ => return Err("不支持的图片格式".into()),
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
