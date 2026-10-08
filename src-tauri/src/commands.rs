use std::path::{Path, PathBuf};

use image::GenericImageView;
use std::time::Duration;

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
use zeroize::Zeroize;

use crate::db;

pub fn refresh(app: &AppHandle) {
    let _ = app.emit("refresh", ());
}

fn license_blocked(app: &AppHandle, error: String) -> bool {
    let _ = app.emit("license-restricted", error);
    true
}

fn require_licensed_or_onboarding(app: &AppHandle) -> Result<(), String> {
    if crate::db::get_setting(app, "onboarding_completed") != Some("1".into()) {
        return Ok(());
    }
    crate::license::require(app)
}

#[tauri::command]
pub fn hf_cached_models(app: AppHandle) -> Vec<crate::hf::CachedHfModel> {
    crate::hf::cached_models(&app)
}

#[tauri::command]
pub fn get_detected_models(app: AppHandle) -> Vec<crate::hf::CachedHfModel> {
    crate::hf::cached_models(&app)
}

#[tauri::command]
pub fn check_model(
    app: AppHandle,
    local_path: String,
) -> Result<crate::hf::CompatibilityResult, String> {
    crate::hf::probe_local_model(&app, &local_path)
}

#[tauri::command]
pub fn activate_detected_model(app: AppHandle, id: String) -> Result<(), String> {
    require_licensed_or_onboarding(&app)?;
    crate::hf::activate_detected_model(&app, &id)
}

