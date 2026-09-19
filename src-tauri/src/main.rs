#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod commands;
mod db;
mod dictation;
mod glass;
mod guardian;
mod hf;
mod monitor;
mod panel;
mod paste;
mod tray;

use std::sync::Mutex;

use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

use crate::db::Db;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|a| a == "--guardian") {
        guardian::run();
        return;
    }
    if !args.iter().any(|a| a == "--restarted") && std::env::var("BARD_NO_GUARDIAN").is_err() {
        if let Some(exe) = args.get(0) {
            if exe.contains(':') || exe.contains('\\') || exe.contains('/') {
                guardian::spawn(exe);
            }
        }
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            guardian::mark_clean_exit();
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

            let stored_keybinds = db::get_setting(&handle, "keybinds").unwrap_or_default();
            if stored_keybinds.is_empty() || stored_keybinds == "{}" {
                db::set_setting(
                    &handle,
                    "keybinds",
                    "{\"capture\":\"Ctrl+Shift+C\",\"panel\":\"Ctrl+Shift+Space\",\"dictate\":\"Ctrl+Space\"}",
                );
            }
            capture::repair_keybinds(&handle);

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
            commands::download_model_cancel,
            commands::delete_model,
            commands::dictate_start,
            commands::dictate_stop,
            commands::dictate_cancel,
            commands::create_section,
            commands::set_active_section,
            commands::rename_section,
            commands::delete_section,
            commands::update_item,
            commands::add_files,
            commands::attach_image,
            commands::detach_attachment,
            commands::add_note_to_image,
            commands::export_markdown,
            commands::set_export_dir,
            commands::get_export_dir,
            commands::set_sound_on_capture,
            commands::get_sound_on_capture,
            commands::get_microphones,
            commands::set_microphone,
            commands::hf_cached_models,
            commands::hf_list_repo,
            commands::hf_install,
            commands::hf_use_cached,
            commands::remove_custom_model,
            commands::get_custom_models,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run bard");
}