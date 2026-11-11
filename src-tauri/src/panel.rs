//! The main window: Bard's clipboard history.
//!
//! It is a compact, frameless window that either floats (moved and resized
//! like any window, and reopened where it was left) or docks to the left or
//! right edge of the screen at full height. A docked window behaves like a
//! shelf: always on top, off the taskbar, and away again the moment you click
//! elsewhere. A floating one stays until you close it.

use std::sync::atomic::{AtomicU64, Ordering};

use tauri::{AppHandle, Emitter, Manager};

use crate::geometry::Side;

pub const COMPANION_WIDTH: f64 = 720.0;
pub const COMPANION_HEIGHT: f64 = 620.0;

/// The first size of a floating main window, in logical pixels.
const FLOAT_WIDTH: f64 = 400.0;
const FLOAT_HEIGHT: f64 = 620.0;
/// The setting naming the dock: "float", "left" or "right".
pub const DOCK_SETTING: &str = "main_dock";
/// The docked width, in logical pixels.
const DOCK_WIDTH_SETTING: &str = "main_dock_width";
const DEFAULT_DOCK_WIDTH: f64 = 380.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dock {
    Float,
    Side(Side),
}

impl Dock {
    pub fn parse(value: &str) -> Option<Dock> {
        match value {
            "float" => Some(Dock::Float),
            "left" => Some(Dock::Side(Side::Left)),
            "right" => Some(Dock::Side(Side::Right)),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Dock::Float => "float",
            Dock::Side(Side::Left) => "left",
            Dock::Side(Side::Right) => "right",
        }
    }
}

pub fn dock(app: &AppHandle) -> Dock {
    crate::db::get_setting(app, DOCK_SETTING)
        .as_deref()
        .and_then(Dock::parse)
        .unwrap_or(Dock::Float)
}

fn dock_width(app: &AppHandle) -> f64 {
    crate::db::get_setting(app, DOCK_WIDTH_SETTING)
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|width| (260.0..=900.0).contains(width))
        .unwrap_or(DEFAULT_DOCK_WIDTH)
}

pub fn hide_companion(app: &AppHandle, win: &tauri::WebviewWindow) {
    crate::geometry::remember(app, win);
    if win.is_fullscreen().unwrap_or(false) {
        let _ = win.set_fullscreen(false);
    }
    let _ = win.hide();
    companions_changed(app);
    if win.label() == "settings" {
        release_settings_later(app);
    }
}

/// How long Settings stays loaded after it is put away.
const SETTINGS_LINGER: std::time::Duration = std::time::Duration::from_secs(120);
static SETTINGS_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Settings holds nothing that is not saved, and it is opened rarely, so it
/// is let go once it has been away for a while rather than keeping a whole
/// web process in memory for the life of the app. It is created again, the
/// way it was the first time, when it is next asked for.
fn release_settings_later(app: &AppHandle) {
    let generation = SETTINGS_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SETTINGS_LINGER);
        if SETTINGS_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if SETTINGS_GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            if let Some(win) = handle.get_webview_window("settings") {
                if !win.is_visible().unwrap_or(false) {
                    let _ = win.destroy();
                }
            }
        });
    });
}

pub fn companions_changed(app: &AppHandle) {
    let open = |label: &str| {
        app.get_webview_window(label)
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false)
    };
    let _ = app.emit_to(
        "main",
        "companions",
        serde_json::json!({ "draw": open("draw"), "settings": open("settings") }),
    );
}

pub fn focus_panel(app: &tauri::AppHandle) {
    if let Some(panel) = app.get_webview_window("main") {
        if panel.is_visible().unwrap_or(false) {
            let _ = panel.set_focus();
        }
    }
}

/// When the window was last shown, so the focus change of showing it is not
/// mistaken for clicking away.
static SHOWN_AT: AtomicU64 = AtomicU64::new(0);
const SETTLE_MS: u64 = 600;

