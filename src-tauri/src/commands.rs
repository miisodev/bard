use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use image::GenericImageView;
use std::time::Duration;

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::db;

/// Shell state changed: tell the windows that read it.
///
/// `emit_to`, not `emit`. The panel is the window that reads `get_state`, and
/// settings is the only other one that does — its panes read the theme, the
/// background and the data settings out of that payload. The dictation overlay
/// and the drawing canvas never have, and every refresh used to be delivered to
/// all four webviews, each of which answered it with an appearance read of its
/// own: one copy cost five invokes across the process, four of them for the
/// eight appearance strings.
pub fn refresh(app: &AppHandle) {
    let _ = app.emit_to("main", "refresh", ());
    // The settings window is created on first use, so it may not exist yet.
    if app.get_webview_window("settings").is_some() {
        let _ = app.emit_to("settings", "refresh", ());
    }
}

/// The appearance changed somewhere every window can see it.
///
/// The theme, the background and Draw's own colours are one small record that
/// every window needs and only changes when someone changes it, so the change
/// is announced once with the record attached — no read per window, no read per
/// refresh, and nothing for a clipboard capture to wake up.
pub fn announce_appearance(app: &AppHandle) {
    let _ = app.emit("appearance", read_appearance(app));
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReport {
    pub path: String,
    pub contents: String,
}

#[tauri::command]
pub fn get_crash_report() -> Result<Option<CrashReport>, String> {
    crate::crash::read_latest_log(&crate::paths::logs_dir())
        .map(|latest| {
            latest.map(|log| CrashReport {
                path: log.path.to_string_lossy().to_string(),
                contents: log.contents,
            })
        })
        .map_err(|error| error.to_string())
}

/// Everything support needs, in one copyable block: versions, storage state,
/// the latest crash report and the tail of the lifecycle log. Local only —
/// it reads files and writes the clipboard; nothing leaves the machine.
#[tauri::command]
pub async fn copy_diagnostic_report(app: AppHandle) -> Result<(), String> {
    let report = diagnostic_report(&app);
    crate::clip::set_text(&app, &report)
}

fn diagnostic_report(app: &AppHandle) -> String {
    let logs = crate::paths::logs_dir();
    let mut out = String::from("bard diagnostics\n");
    out.push_str(&format!("app_version: {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("channel: {}\n", crate::packaging::channel()));
    out.push_str(&format!("os: {}\n", std::env::consts::OS));
    out.push_str(&format!("arch: {}\n", std::env::consts::ARCH));
    out.push_str(&format!("generated_unix_ms: {}\n", db::now_ms()));
    if let Some(notice) = db::storage_notice(app) {
        out.push_str(&format!("storage: {notice}\n"));
    }
    out.push_str("\n--- latest crash report ---\n");
    match crate::crash::read_latest_log(&logs) {
        Ok(Some(log)) => {
            out.push_str(&format!("path: {}\n", log.path.to_string_lossy()));
            out.push_str(&log.contents);
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        Ok(None) => out.push_str("(none recorded)\n"),
        Err(error) => out.push_str(&format!("(unreadable: {error})\n")),
    }
    out.push_str("\n--- lifecycle (recent) ---\n");
    let tail = crate::crash::lifecycle_tail(&logs, 60);
    if tail.is_empty() {
        out.push_str("(none recorded)\n");
    } else {
        out.push_str(&tail);
    }
    out
}

/// The updater refuses the Store and Linux channels itself and verifies the
/// signed manifest.
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
) -> Result<Option<crate::updater::UpdateInfo>, String> {
    crate::updater::check(&app).await
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    crate::updater::install(&app).await
}

#[tauri::command]
pub async fn get_setting_str(app: AppHandle, key: String) -> String {
    const PUBLIC_SETTING_KEYS: &[&str] = &[
        "selected_microphone",
        "theme",
        "clipboard_retention_days",
        "dictation_retention_days",
        "main_dock",
    ];
    if !PUBLIC_SETTING_KEYS.contains(&key.as_str()) {
        return String::new();
    }
    db::get_setting(&app, &key).unwrap_or_default()
}

pub fn dirs_documents() -> String {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .map(|p| format!("{p}\\Documents"))
            .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string())
    }
    #[cfg(not(windows))]
    {
        // `XDG_DOCUMENTS_DIR` is what the xdg-user-dirs tools export into the
        // session; a relative value is refused for the same reason the XDG
        // base directories refuse one.
        let documents = std::env::var_os("XDG_DOCUMENTS_DIR")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_absolute());
        documents
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| std::path::PathBuf::from(home).join("Documents"))
            })
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| std::env::temp_dir().to_string_lossy().to_string())
    }
}

#[tauri::command]
pub fn update_item(app: AppHandle, id: i64, content: String) -> Result<(), String> {
    if content.trim().is_empty() {
        return Err("Add some text, or cancel to keep the original note.".into());
    }
    db::update_item(&app, id, &content)?;
    refresh(&app);
    Ok(())
}

/// The largest file intake will read at all. This is a sanity bound on the
/// copy, not the text limit below.
const MAX_INGEST_FILE_BYTES: u64 = 25 * 1024 * 1024;

/// Extensions stored as an image item, by copying the file into Bard's images
/// folder.
const INGEST_IMAGE_EXTENSIONS: [&str; 9] = [
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tiff", "avif",
];

/// Extensions read as text and stored as a text item.
const INGEST_TEXT_EXTENSIONS: [&str; 13] = [
    "txt", "md", "markdown", "json", "csv", "log", "rs", "ts", "tsx", "js", "jsx", "html", "css",
];

/// What file intake will do with one file, decided from its extension and size
/// before anything is read or copied.
#[derive(Debug, PartialEq, Eq)]
enum IngestKind {
    Image,
    Text,
    /// A text file over the capture cap. Refused with a reason, never
    /// truncated and never stored whole.
    TextTooLarge,
    /// Not an image or a text file: its name is stored as a note.
    FileName,
    /// Not something Bard stores.
    Unsupported,
}

fn ingest_kind(extension: &str, bytes: u64) -> IngestKind {
    if bytes > MAX_INGEST_FILE_BYTES {
        return IngestKind::Unsupported;
    }
    if INGEST_IMAGE_EXTENSIONS.contains(&extension) {
        return IngestKind::Image;
    }
    if INGEST_TEXT_EXTENSIONS.contains(&extension) {
        // The same cap the clipboard watcher applies. Without it a 25 MB log
        // opened with Bard became a 25 MB row in every state payload, while
        // the same text copied from another app was refused at 256 KB.
        return if bytes > crate::monitor::MAX_CAPTURE_TEXT_BYTES as u64 {
            IngestKind::TextTooLarge
        } else {
            IngestKind::Text
        };
    }
    IngestKind::FileName
}

/// Paths are compared the way the filesystem under them compares them: exactly
/// on Linux, where two names that differ only in case are two files, and
/// without case on Windows and macOS, where they are one. Lowercasing every
/// path on every platform used to make one of two case-distinct files vanish
/// from an import on Linux.
fn path_key(path: &Path) -> String {
    let key = path.to_string_lossy().into_owned();
    if cfg!(any(windows, target_os = "macos")) {
        key.to_lowercase()
    } else {
        key
    }
}

