//! Bard's one door to the system clipboard. Every action ends here: Bard
//! never types into another application, it only fills the clipboard and
//! says so.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use base64::Engine;
use tauri::AppHandle;

/// One clipboard handle for the life of the process.
///
/// On X11 (and XWayland) the clipboard is a conversation, not a buffer: the
/// owner has to stay alive to answer every later paste. A handle opened and
/// dropped per write hands its content to a clipboard manager if there is one
/// and loses it if there is not, so a copied slot could vanish the moment the
/// write returned. Keeping one handle keeps the server thread answering.
pub(crate) fn handle() -> Result<Handle, String> {
    let mut slot = CLIPBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if slot.is_none() {
        *slot = Some(arboard::Clipboard::new().map_err(|e| format!("clipboard open: {e}"))?);
    }
    Ok(Handle(slot))
}

static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

/// The open clipboard, held for as long as the caller reads or writes.
pub(crate) struct Handle(MutexGuard<'static, Option<arboard::Clipboard>>);

impl std::ops::Deref for Handle {
    type Target = arboard::Clipboard;
    fn deref(&self) -> &arboard::Clipboard {
        self.0.as_ref().expect("clipboard opened by handle()")
    }
}

impl std::ops::DerefMut for Handle {
    fn deref_mut(&mut self) -> &mut arboard::Clipboard {
        self.0.as_mut().expect("clipboard opened by handle()")
    }
}

/// Let go of the clipboard connection so the next [`handle`] opens a new one.
///
/// The one handle is a live connection with a thread behind it. If that
/// connection breaks, every later read fails and nothing says so; the watcher
/// calls this when a copy it was told about cannot be read, so that a broken
/// connection costs one copy rather than every copy until Bard is restarted.
pub(crate) fn reopen() {
    let mut slot = CLIPBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *slot = None;
}

pub fn set_text(app: &AppHandle, text: &str) -> Result<(), String> {
    // The clipboard is shared with every process on the desktop: another
    // program (a clipboard history service, a remote-desktop bridge, a
    // password manager clearing its copy) can hold it open or replace it in
    // the instant between Bard's write and its read-back. A failed
    // verification is retried with a fresh write a few times before it is
    // reported, so a passing collision does not surface as a failed copy,
    // while a clipboard that genuinely refuses the text still does.
    // Recorded before the write: the watcher can wake on this change before
    // the read-back returns, and it must already know the text is Bard's own.
    crate::monitor::note_own_text(text);
    let mut last = String::new();
    for attempt in 0..CLIPBOARD_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(30 * attempt as u64));
        }
        match write_and_verify_text(text) {
            Ok(()) => {
                crate::monitor::note_own_write(app);
                return Ok(());
            }
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// Put a list of files on the clipboard, as a file manager would.
pub fn set_files(app: &AppHandle, paths: &[String]) -> Result<(), String> {
    let existing: Vec<&String> = paths.iter().filter(|p| Path::new(p).exists()).collect();
    if existing.is_empty() {
        return Err("Those files are no longer where they were copied from.".into());
    }
    crate::monitor::note_own_text(
        &existing
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let mut clipboard = handle()?;
    clipboard
        .set()
        .file_list(&existing)
        .map_err(|e| format!("clipboard write: {e}"))?;
    drop(clipboard);
    crate::monitor::note_own_write(app);
    Ok(())
}

/// What is on the clipboard right now, for the preview shortcut. Read-only:
/// previewing never stores anything.
#[derive(serde::Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// "text", "image", "files" or "empty".
    pub kind: String,
    pub text: Option<String>,
    /// A small PNG data URL for an image.
    pub image: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub files: Vec<String>,
    /// Characters in a text payload, before the excerpt was cut.
    pub length: Option<usize>,
}

const PREVIEW_TEXT_CHARS: usize = 600;
const PREVIEW_IMAGE_EDGE: u32 = 320;

pub fn preview() -> Preview {
    let Ok(mut clipboard) = handle() else {
        return Preview {
            kind: "empty".into(),
            ..Preview::default()
        };
    };
    if let Ok(files) = clipboard.get().file_list() {
        if !files.is_empty() {
            return Preview {
                kind: "files".into(),
                files: files
                    .iter()
                    .map(|f| f.to_string_lossy().into_owned())
                    .collect(),
                ..Preview::default()
            };
        }
    }
    if let Ok(text) = clipboard.get_text() {
        if !text.trim().is_empty() {
            let length = text.chars().count();
            let excerpt: String = text.chars().take(PREVIEW_TEXT_CHARS).collect();
            return Preview {
                kind: "text".into(),
                text: Some(excerpt),
                length: Some(length),
                ..Preview::default()
            };
        }
    }
    if let Ok(image) = clipboard.get_image() {
        let (width, height) = (image.width as u32, image.height as u32);
        let pixels = (width as u64).saturating_mul(height as u64);
        if pixels > crate::monitor::MAX_CAPTURE_IMAGE_PIXELS {
            // A full RGBA decode of a very large clipboard image spikes memory
            // before the 320 px thumbnail exists, so the same pixel cap the
            // capture path applies is enforced here first. Failing silently
            // would look like an empty clipboard; say so plainly instead.
            crate::crash::record_event("capture:preview-image-too-large");
            return Preview {
                kind: "image".into(),
                width: Some(width),
                height: Some(height),
                ..Preview::default()
            };
        }
        let thumbnail = image::RgbaImage::from_raw(width, height, image.bytes.into_owned())
            .map(|rgba| {
                let scale = (PREVIEW_IMAGE_EDGE as f32 / width.max(height).max(1) as f32).min(1.0);
                let w = ((width as f32 * scale).round() as u32).max(1);
                let h = ((height as f32 * scale).round() as u32).max(1);
                image::imageops::thumbnail(&rgba, w, h)
            })
            .and_then(|small| {
                let mut png = Vec::new();
                image::DynamicImage::ImageRgba8(small)
                    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                    .ok()?;
                Some(format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(png)
                ))
            });
        return Preview {
            kind: "image".into(),
            image: thumbnail,
            width: Some(width),
            height: Some(height),
            ..Preview::default()
        };
    }
    Preview {
        kind: "empty".into(),
        ..Preview::default()
    }
}

const CLIPBOARD_ATTEMPTS: usize = 4;

fn write_and_verify_text(text: &str) -> Result<(), String> {
    let mut cb = handle()?;
    cb.set_text(text.to_string())
        .map_err(|e| format!("clipboard write: {e}"))?;
    match cb.get_text() {
        Ok(read) if read == text => Ok(()),
        Ok(_) => Err("The clipboard write could not be verified.".into()),
        Err(e) => Err(format!("clipboard read-back: {e}")),
    }
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
    let bytes = owned_image_bytes(path).ok_or("That image is no longer available.")?;
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let data = arboard::ImageData {
        width: w as usize,
        height: h as usize,
        bytes: rgba.into_raw().into(),
    };
    crate::monitor::note_own_image(&data.bytes);
    let mut cb = handle()?;
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
