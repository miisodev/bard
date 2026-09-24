use std::path::Path;

use base64::Engine;
use image::GenericImageView;
use tauri::AppHandle;

/// Send one native paste chord to the currently focused application. The
/// caller has already written and verified the clipboard payload.
pub fn send_paste(_app: &AppHandle) {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let _synthetic_input = crate::capture::SyntheticInputGuard::enter();
    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        return;
    };
    let _ = enigo.key(Key::Control, Direction::Press);
    std::thread::sleep(std::time::Duration::from_millis(20));
    let _ = enigo.key(Key::Unicode('v'), Direction::Click);
    std::thread::sleep(std::time::Duration::from_millis(20));
    let _ = enigo.key(Key::Control, Direction::Release);
    // Let the low-level hook drain Bard's synthetic events before the guard
    // re-enables gesture recognition.
    std::thread::sleep(std::time::Duration::from_millis(40));
}

pub fn set_text(app: &AppHandle, text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_string()).map_err(|e| e.to_string())?;
    if cb.get_text().map_err(|e| e.to_string())? != text {
        return Err("The clipboard write could not be verified.".into());
    }
    crate::monitor::note_own_write(app);
    Ok(())
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
    let expected = data.clone();
    cb.set_image(data).map_err(|e| e.to_string())?;
    let actual = cb.get_image().map_err(|e| e.to_string())?;
    if actual.width != expected.width
        || actual.height != expected.height
        || actual.bytes != expected.bytes
    {
        return Err("The clipboard image write could not be verified.".into());
    }
    crate::monitor::note_own_write(app);
    Ok(())
}

pub fn image_data(app: &AppHandle, path: &str) -> Option<String> {
    let _ = app;
    let root = crate::db::images_root()?;
    let candidate = Path::new(path);
    if !crate::hf::path_is_within(&root, candidate) {
        return None;
    }
    let metadata = std::fs::metadata(candidate).ok()?;
    if !metadata.is_file() || metadata.len() > 25 * 1024 * 1024 {
        return None;
    }
    let bytes = std::fs::read(candidate).ok()?;
    let format = image::guess_format(&bytes).ok()?;
    let decoded = image::load_from_memory_with_format(&bytes, format).ok()?;
    let (width, height) = decoded.dimensions();
    if width == 0 || height == 0 || width as u64 * height as u64 > 100_000_000 {
        return None;
    }
    let mime = match format {
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        image::ImageFormat::Gif => "image/gif",
        image::ImageFormat::Bmp => "image/bmp",
        image::ImageFormat::Png => "image/png",
        _ => return None,
    };
    Some(format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