/// Native-only file intake used by the Windows Explorer verb and first launch.
/// It is deliberately not a Tauri command: a renderer can read a File object
/// it owns, but it cannot ask Rust to copy an arbitrary path by name.
pub fn ingest_paths(app: &AppHandle, paths: Vec<String>) -> usize {
    let images = app.state::<ImagesDir>().0.clone();
    // The duplicate check reads the library once. It used to call `list_items`
    // inside the loop, so importing n text files scanned the whole table n
    // times. New text is added to the set as it lands, so two files with the
    // same contents in one import still produce one row.
    let mut known_text: std::collections::HashSet<String> = db::list_items(app)
        .into_iter()
        .filter(|item| item.origin == "clipboard" && item.kind == "text")
        .map(|item| item.content)
        .collect();
    let mut added = 0;
    let mut seen = std::collections::HashSet::new();
    for p in paths {
        let path = Path::new(&p);
        if !seen.insert(path_key(path)) {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let Ok(metadata) = std::fs::metadata(path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        match ingest_kind(&ext, metadata.len()) {
            IngestKind::Unsupported => {}
            IngestKind::TextTooLarge => {
                // Say so rather than truncating or silently dropping it. This
                // is the clipboard path's message, because it is the same cap.
                crate::crash::record_event("intake:text-too-large");
                crate::feedback::notify(
                    app,
                    crate::feedback::Note::error(format!(
                        "Too large to keep ({} KB; the limit is {} KB)",
                        metadata.len().div_ceil(1024),
                        crate::monitor::MAX_CAPTURE_TEXT_BYTES / 1024
                    ))
                    .with_detail(source_name(&p)),
                );
            }
            IngestKind::Image => {
                let dest = unique_image_path(&images, &ext);
                if std::fs::copy(path, &dest).is_ok() {
                    let id = db::insert_item_with_origin(
                        app,
                        "",
                        "image",
                        Some(&dest.to_string_lossy()),
                        "clipboard",
                    );
                    if id == db::INSERT_FAILED {
                        // The copy landed but the row did not, so nothing references
                        // this file and no sweep will ever collect it. Undo the copy
                        // rather than leak it, and do not count it as added — the
                        // Explorer verb and the first-launch intake both report this
                        // number back to the person.
                        let _ = std::fs::remove_file(&dest);
                        continue;
                    }
                    db::set_active_item(app, Some(id));
                    added += 1;
                }
            }
            IngestKind::Text => {
                let Ok(text) = std::fs::read_to_string(path) else {
                    continue;
                };
                let text = text.trim();
                // A blank file has nothing to keep, and an empty row would sit
                // in every state payload saying nothing.
                if text.is_empty() || known_text.contains(text) {
                    continue;
                }
                let id = db::insert_item_with_origin(app, text, "text", None, "clipboard");
                // `insert_item_with_origin` returns a sentinel rather than
                // aborting when the write cannot happen. Counting a write that did
                // not occur is how "Added 3 files" becomes a lie on a full disk.
                if id == db::INSERT_FAILED {
                    continue;
                }
                known_text.insert(text.to_string());
                db::set_active_item(app, Some(id));
                added += 1;
            }
            IngestKind::FileName => {
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let name = name.trim();
                if name.is_empty() {
                    continue;
                }
                let id = db::insert_item_with_origin(app, name, "text", None, "clipboard");
                if id == db::INSERT_FAILED {
                    continue;
                }
                db::set_active_item(app, Some(id));
                added += 1;
            }
        }
    }
    refresh(app);
    added
}

fn decode_image_payload(data_base64: &str, ext: &str) -> Result<(Vec<u8>, String), String> {
    if data_base64.len() > 35 * 1024 * 1024 {
        return Err("The image is too large.".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64)
        .map_err(|_| "The image data could not be decoded.".to_string())?;
    if bytes.is_empty() || bytes.len() > 25 * 1024 * 1024 {
        return Err("The image is empty or too large.".into());
    }
    let format = image::guess_format(&bytes)
        .map_err(|_| "That image format is not supported.".to_string())?;
    let decoded = image::load_from_memory_with_format(&bytes, format)
        .map_err(|_| "The image could not be decoded.".to_string())?;
    let (width, height) = decoded.dimensions();
    if width == 0 || height == 0 || width as u64 * height as u64 > 100_000_000 {
        return Err("The image dimensions are too large.".into());
    }
    let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif"
    ) {
        return Err("That image format is not supported.".into());
    }
    Ok((bytes, ext))
}

#[tauri::command]
pub async fn export_markdown(app: AppHandle) -> Result<String, String> {
    // The export is the portability promise, so it reads every row the person
    // kept rather than the windowed timeline the shelf paints from. A failed
    // read is an error: a shorter export that looks complete is exactly the
    // data loss the promise rules out.
    let items = db::all_items(&app)?;
    let sections = db::list_sections(&app);
    let mut attachments: std::collections::HashMap<i64, Vec<String>> =
        std::collections::HashMap::new();
    for attachment in db::list_all_attachments(&app) {
        attachments
            .entry(attachment.item_id)
            .or_default()
            .push(attachment.path.clone());
    }
    // The export directory is a folder the person chose through the native
    // picker, so it is written into as-is and never created. The old code called
    // `create_dir_all` unconditionally, which meant a stale or mistyped setting
    // silently produced a new folder the user never asked for — and a `%TEMP%`
    // fallback meant a failed export could scatter a full copy of their capture
    // history into a temporary directory. Failing loudly is better than either.
    let fallback = dirs_documents();
    let dir = db::get_setting(&app, "export_dir")
        .filter(|d| !d.is_empty() && std::path::Path::new(d).is_dir())
        .unwrap_or(fallback);
    let dir = std::path::PathBuf::from(dir);
    if !dir.is_dir() {
        return Err("Choose a folder to export to in Data › Export folder.".into());
    }
    export_markdown_into(&dir, &sections, &items, &attachments)?;
    Ok(dir.join("bard-export.md").to_string_lossy().to_string())
}

/// Write `bard-export.md` and every file it references into `dir`.
///
/// The previous export wrote one Markdown file whose image links pointed into
/// an `images/` folder it never created and dropped attachments entirely, so an
/// export made before a reinstall opened with dead links and missing files.
/// The export is the product's only portability promise: it now carries the
/// files it names, and says plainly when one cannot be read.
fn export_markdown_into(
    dir: &Path,
    sections: &[db::Section],
    items: &[db::Item],
    attachments: &std::collections::HashMap<i64, Vec<String>>,
) -> Result<(), String> {
    let mut used = std::collections::HashSet::new();
    let mut out = String::from(
        "# Bard export\n\nImages are written into `images/` and attached files into `files/`, beside this file.\n",
    );
    // An item belongs to a section only when that section was read. Everything
    // else is written unfiled instead of being dropped: a failed section read
    // used to make every sectioned item vanish from the export, which is the
    // one thing an export must never do.
    let known: std::collections::HashSet<i64> = sections.iter().map(|section| section.id).collect();
    let mut unfiled: Vec<&db::Item> = Vec::new();
    let mut by_section: std::collections::HashMap<i64, Vec<&db::Item>> =
        std::collections::HashMap::new();
    for item in items {
        match item.section_id.filter(|id| known.contains(id)) {
            Some(section) => by_section.entry(section).or_default().push(item),
            None => unfiled.push(item),
        }
    }
    for item in &unfiled {
        out.push_str(&format!("- {}\n", item.content.replace('\n', "\n  ")));
    }
    for section in sections {
        let Some(section_items) = by_section.get(&section.id) else {
            continue;
        };
        out.push_str(&format!("\n## {}\n\n", section.name));
        for item in section_items {
            if item.kind == "image" {
                match item
                    .image_path
                    .as_deref()
                    .and_then(|path| export_asset(path, dir, "images", &mut used))
                {
                    Some(link) => out.push_str(&format!("- ![image]({link})\n")),
                    None => out.push_str(&format!(
                        "- (image could not be exported: {})\n",
                        source_name(item.image_path.as_deref().unwrap_or_default())
                    )),
                }
            } else {
                out.push_str(&format!("- {}\n", item.content.replace('\n', "\n  ")));
            }
            for path in attachments.get(&item.id).into_iter().flatten() {
                match export_asset(path, dir, "files", &mut used) {
                    Some(link) => out.push_str(&format!(
                        "  - attachment: [{}]({link})\n",
                        source_name(path)
                    )),
                    None => out.push_str(&format!(
                        "  - attachment could not be exported: {}\n",
                        source_name(path)
                    )),
                }
            }
        }
    }
    std::fs::write(dir.join("bard-export.md"), out).map_err(|error| error.to_string())
}

/// Copy one referenced file into `dir/<folder>`, returning the relative link
/// the Markdown carries. `None` means the source could not be read; the caller
/// writes a truthful marker instead of a link that opens nothing.
fn export_asset(
    source: &str,
    dir: &Path,
    folder: &str,
    used: &mut std::collections::HashSet<String>,
) -> Option<String> {
    let source = Path::new(source);
    if !source.is_file() {
        return None;
    }
    let name = unique_export_name(&source_name(&source.to_string_lossy()), used);
    let target = dir.join(folder);
    std::fs::create_dir_all(&target).ok()?;
    std::fs::copy(source, target.join(&name)).ok()?;
    Some(format!("{folder}/{name}"))
}

/// The name a stored path will be exported as: its file name, or a stand-in
/// when the stored value has none.
fn source_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "file".into())
}

