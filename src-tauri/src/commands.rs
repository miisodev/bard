use std::path::PathBuf;

use base64::Engine;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::db;

pub fn refresh(app: &AppHandle) {
    let _ = app.emit("refresh", ());
}

#[tauri::command]
pub fn hf_cached_models() -> Vec<crate::hf::CachedHfModel> {
    crate::hf::cached_models()
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
) -> Result<(), String> {
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
    crate::hf::register_cached(&app, &repo, &file, &local_path, size)
}

#[tauri::command]
pub fn remove_custom_model(app: AppHandle, id: String) {
    if let Some(path) = db::delete_custom_model(&app, &id) {
        if !path.starts_with(std::env::var("USERPROFILE").unwrap_or_default().as_str()) {
            let _ = std::fs::remove_file(&path);
        }
    }
    refresh(&app);
}

#[tauri::command]
pub fn get_custom_models(app: AppHandle) -> Vec<db::CustomModel> {
    db::list_custom_models(&app)
}

#[tauri::command]
pub fn get_setting_str(app: AppHandle, key: String) -> String {
    db::get_setting(&app, &key).unwrap_or_default()
}

pub fn dirs_documents() -> String {
    std::env::var("USERPROFILE")
        .map(|p| format!("{p}\\Documents"))
        .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string())
}

#[tauri::command]
pub fn create_section(app: AppHandle, name: String) -> i64 {
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
    db::rename_section(&app, id, &name);
    refresh(&app);
}

#[tauri::command]
pub fn delete_section(app: AppHandle, id: i64) {
    db::delete_section(&app, id);
    refresh(&app);
}

#[tauri::command]
pub fn update_item(app: AppHandle, id: i64, content: String) {
    db::update_item(&app, id, &content);
    refresh(&app);
}

#[tauri::command]
pub fn add_files(app: AppHandle, paths: Vec<String>) -> usize {
    let images = app.state::<ImagesDir>().0.clone();
    let mut added = 0;
    for p in paths {
        let ext = std::path::Path::new(&p)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tiff", "avif"].contains(&ext.as_str()) {
            let dest = unique_image_path(&images, &ext);
            if std::fs::copy(&p, &dest).is_ok() {
                db::insert_item(&app, "", "image", Some(&dest.to_string_lossy()));
                added += 1;
            }
        } else {
            db::insert_item(&app, &p, "text", None);
            added += 1;
        }
    }
    refresh(&app);
    added
}

#[tauri::command]
pub fn attach_image(app: AppHandle, item_id: i64, data_base64: String, ext: String) -> Result<i64, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64)
        .map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err("empty image".into());
    }
    let images = app.state::<ImagesDir>().0.clone();
    let path = unique_image_path(&images, &ext);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let id = db::insert_attachment(&app, item_id, &path.to_string_lossy());
    refresh(&app);
    Ok(id)
}

#[tauri::command]
pub fn detach_attachment(app: AppHandle, id: i64) {
    if let Some(path) = db::delete_attachment(&app, id) {
        let _ = std::fs::remove_file(path);
    }
    refresh(&app);
}

#[tauri::command]
pub fn add_note_to_image(app: AppHandle, id: i64) {
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
    let fallback = dirs_documents();
    let dir = db::get_setting(&app, "export_dir")
        .filter(|d| !d.is_empty() && std::path::Path::new(d).is_dir())
        .unwrap_or(fallback);
    let path = std::path::Path::new(&dir).join("bard-export.md");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn set_export_dir(app: AppHandle, path: String) {
    if path.is_empty() || std::path::Path::new(&path).is_dir() {
        db::set_setting(&app, "export_dir", &path);
    }
}

#[tauri::command]
pub fn get_export_dir(app: AppHandle) -> String {
    db::get_setting(&app, "export_dir").filter(|d| !d.is_empty()).unwrap_or_else(dirs_documents)
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
    let geo = serde_json::json!({ "x": x, "y": y, "width": width, "height": height });
    db::set_setting(&app, "window_geometry", &geo.to_string());
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
    db::insert_item(&app, &content, "text", None);
    refresh(&app);
}

#[tauri::command]
pub fn add_image(app: AppHandle, data_base64: String, ext: String) -> i64 {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64)
        .unwrap_or_default();
    if bytes.is_empty() {
        return 0;
    }
    let images = app.state::<ImagesDir>().0.clone();
    let path = unique_image_path(&images, &ext);
    let _ = std::fs::write(&path, bytes);
    // Returned so a caller that needs the item afterwards — the drawing
    // window's "Copy and Save" — does not have to guess which row it made.
    let id = db::insert_item(&app, "", "image", Some(&path.to_string_lossy()));
    refresh(&app);
    id
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    show_secondary(&app, "settings", false);
}

