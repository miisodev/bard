use std::path::Path;

use base64::Engine;
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

/// Read a file Bard owns, with the same guards [`image_data`] applies.
///
/// The two functions consume the same DB-stored column, and only one of them
/// checked. `set_image` reached `std::fs::read` with no containment test and no
/// size cap, so it would read an arbitrarily large file into memory — and any
/// future change that let a path reach `items.image_path` would turn it into an
/// arbitrary-file-read-to-clipboard with no guard at all. One helper, so the two
/// callers cannot drift apart again.
fn owned_image_file(path: &str) -> Option<std::path::PathBuf> {
    let root = crate::db::images_root()?;
    let candidate = Path::new(path);
    if !crate::hf::path_is_within(&root, candidate) {
        return None;
    }
    let metadata = std::fs::metadata(candidate).ok()?;
    if !metadata.is_file() || metadata.len() > 25 * 1024 * 1024 {
        return None;
    }
    Some(candidate.to_path_buf())
}

/// The checks a picture must pass before any window is allowed to draw it:
/// a real file Bard owns, inside the size cap, with dimensions that are real
/// numbers rather than a decode of every pixel in it.
///
/// Those dimensions come from the file's header. The card wants nothing else
/// from the picture — it is 236 × 148 CSS pixels — and `load_from_memory` used
/// to be called purely to obtain what `image_dimensions` answers after a few
/// hundred bytes: a full RGBA decode of a 4K screenshot per card per mount,
/// tens of megabytes of transient memory thrown away immediately.
fn owned_image_dimensions(path: &Path) -> Option<(u32, u32)> {
    let (width, height) = image::image_dimensions(path).ok()?;
    if width == 0 || height == 0 || width as u64 * height as u64 > 100_000_000 {
        return None;
    }
    Some((width, height))
}

fn owned_image_bytes(path: &str) -> Option<Vec<u8>> {
    let candidate = owned_image_file(path)?;
    std::fs::read(candidate).ok()
}

/// The webview URL a shelf card loads its picture from: the `asset` protocol
/// rather than a `data:` URL.
///
/// A data URL cost a base64 copy of the whole file per card (a 5 MB screenshot
/// became a 6.7 MB string, decoded again by the compositor at full size). The
/// asset protocol serves the file itself, so Chromium streams and caches it by
/// URL. This is the same URL `convertFileSrc` builds in JavaScript — the scheme
/// is reached at `http://asset.localhost/<path>` on Windows (`useHttpsScheme`
/// stays at its default, `false`) and at `asset://localhost/<path>` elsewhere —
/// built here so every window gets one implementation. The path is encoded the
/// way `encodeURIComponent` writes it, one byte at a time and with no
/// dependency, because a Windows path is nothing but characters the encoding
/// has to escape (`\`, `:`, spaces).
///
/// Every caller has already checked containment and size: the scope registered
/// in `main.rs` covers the images directory, and a path that never leaves
/// `owned_image_file`/`owned_image_dimensions` is the only kind that gets here.
fn asset_url(path: &Path) -> String {
    let mut encoded = String::with_capacity(path.as_os_str().len() + 16);
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => encoded.push(byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    #[cfg(windows)]
    {
        format!("http://asset.localhost/{encoded}")
    }
    #[cfg(not(windows))]
    {
        format!("asset://localhost/{encoded}")
    }
}

/// A card's cover: the file's own asset URL, or nothing when the picture is
/// gone, outside Bard's images folder, or too large to be one Bard stored.
pub fn image_url(path: &str) -> Option<String> {
    let candidate = owned_image_file(path)?;
    owned_image_dimensions(&candidate)?;
    Some(asset_url(&candidate))
}

pub fn set_image(app: &AppHandle, path: &str) -> Result<(), String> {
    crate::monitor::note_own_write(app);
    let bytes = owned_image_bytes(path).ok_or("That image is no longer available.")?;
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
    let bytes = owned_image_bytes(path)?;
    let format = image::guess_format(&bytes).ok()?;
    // Header-only: a full decode was walking every pixel of a 25 MB file to
    // answer a question the header already holds.
    owned_image_dimensions(Path::new(path))?;
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