fn just_shown() -> bool {
    let at = SHOWN_AT.load(Ordering::SeqCst);
    at > 0 && (crate::db::now_ms() as u64).saturating_sub(at) < SETTLE_MS
}

/// Window chrome for the current mode: the decorated setup window, a floating
/// window, or a docked shelf.
fn dress(app: &AppHandle, win: &tauri::WebviewWindow) {
    let setup = crate::db::get_setting(app, "onboarding_completed").as_deref() != Some("1");
    if setup {
        let _ = win.set_title("Set up Bard");
        let _ = win.set_decorations(true);
        let _ = win.set_shadow(true);
        let _ = win.set_always_on_top(false);
        let _ = win.set_skip_taskbar(false);
        let _ = win.set_resizable(true);
        let _ = win.set_min_size(Some(tauri::LogicalSize::new(480.0, 440.0)));
        crate::geometry::restore(app, win, 620.0, 680.0);
        return;
    }
    let _ = win.unmaximize();
    let _ = win.set_title("Bard");
    let _ = win.set_decorations(false);
    let _ = win.set_resizable(true);
    let _ = win.set_min_size(Some(tauri::LogicalSize::new(300.0, 360.0)));
    match dock(app) {
        Dock::Float => {
            let _ = win.set_shadow(true);
            let _ = win.set_always_on_top(false);
            let _ = win.set_skip_taskbar(false);
            crate::geometry::restore(app, win, FLOAT_WIDTH, FLOAT_HEIGHT);
        }
        Dock::Side(side) => {
            let _ = win.set_shadow(false);
            let _ = win.set_always_on_top(true);
            let _ = win.set_skip_taskbar(true);
            crate::geometry::dock(app, win, side, dock_width(app));
        }
    }
}

pub fn show_now(app: &tauri::AppHandle) {
    SHOWN_AT.store(crate::db::now_ms() as u64, Ordering::SeqCst);
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || show_now_inner(&task_app));
}

fn show_now_inner(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let _ = win.unminimize();
    if !win.is_visible().unwrap_or(false) {
        dress(app, &win);
    }
    let _ = win.show();
    let _ = win.set_focus();
    let _ = app.emit_to("main", "panel-shown", dock(app).name());
    companions_changed(app);
}

pub fn hide_now(app: &tauri::AppHandle) {
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || hide_now_inner(&task_app));
}

fn hide_now_inner(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    if dock(app) == Dock::Float {
        crate::geometry::remember(app, &win);
    }
    let _ = win.hide();
}

/// Re-place the window after the dock setting changed.
pub fn apply_dock(app: &AppHandle) {
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(win) = task_app.get_webview_window("main") {
            dress(&task_app, &win);
            let _ = win.show();
            let _ = win.set_focus();
        }
        let _ = task_app.emit_to("main", "panel-shown", dock(&task_app).name());
    });
}

/// Remember the window after the person moved or resized it: the rectangle
/// when floating, the width when docked.
pub fn on_moved(app: &AppHandle) {
    match dock(app) {
        Dock::Float => crate::geometry::remember_soon(app, "main"),
        Dock::Side(_) => {
            let Some(win) = app.get_webview_window("main") else {
                return;
            };
            if !win.is_visible().unwrap_or(false) {
                return;
            }
            let scale = win.scale_factor().unwrap_or(1.0);
            if let Ok(size) = win.inner_size() {
                let logical = (size.width as f64 / scale).round();
                if (260.0..=900.0).contains(&logical) {
                    // A drag on the edge of a docked window fires a move event
                    // for every pixel. Writing the setting for each one is a
                    // database write under the global lock, dozens of times a
                    // second; the width only changes when it actually changes.
                    let previous = crate::db::get_setting(app, DOCK_WIDTH_SETTING)
                        .and_then(|value| value.parse::<f64>().ok());
                    if previous != Some(logical) {
                        crate::db::set_setting(app, DOCK_WIDTH_SETTING, &logical.to_string());
                    }
                }
            }
        }
    }
}

