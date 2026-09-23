use std::{
    hash::{Hash, Hasher},
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};

use tauri::{AppHandle, Emitter, Manager};

use crate::{commands::ImagesDir, db};

static LAST_CONTENT: Mutex<Option<String>> = Mutex::new(None);
static LAST_IMAGE_HASH: Mutex<Option<u64>> = Mutex::new(None);
static LAST_OWN_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn GetClipboardSequenceNumber() -> u32;
    fn IsClipboardFormatAvailable(format: u32) -> i32;
    fn RegisterClipboardFormatW(name: *const u16) -> u16;
    fn GetClipboardData(format: u32) -> *mut std::ffi::c_void;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GlobalSize(memory: *mut std::ffi::c_void) -> usize;
    fn GlobalLock(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn GlobalUnlock(memory: *mut std::ffi::c_void) -> i32;
}

pub fn note_own_write(app: &AppHandle) {
    #[cfg(windows)]
    {
        let seq = unsafe { GetClipboardSequenceNumber() };
        LAST_OWN_SEQ.store(seq as u64, std::sync::atomic::Ordering::SeqCst);
    }
    let _ = app;
}

pub fn capture_text(app: &AppHandle, text: &str) {
    if crate::license::require(app).is_err() {
        return;
    }
    let mut guard = LAST_CONTENT.lock().unwrap();
    if guard.as_deref() == Some(text) {
        return;
    }
    *guard = Some(text.to_string());
    let id = db::insert_item_with_origin(app, text, "text", None, "clipboard");
    db::set_active_item(app, Some(id));
    db::cleanup_origin(app, "clipboard");
    db::prune_history(app, 5_000);
    let _ = app.emit("captured", id);
    let _ = app.emit("refresh", ());
}

fn history_limit(app: &AppHandle) -> usize {
    // Keep the old setting as a bounded safety cap for migrated installs. The
    // user-facing lifespan is age-based and lives in the two retention keys.
    db::get_setting(app, "history_limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(5_000)
        .clamp(100, 100_000)
}

pub fn start(app: AppHandle) {
    thread::spawn(move || {
        let mut last_seq = clip_seq();
        loop {
            thread::sleep(Duration::from_millis(300));
            let seq = clip_seq();
            if seq == last_seq {
                continue;
            }
            last_seq = seq;
            if seq as u64 == LAST_OWN_SEQ.load(std::sync::atomic::Ordering::SeqCst) {
                continue;
            }
            if clip_excluded() {
                continue;
            }
            let mut cb = match arboard::Clipboard::new() {
                Ok(c) => c,
                Err(_) => continue,
            };
            if let Ok(text) = cb.get_text() {
                let t = text.trim().to_string();
                if t.is_empty() || t.contains(crate::capture::SENTINEL) {
                    continue;
                }
                capture_text(&app, &t);
            } else if let Ok(img) = cb.get_image() {
                let _ = capture_image(&app, &img);
            }
        }
    });
}

pub fn capture_image(app: &AppHandle, img: &arboard::ImageData<'_>) -> bool {
    if crate::license::require(app).is_err() {
        return false;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    img.bytes.hash(&mut hasher);
    let hash = hasher.finish();
    {
        let mut guard = LAST_IMAGE_HASH.lock().unwrap();
        if *guard == Some(hash) {
            return true;
        }
        *guard = Some(hash);
    }
    let Some(encoded) = encode_png(img) else {
        let mut guard = LAST_IMAGE_HASH.lock().unwrap();
        *guard = None;
        return false;
    };
    let images = app.state::<ImagesDir>().0.clone();
    let path = images.join(format!(
        "{}-{}.png",
        db::now_ms(),
        std::process::id() & 0xFFFF
    ));
    if std::fs::write(&path, encoded).is_err() {
        let mut guard = LAST_IMAGE_HASH.lock().unwrap();
        *guard = None;
        return false;
    }
    let id =
        db::insert_item_with_origin(app, "", "image", Some(&path.to_string_lossy()), "clipboard");
    db::set_active_item(app, Some(id));
    db::cleanup_origin(app, "clipboard");
    db::prune_history(app, history_limit(app));
    let _ = app.emit("captured", id);
    let _ = app.emit("refresh", ());
    true
}

fn encode_png(img: &arboard::ImageData) -> Option<Vec<u8>> {
    use image::{ExtendedColorType, ImageEncoder};
    let mut png = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png);
    encoder
        .write_image(
            img.bytes.as_ref(),
            img.width as u32,
            img.height as u32,
            ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(png)
}

#[cfg(windows)]
fn clip_seq() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

#[cfg(not(windows))]
fn clip_seq() -> u32 {
    0
}

#[cfg(windows)]
fn registered_format(name: &str, slot: &'static OnceLock<u16>) -> u16 {
    *slot.get_or_init(|| {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { RegisterClipboardFormatW(wide.as_ptr()) }
    })
}

#[cfg(windows)]
fn clipboard_format_dword(format: u16) -> Option<u32> {
    if format == 0 || unsafe { IsClipboardFormatAvailable(format as u32) } == 0 {
        return None;
    }
    unsafe {
        let memory = GetClipboardData(format as u32);
        if memory.is_null() || GlobalSize(memory) < std::mem::size_of::<u32>() {
            return None;
        }
        let locked = GlobalLock(memory);
        if locked.is_null() {
            return None;
        }
        let value = std::ptr::read_unaligned(locked.cast::<u32>());
        let _ = GlobalUnlock(memory);
        Some(value)
    }
}

fn should_exclude_clipboard(
    explicit_exclusion: bool,
    can_include_in_history: Option<u32>,
    can_upload_to_cloud: Option<u32>,
) -> bool {
    explicit_exclusion || can_include_in_history == Some(0) || can_upload_to_cloud == Some(0)
}

#[cfg(windows)]
fn clip_excluded() -> bool {
    static EXCLUDE_MONITOR: OnceLock<u16> = OnceLock::new();
    static INCLUDE_HISTORY: OnceLock<u16> = OnceLock::new();
    static UPLOAD_CLOUD: OnceLock<u16> = OnceLock::new();
    let explicit = registered_format(
        "ExcludeClipboardContentFromMonitorProcessing",
        &EXCLUDE_MONITOR,
    );
    let history = registered_format("CanIncludeInClipboardHistory", &INCLUDE_HISTORY);
    let cloud = registered_format("CanUploadToCloudClipboard", &UPLOAD_CLOUD);
    should_exclude_clipboard(
        explicit != 0 && unsafe { IsClipboardFormatAvailable(explicit as u32) } != 0,
        clipboard_format_dword(history),
        clipboard_format_dword(cloud),
    )
}

#[cfg(not(windows))]
fn clip_excluded() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::should_exclude_clipboard;

    #[test]
    fn clipboard_exclusion_flags_are_fail_closed() {
        assert!(should_exclude_clipboard(true, Some(1), Some(1)));
        assert!(should_exclude_clipboard(false, Some(0), Some(1)));
        assert!(should_exclude_clipboard(false, Some(1), Some(0)));
        assert!(!should_exclude_clipboard(false, Some(1), Some(1)));
        assert!(!should_exclude_clipboard(false, None, None));
    }
}
