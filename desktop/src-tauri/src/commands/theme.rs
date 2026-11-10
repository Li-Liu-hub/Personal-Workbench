//! 外观主题相关命令。

use std::io::Read;
use std::path::{Path, PathBuf};

use base64::Engine;
use personal_desk::theme::{self, ThemeSettings};

use crate::state::{data_directory, development_db};

#[tauri::command]
pub fn theme_get() -> Result<ThemeSettings, String> {
    let database = development_db()?;
    Ok(ThemeSettings::load(
        &database.settings().map_err(|error| error.to_string())?,
    ))
}

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

#[tauri::command]
pub async fn theme_pick_image() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("选择主题背景图片")
        .add_filter("图片", &["png", "jpg", "jpeg", "webp", "bmp"])
        .pick_file()
        .await
        .map(|file| file.path().to_owned())
}

#[tauri::command]
pub fn theme_image(value: String) -> Result<String, String> {
    let data = data_directory()?;
    let path = theme::background_path(&data, &value).map_err(|error| error.to_string())?;
    theme::load_image(&path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(20 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("图片超过 20 MiB".into());
    }
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