/// Keep the stored file name where possible and make it unique when two
/// different files share one. Windows compares names without case, so two
/// files that differ only in case are one name here too.
fn unique_export_name(name: &str, used: &mut std::collections::HashSet<String>) -> String {
    if used.insert(name.to_lowercase()) {
        return name.to_string();
    }
    let path = Path::new(name);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let extension = path.extension().and_then(|e| e.to_str());
    for index in 2..1000 {
        let candidate = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        if used.insert(candidate.to_lowercase()) {
            return candidate;
        }
    }
    let fallback = format!("file-{}", used.len() + 1);
    used.insert(fallback.to_lowercase());
    fallback
}

#[tauri::command]
pub fn set_export_dir(app: AppHandle, path: String) {
    // Only an existing directory is accepted. The frontend routes this through
    // the native folder picker, so the value is expected to be real; storing an
    // arbitrary string would let a compromised renderer point the export at any
    // writable location.
    if !path.is_empty() && std::path::Path::new(&path).is_dir() {
        db::set_setting(&app, "export_dir", &path);
    }
}

#[tauri::command]
pub async fn get_export_dir(app: AppHandle) -> String {
    db::get_setting(&app, "export_dir")
        .filter(|d| !d.is_empty())
        .unwrap_or_else(dirs_documents)
}

#[tauri::command]
pub fn get_microphones() -> Vec<String> {
    crate::dictation::list_input_devices()
}

/// The choices that shape the finished transcript.
///
/// They are one command rather than three settings the renderer can poke at
/// freely: these are the only three knobs the transcription pipeline reads,
/// and keeping them together means the webview cannot write a setting the
/// pipeline never looks at.
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptOptions {
    /// End the transcript with a space, so it never fuses with what is typed
    /// next.
    pub trailing_space: bool,
    /// Take the filler words out.
    pub fillers: bool,
    /// How long a silence has to last before the take ends on its own. Zero
    /// means never: the take ends when the person stops it.
    pub silence_ms: u64,
}

/// The silence window the person may choose, in milliseconds. The first entry
/// is the default; the last means the take never ends on silence.
pub const SILENCE_CHOICES_MS: [u64; 4] = [1600, 1200, 2400, 0];

#[tauri::command]
pub fn get_dictation_options(app: AppHandle) -> TranscriptOptions {
    let options = crate::dictation::transcript_options(&app);
    TranscriptOptions {
        trailing_space: options.trailing_space,
        fillers: options.fillers,
        silence_ms: crate::dictation::silence_auto_stop_ms(&app),
    }
}

#[tauri::command]
pub fn set_dictation_options(app: AppHandle, options: TranscriptOptions) -> Result<(), String> {
    if !SILENCE_CHOICES_MS.contains(&options.silence_ms) {
        return Err("That is not a silence length Bard can use.".into());
    }
    crate::db::set_setting_checked(
        &app,
        "dictation_trailing_space",
        if options.trailing_space { "1" } else { "0" },
    )?;
    crate::db::set_setting_checked(
        &app,
        "dictation_fillers",
        if options.fillers { "1" } else { "0" },
    )?;
    crate::db::set_setting_checked(
        &app,
        "dictation_silence_ms",
        &options.silence_ms.to_string(),
    )?;
    Ok(())
}

#[tauri::command]
pub fn set_microphone(app: AppHandle, name: String) {
    db::set_setting(&app, "selected_microphone", &name);
}

/// The panel's whole world, re-read on every refresh and after every capture.
///
/// `async` for the threading, not for concurrency: Tauri runs a synchronous
/// command inline on the Windows main thread, and this one reads the whole
/// timeline, every attachment, the settings, and enumerates microphones — all on the thread every
/// window shares, where a slow disk or vault stalls painting and input. An
/// `async fn` with no `.await` is spawned by Tauri instead, and the compiler
/// still refuses to let the `Db` mutex guard cross an await. The same reasoning
/// applies to every other `async` command in this file that only reads files,
/// the database or the vault; window-touching commands stay synchronous on
/// purpose.
#[tauri::command]
pub async fn get_state(app: AppHandle) -> db::AppState {
    db::get_state(&app)
}

#[tauri::command]
pub fn add_item(app: AppHandle, content: String) -> Result<(), String> {
    if content.trim().is_empty() {
        return Err("Add some text before saving a note.".into());
    }
    let id = db::insert_item(&app, &content, "text", None);
    // Setting the active item to the sentinel would clear it, so a failed write
    // would also silently deselect whatever the person had been working on.
    if id == db::INSERT_FAILED {
        return Err("Bard could not save the note. Your text is still here; check that the drive has free space and try again.".into());
    }
    db::set_active_item(&app, Some(id));
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub async fn add_image(app: AppHandle, data_base64: String, ext: String) -> Result<i64, String> {
    let (bytes, ext) = decode_image_payload(&data_base64, &ext)?;
    let images = app.state::<ImagesDir>().0.clone();
    let path = unique_image_path(&images, &ext);
    std::fs::write(&path, bytes).map_err(|error| format!("Could not save the image: {error}"))?;
    // Returned so a caller that needs the item afterwards — the drawing
    // window's "Copy and Save" — does not have to guess which row it made.
    let id = db::insert_item(&app, "", "image", Some(&path.to_string_lossy()));
    if id == db::INSERT_FAILED {
        // Same as `attach_image`: the PNG is on disk with no row. Undo the write
        // rather than leave an orphan nothing will ever sweep.
        let _ = std::fs::remove_file(&path);
        return Err("Bard could not save this image. Check that the drive has free space.".into());
    }
    db::set_active_item(&app, Some(id));
    refresh(&app);
    Ok(id)
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    show_secondary(&app, "settings", false);
}

/// The Draw or Settings window, created the first time it is asked for.
///
/// Both used to be declared in `tauri.conf.json`, so every launch built two
/// extra WebView2 renderers — each with its own bundle, its own process and its
/// own working set — that nothing could reach before a click. That is most of
/// the fixed cost of a launch, and it is paid even by an autostart-at-login run
/// whose whole point is to be invisible and cheap. The panel and the dictation
/// overlay stay declared: the panel is what a launch shows, and the overlay has
/// to be ready within a frame of the first Ctrl+Ctrl.
///
/// The two created here keep the label, size, decorations and behaviour the
/// config gave them — the label is what keeps their capabilities
/// (`capabilities/draw.json`, `capabilities/settings.json`) and the
/// `on_window_event` handling in `main.rs` working unchanged.
fn companion(app: &AppHandle, label: &str) -> Option<tauri::WebviewWindow> {
    if let Some(window) = app.get_webview_window(label) {
        return Some(window);
    }
    let (url, title, min) = match label {
        "draw" => ("draw.html", "Draw", (480.0, 400.0)),
        "settings" => ("settings.html", "Settings", (600.0, 440.0)),
        "guide" => ("guide.html", "Shortcuts", (320.0, 320.0)),
        _ => return None,
    };
    let size = companion_size(label);
    let built = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(size.0, size.1)
        .min_inner_size(min.0, min.1)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        // Windows frames every Bard window the same way: its own corners and
        // shadow. Elsewhere the page draws the edge.
        .shadow(cfg!(windows))
        .always_on_top(false)
        .skip_taskbar(false)
        .visible(false)
        .on_page_load(|window, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                mark_companion_loaded(window.app_handle(), window.label());
            }
        })
        .build()
        .ok()?;
    Some(built)
}

/// The first size of a companion window, in logical pixels.
fn companion_size(label: &str) -> (f64, f64) {
    if label == "guide" {
        (crate::panel::GUIDE_WIDTH, crate::panel::GUIDE_HEIGHT)
    } else {
        (
            crate::panel::COMPANION_WIDTH,
            crate::panel::COMPANION_HEIGHT,
        )
    }
}

/// WebView2 creation must not run inside a synchronous IPC/menu callback.
/// Its initialization needs that same UI event loop and otherwise deadlocks.
/// Serialize first creation on a worker, then return display/focus operations
/// to the UI thread. Every entry point (button, tray and gesture) uses this.
fn with_companion(
    app: &AppHandle,
    label: &str,
    action: impl FnOnce(&AppHandle, tauri::WebviewWindow) + Send + 'static,
) {
    let label = match label {
        "draw" => "draw",
        "settings" => "settings",
        "guide" => "guide",
        _ => return,
    };
    let handle = app.clone();
    if let Some(window) = app.get_webview_window(label) {
        let _ = app.run_on_main_thread(move || action(&handle, window));
        return;
    }
    tauri::async_runtime::spawn_blocking(move || {
        static CREATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = CREATION.lock().unwrap_or_else(|error| error.into_inner());
        let window = companion(&handle, label);
        drop(guard);
        let Some(window) = window else {
            crate::crash::record_event("window:companion-create-failed");
            let _ = handle.emit_to(
                "main",
                "capture-failed",
                "Bard could not open that window. Try again.",
            );
            return;
        };
        let task_handle = handle.clone();
        let _ = handle.run_on_main_thread(move || action(&task_handle, window));
    });
}

