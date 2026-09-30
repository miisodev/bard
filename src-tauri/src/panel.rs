use tauri::{AppHandle, Emitter, Manager};

/// The panel's size on its first open, in logical pixels (tauri.conf.json).
pub const PANEL_WIDTH: f64 = 380.0;
pub const PANEL_HEIGHT: f64 = 620.0;
/// Settings' and Draw's size on their first open.
pub const COMPANION_WIDTH: f64 = 720.0;
pub const COMPANION_HEIGHT: f64 = 620.0;

/// Put a companion away, remembering where it was. Fullscreen is left first,
/// so the next open starts from the window's own geometry, not the monitor's.
pub fn hide_companion(app: &AppHandle, win: &tauri::WebviewWindow) {
    crate::geometry::remember(app, win);
    if win.is_fullscreen().unwrap_or(false) {
        let _ = win.set_fullscreen(false);
    }
    let _ = win.hide();
    companions_changed(app);
}

/// Tell the panel which companions are open, so its Draw and Settings
/// buttons show as pressed while their window is out.
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

/// Set when the panel is shown deliberately. The auto-hide checks it and
/// stands down, because a window that has just been summoned goes through a
/// burst of focus changes while it takes the foreground — and hiding on one of
/// those is why summoning sometimes appeared to do nothing and needed a second
/// try.
static SHOWN_AT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How long after a deliberate show the auto-hide keeps out of the way.
const SETTLE_MS: u64 = 700;

fn mark_shown() {
    SHOWN_AT.store(
        crate::db::now_ms() as u64,
        std::sync::atomic::Ordering::SeqCst,
    );
}

fn just_shown() -> bool {
    let at = SHOWN_AT.load(std::sync::atomic::Ordering::SeqCst);
    at > 0 && (crate::db::now_ms() as u64).saturating_sub(at) < SETTLE_MS
}

/// Window operations are a Windows-main-thread responsibility. The global
/// input listener and tray callbacks are not on that thread, so every public
/// entry point queues the actual work instead of calling Tauri directly.
pub fn show_now(app: &tauri::AppHandle) {
    remember_foreground();
    mark_shown();
    let app = app.clone();
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || show_now_inner(&task_app));
}

fn show_now_inner(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let _ = win.unminimize();
    crate::geometry::restore(app, &win, PANEL_WIDTH, PANEL_HEIGHT);
    let _ = win.show();
    let _ = win.set_focus();
    let _ = app.emit("panel-shown", ());
    companions_changed(app);
}

pub fn hide_now(app: &tauri::AppHandle) {
    let app = app.clone();
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || hide_now_inner(&task_app));
}

fn hide_now_inner(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    crate::geometry::remember(app, &win);
    let _ = win.hide();
    restore_foreground();
}

/// Bumped on every focus change. A pending auto-hide carries the value it was
/// scheduled with and abandons itself if anything has happened since, which is
/// what makes a refocus — or a second blur — cancel it cleanly.
static FOCUS_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Grace period before an unfocused panel hides. Long enough to absorb the
/// focus flicker of opening one of our own windows or a native file dialog,
/// short enough that the panel is gone by the time you have looked away.
const HIDE_GRACE_MS: u64 = 140;

/// Does the foreground window belong to us?
///
/// The obvious version of this asks each of our own windows `is_focused()`,
/// and it does not work: queried during a focus transition it answers about a
/// window that is merely the most recently active, so a foreign window taking
/// focus reads as "one of ours is focused" and the panel never hides. Asking
/// the OS which process owns the foreground window has no such ambiguity, and
/// it covers every window we might grow later without a list to maintain.
#[cfg(windows)]
fn foreground_is_ours() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> isize;
        fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    }
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd == 0 {
            return false;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        pid == std::process::id()
    }
}

#[cfg(not(windows))]
fn foreground_is_ours() -> bool {
    false
}

/// The panel is summoned, so it dismisses itself once it is no longer what you
/// are using — including when it never lost focus in the webview's opinion,
/// which is why this is driven by the native window event rather than a blur
/// listener in JS.
pub fn on_focus_change(app: &AppHandle, focused: bool) {
    use std::sync::atomic::Ordering;

    let generation = FOCUS_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if focused {
        return;
    }

    // Focus events arrive while the window is still being created, before
    // setup() has opened the database. Reading a setting here panics the app
    // before it has drawn anything.
    if !crate::db::is_ready(app) {
        return;
    }

    // Onboarding is a first-run flow, not a summoned panel: hiding it the
    // instant the window loses focus would strand a new user with nothing on
    // screen and no idea how to get it back.
    if crate::db::get_setting(app, "onboarding_completed") != Some("1".into()) {
        return;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(HIDE_GRACE_MS));
        if FOCUS_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        if just_shown() {
            return;
        }
        // Settings and the drawing canvas are ours; opening one should not
        // yank the panel away behind it.
        if foreground_is_ours() {
            return;
        }
        // Window operations have to happen on the main thread on Windows.
        // Called straight from this timer thread, hide() returns Ok and does
        // nothing at all.
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || hide_now_inner(&handle));
    });
}

pub fn toggle_now(app: &AppHandle) {
    let app = app.clone();
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || toggle_now_inner(&task_app));
}

fn toggle_now_inner(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    if win.is_visible().unwrap_or(false) {
        if win.is_focused().unwrap_or(false) {
            hide_now_inner(app);
        } else {
            // Refocusing an already-visible panel. Deliberately does *not* emit
            // `panel-shown`: the panel's composer asserts focus when it hears that,
            // so announcing a refocus would throw away whatever the person was
            // doing inside it — a half-typed inline edit, a card selection, the
            // search field — and send their next keystrokes to the composer.
            remember_foreground();
            let _ = win.set_focus();
        }
    } else {
        remember_foreground();
        mark_shown();
        show_now_inner(app);
    }
}

#[cfg(windows)]
mod fg {
    use std::sync::atomic::{AtomicIsize, Ordering};

    static LAST_HWND: AtomicIsize = AtomicIsize::new(0);

    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> isize;
        fn SetForegroundWindow(hwnd: isize) -> i32;
    }

    pub fn remember() {
        let hwnd = unsafe { GetForegroundWindow() };
        LAST_HWND.store(hwnd, Ordering::SeqCst);
    }

    pub fn restore() {
        let hwnd = LAST_HWND.load(Ordering::SeqCst);
        if hwnd != 0 {
            unsafe {
                SetForegroundWindow(hwnd);
            }
        }
    }
}

#[cfg(not(windows))]
mod fg {
    pub fn remember() {}
    pub fn restore() {}
}

fn remember_foreground() {
    fg::remember();
}

fn restore_foreground() {
    fg::restore();
}