#[tauri::command]
pub fn forget_detected_model(app: AppHandle, id: String) -> Result<(), String> {
    if !crate::catalog::find(&id).is_some() && crate::db::custom_model_by_id(&app, &id).is_none() {
        // An unregistered cache/app-directory candidate has no Bard-owned row;
        // forgetting it simply removes the transient inventory entry.
        refresh(&app);
        return Ok(());
    }
    crate::dictation::delete_model(&app, &id)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub async fn hf_list_repo(repo: String) -> Result<Vec<crate::hf::HfFile>, String> {
    crate::hf::list_repo_files(&repo).await
}

#[tauri::command]
pub async fn hf_install(
    app: AppHandle,
    repo: String,
    file: String,
    size: u64,
    sha256: String,
) -> Result<String, String> {
    require_licensed_or_onboarding(&app)?;
    crate::hf::install_custom_model(&app, &repo, &file, size, &sha256).await
}

#[tauri::command]
pub fn hf_use_cached(
    app: AppHandle,
    repo: String,
    file: String,
    local_path: String,
    size: u64,
) -> Result<(), String> {
    require_licensed_or_onboarding(&app)?;
    // One load, not two. This used to register the path and then activate it,
    // and each of those opened the file with the engine — so choosing a detected
    // model meant two full opens of a multi-gigabyte GGUF for one click.
    crate::hf::register_and_activate_cached(&app, &repo, &file, &local_path, size)
}

#[tauri::command]
pub fn remove_custom_model(app: AppHandle, id: String) -> Result<(), String> {
    crate::dictation::delete_model(&app, &id)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn get_custom_models(app: AppHandle) -> Vec<db::CustomModel> {
    db::list_custom_models(&app)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PostprocessingSettings {
    pub enabled: bool,
    pub model_id: String,
    pub model_ids: Vec<String>,
    pub model_name: Option<String>,
    pub key_configured: bool,
    pub status_message: Option<String>,
}

fn postprocessing_config(app: &AppHandle) -> crate::cloud::Config {
    crate::cloud::Config::from_setting_values(
        db::get_setting(app, crate::cloud::ENABLED_SETTING_KEY),
        db::get_setting(app, crate::cloud::MODEL_IDS_SETTING_KEY),
    )
}

fn postprocessing_settings(app: &AppHandle) -> Result<PostprocessingSettings, String> {
    let config = postprocessing_config(app);
    let (status, credential_error) = match crate::cloud::status(&config) {
        Ok(status) => (status, false),
        Err(crate::cloud::Error::CredentialStore { .. }) => (
            crate::cloud::Status {
                enabled: config.enabled,
                has_api_key: false,
                has_valid_model: config.has_valid_model(),
                ready: false,
                model_ids: config.model_ids.clone(),
            },
            true,
        ),
        Err(error) => return Err(error.to_string()),
    };
    let model_id = config.active_model_id().unwrap_or_default().to_string();
    let status_message = if credential_error {
        Some("Windows Credential Manager is unavailable; cloud processing is off.".into())
    } else if !status.ready && config.enabled {
        Some("Refine needs a saved key and model ID.".into())
    } else {
        None
    };
    Ok(PostprocessingSettings {
        enabled: config.enabled && !credential_error,
        model_id: model_id.clone(),
        model_ids: config.model_ids.clone(),
        model_name: (!model_id.is_empty()).then_some(model_id),
        key_configured: status.has_api_key,
        status_message,
    })
}

#[tauri::command]
pub fn get_postprocessing(app: AppHandle) -> Result<PostprocessingSettings, String> {
    postprocessing_settings(&app)
}

#[tauri::command]
pub fn save_postprocessing(
    app: AppHandle,
    enabled: bool,
    model_id: Option<String>,
    model_ids: Option<Vec<String>>,
    mut api_key: Option<String>,
    clear_key: Option<bool>,
) -> Result<PostprocessingSettings, String> {
    if enabled {
        crate::license::require(&app)?;
    }
    let clearing_key = clear_key.unwrap_or(false) || api_key.as_deref().is_some_and(str::is_empty);
    let current = postprocessing_config(&app);
    let ids = if clearing_key {
        // Clearing a credential must remain possible even if a stale model
        // setting is malformed; preserve the last known model list verbatim.
        current.model_ids
    } else {
        let raw_models = model_ids
            .filter(|ids| !ids.is_empty())
            .map(|ids| ids.join(","))
            .or(model_id)
            .unwrap_or_default();
        crate::cloud::parse_model_ids_checked(&raw_models).map_err(|error| error.to_string())?
    };

    let credential_result: Result<(), String> = if clearing_key {
        crate::cloud::delete_api_key_bounded().map_err(|error| error.to_string())
    } else if let Some(key) = api_key.as_deref().filter(|key| !key.trim().is_empty()) {
        crate::cloud::set_api_key_bounded(key.to_owned()).map_err(|error| error.to_string())
    } else {
        Ok(())
    };
    if let Some(key) = api_key.as_mut() {
        key.zeroize();
    }
    credential_result?;

    let next_enabled = enabled
        && !ids.is_empty()
        && crate::cloud::has_api_key().map_err(|error| error.to_string())?;
    let next_config = crate::cloud::Config {
        enabled: next_enabled,
        model_ids: ids,
    };
    db::set_settings_checked(
        &app,
        &[
            (
                crate::cloud::ENABLED_SETTING_KEY,
                next_config.enabled_setting_value(),
            ),
            (
                crate::cloud::MODEL_IDS_SETTING_KEY,
                &next_config.model_ids_setting_value(),
            ),
        ],
    )?;
    postprocessing_settings(&app)
}

#[tauri::command]
pub async fn test_postprocessing(app: AppHandle) -> Result<serde_json::Value, String> {
    crate::crash::record_event("cloud:test-start");
    let config = postprocessing_config(&app);
    let result = crate::cloud::test_connection(&config, Duration::from_secs(15))
        .await
        .map_err(|error| error.to_string());
    crate::crash::record_event("cloud:test-end");
    let message = result?;
    Ok(serde_json::json!({ "ok": true, "message": message }))
}

#[tauri::command]
pub fn test_postprocessing_cancel() {
    crate::cloud::cancel_test_connection();
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReport {
    pub path: String,
    pub contents: String,
}

#[tauri::command]
pub fn get_crash_report() -> Result<Option<CrashReport>, String> {
    let directory = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("APPDATA"))
        .map(|base| {
            std::path::Path::new(&base)
                .join("com.miisodev.bard")
                .join("logs")
        })
        .unwrap_or_else(|_| std::env::temp_dir().join("com.miisodev.bard").join("logs"));
    crate::crash::read_latest_log(&directory)
        .map(|latest| {
            latest.map(|log| CrashReport {
                path: log.path.to_string_lossy().to_string(),
                contents: log.contents,
            })
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
) -> Result<Option<crate::updater::UpdateInfo>, String> {
    crate::license::require(&app)?;
    crate::updater::check(&app).await
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    crate::license::require(&app)?;
    crate::updater::install(&app).await
}

#[tauri::command]
pub fn start_trial(app: AppHandle) -> Result<crate::license::LicenseStatus, String> {
    crate::license::start_trial(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_license(app: AppHandle) -> crate::license::LicenseStatus {
    crate::license::status(&app)
}

#[tauri::command]
pub async fn activate_license(
    app: AppHandle,
    key: String,
) -> Result<crate::license::LicenseStatus, String> {
    crate::license::activate(&app, key)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn refresh_license(app: AppHandle) -> crate::license::LicenseStatus {
    crate::license::refresh(&app).await
}

#[tauri::command]
pub async fn deactivate_license(app: AppHandle) -> Result<crate::license::LicenseStatus, String> {
    crate::license::deactivate(&app)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn clear_license(app: AppHandle) -> Result<crate::license::LicenseStatus, String> {
    crate::license::clear(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_setting_str(app: AppHandle, key: String) -> String {
    const PUBLIC_SETTING_KEYS: &[&str] = &[
        "selected_microphone",
        "dictation_overlay_position",
        "openrouter_postprocess_enabled",
        "openrouter_postprocess_model_ids",
        "openrouter_postprocess_model_id",
        "theme",
        "capture_double_shift",
        "sound_on_capture",
        "clipboard_retention_days",
        "dictation_retention_days",
    ];
    if !PUBLIC_SETTING_KEYS.contains(&key.as_str()) {
        return String::new();
    }
    db::get_setting(&app, &key).unwrap_or_default()
}

pub fn dirs_documents() -> String {
    std::env::var("USERPROFILE")
        .map(|p| format!("{p}\\Documents"))
        .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string())
}

#[tauri::command]
pub fn create_section(app: AppHandle, name: String) -> i64 {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error);
        return 0;
    }
    let id = db::find_or_create_section(&app, &name);
    db::set_active_section(&app, Some(id));
    refresh(&app);
    id
}

#[tauri::command]
pub fn set_active_section(app: AppHandle, id: Option<i64>) {
    db::set_active_section(&app, id);
    refresh(&app);
}

#[tauri::command]
pub fn rename_section(app: AppHandle, id: i64, name: String) {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error);
        return;
    }
    db::rename_section(&app, id, &name);
    refresh(&app);
}

#[tauri::command]
pub fn delete_section(app: AppHandle, id: i64) {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error);
        return;
    }
    db::delete_section(&app, id);
    refresh(&app);
}

#[tauri::command]
pub fn update_item(app: AppHandle, id: i64, content: String) {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error);
        return;
    }
    db::update_item(&app, id, &content);
    refresh(&app);
}

/// Native-only file intake used by the Windows Explorer verb and first launch.
/// It is deliberately not a Tauri command: a renderer can read a File object
/// it owns, but it cannot ask Rust to copy an arbitrary path by name.
pub fn ingest_paths(app: &AppHandle, paths: Vec<String>) -> usize {
    if let Err(error) = crate::license::require(app) {
        license_blocked(app, error);
        return 0;
    }
    let images = app.state::<ImagesDir>().0.clone();
    let mut added = 0;
    let mut seen = std::collections::HashSet::new();
    for p in paths {
        if !seen.insert(Path::new(&p).to_string_lossy().to_ascii_lowercase()) {
            continue;
        }
        let path = Path::new(&p);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let Ok(metadata) = std::fs::metadata(path) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > 25 * 1024 * 1024 {
            continue;
        }
        if [
            "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tiff", "avif",
        ]
        .contains(&ext.as_str())
        {
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
        } else if matches!(
            ext.as_str(),
            "txt"
                | "md"
                | "markdown"
                | "json"
                | "csv"
                | "log"
                | "rs"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "html"
                | "css"
        ) {
            if let Ok(text) = std::fs::read_to_string(path) {
                let text = text.trim();
                if db::list_items(app)
                    .iter()
                    .any(|item| item.origin == "clipboard" && item.content == text)
                {
                    continue;
                }
                let id = db::insert_item_with_origin(app, text, "text", None, "clipboard");
                // `insert_item_with_origin` returns a sentinel rather than
                // aborting when the write cannot happen. Counting a write that did
                // not occur is how "Added 3 files" becomes a lie on a full disk.
                if id == db::INSERT_FAILED {
                    continue;
                }
                db::set_active_item(app, Some(id));
                added += 1;
            }
        } else if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            let id = db::insert_item_with_origin(app, name, "text", None, "clipboard");
            if id == db::INSERT_FAILED {
                continue;
            }
            db::set_active_item(app, Some(id));
            added += 1;
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
pub fn attach_image(
    app: AppHandle,
    item_id: i64,
    data_base64: String,
    ext: String,
) -> Result<i64, String> {
    crate::license::require(&app)?;
    let (bytes, ext) = decode_image_payload(&data_base64, &ext)?;
    let images = app.state::<ImagesDir>().0.clone();
    let path = unique_image_path(&images, &ext);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let id = db::insert_attachment(&app, item_id, &path.to_string_lossy());
    if id == db::INSERT_FAILED {
        // The file is on disk with no row referencing it. `Ok(0)` would tell the
        // renderer it has an attachment id it does not have, and the file would
        // never be collected.
        let _ = std::fs::remove_file(&path);
        return Err("Bard could not attach this file. Check that the drive has free space.".into());
    }
    refresh(&app);
    Ok(id)
}

#[tauri::command]
pub fn detach_attachment(app: AppHandle, id: i64) -> Result<(), String> {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error.clone());
        return Err(error);
    }
    if let Some(path) = db::delete_attachment(&app, id)? {
        db::remove_owned_image_file(&app, &path);
    }
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn add_note_to_image(app: AppHandle, id: i64) {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error);
        return;
    }
    db::add_note_to_image(&app, id);
    refresh(&app);
}

#[tauri::command]
pub fn export_markdown(app: AppHandle) -> Result<String, String> {
    let state = db::get_state(&app);
    let mut out = String::from("# Bard export\n\n");
    let mut unfiled: Vec<&db::Item> = vec![];
    let mut by_section: Vec<(i64, &db::Item)> = vec![];
    for item in state.items.iter() {
        match item.section_id {
            None => unfiled.push(item),
            Some(s) => by_section.push((s, item)),
        }
    }
    for item in &unfiled {
        out.push_str(&format!("- {}\n", item.content.replace('\n', "\n  ")));
    }
    for section in state.sections {
        if !unfiled.is_empty() {
            out.push('\n');
            unfiled.clear();
        }
        let items: Vec<_> = by_section
            .iter()
            .filter(|(s, _)| *s == section.id)
            .map(|(_, i)| *i)
            .collect();
        if items.is_empty() {
            continue;
        }
        out.push_str(&format!("\n## {}\n\n", section.name));
        for item in items {
            if item.kind == "image" {
                let name = item
                    .image_path
                    .as_deref()
                    .and_then(|p| std::path::Path::new(p).file_name())
                    .and_then(|f| f.to_str())
                    .unwrap_or("image");
                out.push_str(&format!("- ![image](images/{})\n", name));
            } else {
                out.push_str(&format!("- {}\n", item.content.replace('\n', "\n  ")));
            }
        }
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
    let path = dir.join("bard-export.md");
    std::fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
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
pub fn get_export_dir(app: AppHandle) -> String {
    db::get_setting(&app, "export_dir")
        .filter(|d| !d.is_empty())
        .unwrap_or_else(dirs_documents)
}

#[tauri::command]
pub fn set_sound_on_capture(app: AppHandle, enabled: bool) {
    db::set_setting(&app, "sound_on_capture", if enabled { "1" } else { "0" });
}

#[tauri::command]
pub fn get_sound_on_capture(app: AppHandle) -> bool {
    db::get_setting(&app, "sound_on_capture") != Some("0".into())
}

#[tauri::command]
pub fn get_microphones() -> Vec<String> {
    crate::dictation::list_input_devices()
}

#[tauri::command]
pub fn set_microphone(app: AppHandle, name: String) {
    db::set_setting(&app, "selected_microphone", &name);
}

#[tauri::command]
pub fn save_window_geometry(app: AppHandle, x: i32, y: i32, width: u32, height: u32) {
    // The panel reports its own resize; what is stored is read back from the
    // window itself, so a minimised or off-screen rectangle is never kept.
    let _ = (x, y, width, height);
    if let Some(win) = app.get_webview_window("main") {
        crate::geometry::remember(&app, &win);
    }
}

#[tauri::command]
pub fn minimize_panel(app: AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.minimize();
    }
}

#[tauri::command]
pub fn get_state(app: AppHandle) -> db::AppState {
    db::get_state(&app)
}

#[tauri::command]
pub fn add_item(app: AppHandle, content: String) {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error);
        return;
    }
    let id = db::insert_item(&app, &content, "text", None);
    // Setting the active item to the sentinel would clear it, so a failed write
    // would also silently deselect whatever the person had been working on.
    if id != db::INSERT_FAILED {
        db::set_active_item(&app, Some(id));
    }
    refresh(&app);
}