/// Whether the drawing window has finished loading its bundle in this process.
///
/// The seed below arrives as an event, and an event delivered before the page's
/// listeners exist is an event nobody hears. While the window was created at
/// launch this could not happen — it had been loading since startup — so the
/// seed now waits for the load, which is the only thing that changed.
static DRAW_LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn mark_companion_loaded(app: &AppHandle, label: &str) {
    if label != "draw" {
        return;
    }
    DRAW_LOADED.store(true, Ordering::SeqCst);
    flush_draw_seed(app);
}

/// The item the drawing window has been asked to annotate but has not been told
/// about yet. Set when the request is made, cleared when the window hears it.
static PENDING_DRAW_SEED: std::sync::Mutex<Option<i64>> = std::sync::Mutex::new(None);

/// Open Draw seeded with one image, from the panel's card menu.
fn seed_draw(app: &AppHandle, item_id: i64) {
    {
        let mut guard = PENDING_DRAW_SEED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(item_id);
    }
    // Already loaded: the listener is there, so it can be told now.
    if DRAW_LOADED.load(Ordering::SeqCst) {
        flush_draw_seed(app);
    }
}

fn flush_draw_seed(app: &AppHandle) {
    let item_id = {
        let mut guard = PENDING_DRAW_SEED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.take()
    };
    let Some(item_id) = item_id else {
        return;
    };
    let _ = app.emit_to(
        "draw",
        "draw-seed",
        serde_json::json!({ "itemId": item_id }),
    );
}

/// Show Settings or Draw where it was last left (centred the first time).
/// Both are ordinary windows now: movable, resizable, on the taskbar, and
/// independent of the panel and of each other.
pub fn show_secondary(app: &AppHandle, label: &str, _center: bool) {
    with_companion(app, label, |app, win| show_secondary_window(app, &win));
}

fn show_secondary_window(app: &AppHandle, win: &tauri::WebviewWindow) {
    if win.is_fullscreen().unwrap_or(false) {
        let _ = win.set_fullscreen(false);
    }
    let _ = win.unminimize();
    if !win.is_visible().unwrap_or(false) {
        let size = companion_size(win.label());
        crate::geometry::restore(app, win, size.0, size.1);
    }
    crate::panel::mark_shown();
    let _ = win.show();
    crate::front::bring(win);
    crate::panel::companions_changed(app);
}

/// The panel's Draw and Settings buttons are toggles: the second click puts
/// the companion away again, and focus returns to the panel.
#[tauri::command]
pub fn toggle_companion(app: AppHandle, label: String) -> Result<(), String> {
    use tauri::Manager;
    if !matches!(label.as_str(), "draw" | "settings") {
        return Err("Bard has no such window.".into());
    }
    // An open companion just goes away.
    if let Some(win) = app.get_webview_window(&label) {
        if win.is_visible().unwrap_or(false) && !win.is_minimized().unwrap_or(false) {
            crate::panel::hide_companion(&app, &win);
            crate::panel::focus_panel(&app);
            return Ok(());
        }
    }
    show_secondary(&app, &label, false);
    Ok(())
}

#[tauri::command]
pub fn set_sort(app: AppHandle, order: String) -> Result<(), String> {
    if !db::SORT_ORDERS.contains(&order.as_str()) {
        return Err("Bard has no such order.".into());
    }
    db::set_setting_checked(&app, "sort_order", &order)?;
    refresh(&app);
    Ok(())
}

fn toggle_secondary_alone(app: &AppHandle, label: &str) {
    with_companion(app, label, |app, win| {
        if win.is_visible().unwrap_or(false) && win.is_focused().unwrap_or(false) {
            crate::panel::hide_companion(app, &win);
            return;
        }
        show_secondary_window(app, &win);
    });
}

/// Left Alt × 2 opens Draw as a single window. It runs from the raw-input
/// worker, so every native window operation is queued onto the main thread.
pub fn draw_shortcut(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        toggle_secondary_alone(&handle, "draw");
    });
}

/// Right Alt × 2 opens Settings as a single window, matching the tray action
/// without bringing the panel back beside it.
pub fn settings_shortcut(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || toggle_secondary_alone(&handle, "settings"));
}

/// The shortcut guide, from its chord, the tray or the main window.
pub fn guide_shortcut(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || toggle_secondary_alone(&handle, "guide"));
}

#[tauri::command]
pub fn open_guide(app: AppHandle) {
    show_secondary(&app, "guide", false);
}

#[tauri::command]
pub fn open_draw(app: AppHandle) {
    show_secondary(&app, "draw", true);
}

/// Write the sketch as an SVG to wherever the person says. The suggested name
/// is only an offer to the platform's own Save dialog, sanitised so a name
/// from the canvas can never smuggle a path; `None` means the dialog was
/// closed, which is not a failure.
#[tauri::command]
pub async fn save_svg(
    app: AppHandle,
    svg: String,
    suggested_name: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    crate::panel::hold(true);
    let picked = app
        .dialog()
        .file()
        .set_title("Save sketch")
        .add_filter("SVG image", &["svg"])
        .set_file_name(svg_file_name(&suggested_name))
        .blocking_save_file();
    crate::panel::hold(false);
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|error| format!("Bard could not use that location: {error}"))?;
    std::fs::write(&path, svg.as_bytes())
        .map_err(|error| format!("Bard could not save the sketch: {error}"))?;
    Ok(Some(path.to_string_lossy().to_string()))
}

/// A safe name for the Save dialog: the sketch's own, shorn of path
/// separators and the characters Windows refuses in a name, with the `.svg`
/// the filter promises. The dialog still gets the last word — this only has
/// to be a good first offer.
fn svg_file_name(suggested: &str) -> String {
    let cleaned: String = suggested
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    let stem: String = cleaned.chars().take(80).collect();
    let stem = stem.trim_end_matches([' ', '.']);
    let stem = if stem.is_empty() { "Sketch" } else { stem };
    if stem.to_ascii_lowercase().ends_with(".svg") {
        stem.to_string()
    } else {
        format!("{stem}.svg")
    }
}

/// Open Draw with one of the panel's images as a starting point. The panel
/// calls this from an image card's menu; Draw then fetches the pixels itself
/// through `item_image_data`, so the image crosses the IPC once. The seed is
/// the item id and nothing else.
#[tauri::command]
pub fn open_draw_with_image(app: AppHandle, item_id: i64) -> Result<(), String> {
    show_secondary(&app, "draw", false);
    // The window may have just been created, in which case its bundle is still
    // loading and its listener does not exist yet: the seed is held until the
    // page says it is loaded.
    seed_draw(&app, item_id);
    Ok(())
}

/// The canvas's tools, the frontend's own set: Select, Pen, Line, Arrow,
/// Rectangle, Ellipse, Text.
const DRAW_TOOLS: &[&str] = &["select", "pen", "line", "arrow", "rect", "ellipse", "text"];

/// A fresh toolbar: pen, the page's own ink and a 4px stroke — Draw's own
/// defaults, so the first launch and a stored setting agree. The default
/// colour is the frontend's first swatch, whose value is the dark page's ink;
/// the canvas resolves it against whichever page is up.
const DRAW_DEFAULT_TOOL: &str = "pen";
const DRAW_DEFAULT_COLOR: &str = "#1d1d1f";
const DRAW_DEFAULT_WIDTH: i64 = 4;

/// Wider than any stroke the toolbar offers, so a bad caller is refused here
/// rather than painting a wall; 1 is the thinnest pen.
const DRAW_WIDTH_MIN: i64 = 1;
const DRAW_WIDTH_MAX: i64 = 64;

/// A stored tool, or the default when the row is missing or names a tool this
/// build has never heard of.
fn draw_tool_or_default(tool: Option<String>) -> String {
    tool.filter(|tool| DRAW_TOOLS.contains(&tool.as_str()))
        .unwrap_or_else(|| DRAW_DEFAULT_TOOL.into())
}

