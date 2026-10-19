use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};

use image::GenericImageView;
use std::time::Duration;

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
use zeroize::Zeroize;

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
pub async fn hf_cached_models(app: AppHandle) -> Vec<crate::hf::CachedHfModel> {
    crate::hf::cached_models(&app)
}

#[tauri::command]
pub async fn get_detected_models(app: AppHandle) -> Vec<crate::hf::CachedHfModel> {
    crate::hf::cached_models(&app)
}

#[tauri::command]
pub async fn check_model(
    app: AppHandle,
    local_path: String,
) -> Result<crate::hf::CompatibilityResult, String> {
    crate::hf::probe_local_model(&app, &local_path)
}

#[tauri::command]
pub async fn activate_detected_model(app: AppHandle, id: String) -> Result<(), String> {
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
pub async fn hf_use_cached(
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
    crate::paste::set_text(&app, &report)
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
    let status = crate::license::status(app);
    out.push_str(&format!("licence_state: {}\n", status.state));
    out.push_str(&format!(
        "licence_revalidation_failing: {}\n",
        status.revalidation_failing
    ));
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
pub async fn get_license(app: AppHandle) -> crate::license::LicenseStatus {
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
pub async fn clear_license(app: AppHandle) -> Result<crate::license::LicenseStatus, String> {
    crate::license::clear(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_setting_str(app: AppHandle, key: String) -> String {
    const PUBLIC_SETTING_KEYS: &[&str] = &[
        "selected_microphone",
        "dictation_overlay_position",
        // The review sheet's switch, single-sourced with the take path so the
        // public read and the delivery decision cannot name different keys.
        crate::dictation::REVIEW_SETTING_KEY,
        "openrouter_postprocess_enabled",
        "openrouter_postprocess_model_ids",
        "openrouter_postprocess_model_id",
        "theme",
        "capture_double_shift",
        "sound_on_capture",
        "clipboard_retention_days",
        "dictation_retention_days",
        // The shelf's edge, so both the panel (its `data-edge` layout) and the
        // Settings selector can read which edge it is docked to.
        "shelf_edge",
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
pub fn list_packs(app: AppHandle) -> Vec<db::PackSummary> {
    db::list_packs(&app)
}

#[tauri::command]
pub fn get_pack(app: AppHandle, id: i64) -> Result<db::PackDetail, String> {
    db::get_pack(&app, id)
}

#[tauri::command]
pub fn create_pack(
    app: AppHandle,
    name: String,
    item_ids: Vec<i64>,
) -> Result<db::PackSummary, String> {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error.clone());
        return Err(error);
    }
    let pack = db::create_pack(&app, &name, &item_ids)?;
    refresh(&app);
    Ok(pack)
}

#[tauri::command]
pub fn rename_pack(app: AppHandle, id: i64, name: String) -> Result<(), String> {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error.clone());
        return Err(error);
    }
    db::rename_pack(&app, id, &name)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn delete_pack(app: AppHandle, id: i64) -> Result<(), String> {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error.clone());
        return Err(error);
    }
    db::delete_pack(&app, id)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn pack_add_items(app: AppHandle, id: i64, item_ids: Vec<i64>) -> Result<(), String> {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error.clone());
        return Err(error);
    }
    db::pack_add_items(&app, id, &item_ids)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn pack_remove_items(app: AppHandle, id: i64, item_ids: Vec<i64>) -> Result<(), String> {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error.clone());
        return Err(error);
    }
    db::pack_remove_items(&app, id, &item_ids)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn pack_reorder(app: AppHandle, id: i64, item_ids: Vec<i64>) -> Result<(), String> {
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error.clone());
        return Err(error);
    }
    db::pack_reorder(&app, id, &item_ids)?;
    refresh(&app);
    Ok(())
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
pub async fn attach_image(
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
pub async fn export_markdown(app: AppHandle) -> Result<String, String> {
    let state = db::get_state(&app);
    let mut attachments: std::collections::HashMap<i64, Vec<String>> =
        std::collections::HashMap::new();
    for attachment in &state.attachments {
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
    export_markdown_into(&dir, &state.sections, &state.items, &attachments)?;
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
    let mut unfiled: Vec<&db::Item> = vec![];
    let mut by_section: Vec<(i64, &db::Item)> = vec![];
    for item in items {
        match item.section_id {
            None => unfiled.push(item),
            Some(section) => by_section.push((section, item)),
        }
    }
    for item in &unfiled {
        out.push_str(&format!("- {}\n", item.content.replace('\n', "\n  ")));
    }
    for section in sections {
        if !unfiled.is_empty() {
            out.push('\n');
            unfiled.clear();
        }
        let section_items: Vec<_> = by_section
            .iter()
            .filter(|(id, _)| *id == section.id)
            .map(|(_, item)| *item)
            .collect();
        if section_items.is_empty() {
            continue;
        }
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
pub fn set_sound_on_capture(app: AppHandle, enabled: bool) {
    db::set_setting(&app, "sound_on_capture", if enabled { "1" } else { "0" });
}

#[tauri::command]
pub async fn get_sound_on_capture(app: AppHandle) -> bool {
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

/// The panel's whole world, re-read on every refresh and after every capture.
///
/// `async` for the threading, not for concurrency: Tauri runs a synchronous
/// command inline on the Windows main thread, and this one reads the whole
/// timeline, every attachment, the settings, the licence (Credential Manager
/// four to six times) and enumerates microphones — all on the thread every
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
pub async fn add_image(app: AppHandle, data_base64: String, ext: String) -> Result<i64, String> {
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
        _ => return None,
    };
    let built = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(
            crate::panel::COMPANION_WIDTH,
            crate::panel::COMPANION_HEIGHT,
        )
        .min_inner_size(min.0, min.1)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .shadow(false)
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
    let Some(win) = companion(app, label) else {
        return;
    };
    if label == "draw" {
        // Draw's delivery target, remembered as it opens. A panel button runs
        // this with the panel in front, so the app the panel was summoned over
        // stands in for a foreground that is one of ours; the gesture path has
        // already recorded the person's own app, and sees the same foreground
        // again here.
        remember_draw_target();
    }
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
    // An open companion just goes away, and putting a window away never needs a
    // licence. Only opening one does, and until then nothing is created: a
    // restricted click must not leave a hidden window behind it.
    if let Some(win) = app.get_webview_window(&label) {
        if win.is_visible().unwrap_or(false) && !win.is_minimized().unwrap_or(false) {
            crate::panel::hide_companion(&app, &win);
            crate::panel::focus_panel(&app);
            return Ok(());
        }
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
    let Some(win) = companion(app, label) else {
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
        // The person is still in their own app here: this is the window the
        // finished sketch goes back to.
        remember_draw_target();
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

/// The window a sketch is delivered to: the app Draw was opened over. The
/// gesture runs while the person is still in their app, so the live foreground
/// is the target; a panel button runs with the panel in front, so the window
/// the panel was summoned over stands in. Never one of ours — a sketch must
/// land where it was drawn for, and the clipboard is the honest fallback when
/// knowledge of that window is gone.
static DRAW_TARGET: AtomicIsize = AtomicIsize::new(0);

/// Remember where Draw was opened over. Called as the window opens, from the
/// gesture path and from the panel's own show path.
fn remember_draw_target() {
    let foreground = crate::fg::current();
    let remembered = crate::panel::remembered_foreground();
    DRAW_TARGET.store(
        draw_target_choice(
            foreground,
            crate::fg::is_ours(foreground),
            remembered,
            crate::fg::is_ours(remembered),
        ),
        Ordering::SeqCst,
    );
}

/// The rule, kept pure so it has tests: the live foreground when it is not
/// ours, the panel's memory when it is, and nothing at all when both are
/// Bard's own windows or absent.
fn draw_target_choice(
    foreground: isize,
    foreground_is_ours: bool,
    remembered: isize,
    remembered_is_ours: bool,
) -> isize {
    if foreground != 0 && !foreground_is_ours {
        return foreground;
    }
    if remembered != 0 && !remembered_is_ours {
        return remembered;
    }
    0
}

/// What every failed hand-off says. The sketch is already on the clipboard —
/// the window copies it before it asks — so the person is one keystroke away
/// wherever they are.
const DRAW_HANDOFF_FALLBACK: &str = "Saved and copied — paste it with Ctrl+V.";

/// How long the hand-off waits for the remembered window to come forward. The
/// hide and the focus are synchronous; this is the settle for the OS's own
/// focus arbitration, bounded so the click never feels swallowed.
const DRAW_SEND_SETTLE: Duration = Duration::from_millis(160);

/// Deliver the sketch on the clipboard to the app Draw was opened over.
///
/// The companion is hidden and the remembered app is brought forward before
/// the paste chord, because the chord goes wherever the focus is when it is
/// sent. A missing or unusable target, or Bard still holding the foreground
/// when the chord is due, ends in the person-readable fallback instead — a
/// chord landing in the wrong window is worse than one the person makes
/// themselves.
#[tauri::command]
pub fn send_drawing(app: AppHandle) -> Result<(), String> {
    let target = DRAW_TARGET.load(Ordering::SeqCst);
    if target == 0 || crate::fg::is_ours(target) {
        crate::crash::record_event("draw:send-no-target");
        return Err(DRAW_HANDOFF_FALLBACK.into());
    }
    if let Some(win) = app.get_webview_window("draw") {
        crate::panel::hide_companion(&app, &win);
    }
    crate::fg::focus(target);
    std::thread::sleep(DRAW_SEND_SETTLE);
    if crate::fg::foreground_is_ours() {
        crate::crash::record_event("draw:send-focus-stolen");
        return Err(DRAW_HANDOFF_FALLBACK.into());
    }
    crate::paste::send_paste(&app);
    crate::crash::record_event("draw:send-pasted");
    Ok(())
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
    let picked = app
        .dialog()
        .file()
        .set_title("Save sketch")
        .add_filter("SVG image", &["svg"])
        .set_file_name(svg_file_name(&suggested_name))
        .blocking_save_file();
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
    if let Err(error) = crate::license::require(&app) {
        license_blocked(&app, error.clone());
        return Err(error);
    }
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
pub async fn clear_completed(app: AppHandle) -> Result<(), String> {
    let ids: Vec<i64> = db::completed_items(&app)
        .into_iter()
        .map(|item| item.id)
        .collect();
    db::delete_items_with_files(&app, &ids)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub async fn clear_history(app: AppHandle) -> Result<(), String> {
    db::clear_history(&app)?;
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub async fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
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
        // The shelf slides out now instead of vanishing, so the paste cannot be
        // timed off a fixed sleep: too early and the chord lands in the shelf
        // itself, and there is no later chance. Wait for the handshake — the
        // window is hidden and the foreground is back by the time it is open —
        // and paste anyway when it never comes, because not pasting is the one
        // outcome the chord cannot have.
        crate::panel::wait_hidden(PASTE_BACK_WAIT);
        crate::paste::send_paste(&app);
    });
}

/// How long `paste_back` waits for the shelf to finish sliding away. Longer
/// than the hide's own wait for the frontend's ack, so a listening frontend is
/// never cut off, and bounded, so a renderer that never answers cannot swallow
/// the paste.
const PASTE_BACK_WAIT: Duration = Duration::from_millis(700);

#[tauri::command]
pub async fn copy_item(app: AppHandle, id: i64) -> Result<(), String> {
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
pub async fn prune_history_now(app: AppHandle) -> Result<usize, String> {
    let mut removed = 0usize;
    for origin in ["clipboard", "dictation"] {
        removed += db::prune_expired(&app, origin)?;
    }
    removed += db::prune_history(&app, db::AUTOMATIC_ITEM_CEILING)?;
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
    crate::license::erase_local(&app).map_err(|error| error.to_string())?;
    crate::cloud::delete_api_key().map_err(|error| error.to_string())?;
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
    crate::paste::image_data(&app, &item.image_path?)
}

#[tauri::command]
pub async fn attachment_image_data(app: AppHandle, id: i64) -> Option<String> {
    let path = db::attachment_path(&app, id)?;
    crate::paste::image_data(&app, &path)
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
    crate::paste::image_url(&item.image_path?)
}

/// An attachment thumbnail's picture, on the same terms as [`item_cover_url`].
#[tauri::command]
pub async fn attachment_cover_url(app: AppHandle, id: i64) -> Option<String> {
    let path = db::attachment_path(&app, id)?;
    crate::paste::image_url(&path)
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
    // whether the Windows opener splits on either character inside a single
    // argument is a behaviour of Explorer this project should not depend on.
    // The two real call sites pass fixed `https://miiso.dev/...` paths, so a
    // tight allowlist costs nothing.
    let allowed = url.starts_with("https://miiso.dev/")
        && url.len() <= 256
        && url
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/:.-_=".contains(c));
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
pub fn set_capture_double_shift(app: AppHandle, enabled: bool) {
    db::set_setting(
        &app,
        "capture_double_shift",
        if enabled { "1" } else { "0" },
    );
}

/// Turn the dictation review sheet on or off. Off by default, and stored as a
/// setting so the take path reads it without knowing who changed it.
#[tauri::command]
pub fn set_dictation_review(app: AppHandle, enabled: bool) {
    db::set_setting(
        &app,
        crate::dictation::REVIEW_SETTING_KEY,
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

/// Dock the shelf to one of the screen's four edges. A visible shelf moves the
/// moment the edge is chosen — the edge is the thing the person is looking at —
/// and the next show anchors to whatever was stored.
#[tauri::command]
pub fn set_shelf_edge(app: AppHandle, edge: String) -> Result<(), String> {
    let Some(edge) = crate::geometry::Edge::checked(&edge) else {
        return Err("The shelf docks to the top, right, bottom or left edge.".into());
    };
    db::set_setting_checked(&app, crate::geometry::EDGE_SETTING, edge.as_str())?;
    if let Some(win) = app.get_webview_window("main") {
        if win.is_visible().unwrap_or(false) {
            // Window operations are a Windows-main-thread responsibility, and
            // this is called from whichever window's Settings pane it came from.
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(win) = handle.get_webview_window("main") {
                    crate::geometry::anchor(&handle, &win, edge);
                }
            });
        }
    }
    // Every window re-reads what it needs on this, so a panel that is already
    // out can lay itself out for the new edge instead of waiting for the next
    // show. It is the same signal every other setting change raises.
    refresh(&app);
    // Its own event besides the refresh, so a shelf that is already out can
    // re-lay itself out the moment the edge changes rather than one round-trip
    // later.
    let _ = app.emit("shelf-edge", edge.as_str());
    Ok(())
}

/// The panel's sign that the shelf has finished sliding out. The window is not
/// hidden and the foreground does not go back until this arrives, so the slide
/// is never cut off by the window disappearing underneath it.
#[tauri::command]
pub fn shelf_hidden() {
    crate::panel::shelf_hidden_ack();
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
pub async fn set_model(app: AppHandle, id: String) -> Result<(), String> {
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

/// Confirm a reviewed take with the sheet's current body: paste it into the
/// window the take started in, or file it in the timeline if Bard had focus.
#[tauri::command]
pub fn dictation_confirm(app: AppHandle, text: String) -> Result<(), String> {
    crate::dictation::confirm(&app, text)
}

/// Put the sheet's current body on the clipboard, leaving the sheet up.
#[tauri::command]
pub fn dictation_copy(app: AppHandle, text: String) -> Result<(), String> {
    crate::dictation::copy(&app, text)
}

/// Dismiss the sheet: the body stays on the clipboard best-effort, and the
/// take ends in Idle.
#[tauri::command]
pub fn dictation_dismiss(app: AppHandle, text: String) {
    crate::dictation::dismiss(&app, text);
}

/// The clipboard's image as a data URL, or null when it holds none (or the
/// licence restricts content). Draw's Ctrl+V intake — the pixels come from the
/// same clipboard Bard's monitor reads, with the same size caps.
#[tauri::command]
pub fn clipboard_image_data(app: AppHandle) -> Option<String> {
    if crate::license::require(&app).is_err() {
        return None;
    }
    let mut clipboard = arboard::Clipboard::new().ok()?;
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
mod draw_tests {
    use super::*;

    #[test]
    fn the_live_foreground_is_the_delivery_target() {
        assert_eq!(draw_target_choice(12, false, 0, false), 12);
        // The live foreground beats the panel's older memory.
        assert_eq!(draw_target_choice(12, false, 34, false), 12);
    }

    #[test]
    fn a_foreground_of_ours_falls_back_to_the_panels_memory() {
        assert_eq!(draw_target_choice(12, true, 34, false), 34);
        // No foreground at all is the same question as one of ours.
        assert_eq!(draw_target_choice(0, false, 34, false), 34);
    }

    #[test]
    fn bard_itself_is_never_a_delivery_target() {
        assert_eq!(draw_target_choice(12, true, 34, true), 0);
        assert_eq!(draw_target_choice(0, false, 34, true), 0);
        assert_eq!(draw_target_choice(0, false, 0, false), 0);
    }

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
