#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod catalog;
mod commands;
mod db;
mod dictation;
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

/// Passed by the autostart registration, and by nothing else.
const AUTOSTART_FLAG: &str = "--autostart";

/// Tauri resolves the app data directory from the bundle identifier, and this
/// has to match it exactly: `%APPDATA%\<identifier>`. Opening the database
/// here rather than in setup() is what makes every later read safe.
fn open_database() -> Option<rusqlite::Connection> {
    let base = std::env::var("APPDATA").ok()?;
    let dir = std::path::Path::new(&base).join("com.miisodev.bard");
    std::fs::create_dir_all(&dir).ok()?;
    db::init(&dir.join("bard.db")).ok()
}

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
        // The autostart entry carries a flag so the app can tell "Windows
        // started me at login" from "someone opened me", and behave
        // differently: the first belongs in the tray, the second on screen.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_FLAG]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // The database is opened here, before the builder runs, rather than in
        // setup(). Windows declared in tauri.conf are created before setup(),
        // so anything reacting to a window event — a focus change, the webview
        // asking for state — could reach the database while it was still None
        // and take the whole app down on an unwrap. Under `panic = "abort"`
        // that is a silent exit with no window and no message.
        .manage(Db(Mutex::new(open_database())))
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            // Already opened before the builder ran; this only covers the case
            // where that failed, so the app still has a working database
            // rather than limping on with none.
            if app.state::<Db>().0.lock().unwrap().is_none() {
                let state = db::init(&data_dir.join("bard.db"))?;
                *app.state::<Db>().0.lock().unwrap() = Some(state);
            }
            let images_dir = data_dir.join("images");
            std::fs::create_dir_all(&images_dir)?;
            app.manage(commands::ImagesDir(images_dir));

            // Re-assert the login entry so it always points at *this* executable:
            // an install to a new location would otherwise leave Windows
            // launching the old path forever.
            //
            // Debug builds are excluded deliberately. Autostart registers
            // whatever executable is running, so a single `tauri dev` session
            // silently repointed the user's login entry at
            // target\debug\bard.exe — a build that only works while the Vite
            // dev server happens to be up. Development must not rewrite what
            // the machine launches at login.
            if !cfg!(debug_assertions)
                && db::get_setting(&handle, "autostart_enabled") == Some("1".into())
            {
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


            let main = app.get_webview_window("main").unwrap();
            panel::position(&main);

            // Opening the app should open the app. It lives in the tray, but
            // launching it and being shown nothing reads as a failure to
            // start — so the panel appears unless Windows launched us at
            // login, which is the one case where staying quiet is the point.
            let launched_at_login = std::env::args().any(|a| a == AUTOSTART_FLAG);
            if !launched_at_login {
                panel::show_now(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing a secondary window must hide it, not destroy it. Tauri
            // drops a closed window, after which get_webview_window returns
            // None and "open settings" silently does nothing — which is what
            // made settings and the canvas unreachable once either had been
            // closed.
            if matches!(window.label(), "settings" | "draw") {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
                return;
            }
            if window.label() != "main" {
                return;
            }
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    panel::hide_now(window.app_handle());
                }
                // The panel is summoned, so it dismisses itself the moment it
                // stops being the thing you are using. This lives here rather
                // than on the webview's blur event because the webview is not
                // told reliably when the *native* window loses activation —
                // which is exactly the case that matters, a click into another
                // application.
                WindowEvent::Focused(focused) => {
                    panel::on_focus_change(window.app_handle(), *focused);
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::get_setting_str,
            commands::add_item,
            commands::add_image,
            commands::open_draw,
            commands::open_settings,
            commands::delete_item,
            commands::set_favorite,
            commands::set_done,
            commands::clear_completed,
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