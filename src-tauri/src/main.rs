#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backup;
mod catalog;
mod clip;
mod commands;
mod crash;
mod db;
mod desktop;
mod dictation;
mod feedback;
mod front;
mod geometry;
mod guardian;
mod hf;
#[cfg(target_os = "linux")]
mod linux_portal;
mod monitor;
#[cfg(test)]
#[path = "../native_runtime.rs"]
mod native_runtime;
mod packaging;
mod panel;
mod paths;
mod pill;
mod shortcuts;
mod sound;
mod tray;
mod ttl;
mod updater;

use std::sync::Mutex;

use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

use crate::db::Db;

/// Passed by the autostart registration, and by nothing else.
const AUTOSTART_FLAG: &str = "--autostart";

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

fn open_database() -> (rusqlite::Connection, Option<String>) {
    let Some(dir) = paths::data_dir() else {
        // No resolvable data directory. Keep the app running and say plainly
        // that this session's history is not being kept.
        return (
            db::in_memory(),
            Some(
                "Bard could not find its local application-data folder, so this session's \
                 history will not be kept after Bard closes."
                    .into(),
            ),
        );
    };
    let mut notices: Vec<String> = Vec::new();
    // A file the migration cannot move stays where it is and keeps working
    // from there; it is never a reason to start without a database. It is
    // recorded for the diagnostic report rather than printed: a windowed
    // release build has no stderr to print it to, and the report is where the
    // evidence is actually read.
    if let Err(error) = migrate_roaming_data(&dir) {
        crate::crash::record_event(&format!("db:roaming-migration-failed:{:?}", error.kind()));
        log::warn!("Bard could not finish moving roaming data: {error}");
        notices.push(
            "Bard could not finish moving data from its old folder; some files may still be \
             in the old place. Copy a diagnostic report in Settings › Data to see more."
                .into(),
        );
    }
    let _ = std::fs::create_dir_all(&dir);
    let (connection, notice) = db::open_or_recover(&dir.join("bard.db"));
    if let Some(notice) = notice {
        notices.push(notice);
    }
    if let Some(legacy) = legacy_roaming_dir() {
        db::rebase_legacy_paths(&connection, &legacy, &dir);
    }
    // A catalog update can retire the model a returning user had chosen. Unset
    // the stored choice here, before any window can read it, so the settings
    // surface and dictation both say "no model chosen" rather than silently
    // disagreeing with a setting neither can resolve.
    db::clear_stale_model_selection(&connection);
    (
        connection,
        if notices.is_empty() {
            None
        } else {
            Some(notices.join(" "))
        },
    )
}

/// Whether this argument reads as one of the app's own flags rather than a
/// name to ingest.
///
/// `--` opens a flag everywhere. On Windows `/` does too, and every Windows
/// path carries a drive letter or a backslash, so the `/` rule there never
/// hid a real file. On POSIX it hid every one of them: `/home/me/note.txt`
/// starts with `/` and is exactly what a file manager's "Open with Bard"
/// hands over. So the `/` half only follows the flag rule on the platform it
/// came from, and everywhere else an argument is a path when the filesystem
/// actually has something at that name.
fn is_flag(arg: &str) -> bool {
    arg.starts_with("--") || (cfg!(target_os = "windows") && arg.starts_with('/'))
}