#[tauri::command]
pub fn add_image(app: AppHandle, data_base64: String, ext: String) -> Result<i64, String> {
    crate::license::require(&app)?;
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

/// Show Settings or Draw where it was last left (centred the first time).
/// Both are ordinary windows now: movable, resizable, on the taskbar, and
/// independent of the panel and of each other.
pub fn show_secondary(app: &AppHandle, label: &str, _center: bool) {
    use tauri::Manager;
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    if win.is_fullscreen().unwrap_or(false) {
        let _ = win.set_fullscreen(false);
    }
    let _ = win.unminimize();
    if !win.is_visible().unwrap_or(false) {
        crate::geometry::restore(
            app,
            &win,
            crate::panel::COMPANION_WIDTH,
            crate::panel::COMPANION_HEIGHT,
        );
    }
    let _ = win.show();
    let _ = win.set_focus();
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
    let Some(win) = app.get_webview_window(&label) else {
        return Ok(());
    };
    if win.is_visible().unwrap_or(false) && !win.is_minimized().unwrap_or(false) {
        crate::panel::hide_companion(&app, &win);
        crate::panel::focus_panel(&app);
        return Ok(());
    }
    if label == "draw" {
        if let Err(error) = crate::license::require(&app) {
            license_blocked(&app, error);
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

fn hide_panel_for_secondary(app: &AppHandle) {
    use tauri::Manager;
    if let Some(panel) = app.get_webview_window("main") {
        let _ = panel.hide();
    }
}

fn toggle_secondary_alone(app: &AppHandle, label: &str) {
    use tauri::Manager;
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    if win.is_visible().unwrap_or(false) && win.is_focused().unwrap_or(false) {
        crate::panel::hide_companion(app, &win);
        return;
    }
    // A gesture opens that one window; the panel steps aside for it.
    hide_panel_for_secondary(app);
    show_secondary(app, label, false);
}

/// Left Alt × 2 opens Draw as a single window. It runs from the raw-input
/// worker, so every native window operation is queued onto the main thread.
pub fn draw_gesture(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Err(error) = crate::license::require(&handle) {
            license_blocked(&handle, error);
            return;
        }
        toggle_secondary_alone(&handle, "draw");
    });
}

/// Right Alt × 2 opens Settings as a single window, matching the tray action
/// without bringing the panel back beside it.
pub fn settings_gesture(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || toggle_secondary_alone(&handle, "settings"));
}

#[tauri::command]
pub fn open_draw(app: AppHandle) {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error);
        return;
    }
    show_secondary(&app, "draw", true);
}

