use std::{
    hash::{Hash, Hasher},
    sync::Mutex,
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
    fn IsClipboardFormatAvailable(fmt: u32) -> i32;
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
    let mut guard = LAST_CONTENT.lock().unwrap();
    if guard.as_deref() == Some(text) {
        return;
    }
    *guard = Some(text.to_string());
    let id = db::insert_item(app, text, "text", None);
    db::prune_history(app, history_limit(app));
    let _ = app.emit("captured", id);
    let _ = app.emit("refresh", ());
}

fn history_limit(app: &AppHandle) -> usize {
    db::get_setting(app, "history_limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(500)
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
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                img.bytes.hash(&mut hasher);
                let hash = hasher.finish();
                let mut guard = LAST_IMAGE_HASH.lock().unwrap();
                if *guard == Some(hash) {
                    continue;
                }
                *guard = Some(hash);
                let images = app.state::<ImagesDir>().0.clone();
                let path = images.join(format!(
                    "{}-{}.png",
                    db::now_ms(),
                    std::process::id() & 0xFFFF
                ));
                if let Some(encoded) = encode_png(&img) {
                    let _ = std::fs::write(&path, encoded);
                    let id = db::insert_item(&app, "", "image", Some(&path.to_string_lossy()));
                    db::prune_history(&app, history_limit(&app));
                    let _ = app.emit("captured", id);
                    let _ = app.emit("refresh", ());
                }
            }
        }
    });
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
fn clip_excluded() -> bool {
    unsafe { IsClipboardFormatAvailable(0x4000) != 0 }
}

#[cfg(not(windows))]
fn clip_excluded() -> bool {
    false
}