#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod catalog;
mod cloud;
mod commands;
mod crash;
mod credential;
mod db;
mod dictation;
mod fg;
mod geometry;
mod gesture;
mod guardian;
mod hf;
mod hook;
mod license;
mod monitor;
mod packaging;
mod panel;
mod paste;
mod tray;
mod updater;

use std::sync::Mutex;

use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

use crate::db::Db;

/// Passed by the autostart registration, and by nothing else.
const AUTOSTART_FLAG: &str = "--autostart";

/// Tauri resolves the local application-data directory from the bundle
/// identifier as `%LOCALAPPDATA%\<identifier>`. Clipboard history, transcripts,
/// images, and models deliberately stay off roaming profiles. The database is
/// opened before setup so every later read is safe.
fn diagnostics_dir() -> std::path::PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("APPDATA"))
        .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());
    std::path::Path::new(&base)
        .join("com.miisodev.bard")
        .join("logs")
}

fn local_data_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var("LOCALAPPDATA").ok()?;
    Some(std::path::Path::new(&base).join("com.miisodev.bard"))
}

fn legacy_roaming_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(std::path::Path::new(&base).join("com.miisodev.bard"))
}

fn move_legacy_contents(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    if !source.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if to.exists() {
            // A folder present on both sides — `images` and `models` are
            // created locally at startup — is merged file by file; skipping it
            // whole stranded every image and model in the roaming profile.
            // A file present on both sides keeps the local copy: a remaining
            // roaming file is evidence, not permission to destroy one side of
            // the user's data.
            if from.is_dir() && to.is_dir() {
                move_legacy_contents(&from, &to)?;
            }
            continue;
        }
        if std::fs::rename(&from, &to).is_err() {
            copy_then_remove(&from, &to)?;
        }
    }
    let _ = std::fs::remove_dir(source);
    Ok(())
}

fn copy_then_remove(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    if source.is_dir() {
        std::fs::create_dir_all(destination)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_then_remove(&entry.path(), &destination.join(entry.file_name()))?;
        }
        std::fs::remove_dir(source)
    } else {
        std::fs::copy(source, destination)?;
        std::fs::remove_file(source)
    }
}

fn migrate_roaming_data(destination: &std::path::Path) -> std::io::Result<()> {
    if let Some(source) = legacy_roaming_dir() {
        if source != destination {
            move_legacy_contents(&source, destination)?;
        }
    }
    std::fs::create_dir_all(destination)
}

fn open_database() -> Option<rusqlite::Connection> {
    let dir = local_data_dir()?;
    // A file the migration cannot move stays where it is and keeps working
    // from there; it is never a reason to start without a database.
    if let Err(error) = migrate_roaming_data(&dir) {
        eprintln!("Bard could not finish moving roaming data: {error}");
    }
    std::fs::create_dir_all(&dir).ok()?;
    let connection = db::init(&dir.join("bard.db")).ok()?;
    if let Some(legacy) = legacy_roaming_dir() {
        db::rebase_legacy_paths(&connection, &legacy, &dir);
    }
    Some(connection)
}

fn usable_paths(args: &[String]) -> Vec<String> {
    args.iter()
        .skip(1)
        .filter(|arg| {
            !arg.starts_with("--") && !arg.starts_with('/') && std::path::Path::new(arg).is_file()
        })
        .cloned()
        .collect()
}

fn ingest_startup_paths(app: &tauri::AppHandle, args: &[String]) -> bool {
    let paths = usable_paths(args);
    if paths.is_empty() {
        return false;
    }
    commands::ingest_paths(app, paths);
    panel::show_now(app);
    true
}