#[tauri::command]
pub fn delete_item(app: AppHandle, id: i64) -> Result<(), String> {
    db::delete_items_with_files(&app, &[id])?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn set_favorite(app: AppHandle, id: i64, favorite: bool) {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error);
        return;
    }
    db::set_favorite(&app, id, favorite);
    refresh(&app);
}

/// Pin an item to the top of the panel, where it is one of the three slots
/// Right Shift × 2 + 1/2/3 pastes. A fourth pin is refused, not rotated in:
/// silently unpinning something would move what a learned slot pastes.
#[tauri::command]
pub fn set_pinned(app: AppHandle, id: i64, pinned: bool) -> Result<(), String> {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error.clone());
        return Err(error);
    }
    let result = db::set_pinned(&app, id, pinned).map_err(|error| error.to_string());
    refresh(&app);
    result
}

#[tauri::command]
pub fn set_done(app: AppHandle, id: i64, done: bool) {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error);
        return;
    }
    db::set_done(&app, id, done);
    refresh(&app);
}

#[tauri::command]
pub fn clear_completed(app: AppHandle) -> Result<(), String> {
    let ids: Vec<i64> = db::completed_items(&app)
        .into_iter()
        .map(|item| item.id)
        .collect();
    db::delete_items_with_files(&app, &ids)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<(), String> {
    db::clear_history(&app)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    crate::paste::set_text(&app, &text)
}

/// Put the panel away, give focus back to the app it was summoned over, and
/// paste what the panel has just written to the clipboard. The caller writes
/// and verifies the clipboard first (copy_text / copy_item), so a failed write
/// never turns into pasting whatever was there before.
#[tauri::command]
pub fn paste_back(app: AppHandle) {
    crate::panel::hide_now(&app);
    std::thread::spawn(move || {
        // hide_now queues the hide and the foreground restore on the UI
        // thread; the chord has to land after the target window is active.
        std::thread::sleep(std::time::Duration::from_millis(160));
        crate::paste::send_paste(&app);
    });
}

#[tauri::command]
pub fn copy_item(app: AppHandle, id: i64) -> Result<(), String> {
    let items = db::list_items(&app);
    let Some(item) = items.into_iter().find(|i| i.id == id) else {
        return Err("item not found".into());
    };
    if item.kind == "image" {
        if let Some(p) = item.image_path {
            crate::paste::set_image(&app, &p)?;
        }
    } else {
        crate::paste::set_text(&app, &item.content)?;
    }
    db::set_active_item(&app, Some(id));
    refresh(&app);
    Ok(())
}

/// Paste one of the first three timeline items into the foreground app. Pins
/// lead the timeline, so slots 1–3 are the pinned items, then the panel order.
/// The global Right Shift × 2 + number gesture calls this without opening the
/// panel, so the active application remains the paste target.
pub fn paste_item(app: &AppHandle, slot: u8) -> Result<(), String> {
    if !(1..=3).contains(&slot) {
        return Err("Bard only exposes item slots 1 through 3.".into());
    }
    let items = db::timeline(app);
    let Some(item) = items.get(slot as usize - 1) else {
        return Err(format!("Bard has no item {slot} to paste."));
    };
    if item.kind == "image" {
        let path = item
            .image_path
            .as_deref()
            .ok_or_else(|| "That image is no longer available.".to_string())?;
        crate::paste::set_image(app, path)?;
    } else {
        crate::paste::set_text(app, &item.content)?;
    }
    db::set_active_item(app, Some(item.id));
    crate::paste::send_paste(app);
    refresh(app);
    Ok(())
}

#[tauri::command]
pub fn copy_attachment(app: AppHandle, id: i64) -> Result<(), String> {
    let path = db::attachment_path(&app, id).ok_or("Attachment not found")?;
    crate::paste::set_image(&app, &path)
}

#[tauri::command]
pub fn set_active_item(app: AppHandle, id: Option<i64>) {
    db::set_active_item(&app, id);
    refresh(&app);
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
/// The count cap is applied here rather than on the capture path, so pruning is
/// something a person asked for and watches happen, not something that silently
/// deletes their data mid-session.
#[tauri::command]
pub fn prune_history_now(app: AppHandle) -> Result<usize, String> {
    let mut removed = 0usize;
    for origin in ["clipboard", "dictation"] {
        removed += db::prune_expired(&app, origin)?;
    }
    removed += db::prune_history(&app, db::AUTOMATIC_ITEM_CEILING)?;
    refresh(&app);
    Ok(removed)
}

#[tauri::command]
pub fn get_retention_days(app: AppHandle, origin: String) -> Option<i64> {
    db::retention_days(&app, &origin)
}

#[tauri::command]
pub fn delete_all_data(app: AppHandle) -> Result<(), String> {
    db::delete_all_data(&app)?;
    crate::license::erase_local(&app).map_err(|error| error.to_string())?;
    crate::cloud::delete_api_key().map_err(|error| error.to_string())?;
    let _ = std::fs::remove_file(crate::guardian::marker_path());
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn clear_captures(app: AppHandle) -> Result<(), String> {
    db::clear_captures(&app)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn item_image_data(app: AppHandle, id: i64) -> Option<String> {
    let item = db::item_by_id(&app, id)?;
    if item.kind != "image" {
        return None;
    }
    crate::paste::image_data(&app, &item.image_path?)
}

#[tauri::command]
pub fn attachment_image_data(app: AppHandle, id: i64) -> Option<String> {
    let path = db::attachment_path(&app, id)?;
    crate::paste::image_data(&app, &path)
}

/// What every window needs to paint itself: the theme and the background.
/// Deliberately narrow — the overlay reads it too, and has no business seeing
/// the clipboard history that `get_state` carries.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    theme: String,
    background: String,
    background_color: String,
    draw_background: String,
    draw_background_color: String,
}

#[tauri::command]
pub fn get_appearance(app: AppHandle) -> Appearance {
    if !db::is_ready(&app) {
        return Appearance {
            theme: "system".into(),
            background: "theme".into(),
            background_color: String::new(),
            draw_background: "theme".into(),
            draw_background_color: String::new(),
        };
    }
    Appearance {
        theme: db::get_setting(&app, "theme").unwrap_or_else(|| "system".into()),
        background: db::get_setting(&app, "background").unwrap_or_else(|| "theme".into()),
        background_color: db::get_setting(&app, "background_color").unwrap_or_default(),
        draw_background: db::get_setting(&app, "draw_background").unwrap_or_else(|| "theme".into()),
        draw_background_color: db::get_setting(&app, "draw_background_color").unwrap_or_default(),
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
    Ok(())
}

#[tauri::command]
pub fn set_theme(app: AppHandle, theme: String) {
    db::set_setting(&app, "theme", &theme);
    // Every window reads the theme out of app state, so without this only the
    // window that made the change would repaint — which is exactly how the
    // panel stayed light while settings went dark.
    refresh(&app);
}

#[tauri::command]
pub fn set_onboarding_completed(app: AppHandle, done: bool) {
    db::set_setting(&app, "onboarding_completed", if done { "1" } else { "0" });
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    if crate::packaging::is_packaged() {
        return Err("Windows manages how the Store copy of Bard starts.".into());
    }
    if enabled {
        app.autolaunch().enable().map_err(|e| e.to_string())?;
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())?;
    }
    db::set_setting(&app, "autostart_enabled", if enabled { "1" } else { "0" });
    Ok(())
}

/// Opens a miiso.dev page — purchase, licence management, terms, privacy — in
/// the default browser. The webview has no opener of its own, so a plain link
/// went nowhere; only Bard's own site is allowed through.
#[tauri::command]
pub fn open_link(url: String) -> Result<(), String> {
    // `&` and `%` are gone from the allowlist. This is not command injection —
    // `Command` does not go through a shell and the scheme is pinned — but
    // whether Explorer splits on either character inside a single argument is a
    // behaviour of Explorer this project should not depend on. The two real
    // call sites pass fixed `https://miiso.dev/...` paths, so a tight allowlist
    // costs nothing.
    let allowed = url.starts_with("https://miiso.dev/")
        && url.len() <= 256
        && url
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/:.-_=".contains(c));
    if !allowed {
        return Err("Bard only opens pages on miiso.dev.".into());
    }
    std::process::Command::new("explorer.exe")
        .arg(&url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open the browser: {error}"))
}

/// The Store copy's login start is its manifest's startup task, which Windows
/// lists under Settings › Apps › Startup; that page is where it is switched.
#[tauri::command]
pub fn open_startup_settings() -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg("ms-settings:startupapps")
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open Windows Settings: {error}"))
}

#[tauri::command]
pub fn set_capture_double_shift(app: AppHandle, enabled: bool) {
    db::set_setting(
        &app,
        "capture_double_shift",
        if enabled { "1" } else { "0" },
    );
}

#[tauri::command]
pub fn capture_now(app: AppHandle) {
    crate::capture::capture_selection(&app);
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
pub fn toggle_panel(app: AppHandle) {
    crate::panel::toggle_now(&app);
}

#[tauri::command]
pub fn get_models(app: AppHandle) -> Vec<crate::dictation::ModelInfo> {
    crate::dictation::models(&app)
}

#[tauri::command]
pub fn get_model(app: AppHandle) -> crate::dictation::ModelStatus {
    crate::dictation::model_status(&app)
}

#[tauri::command]
pub fn set_model(app: AppHandle, id: String) -> Result<(), String> {
    require_licensed_or_onboarding(&app)?;
    crate::hf::activate_model(&app, &id)
}

#[tauri::command]
pub async fn download_model(app: AppHandle, id: String) -> Result<(), String> {
    require_licensed_or_onboarding(&app)?;
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

#[tauri::command]
pub fn dictate_start(app: AppHandle) {
    crate::dictation::start(&app);
}

#[tauri::command]
pub fn dictate_stop(app: AppHandle) {
    crate::dictation::stop(&app);
}

#[tauri::command]
pub fn dictate_cancel(app: AppHandle) {
    crate::dictation::cancel(&app);
}

#[tauri::command]
pub fn set_dictation_overlay_position(app: AppHandle, position: String) -> Result<(), String> {
    crate::dictation::set_overlay_position(&app, &position)
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
            include_str!("../capabilities/overlay.json"),
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
