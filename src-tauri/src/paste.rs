use std::path::Path;

use base64::Engine;
use tauri::AppHandle;

pub fn set_text(app: &AppHandle, text: &str) -> Result<(), String> {
    crate::monitor::note_own_write(app);
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_string()).map_err(|e| e.to_string())
}

pub fn set_image(app: &AppHandle, path: &str) -> Result<(), String> {
    crate::monitor::note_own_write(app);
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let data = arboard::ImageData {
        width: w as usize,
        height: h as usize,
        bytes: rgba.into_raw().into(),
    };
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_image(data).map_err(|e| e.to_string())
}

pub fn image_data(app: &AppHandle, path: &str) -> Option<String> {
    let _ = app;
    let bytes = std::fs::read(path).ok()?;
    let mime = match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        _ => "image/png",
    };
    Some(format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}