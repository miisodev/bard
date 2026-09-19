use tauri::{AppHandle, Emitter, Manager};

pub fn position(win: &tauri::WebviewWindow) {
    let Some(monitor) = win.current_monitor().unwrap_or(None) else {
        return;
    };
    let m = monitor.position();
    let size = monitor.size();
    let outer = win.outer_size().unwrap_or_default();
    let scale = win.scale_factor().unwrap_or(1.0);
    let x = m.x + size.width as i32 - (outer.width as f64 * scale) as i32 - (16.0 * scale) as i32;
    let y = m.y + ((size.height as f64 - outer.height as f64 * scale) / 2.0) as i32;
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
}

fn apply_saved_geometry(app: &tauri::AppHandle, win: &tauri::WebviewWindow) -> bool {
    let Some(raw) = crate::db::get_setting(app, "window_geometry") else {
        return false;
    };
    let Ok(geo) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    let (Some(x), Some(y), Some(w), Some(h)) = (
        geo.get("x").and_then(|v| v.as_i64()),
        geo.get("y").and_then(|v| v.as_i64()),
        geo.get("width").and_then(|v| v.as_i64()),
        geo.get("height").and_then(|v| v.as_i64()),
    ) else {
        return false;
    };
    let visible = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .any(|m| {
            let r = m.position();
            let s = m.size();
            let ox = x.max(r.x as i64).min((r.x + s.width as i32) as i64);
            let oy = y.max(r.y as i64).min((r.y + s.height as i32) as i64);
            let ow = w - (x - ox);
            let oh = h - (y - oy);
            ow > 120 && oh > 120
        });
    if !visible {
        return false;
    }
    let _ = win.set_size(tauri::PhysicalSize::new(w.max(340) as u32, h.max(420) as u32));
    let _ = win.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
    true
}

fn save_geometry(app: &tauri::AppHandle, win: &tauri::WebviewWindow) {
    let Ok(pos) = win.outer_position() else {
        return;
    };
    let Ok(size) = win.outer_size() else {
        return;
    };
    let geo = serde_json::json!({
        "x": pos.x,
        "y": pos.y,
        "width": size.width,
        "height": size.height
    });
    crate::db::set_setting(app, "window_geometry", &geo.to_string());
}

pub fn show_now(app: &tauri::AppHandle) {
    remember_foreground();
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    if !apply_saved_geometry(app, &win) {
        position(&win);
    }
    let _ = win.show();
    let _ = win.unminimize();
    let _ = win.set_focus();
    let _ = app.emit("panel-shown", ());
}

pub fn hide_now(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    save_geometry(app, &win);
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
        // Settings and the drawing canvas are ours; opening one should not
        // yank the panel away behind it.
        if foreground_is_ours() {
            return;
        }
        // Window operations have to happen on the main thread on Windows.
        // Called straight from this timer thread, hide() returns Ok and does
        // nothing at all.
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || hide_now(&handle));
    });
}

pub fn toggle_now(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    if win.is_visible().unwrap_or(false) {
        if win.is_focused().unwrap_or(false) {
            hide_now(app);
        } else {
            remember_foreground();
            let _ = win.set_focus();
            let _ = app.emit("panel-shown", ());
        }
    } else {
        show_now(app);
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