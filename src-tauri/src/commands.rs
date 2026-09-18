use std::path::PathBuf;

use base64::Engine;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::db;

pub fn refresh(app: &AppHandle) {
    let _ = app.emit("refresh", ());
}

#[tauri::command]
pub fn get_setting_str(app: AppHandle, key: String) -> String {
    db::get_setting(&app, &key).unwrap_or_default()
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
pub fn add_image(app: AppHandle, data_base64: String, ext: String) {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64)
        .unwrap_or_default();
    if bytes.is_empty() {
        return;
    }
    let images = app.state::<ImagesDir>().0.clone();
    let path = unique_image_path(&images, &ext);
    let _ = std::fs::write(&path, bytes);
    db::insert_item(&app, "", "image", Some(&path.to_string_lossy()));
    refresh(&app);
}

#[tauri::command]
pub fn delete_item(app: AppHandle, id: i64) {
    let item = db::list_items(&app).into_iter().find(|i| i.id == id);
    db::delete_item(&app, id);
    if let Some(i) = item {
        if i.kind == "image" {
            if let Some(p) = i.image_path {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    refresh(&app);
}

#[tauri::command]
pub fn set_favorite(app: AppHandle, id: i64, favorite: bool) {
    db::set_favorite(&app, id, favorite);
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
pub fn set_provider(app: AppHandle, base_url: String, api_key: String, model: String) {
    db::set_setting(&app, "provider_base_url", &base_url);
    db::set_setting(&app, "provider_api_key", &api_key);
    db::set_setting(&app, "provider_model", &model);
    refresh(&app);
}

#[tauri::command]
pub fn clear_provider(app: AppHandle) {
    db::set_setting(&app, "provider_api_key", "");
    refresh(&app);
}

#[tauri::command]
pub async fn ask(
    app: AppHandle,
    prompt: String,
    context_item_ids: Vec<i64>,
) -> Result<String, String> {
    crate::ask::ask(&app, &prompt, &context_item_ids).await
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