#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ask;
mod capture;
mod commands;
mod db;
mod dictation;
mod glass;
mod monitor;
mod panel;
mod paste;
mod tray;

use std::sync::Mutex;

use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

use crate::db::Db;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            panel::show_now(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(Db(Mutex::new(None)))
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let state = db::init(&data_dir.join("bard.db"))?;
            *app.state::<Db>().0.lock().unwrap() = Some(state);
            let images_dir = data_dir.join("images");
            std::fs::create_dir_all(&images_dir)?;
            app.manage(commands::ImagesDir(images_dir));

            if db::get_setting(&handle, "autostart_enabled") == Some("1".into()) {
                let _ = handle.autolaunch().enable();
            }

            monitor::start(handle.clone());
            capture::start(handle.clone());
            dictation::init(handle.clone());

            tray::create(app)?;

            glass::apply_all(&handle, &db::get_setting(&handle, "theme").unwrap_or_default());

            let main = app.get_webview_window("main").unwrap();
            panel::position(&main);

            if db::get_setting(&handle, "onboarding_completed") != Some("1".into()) {
                panel::show_now(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    panel::hide_now(window.app_handle());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::get_setting_str,
            commands::add_item,
            commands::add_image,
            commands::delete_item,
            commands::set_favorite,
            commands::clear_history,
            commands::copy_text,
            commands::copy_item,
            commands::image_data,
            commands::set_theme,
            commands::set_keybinds,
            commands::set_onboarding_completed,
            commands::set_autostart,
            commands::set_capture_double_shift,
            commands::capture_now,
            commands::save_window_geometry,
            commands::minimize_panel,
            commands::show_panel,
            commands::hide_panel,
            commands::toggle_panel,
            commands::get_models,
            commands::get_model,
            commands::set_model,
            commands::download_model,
            commands::delete_model,
            commands::dictate_start,
            commands::dictate_stop,
            commands::dictate_cancel,
            commands::set_provider,
            commands::clear_provider,
            commands::ask,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run bard");
}