static FOCUS_GENERATION: AtomicU64 = AtomicU64::new(0);
/// Whether a focus check is already waiting to run.
static FOCUS_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Set while one of Bard's own native dialogs (a save or folder picker) is
/// open: it takes the focus without the person having left Bard.
static HOLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn hold(on: bool) {
    HOLD.store(on, Ordering::SeqCst);
    if !on {
        // Coming back from the dialog is not clicking away.
        mark_shown();
    }
}

/// A window was just shown: the focus change that follows is its own.
pub fn mark_shown() {
    SHOWN_AT.store(crate::db::now_ms() as u64, Ordering::SeqCst);
}

/// "1" when the person asked for Bard's windows to stay where they are.
pub const STAY_SETTING: &str = "windows_stay";

pub fn stay_open(app: &AppHandle) -> bool {
    crate::db::get_setting(app, STAY_SETTING).as_deref() == Some("1")
}

/// Bard's windows are summoned, so they go away when the person clicks into
/// something else — the main window, Draw and Settings alike, floating or
/// docked. Moving between Bard's own windows is not leaving.
pub fn on_focus_change(app: &AppHandle, focused: bool) {
    let generation = FOCUS_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if focused {
        // Back from whatever took the focus, a dialog of ours included.
        if HOLD.swap(false, Ordering::SeqCst) {
            mark_shown();
        }
        return;
    }
    if !crate::db::is_ready(app) {
        return;
    }
    if crate::db::get_setting(app, "onboarding_completed") != Some("1".into()) || stay_open(app) {
        return;
    }
    // The panel is focused and blurred constantly, and each change used to
    // start a thread that slept 160 ms to see whether the focus really moved
    // away. One check at a time is enough: a newer focus change while one is
    // waiting simply takes its place.
    if FOCUS_PENDING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(160));
        FOCUS_PENDING.store(false, Ordering::SeqCst);
        if FOCUS_GENERATION.load(Ordering::SeqCst) != generation
            || just_shown()
            || HOLD.load(Ordering::SeqCst)
        {
            return;
        }
        let ours_focused = ["main", "settings", "draw"].iter().any(|label| {
            app.get_webview_window(label)
                .and_then(|w| w.is_focused().ok())
                .unwrap_or(false)
        });
        if ours_focused {
            return;
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            for label in ["settings", "draw"] {
                if let Some(win) = handle.get_webview_window(label) {
                    if win.is_visible().unwrap_or(false) {
                        hide_companion(&handle, &win);
                    }
                }
            }
            hide_now_inner(&handle);
        });
    });
}

pub fn toggle_now(app: &AppHandle) {
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(win) = task_app.get_webview_window("main") else {
            return;
        };
        if win.is_visible().unwrap_or(false) && !win.is_minimized().unwrap_or(false) {
            if win.is_focused().unwrap_or(false) {
                hide_now_inner(&task_app);
            } else {
                SHOWN_AT.store(crate::db::now_ms() as u64, Ordering::SeqCst);
                let _ = win.set_focus();
            }
        } else {
            SHOWN_AT.store(crate::db::now_ms() as u64, Ordering::SeqCst);
            show_now_inner(&task_app);
        }
    });
}

/// Show the main window with the shortcut guide open over it.
pub fn show_guide(app: &AppHandle) {
    show_now(app);
    let app = app.clone();
    std::thread::spawn(move || {
        // Let a just-created page attach its listeners.
        std::thread::sleep(std::time::Duration::from_millis(120));
        let _ = app.emit_to("main", "show-guide", ());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dock_names_round_trip() {
        for name in ["float", "left", "right"] {
            assert_eq!(Dock::parse(name).unwrap().name(), name);
        }
        assert_eq!(Dock::parse("bottom"), None);
    }
}