/// A stored stroke width, or the default when the row is missing or outside
/// the canvas's range.
fn draw_width_or_default(width: Option<i64>) -> i64 {
    width
        .filter(|width| (DRAW_WIDTH_MIN..=DRAW_WIDTH_MAX).contains(width))
        .unwrap_or(DRAW_DEFAULT_WIDTH)
}

/// Whether a pen colour can be stored. Deliberately lenient: the canvas offers
/// the theme's inks, its hues and a custom hex, and the value is a person's
/// choice, not a path or an identifier — bounded and printable is enough.
fn draw_color_ok(color: &str) -> bool {
    !color.is_empty() && color.len() <= 32 && !color.chars().any(char::is_control)
}

/// A stored pen colour, or the default.
fn draw_color_or_default(color: Option<String>) -> String {
    color
        .filter(|color| draw_color_ok(color))
        .unwrap_or_else(|| DRAW_DEFAULT_COLOR.into())
}

/// Save the toolbar's three choices. One write for all three, so a crash
/// cannot leave the tool from one session beside the colour from another, and
/// validated here so only values the canvas can draw with are stored.
#[tauri::command]
pub fn set_draw_prefs(
    app: AppHandle,
    tool: String,
    color: String,
    width: i64,
) -> Result<(), String> {
    if !DRAW_TOOLS.contains(&tool.as_str()) {
        return Err("Bard has no such drawing tool.".into());
    }
    if !(DRAW_WIDTH_MIN..=DRAW_WIDTH_MAX).contains(&width) {
        return Err(format!(
            "A drawing width is {} to {}.",
            DRAW_WIDTH_MIN, DRAW_WIDTH_MAX
        ));
    }
    if !draw_color_ok(&color) {
        return Err("That colour cannot be saved for the pen.".into());
    }
    db::set_settings_checked(
        &app,
        &[
            ("draw_tool", tool.as_str()),
            ("draw_color", color.as_str()),
            ("draw_width", &width.to_string()),
        ],
    )
}