fn main() {
    crash::install(&diagnostics_dir());
    crash::record_event("app:start");
    guardian::install_console_handler();
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|a| a == "--guardian") {
        guardian::run();
        return;
    }
    // A clean-exit marker belongs to the previous process. Leaving it on disk
    // would make a crash during this launch look clean to the new guardian.
    guardian::clear_stale_marker();
    // The old relaunch watchdog could not distinguish a WebView2 startup
    // failure from a person choosing End Task. Relicensing a deliberate kill
    // is worse than surfacing a crash, so the guardian is opt-in for diagnosis
    // and is no longer part of normal operation.
    if std::env::var("BARD_ENABLE_GUARDIAN").is_ok()
        && !args.iter().any(|a| a == "--restarted")
        && std::env::var("BARD_NO_GUARDIAN").is_err()
    {
        if let Some(exe) = args.first() {
            if exe.contains(':') || exe.contains('\\') || exe.contains('/') {
                guardian::spawn(exe);
            }
        }
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            guardian::mark_clean_exit();
            ingest_startup_paths(app, &args);
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        // The database is opened here, before the builder runs, rather than in
        // setup(). Windows declared in tauri.conf are created before setup(),
        // so anything reacting to a window event — a focus change, the webview
        // asking for state — could reach the database while it was still None
        // and take the whole app down on an unwrap. Under `panic = "abort"`
        // that is a silent exit with no window and no message.
        .manage(Db(Mutex::new(open_database())))
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = local_data_dir()
                .ok_or("Bard could not resolve its local application-data directory.")?;
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
            crate::license::spawn_revalidation(handle.clone());
            let startup_args: Vec<String> = std::env::args().collect();
            let launched_with_paths = ingest_startup_paths(&handle, &startup_args);

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
            //
            // The Store copy is started at login by its manifest's startup
            // task, which Windows owns; a Run-key entry from inside a package
            // would only land in the package's private registry.
            if !cfg!(debug_assertions)
                && !packaging::is_packaged()
                && db::get_setting(&handle, "autostart_enabled") == Some("1".into())
            {
                let _ = handle.autolaunch().enable();
            }

            // Retention is the app's central data promise, so a failed pass is
            // recorded and reported rather than discarded. It used to be
            // `let _ =`, which made a failure indistinguishable from "nothing
            // expired".
            for origin in ["clipboard", "dictation"] {
                if let Err(error) = db::cleanup_origin(&handle, origin) {
                    crate::crash::record_event("db:startup-retention-failed");
                    log::warn!("startup retention pass failed for {origin}: {error}");
                }
            }
            match db::sweep_orphaned_images(&handle) {
                Ok(0) => {}
                Ok(count) => crate::crash::record_event(&format!("db:orphans-swept:{count}")),
                Err(error) => log::warn!("orphaned image sweep failed: {error}"),
            }

            monitor::start(handle.clone());
            capture::start(handle.clone());
            dictation::init(handle.clone());

            tray::create(app)?;
            // The panel is a docked shelf now: it anchors itself to its edge —
            // on the monitor its setting names, or under the pointer — on every
            // show, so there is no saved rectangle to restore and nothing to
            // place at startup. Its window stays hidden until the first show.

            // Opening the app should open the app. It lives in the tray, but
            // launching it and being shown nothing reads as a failure to
            // start — so the panel appears unless Windows launched us at
            // login, which is the one case where staying quiet is the point.
            let launched_at_login = std::env::args().any(|a| a == AUTOSTART_FLAG)
                || (packaging::is_packaged() && packaging::launched_near_logon());
            if !launched_at_login && !launched_with_paths {
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
                match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        if let Some(companion) =
                            window.app_handle().get_webview_window(window.label())
                        {
                            panel::hide_companion(window.app_handle(), &companion);
                        }
                        panel::focus_panel(window.app_handle());
                    }
                    WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                        geometry::remember_soon(window.app_handle(), window.label());
                    }
                    _ => {}
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
                //
                // Moved and Resized are deliberately not handled for the
                // panel: they are the docking code's own anchoring, not a place
                // a person put the window, and remembering them would fill
                // `geometry:main` with a rectangle nothing reads.
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
            commands::toggle_companion,
            commands::set_sort,
            commands::delete_item,
            commands::set_favorite,
            commands::set_pinned,
            commands::set_done,
            commands::clear_completed,
            commands::clear_history,
            commands::clear_captures,
            commands::delete_all_data,
            commands::copy_text,
            commands::copy_item,
            commands::paste_back,
            commands::copy_attachment,
            commands::item_image_data,
            commands::attachment_image_data,
            commands::set_theme,
            commands::set_onboarding_completed,
            commands::set_autostart,
            commands::open_startup_settings,
            commands::open_link,
            commands::get_appearance,
            commands::set_background,
            commands::set_draw_background,
            commands::set_capture_double_shift,
            commands::capture_now,
            commands::save_window_geometry,
            commands::minimize_panel,
            commands::show_panel,
            commands::hide_panel,
            commands::toggle_panel,
            commands::set_shelf_edge,
            commands::shelf_hidden,
            commands::get_models,
            commands::get_model,
            commands::set_model,
            commands::download_model,
            commands::download_model_cancel,
            commands::delete_model,
            commands::dictate_start,
            commands::dictate_stop,
            commands::dictate_cancel,
            commands::set_dictation_overlay_position,
            commands::set_dictation_review,
            commands::dictation_confirm,
            commands::dictation_copy,
            commands::dictation_dismiss,
            commands::create_section,
            commands::set_active_section,
            commands::rename_section,
            commands::delete_section,
            commands::list_packs,
            commands::get_pack,
            commands::create_pack,
            commands::rename_pack,
            commands::delete_pack,
            commands::pack_add_items,
            commands::pack_remove_items,
            commands::pack_reorder,
            commands::update_item,
            commands::attach_image,
            commands::detach_attachment,
            commands::add_note_to_image,
            commands::export_markdown,
            commands::set_export_dir,
            commands::get_export_dir,
            commands::set_active_item,
            commands::set_retention_days,
            commands::get_retention_days,
            commands::prune_history_now,
            commands::set_sound_on_capture,
            commands::get_sound_on_capture,
            commands::get_microphones,
            commands::set_microphone,
            commands::hf_cached_models,
            commands::get_detected_models,
            commands::check_model,
            commands::activate_detected_model,
            commands::forget_detected_model,
            commands::hf_list_repo,
            commands::hf_install,
            commands::hf_use_cached,
            commands::remove_custom_model,
            commands::get_custom_models,
            commands::get_postprocessing,
            commands::save_postprocessing,
            commands::test_postprocessing,
            commands::test_postprocessing_cancel,
            commands::get_crash_report,
            commands::get_license,
            commands::start_trial,
            commands::activate_license,
            commands::refresh_license,
            commands::deactivate_license,
            commands::clear_license,
            commands::check_for_update,
            commands::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run bard");
}