pub fn show_secondary(app: &AppHandle, label: &str, center: bool) {
    use tauri::Manager;
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    if center {
        let _ = win.center();
    }
    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
}

#[tauri::command]
pub fn open_draw(app: AppHandle) {
    show_secondary(&app, "draw", true);
}

#[tauri::command]
pub fn delete_item(app: AppHandle, id: i64) {
    let files = db::item_files(&app, id);
    db::delete_item(&app, id);
    for f in files {
        let _ = std::fs::remove_file(f);
    }
    refresh(&app);
}

#[tauri::command]
pub fn set_favorite(app: AppHandle, id: i64, favorite: bool) {
    db::set_favorite(&app, id, favorite);
    refresh(&app);
}

#[tauri::command]
pub fn set_done(app: AppHandle, id: i64, done: bool) {
    db::set_done(&app, id, done);
    refresh(&app);
}

#[tauri::command]
pub fn clear_completed(app: AppHandle) {
    for item in db::completed_items(&app) {
        if item.kind == "image" {
            if let Some(p) = item.image_path {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    db::clear_completed(&app);
    refresh(&app);
}

#[tauri::command]
pub fn clear_history(app: AppHandle) {
    for item in db::list_items(&app) {
        if item.kind == "image" {
            if let Some(p) = item.image_path {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    db::clear_history(&app);
    refresh(&app);
}

#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    crate::paste::set_text(&app, &text)
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
    Ok(())
}

#[tauri::command]
pub fn image_data(app: AppHandle, path: String) -> Option<String> {
    crate::paste::image_data(&app, &path)
}

#[tauri::command]
pub fn set_theme(app: AppHandle, theme: String) {
    db::set_setting(&app, "theme", &theme);
    crate::glass::apply_all(&app, &theme);
    // Every window reads the theme out of app state, so without this only the
    // window that made the change would repaint — which is exactly how the
    // panel stayed light while settings went dark.
    refresh(&app);
}

#[tauri::command]
pub fn set_keybinds(app: AppHandle, json: String) {
    db::set_setting(&app, "keybinds", &json);
    crate::capture::register_shortcuts(&app);
}

#[tauri::command]
pub fn set_onboarding_completed(app: AppHandle, done: bool) {
    db::set_setting(&app, "onboarding_completed", if done { "1" } else { "0" });
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch().enable().map_err(|e| e.to_string())?;
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())?;
    }
    db::set_setting(&app, "autostart_enabled", if enabled { "1" } else { "0" });
    Ok(())
}

#[tauri::command]
pub fn set_capture_double_shift(app: AppHandle, enabled: bool) {
    db::set_setting(&app, "capture_double_shift", if enabled { "1" } else { "0" });
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
pub fn set_model(app: AppHandle, id: String) {
    db::set_setting(&app, "model_id", &id);
    refresh(&app);
}

#[tauri::command]
pub async fn download_model(app: AppHandle, id: String) -> Result<(), String> {
    crate::dictation::download_model(&app, &id).await
}

#[tauri::command]
pub fn delete_model(app: AppHandle, id: String) {
    crate::dictation::delete_model(&app, &id);
    refresh(&app);
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

pub struct ImagesDir(pub PathBuf);

fn unique_image_path(dir: &PathBuf, ext: &str) -> PathBuf {
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