/// The arguments that name files to ingest at startup.
fn usable_paths(args: &[String]) -> Vec<String> {
    args.iter()
        .skip(1)
        .filter(|arg| !is_flag(arg) && std::path::Path::new(arg).is_file())
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
    // Before anything can start a thread or initialise GTK.
    #[cfg(target_os = "linux")]
    desktop::prefer_xwayland_windows();
    crash::install(&paths::logs_dir());
    crash::record_event("app:start");
    guardian::install_console_handler();
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--check-speech-backends") {
        match transcribe_cpp::init_backends_default() {
            Ok(()) if transcribe_cpp::device_count() > 0 => {
                println!("Speech backends ready: {}", transcribe_cpp::device_count());
            }
            result => {
                eprintln!("Speech backends unavailable: {result:?}");
                std::process::exit(1);
            }
        }
        return;
    }
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
    // The database is opened here, before the builder runs, rather than in
    // setup(). Windows declared in tauri.conf are created before setup(), so
    // anything reacting to a window event — a focus change, the webview asking
    // for state — could reach the database before setup had a chance to open
    // it. `open_or_recover` always yields a working connection (a temporary
    // one when the file on disk cannot be used), so no read has to unwrap an
    // absent database, and under `panic = "abort"` a failed open used to be a
    // silent exit with no window and no message.
    let (connection, storage_notice) = open_database();
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
        .manage(Db(Mutex::new(connection)))
        .manage(db::DbNotice(Mutex::new(storage_notice)))
        .setup(|app| {
            let handle = app.handle().clone();
            // The data directory is wanted, not required. `open_database`
            // already decided how to run without one: an in-memory database and
            // a notice that says plainly that this session's history is not
            // being kept. Ending the process here instead would make that
            // fallback unreachable and give the person a silent exit rather
            // than the explanation.
            let data_dir = paths::data_dir();
            let images_dir = match &data_dir {
                Some(dir) => {
                    if let Err(error) = std::fs::create_dir_all(dir) {
                        crate::crash::record_event("db:data-dir-unavailable");
                        log::warn!("Bard could not use its data directory: {error}");
                    }
                    let images = dir.join("images");
                    let _ = std::fs::create_dir_all(&images);
                    images
                }
                None => {
                    crate::crash::record_event("db:data-dir-unresolved");
                    // Somewhere writable enough to hold this session's images,
                    // so the app still runs and still shows what it can.
                    std::env::temp_dir()
                        .join("com.miisodev.bard")
                        .join("images")
                }
            };
            // The shelf's image covers are loaded from the `asset` protocol, so
            // the images folder — and nothing else — goes into its scope. The
            // URL for a file is only ever handed out after `paste` has checked
            // that the file is inside this folder and inside the size cap, so
            // this is the second lock on the same door.
            if let Err(error) = handle
                .asset_protocol_scope()
                .allow_directory(&images_dir, false)
            {
                log::warn!("image covers will fall back to nothing: {error}");
            }
            app.manage(commands::ImagesDir(images_dir));
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
                && packaging::channel() != "flatpak"
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
            // A database that was just replaced or bypassed references none of
            // the images beside it, and the sweep deletes files no row
            // references — so after a recovery it would delete exactly the
            // images the quarantined file was set aside to keep. Skip this
            // launch's pass; a later launch with a working database sweeps
            // normally.
            if db::recovered_this_launch() {
                crate::crash::record_event("db:orphan-sweep-skipped-after-recovery");
            } else {
                match db::sweep_orphaned_images(&handle) {
                    Ok(0) => {}
                    Ok(count) => crate::crash::record_event(&format!("db:orphans-swept:{count}")),
                    Err(error) => log::warn!("orphaned image sweep failed: {error}"),
                }
            }

            feedback::init(&handle);
            monitor::start(handle.clone());
            dictation::init(handle.clone());
            shortcuts::start(&handle);
            // On Wayland the desktop grants shortcuts through its portal. Ask
            // once setup is done, and not again after the person declined.
            #[cfg(target_os = "linux")]
            if desktop::is_wayland()
                && db::get_setting(&handle, "onboarding_completed").as_deref() == Some("1")
                && db::get_setting(&handle, linux_portal::DECLINED_SETTING).as_deref() != Some("1")
            {
                let portal_app = handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = linux_portal::connect_at_launch(portal_app.clone()).await {
                        log::warn!("desktop shortcuts unavailable: {error}");
                        commands::refresh(&portal_app);
                    }
                });
            }

            tray::create(app)?;

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
            if matches!(window.label(), "settings" | "draw" | "guide") {
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
                    WindowEvent::Focused(focused) => {
                        panel::on_focus_change(window.app_handle(), *focused);
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
                WindowEvent::Focused(focused) => {
                    panel::on_focus_change(window.app_handle(), *focused);
                }
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                    panel::on_moved(window.app_handle());
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
            commands::open_guide,
            commands::open_draw_with_image,
            commands::save_svg,
            commands::set_draw_prefs,
            commands::open_settings,
            commands::toggle_companion,
            commands::set_sort,
            commands::delete_item,
            commands::set_favorite,
            commands::set_pinned,
            commands::clear_history,
            commands::clear_captures,
            commands::delete_all_data,
            commands::copy_item,
            commands::item_image_data,
            commands::item_cover_url,
            commands::clipboard_image_data,
            commands::set_theme,
            commands::set_onboarding_completed,
            commands::set_autostart,
            commands::configure_desktop,
            commands::open_startup_settings,
            commands::open_link,
            commands::get_appearance,
            commands::set_background,
            commands::set_draw_background,
            commands::show_panel,
            commands::hide_panel,
            commands::get_models,
            commands::set_model,
            commands::download_model,
            commands::download_model_cancel,
            commands::delete_model,
            commands::update_item,
            commands::export_markdown,
            commands::set_export_dir,
            commands::get_export_dir,
            commands::set_retention_days,
            commands::get_retention_days,
            commands::prune_history_now,
            commands::get_microphones,
            commands::set_microphone,
            commands::get_dictation_options,
            commands::set_dictation_options,
            commands::get_crash_report,
            commands::copy_diagnostic_report,
            backup::backup_now,
            backup::restore_from_backup,
            commands::check_for_update,
            commands::install_update,
            commands::set_dock,
            commands::get_feedback,
            commands::set_feedback,
            commands::get_stay_open,
            commands::set_stay_open,
            commands::hold_windows,
            commands::set_capture_paused,
            commands::notify_error,
            shortcuts::get_shortcuts,
            shortcuts::set_shortcut,
            shortcuts::reset_shortcuts,
            shortcuts::check_shortcuts,
            shortcuts::save_shortcuts,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run bard");
}
