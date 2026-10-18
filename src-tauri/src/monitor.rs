use std::{
    hash::{Hash, Hasher},
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};

use tauri::{AppHandle, Emitter, Manager};

use crate::{commands::ImagesDir, db};

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

/// The largest clipboard text Bard will store, in bytes.
///
/// Nothing bounded this before. A single 200 MB copy out of a log viewer was
/// written to the database and then re-serialised into every open webview on
/// every refresh event, which left the panel permanently unresponsive with no
/// way back short of deleting the item by hand. Bard is a background agent that
/// must never be the reason a machine feels wrong, so an oversized payload is
/// refused with a visible reason rather than absorbed.
pub const MAX_CAPTURE_TEXT_BYTES: usize = 256 * 1024;

/// Hash of the last captured text, rather than the text itself.
///
/// The previous code kept a full `String` copy of the most recent payload alive
/// for the lifetime of the process, purely to compare against the next one. A
/// 64-bit hash answers the same question — has this exact text been captured
/// already — without retaining a user's clipboard contents in memory
/// indefinitely.
static LAST_CONTENT_HASH: Mutex<Option<u64>> = Mutex::new(None);

fn content_hash(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Record content Bard put on the clipboard itself, so the watcher does not
/// capture it back as if the person had copied it.
///
/// Dictation writes the transcript to the clipboard and then files it in the
/// timeline, going through `insert_item_with_origin` rather than through
/// `capture_text`. Without this the watcher had never seen that text, so every
/// dictated line came back a moment later as a second `clipboard` row.
pub fn note_own_text(text: &str) {
    let digest = content_hash(text);
    let mut guard = LAST_CONTENT_HASH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(digest);
}

pub fn capture_text(app: &AppHandle, text: &str) {
    if crate::license::require(app).is_err() {
        return;
    }
    let digest = content_hash(text);
    {
        // A poisoned lock must not kill capture for the rest of the session, so
        // degrade to "have not seen this" rather than panicking the monitor.
        let mut guard = LAST_CONTENT_HASH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *guard == Some(digest) {
            return;
        }
        *guard = Some(digest);
    }

    if text.len() > MAX_CAPTURE_TEXT_BYTES {
        crate::crash::record_event("capture:text-too-large");
        let _ = app.emit(
            "capture-failed",
            format!(
                "That selection is {} KB. Bard keeps items up to {} KB.",
                text.len() / 1024,
                MAX_CAPTURE_TEXT_BYTES / 1024
            ),
        );
        return;
    }

    let id = db::insert_item_with_origin(app, text, "text", None, "clipboard");
    if id == db::INSERT_FAILED {
        // The write did not happen. `insert_item_with_origin` has already
        // reported it; emitting `captured` here would tell four webviews an item
        // exists that is not in the timeline.
        return;
    }
    // The happy path used to leave no breadcrumb at all, so the log could not tell
    // a successful capture from a gesture that did nothing — the same gap
    // `capture:no-selection` was written to close, still open here.
    crate::crash::record_event("capture:stored");
    db::set_active_item(app, Some(id));
    // Age-based retention is the only automatic deletion. The old call to
    // `prune_history` here deleted the oldest automatic items past 5,000 on
    // every single capture, including when the user had chosen Forever.
    if let Err(error) = db::cleanup_origin(app, "clipboard") {
        crate::crash::record_event("capture:retention-failed");
        let _ = app.emit("capture-failed", error);
    }
    let _ = app.emit("captured", id);
    let _ = app.emit("refresh", ());
}

/// Poll the clipboard for new content.
///
/// This loop is the product. It had no heartbeat and no supervision, so when it
/// stopped — a panicking iteration, a poisoned lock — capture was simply gone
/// with nothing anywhere saying so, which is the same class of silent failure the
/// raw-input listener had. Each iteration is now panic-contained so one bad poll
/// cannot end capture for the session, and a periodic heartbeat records that the
/// loop is alive.
pub fn start(app: AppHandle) {
    thread::spawn(move || {
        let mut last_seq = clip_seq();
        let mut ticks: u64 = 0;
        loop {
            // 150 ms, not 300. The check the sleep gates is `clip_seq`, a
            // single cheap API call with no allocation that runs before any
            // clipboard read, so the poll is a heartbeat rather than work; and
            // halving it halves the one delay the user can feel, between the
            // copy and the item appearing.
            thread::sleep(Duration::from_millis(150));
            ticks += 1;
            if ticks.is_multiple_of(HEARTBEAT_TICKS) {
                crate::crash::record_event("monitor:alive");
            }
            let polled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let seq = clip_seq();
                if seq == last_seq {
                    return;
                }
                last_seq = seq;
                if seq as u64 == LAST_OWN_SEQ.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                if clip_excluded() {
                    return;
                }
                let mut cb = match arboard::Clipboard::new() {
                    Ok(c) => c,
                    Err(_) => return,
                };
                if let Ok(text) = cb.get_text() {
                    let t = text.trim().to_string();
                    if t.is_empty() || t.contains(crate::capture::SENTINEL) {
                        return;
                    }
                    // Re-read the sequence number *after* taking the content. The
                    // first read and the content read are not atomic, so a write
                    // landing between them — which is exactly what a dictation
                    // finishing does — made the own-write guard miss, and the
                    // clipboard watcher then captured Bard's own transcript as if
                    // the person had copied it. Every dictated line appeared twice
                    // in the timeline.
                    let after = clip_seq();
                    if after != seq {
                        // Something wrote while we were reading. The value in hand
                        // is already the newer one, so re-evaluate it on the next
                        // poll rather than deciding on a torn pair.
                        return;
                    }
                    capture_text(&app, &t);
                } else if let Ok(img) = cb.get_image() {
                    let _ = capture_image(&app, &img);
                }
            }));
            if polled.is_err() {
                crate::crash::record_event("monitor:iteration-failed");
            }
        }
    });
}