#[tauri::command]
pub fn delete_item(app: AppHandle, id: i64) -> Result<(), String> {
    db::delete_items_with_files(&app, &[id])?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn set_favorite(app: AppHandle, id: i64, favorite: bool) {
    db::set_favorite(&app, id, favorite);
    refresh(&app);
}

/// Pin an item to the top of the panel, where it is one of the three slots
/// Right Shift × 2 + 1/2/3 pastes. A fourth pin is refused, not rotated in:
/// silently unpinning something would move what a learned slot pastes.
#[tauri::command]
pub fn set_pinned(app: AppHandle, id: i64, pinned: bool) -> Result<(), String> {
    let result = db::set_pinned(&app, id, pinned).map_err(|error| error.to_string());
    refresh(&app);
    result
}

#[tauri::command]
pub async fn clear_history(app: AppHandle) -> Result<(), String> {
    db::clear_history(&app)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub async fn copy_item(app: AppHandle, id: i64) -> Result<(), String> {
    let item = db::item_by_id(&app, id).ok_or("That item no longer exists.")?;
    let detail = put_on_clipboard(&app, &item).inspect_err(|error| {
        crate::feedback::notify(
            &app,
            crate::feedback::Note::error("Couldn't copy").with_detail(error.clone()),
        );
    })?;
    db::set_active_item(&app, Some(id));
    refresh(&app);
    crate::feedback::notify(&app, crate::feedback::Note::selected(None, None, detail));
    Ok(())
}

/// Put one item on the clipboard and return a one-line description of it.
pub fn put_on_clipboard(app: &AppHandle, item: &db::Item) -> Result<String, String> {
    match item.kind.as_str() {
        "image" => {
            let path = item
                .image_path
                .as_deref()
                .ok_or_else(|| "That image is no longer available.".to_string())?;
            crate::clip::set_image(app, path)?;
            Ok("Image".into())
        }
        "file" => {
            let paths: Vec<String> = item
                .content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(String::from)
                .collect();
            crate::clip::set_files(app, &paths)?;
            Ok(crate::feedback::excerpt(&item.content, 64))
        }
        _ => {
            crate::clip::set_text(app, &item.content)?;
            Ok(crate::feedback::excerpt(&item.content, 64))
        }
    }
}

/// The three items the slot shortcuts reach: pins first, in pin order, then
/// the most recent.
fn slot_items(app: &AppHandle) -> Vec<db::Item> {
    // The one definition of a slot, shared with the state the panel reads, so
    // the number on a row and the shortcut that copies it cannot disagree.
    db::slot_items(app)
}

/// The slots as the pill lists them.
pub fn slot_rail(items: &[db::Item]) -> Vec<crate::feedback::Slot> {
    items
        .iter()
        .map(|item| crate::feedback::Slot {
            kind: item.kind.clone(),
            label: match item.kind.as_str() {
                "image" => "Image".to_string(),
                "file" => {
                    let mut names = item
                        .content
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(|path| {
                            path.rsplit(['/', '\\'])
                                .find(|part| !part.is_empty())
                                .unwrap_or(path)
                                .to_string()
                        });
                    let first = names.next().unwrap_or_default();
                    match names.count() {
                        0 => first,
                        more => format!("{first} +{more}"),
                    }
                }
                _ => crate::feedback::excerpt(&item.content, 48),
            },
            pinned: item.pinned_at.is_some(),
        })
        .collect()
}

/// Put slot `slot` (1–3) on the clipboard.
pub fn select_slot(app: &AppHandle, slot: u8) {
    let items = slot_items(app);
    let filled = items.len() as u8;
    let Some(item) = items.get(slot.saturating_sub(1) as usize) else {
        crate::feedback::notify(
            app,
            crate::feedback::Note::error(format!("Nothing in slot {slot} yet")),
        );
        return;
    };
    CYCLE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .replace((slot, std::time::Instant::now()));
    match put_on_clipboard(app, item) {
        Ok(detail) => {
            db::set_active_item(app, Some(item.id));
            refresh(app);
            crate::feedback::notify(
                app,
                crate::feedback::Note::selected(Some(slot), Some(filled), detail)
                    .with_rail(slot_rail(&items)),
            );
        }
        Err(error) => crate::feedback::notify(
            app,
            crate::feedback::Note::error("Couldn't copy").with_detail(error),
        ),
    }
}

/// The slot the cycle shortcut last chose, and when.
static CYCLE: std::sync::Mutex<Option<(u8, std::time::Instant)>> = std::sync::Mutex::new(None);
/// After this long, a cycle starts over from what is on the clipboard.
const CYCLE_RESET: Duration = Duration::from_secs(5);

/// Step to the next of the three slots. A fresh cycle starts after the item
/// that is already on the clipboard, so the first press always changes it.
pub fn cycle_slot(app: &AppHandle) {
    let items = slot_items(app);
    if items.is_empty() {
        crate::feedback::notify(
            app,
            crate::feedback::Note::error("Nothing to cycle through yet"),
        );
        return;
    }
    let count = items.len() as u8;
    let last = CYCLE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .filter(|(_, at)| at.elapsed() < CYCLE_RESET)
        .map(|(slot, _)| slot);
    let current = last.or_else(|| {
        let active = db::get_setting(app, "active_item_id").and_then(|v| v.parse::<i64>().ok());
        items
            .iter()
            .position(|item| Some(item.id) == active)
            .map(|index| index as u8 + 1)
    });
    let next = match current {
        Some(slot) => slot % count + 1,
        None => 1,
    };
    select_slot(app, next);
}

#[tauri::command]
pub fn set_dock(app: AppHandle, dock: String) -> Result<(), String> {
    let parsed = crate::panel::Dock::parse(&dock).ok_or("Bard docks left, right or floats.")?;
    db::set_setting_checked(&app, crate::panel::DOCK_SETTING, parsed.name())?;
    crate::panel::apply_dock(&app);
    refresh(&app);
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feedback {
    pill: bool,
    sound: bool,
}

#[tauri::command]
pub fn get_feedback() -> Feedback {
    Feedback {
        pill: crate::feedback::pill_on(),
        sound: crate::feedback::sound_on(),
    }
}

#[tauri::command]
pub fn get_stay_open(app: AppHandle) -> bool {
    crate::panel::stay_open(&app)
}

#[tauri::command]
pub fn set_stay_open(app: AppHandle, stay: bool) -> Result<bool, String> {
    db::set_setting_checked(
        &app,
        crate::panel::STAY_SETTING,
        if stay { "1" } else { "0" },
    )?;
    Ok(stay)
}

/// A window is about to open, or has just closed, one of the platform's own
/// dialogs. While it is open, losing focus to it is not leaving Bard.
#[tauri::command]
pub fn hold_windows(hold: bool) {
    crate::panel::hold(hold);
}

#[tauri::command]
pub fn set_feedback(app: AppHandle, pill: bool, sound: bool) -> Feedback {
    crate::feedback::set_channels(&app, pill, sound);
    if pill || sound {
        crate::feedback::notify(&app, crate::feedback::Note::info("info", "Feedback on"));
    }
    get_feedback()
}

#[tauri::command]
pub fn set_capture_paused(app: AppHandle, paused: bool) -> bool {
    crate::monitor::set_paused(&app, paused)
}

#[tauri::command]
pub fn notify_error(app: AppHandle, text: String) {
    crate::feedback::notify(&app, crate::feedback::Note::error(text));
}

#[tauri::command]
pub fn set_retention_days(app: AppHandle, origin: String, days: Option<i64>) -> Result<(), String> {
    db::set_retention_days(&app, &origin, days)?;
    // Applying a shorter lifespan should take effect now, not at the next
    // capture. The result is propagated so the caller learns if the pass failed
    // rather than assuming the change applied.
    db::cleanup_origin(&app, &origin)?;
    refresh(&app);
    Ok(())
}

/// Run one retention pass on demand, reporting what it removed.
///
/// This returned `()` and discarded every result, so the Data pane could neither
/// confirm success nor show a failure — the spec names both strings. It now
/// returns a count and propagates the first error, so the caller can say
/// `History pruned.` or `Couldn't prune history. Try again.` honestly.
///
/// It removes only what is past the lifespans the person chose, which is
/// exactly what the pane promises: "Removes everything past the lifespans
/// above, right now. It does not change them." The 5,000-item ceiling is a
/// bound on how much the shelf re-reads, not a lifespan, and applying it here
/// deleted automatic items of any age — including pinned ones — that no
/// setting said to remove.
#[tauri::command]
pub async fn prune_history_now(app: AppHandle) -> Result<usize, String> {
    let mut removed = 0usize;
    for origin in ["clipboard", "dictation"] {
        removed += db::prune_expired(&app, origin)?;
    }
    refresh(&app);
    Ok(removed)
}

#[tauri::command]
pub async fn get_retention_days(app: AppHandle, origin: String) -> Option<i64> {
    db::retention_days(&app, &origin)
}

#[tauri::command]
pub async fn delete_all_data(app: AppHandle) -> Result<(), String> {
    db::delete_all_data(&app)?;
    let _ = std::fs::remove_file(crate::guardian::marker_path());
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub async fn clear_captures(app: AppHandle) -> Result<(), String> {
    db::clear_captures(&app)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub async fn item_image_data(app: AppHandle, id: i64) -> Option<String> {
    let item = db::item_by_id(&app, id)?;
    if item.kind != "image" {
        return None;
    }
    crate::clip::image_data(&app, &item.image_path?)
}

/// An image card's picture: the file's asset URL, not its bytes.
///
/// The card is 236 × 148 CSS pixels. It used to receive the whole file as a
/// `data:` URL — base64 in Rust, a fresh copy over the IPC per card per mount,
/// then a full-size bitmap in the compositor, which is most of what paging a
/// shelf of screenshots cost. The URL is a few dozen bytes, the webview streams
/// the file itself, and Chromium caches it by URL across page flips.
///
/// `item_image_data` above stays exactly as it was: Draw seeds a picture into a
/// canvas and exports it with `toDataURL`, which an asset URL would taint.
#[tauri::command]
pub async fn item_cover_url(app: AppHandle, id: i64) -> Option<String> {
    let item = db::item_by_id(&app, id)?;
    if item.kind != "image" {
        return None;
    }
    crate::clip::image_url(&item.image_path?)
}

/// What every window needs to paint itself: the theme and the background.
/// Deliberately narrow — the overlay reads it too, and has no business seeing
/// the clipboard history that `get_state` carries.
///
/// It travels two ways: read once per window on mount, and pushed to every
/// window as the `appearance` event when one of them changes it. `Clone` is for
/// the second; the read is a handful of settings and a couple of stat calls.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    theme: String,
    background: String,
    background_color: String,
    draw_background: String,
    draw_background_color: String,
    draw_tool: String,
    draw_color: String,
    draw_width: i64,
}

#[tauri::command]
pub async fn get_appearance(app: AppHandle) -> Appearance {
    read_appearance(&app)
}

fn read_appearance(app: &AppHandle) -> Appearance {
    if !db::is_ready(app) {
        return Appearance {
            theme: "system".into(),
            background: "theme".into(),
            background_color: String::new(),
            draw_background: "theme".into(),
            draw_background_color: String::new(),
            draw_tool: DRAW_DEFAULT_TOOL.into(),
            draw_color: DRAW_DEFAULT_COLOR.into(),
            draw_width: DRAW_DEFAULT_WIDTH,
        };
    }
    Appearance {
        theme: db::get_setting(app, "theme").unwrap_or_else(|| "system".into()),
        background: db::get_setting(app, "background").unwrap_or_else(|| "theme".into()),
        background_color: db::get_setting(app, "background_color").unwrap_or_default(),
        draw_background: db::get_setting(app, "draw_background").unwrap_or_else(|| "theme".into()),
        draw_background_color: db::get_setting(app, "draw_background_color").unwrap_or_default(),
        draw_tool: draw_tool_or_default(db::get_setting(app, "draw_tool")),
        draw_color: draw_color_or_default(db::get_setting(app, "draw_color")),
        draw_width: draw_width_or_default(
            db::get_setting(app, "draw_width").and_then(|value| value.parse().ok()),
        ),
    }
}

fn valid_hex(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Draw's canvas: "theme" follows Light/Dark, "light" and "dark" pin one,
/// "custom" takes a `#rrggbb` colour. Exports carry the same background.
#[tauri::command]
pub fn set_draw_background(
    app: AppHandle,
    mode: String,
    color: Option<String>,
) -> Result<(), String> {
    if !matches!(mode.as_str(), "theme" | "light" | "dark" | "custom") {
        return Err("Unknown canvas.".into());
    }
    if let Some(color) = color {
        if !valid_hex(&color) {
            return Err("A custom canvas needs a colour like #ffffff.".into());
        }
        db::set_setting(&app, "draw_background_color", &color.to_ascii_lowercase());
    }
    if mode == "custom"
        && db::get_setting(&app, "draw_background_color")
            .unwrap_or_default()
            .is_empty()
    {
        return Err("Choose a colour for the custom canvas.".into());
    }
    db::set_setting(&app, "draw_background", &mode);
    refresh(&app);
    announce_appearance(&app);
    Ok(())
}

/// The panel's background: "theme" follows Light/Dark, "light" and "dark" pin
/// one, "custom" takes a `#rrggbb` colour.
#[tauri::command]
pub fn set_background(app: AppHandle, mode: String, color: Option<String>) -> Result<(), String> {
    if !matches!(mode.as_str(), "theme" | "light" | "dark" | "custom") {
        return Err("Unknown background.".into());
    }
    if let Some(color) = color {
        let valid = color.len() == 7
            && color.starts_with('#')
            && color[1..].chars().all(|c| c.is_ascii_hexdigit());
        if !valid {
            return Err("A custom background needs a colour like #1c1c1f.".into());
        }
        db::set_setting(&app, "background_color", &color.to_ascii_lowercase());
    }
    if mode == "custom"
        && db::get_setting(&app, "background_color")
            .unwrap_or_default()
            .is_empty()
    {
        return Err("Choose a colour for the custom background.".into());
    }
    db::set_setting(&app, "background", &mode);
    refresh(&app);
    announce_appearance(&app);
    Ok(())
}

#[tauri::command]
pub fn set_theme(app: AppHandle, theme: String) {
    db::set_setting(&app, "theme", &theme);
    // Every window paints from the theme, so the change is pushed with the
    // record attached — without it only the window that made the change would
    // repaint, which is exactly how the panel stayed light while settings went
    // dark. `refresh` still follows because the settings pane's own controls
    // read `state.theme`, and a stale radio button is the same defect wearing a
    // different hat.
    announce_appearance(&app);
    refresh(&app);
}

#[tauri::command]
pub fn set_onboarding_completed(app: AppHandle, done: bool) -> Result<(), String> {
    db::set_setting_checked(&app, "onboarding_completed", if done { "1" } else { "0" })?;
    crate::panel::show_now(&app);
    // On Wayland, a person who skipped the desktop's shortcut request during
    // setup is asked once now; one who declined it is not asked again.
    #[cfg(target_os = "linux")]
    if done
        && crate::desktop::is_wayland()
        && !crate::linux_portal::shortcuts_ready()
        && db::get_setting(&app, crate::linux_portal::DECLINED_SETTING).as_deref() != Some("1")
    {
        let portal_app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::linux_portal::configure_shortcuts(portal_app).await;
        });
    }
    Ok(())
}

#[tauri::command]
pub async fn configure_desktop(app: AppHandle, feature: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if !crate::desktop::is_wayland() {
            return Err("Desktop integration is already available in this session.".into());
        }
        match feature.as_str() {
            "shortcuts" => crate::linux_portal::configure_shortcuts(app).await,
            "edit-shortcuts" => crate::linux_portal::edit_shortcuts(app).await,
            _ => Err("That desktop integration is not available.".into()),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app, feature);
        Err("Desktop integration is already available on Windows.".into())
    }
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    if crate::packaging::is_packaged() {
        return Err("Windows manages how the Store copy of Bard starts.".into());
    }
    #[cfg(target_os = "linux")]
    if crate::packaging::channel() == "flatpak" {
        crate::linux_portal::set_autostart(enabled).await?;
        db::set_setting(&app, "autostart_enabled", if enabled { "1" } else { "0" });
        return Ok(());
    }
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable().map_err(|e| e.to_string())?;
    } else {
        // Turning off a login item that was never registered is already the
        // state asked for. On Windows both the check and the removal fail with
        // "cannot find the file specified" when the per-user Run key or its
        // value does not exist, which left a fresh install unable to finish
        // setup with Start at login left off. "Not found" means not
        // registered; any other failure, or an entry still present, is real.
        let registered = match launcher.is_enabled() {
            Ok(registered) => registered,
            Err(error) if login_item_absent(&error.to_string()) => false,
            Err(_) => true,
        };
        if registered {
            if let Err(error) = launcher.disable() {
                let message = error.to_string();
                if !login_item_absent(&message) {
                    return Err(message);
                }
            }
        }
    }
    db::set_setting(&app, "autostart_enabled", if enabled { "1" } else { "0" });
    Ok(())
}

/// Whether an autostart error only says the login item is not there: the OS
/// "not found" error (ERROR_FILE_NOT_FOUND / ENOENT), which every platform's
/// auto-launch backend surfaces as "os error 2".
fn login_item_absent(message: &str) -> bool {
    message.contains("(os error 2)")
}

/// Opens a miiso.dev page — terms, privacy, support — in the default browser. The webview has no opener of its own, so a
/// plain link went nowhere; only Bard's own site is allowed through.
#[tauri::command]
pub fn open_link(url: String) -> Result<(), String> {
    // `&` and `%` are gone from the allowlist. This is not command injection —
    // `Command` does not go through a shell and the scheme is pinned — but
    // whether the Windows opener splits on either character inside a single
    // argument is a behaviour of Explorer this project should not depend on.
    // The two real call sites pass fixed `https://miiso.dev/...` paths, so a
    // tight allowlist costs nothing.
    //
    // `#` stays: the Support link and the Refine pane's "Report it" both end in
    // the site's `#support` fragment, and rejecting it made both links silently
    // do nothing. A fragment is never sent to a server, and the host is already
    // pinned to miiso.dev.
    let allowed = (url.starts_with("https://miiso.dev/")
        || url.starts_with("https://github.com/miisodev/"))
        && url.len() <= 256
        && url
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/:.-_=#".contains(c));
    if !allowed {
        return Err("Bard only opens pages on miiso.dev.".into());
    }
    #[cfg(windows)]
    let opener = "explorer.exe";
    #[cfg(not(windows))]
    let opener = "xdg-open";
    std::process::Command::new(opener)
        .arg(&url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open the browser: {error}"))
}

/// The Store copy's login start is its manifest's startup task, which Windows
/// lists under Settings › Apps › Startup; that page is where it is switched.
#[tauri::command]
pub fn open_startup_settings() -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg("ms-settings:startupapps")
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open Windows Settings: {error}"))
    }
    #[cfg(not(windows))]
    {
        // Autostart on Linux is Bard's own `.desktop` entry in
        // `~/.config/autostart`, which the Settings toggle writes directly;
        // the desktop has no per-application startup page to open, so the
        // Store-only button that reaches this command never appears and there
        // is deliberately nothing to do here.
        Ok(())
    }
}

#[tauri::command]
pub fn show_panel(app: AppHandle) {
    crate::panel::show_now(&app);
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) {
    crate::panel::hide_now(&app);
}

#[tauri::command]
pub fn get_models(app: AppHandle) -> Vec<crate::dictation::ModelInfo> {
    crate::dictation::models(&app)
}

#[tauri::command]
pub async fn set_model(app: AppHandle, id: String) -> Result<(), String> {
    crate::hf::activate_model(&app, &id)
}

#[tauri::command]
pub async fn download_model(app: AppHandle, id: String) -> Result<(), String> {
    crate::dictation::download_model(&app, &id).await
}

#[tauri::command]
pub fn delete_model(app: AppHandle, id: String) -> Result<(), String> {
    crate::dictation::delete_model(&app, &id)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn download_model_cancel() {
    crate::dictation::cancel_download();
}

/// The clipboard's image as a data URL, or null when it holds none. Draw's Ctrl+V intake — the pixels come from the
/// same clipboard Bard's monitor reads, with the same size caps.
#[tauri::command]
pub fn clipboard_image_data() -> Option<String> {
    let mut clipboard = crate::clip::handle().ok()?;
    let image = clipboard.get_image().ok()?;
    let encoded = crate::monitor::encode_png(&image)?;
    let base64 = base64::engine::general_purpose::STANDARD.encode(&encoded);
    Some(format!("data:image/png;base64,{base64}"))
}

pub struct ImagesDir(pub PathBuf);

fn unique_image_path(dir: &Path, ext: &str) -> PathBuf {
    let now = db::now_ms();
    for i in 0..100 {
        let name = format!("{}-{}.{}", now, i, ext);
        let p = dir.join(name);
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{}-{}.png", now, std::process::id()))
}

#[cfg(test)]
mod acl_tests {
    /// Tauri refuses any command the ACL does not grant ("Command … not allowed
    /// by ACL"), and the browser preview's mock cannot see that. 2.0.2 shipped
    /// `set_pinned` registered but never allow-listed. Every handler in
    /// `main.rs` must be in `build.rs`'s manifest and granted by a capability.
    #[test]
    fn every_registered_command_is_allow_listed() {
        let main = include_str!("main.rs");
        let build = include_str!("../build.rs");
        let capabilities = [
            include_str!("../capabilities/main.json"),
            include_str!("../capabilities/settings.json"),
            include_str!("../capabilities/draw.json"),
            include_str!("../capabilities/pill.json"),
        ]
        .concat();
        let handlers = main
            .split("generate_handler![")
            .nth(1)
            .and_then(|rest| rest.split(']').next())
            .expect("main.rs registers commands with generate_handler!");
        let mut missing = Vec::new();
        for entry in handlers.split(',') {
            let Some(name) = entry.trim().rsplit("::").next().filter(|n| !n.is_empty()) else {
                continue;
            };
            if !build.contains(&format!("\"{name}\"")) {
                missing.push(format!("{name}: not in build.rs COMMANDS"));
            }
            let permission = format!("\"allow-{}\"", name.replace('_', "-"));
            if !capabilities.contains(&permission) {
                missing.push(format!("{name}: no capability grants {permission}"));
            }
        }
        assert!(
            missing.is_empty(),
            "commands the ACL would refuse:\n{}",
            missing.join("\n")
        );
    }
}

#[cfg(test)]
mod export_tests {
    use super::*;

    fn test_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bard-export-{label}-{}", db::now_ms()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn item(
        id: i64,
        section: Option<i64>,
        content: &str,
        kind: &str,
        image: Option<&str>,
    ) -> db::Item {
        db::Item {
            id,
            section_id: section,
            content: content.into(),
            kind: kind.into(),
            image_path: image.map(str::to_string),
            origin: "manual".into(),
            favorite: 0,
            done: 0,
            created_at: 0,
            updated_at: 0,
            pinned_at: None,
        }
    }

    #[test]
    fn an_export_carries_every_file_it_links_and_marks_the_missing() {
        let root = test_dir("fidelity");
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        let image = source.join("shot.png");
        std::fs::write(&image, b"png").unwrap();
        let attachment = source.join("clip.txt");
        std::fs::write(&attachment, b"attached").unwrap();
        let missing = source.join("gone.png").to_string_lossy().to_string();

        let items = vec![
            item(1, None, "keep this", "text", None),
            item(2, Some(7), "", "image", Some(&image.to_string_lossy())),
            item(3, Some(7), "", "image", Some(&missing)),
            item(4, Some(7), "note", "text", None),
        ];
        let mut attachments = std::collections::HashMap::new();
        attachments.insert(4, vec![attachment.to_string_lossy().to_string()]);
        let sections = vec![db::Section {
            id: 7,
            name: "Work".into(),
        }];
        let dir = root.join("out");
        std::fs::create_dir_all(&dir).unwrap();

        export_markdown_into(&dir, &sections, &items, &attachments).unwrap();

        let markdown = std::fs::read_to_string(dir.join("bard-export.md")).unwrap();
        assert!(markdown.contains("- keep this"));
        assert!(markdown.contains("## Work"));
        assert!(markdown.contains("![image](images/shot.png)"));
        assert!(dir.join("images").join("shot.png").is_file());
        assert!(markdown.contains("[clip.txt](files/clip.txt)"));
        assert!(dir.join("files").join("clip.txt").is_file());
        // A file that could not be read is named, never linked to nothing.
        assert!(markdown.contains("image could not be exported: gone.png"));
        assert!(!markdown.contains("![image](images/gone.png)"));
        std::fs::remove_dir_all(root).ok();
    }

    /// An item whose section was not in the section list used to disappear from
    /// the export entirely: only sections that were read were printed, and the
    /// item was never written anywhere else. A failed section read must not
    /// cost the person their items.
    #[test]
    fn an_item_whose_section_is_missing_is_still_exported() {
        let root = test_dir("missing-section");
        let dir = root.join("out");
        std::fs::create_dir_all(&dir).unwrap();
        let items = vec![item(1, Some(7), "filed but unreachable", "text", None)];
        let attachments = std::collections::HashMap::new();

        export_markdown_into(&dir, &[], &items, &attachments).unwrap();

        let markdown = std::fs::read_to_string(dir.join("bard-export.md")).unwrap();
        assert!(
            markdown.contains("- filed but unreachable"),
            "an item must never be dropped because its section was not read"
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn two_files_with_the_same_name_are_both_exported() {
        let mut used = std::collections::HashSet::new();
        assert_eq!(unique_export_name("shot.png", &mut used), "shot.png");
        assert_eq!(unique_export_name("shot.png", &mut used), "shot-2.png");
        // Windows compares names without case, so the third one cannot be
        // "SHOT.PNG" either.
        assert_eq!(unique_export_name("SHOT.PNG", &mut used), "SHOT-3.PNG");
        assert_eq!(
            unique_export_name("no-extension", &mut used),
            "no-extension"
        );
    }
}

#[cfg(test)]
mod ingest_tests {
    use super::*;

    /// File intake is the same door as the clipboard: a text file over the
    /// watcher's cap must be refused, not stored whole and not truncated.
    #[test]
    fn file_intake_applies_the_clipboard_capture_cap_to_text() {
        let cap = crate::monitor::MAX_CAPTURE_TEXT_BYTES as u64;
        assert_eq!(ingest_kind("log", cap), IngestKind::Text);
        assert_eq!(ingest_kind("log", cap + 1), IngestKind::TextTooLarge);
        assert_eq!(ingest_kind("txt", 1), IngestKind::Text);
        // The 25 MB bound still applies to every kind of file.
        assert_eq!(
            ingest_kind("png", MAX_INGEST_FILE_BYTES + 1),
            IngestKind::Unsupported
        );
        assert_eq!(ingest_kind("png", MAX_INGEST_FILE_BYTES), IngestKind::Image);
        // A file that is neither an image nor text keeps its name as a note.
        assert_eq!(ingest_kind("zip", 10), IngestKind::FileName);
    }

    /// Two names that differ only in case are two files on Linux, and one on
    /// the case-insensitive filesystems. Lowercasing every path collapsed the
    /// pair on Linux and silently skipped one of them.
    #[test]
    fn case_distinct_paths_are_one_only_where_the_filesystem_says_so() {
        let upper = path_key(Path::new("/tmp/Report.txt"));
        let lower = path_key(Path::new("/tmp/report.txt"));
        if cfg!(any(windows, target_os = "macos")) {
            assert_eq!(upper, lower);
        } else {
            assert_ne!(
                upper, lower,
                "Linux has two files here, so an import must keep both"
            );
        }
    }
}

#[cfg(test)]
mod draw_tests {
    use super::*;

    #[test]
    fn a_suggested_svg_name_keeps_its_stem_and_gains_the_extension() {
        assert_eq!(svg_file_name("Sketch"), "Sketch.svg");
        assert_eq!(svg_file_name("sketch.svg"), "sketch.svg");
        assert_eq!(svg_file_name("Sketch.SVG"), "Sketch.SVG");
        assert_eq!(svg_file_name("café"), "café.svg");
    }

    #[test]
    fn a_suggested_svg_name_can_never_carry_a_path() {
        let name = svg_file_name("../../etc/passwd");
        assert!(!name.contains('/') && !name.contains('\\'));
        assert!(name.ends_with(".svg"));
        let name = svg_file_name("a/b\\c:d*e?");
        assert_eq!(name, "a b c d e.svg");
    }

    #[test]
    fn an_empty_or_dotted_svg_name_falls_back_to_the_default() {
        assert_eq!(svg_file_name(""), "Sketch.svg");
        assert_eq!(svg_file_name("   "), "Sketch.svg");
        assert_eq!(svg_file_name("..."), "Sketch.svg");
        assert_eq!(svg_file_name("sketch."), "sketch.svg");
    }

    #[test]
    fn a_long_svg_name_is_capped() {
        let name = svg_file_name(&"x".repeat(400));
        assert!(name.len() <= 84, "{name}");
        assert!(name.ends_with(".svg"));
    }

    #[test]
    fn a_stored_tool_this_build_knows_survives_a_read() {
        assert_eq!(draw_tool_or_default(Some("rect".into())), "rect");
        assert_eq!(draw_tool_or_default(Some("spray".into())), "pen");
        assert_eq!(draw_tool_or_default(None), "pen");
    }

    #[test]
    fn a_stored_width_outside_the_canvas_range_reads_as_the_default() {
        assert_eq!(draw_width_or_default(Some(8)), 8);
        assert_eq!(draw_width_or_default(Some(0)), 4);
        assert_eq!(draw_width_or_default(Some(65)), 4);
        assert_eq!(draw_width_or_default(None), 4);
    }

    #[test]
    fn a_stored_colour_is_kept_leniently_but_bounded() {
        assert!(draw_color_ok("#1d1d1f"));
        assert!(draw_color_ok("red"));
        assert!(!draw_color_ok(""));
        assert!(!draw_color_ok(&"a".repeat(33)));
        assert!(!draw_color_ok("two\nlines"));
        assert_eq!(draw_color_or_default(Some("#8e5cf6".into())), "#8e5cf6");
        assert_eq!(draw_color_or_default(Some(String::new())), "#1d1d1f");
        assert_eq!(draw_color_or_default(None), "#1d1d1f");
    }

    #[test]
    fn every_tool_the_frontend_draws_with_is_known_here() {
        for tool in ["select", "pen", "line", "arrow", "rect", "ellipse", "text"] {
            assert!(DRAW_TOOLS.contains(&tool), "{tool}");
        }
    }
}

#[cfg(test)]
mod login_item_tests {
    use super::login_item_absent;

    #[test]
    fn a_missing_login_item_is_not_a_failure() {
        assert!(login_item_absent(
            "The system cannot find the file specified. (os error 2)"
        ));
        assert!(login_item_absent("No such file or directory (os error 2)"));
        assert!(!login_item_absent("Access is denied. (os error 5)"));
        assert!(!login_item_absent("os error 22"));
    }
}