/// Roughly every two and a half minutes at the 150 ms poll interval. An idle
/// install writes one line; this is a liveness marker, not a log of activity.
const HEARTBEAT_TICKS: u64 = 1_000;

pub fn capture_image(app: &AppHandle, img: &arboard::ImageData<'_>) -> bool {
    if crate::license::require(app).is_err() {
        return false;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    img.bytes.hash(&mut hasher);
    let hash = hasher.finish();
    {
        let mut guard = LAST_IMAGE_HASH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *guard == Some(hash) {
            return true;
        }
        *guard = Some(hash);
    }
    let Some(encoded) = encode_png(img) else {
        let mut guard = LAST_IMAGE_HASH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
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
        let mut guard = LAST_IMAGE_HASH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
        return false;
    }
    let id =
        db::insert_item_with_origin(app, "", "image", Some(&path.to_string_lossy()), "clipboard");
    if id == db::INSERT_FAILED {
        // The PNG is on disk with no row referencing it. Remove it rather than
        // leaving an orphan no sweep will ever collect.
        let _ = std::fs::remove_file(&path);
        return false;
    }
    db::set_active_item(app, Some(id));
    if let Err(error) = db::cleanup_origin(app, "clipboard") {
        crate::crash::record_event("capture:retention-failed");
        let _ = app.emit("capture-failed", error);
    }
    let _ = app.emit("captured", id);
    let _ = app.emit("refresh", ());
    true
}

/// The largest image Bard will store, by pixel count and by encoded size.
///
/// A 4K screenshot is tens of megabytes of raw DIB, and the encode allocated an
/// unbounded `Vec` on the monitor thread — so one large screenshot meant tens of
/// megabytes and tens of milliseconds of work on the thread that also has to
/// notice the next copy. Both limits are checked before any of that happens.
pub const MAX_CAPTURE_IMAGE_PIXELS: u64 = 40_000_000;
pub const MAX_CAPTURE_IMAGE_BYTES: usize = 25 * 1024 * 1024;

/// A `Vec` that refuses to grow past a limit.
///
/// The byte cap has to be enforced *while* the encoder runs, not after it
/// returns. A 40 MP screenshot is comfortably inside the pixel limit and its PNG
/// can still exceed 25 MB, so a post-encode check rejects the image only after the
/// allocation and the CPU have already been spent — which is the entire cost the
/// limit exists to avoid, paid on the monitor thread that also has to notice the
/// next copy.
struct CappedBytes {
    bytes: Vec<u8>,
    cap: usize,
}

impl CappedBytes {
    fn new(cap: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(cap.min(1 << 20)),
            cap,
        }
    }
}

impl std::io::Write for CappedBytes {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len() + buffer.len() > self.cap {
            return Err(std::io::Error::other(
                "encoded image exceeds the size Bard stores",
            ));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn encode_png(img: &arboard::ImageData) -> Option<Vec<u8>> {
    use image::{ExtendedColorType, ImageEncoder};
    let pixels = (img.width as u64).saturating_mul(img.height as u64);
    if pixels > MAX_CAPTURE_IMAGE_PIXELS {
        crate::crash::record_event("capture:image-too-large");
        return None;
    }
    let mut png = CappedBytes::new(MAX_CAPTURE_IMAGE_BYTES);
    let encoder = image::codecs::png::PngEncoder::new(&mut png);
    encoder
        .write_image(
            img.bytes.as_ref(),
            img.width as u32,
            img.height as u32,
            ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(png.bytes)
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
