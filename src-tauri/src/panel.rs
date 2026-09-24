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

/// Logical pixels between the panel and a companion. Companions are frameless,
/// so their outer rectangle is exactly what is drawn and this gap is what the
/// eye sees — with a native frame, the invisible resize border added 7 px and
/// dropped the companion's visible bottom edge short of the panel's.
const COMPANION_GAP: f64 = 8.0;

/// A companion's width in logical pixels, as declared in tauri.conf.json.
/// Placement uses this rather than the window's current width: a companion
/// hidden while fullscreen used to come back at the monitor's width and the
/// panel's height — a letterbox strip rather than a window.
const COMPANION_WIDTH: f64 = 720.0;

/// Put a companion away. Fullscreen is left first, so the next open starts
/// from the companion's own geometry instead of the monitor's.
pub fn hide_companion(app: &AppHandle, win: &tauri::WebviewWindow) {
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

/// Place Settings or Draw as a companion to the panel. The companion follows
/// the panel's top edge and physical height, choosing the side with the most
/// usable work area instead of blindly centring a second document window.
/// The fallback still keeps the window on-screen on small displays.
pub fn place_companion(app: &tauri::AppHandle, label: &str) {
    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let Some(companion) = app.get_webview_window(label) else {
        return;
    };
    let Ok(main_pos) = main.outer_position() else {
        return;
    };
    let Ok(main_size) = main.outer_size() else {
        return;
    };
    let Ok(companion_size) = companion.outer_size() else {
        return;
    };
    let Some(monitor) = main
        .current_monitor()
        .unwrap_or(None)
        .or_else(|| app.primary_monitor().unwrap_or(None))
    else {
        return;
    };
    let work = monitor.work_area();
    let work_x = work.position.x;
    let work_y = work.position.y;
    let work_right = work_x + work.size.width as i32;
    let work_bottom = work_y + work.size.height as i32;

    let scale = main.scale_factor().unwrap_or(1.0);
    let gap = (COMPANION_GAP * scale).round() as i32;
    let target_width = ((COMPANION_WIDTH * scale).round() as u32).min(work.size.width);
    let target_height = main_size.height.min(work.size.height);
    let (frame_width, frame_height) = companion
        .inner_size()
        .ok()
        .map(|inner| {
            (
                companion_size.width.saturating_sub(inner.width),
                companion_size.height.saturating_sub(inner.height),
            )
        })
        .unwrap_or((0, 0));
    let right_x = main_pos
        .x
        .saturating_add(main_size.width as i32)
        .saturating_add(gap);
    let left_x = main_pos
        .x
        .saturating_sub(target_width as i32)
        .saturating_sub(gap);
    let right_fits = right_x.saturating_add(target_width as i32) <= work_right;
    let left_fits = left_x >= work_x;
    let right_space = (work_right - right_x).max(0);
    let left_space = (left_x - work_x).max(0);
    let x = if right_fits || (!left_fits && right_space >= left_space) {
        right_x
    } else if left_fits {
        left_x
    } else if right_space >= left_space {
        right_x
    } else {
        left_x
    };
    let y = main_pos
        .y
        .max(work_y)
        .min(work_bottom.saturating_sub(target_height as i32));
    let clamped_x = x
        .max(work_x)
        .min(work_right.saturating_sub(target_width as i32));
    let clamped_y = y
        .max(work_y)
        .min(work_bottom.saturating_sub(target_height as i32));
    let client_width = target_width.saturating_sub(frame_width).max(480);
    let client_height = target_height.saturating_sub(frame_height).max(280);
    let _ = companion.set_size(tauri::PhysicalSize::new(client_width, client_height));
    let _ = companion.set_position(tauri::PhysicalPosition::new(clamped_x, clamped_y));
}

pub fn reposition_companions(app: &tauri::AppHandle) {
    for label in ["settings", "draw"] {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
        // A fullscreen Draw owns its monitor; moving the panel must not pull
        // it back into a companion's frame.
        if window.is_visible().unwrap_or(false) && !window.is_fullscreen().unwrap_or(false) {
            place_companion(app, label);
        }
    }
}

pub fn focus_panel(app: &tauri::AppHandle) {
    if let Some(panel) = app.get_webview_window("main") {
        if panel.is_visible().unwrap_or(false) {
            let _ = panel.set_focus();
        }
    }
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
    // Clamped at both ends: a stored size is data from a previous run, and a
    // run that got it wrong should not be able to hand this one a window
    // bigger than the screen.
    let (max_w, max_h) = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| (m.size().width as i64, m.size().height as i64))
        .fold((1280, 1440), |acc, s| (acc.0.max(s.0), acc.1.max(s.1)));
    let _ = win.set_size(tauri::PhysicalSize::new(
        w.clamp(340, max_w) as u32,
        h.clamp(420, max_h) as u32,
    ));
    let _ = win.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
    true
}

fn save_geometry(app: &tauri::AppHandle, win: &tauri::WebviewWindow) {
    let Ok(pos) = win.outer_position() else {
        return;
    };
    // Saving and restoring a size is only safe while the window has no frame.
    // With one, `set_size` and the size accessors disagree by the frame, so a
    // launch restores what it saved *plus* the frame and saves the larger
    // number back: 380 to 396 to 424 to 440 to 476, with nobody touching it.
    // `shadow: false` keeps the two equal. If the panel ever gains a real
    // drop shadow it must come from a transparent margin inside the window
    // (see docs/PARITY.md D2) rather than from a frame, or this returns.
    let Ok(size) = win.inner_size() else {
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
    if !apply_saved_geometry(app, &win) {
        position(&win);
    }
    let _ = win.show();
    let _ = win.unminimize();
    reposition_companions(app);
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
    save_geometry(app, &win);
    for label in ["settings", "draw"] {
        if let Some(companion) = app.get_webview_window(label) {
            if companion.is_visible().unwrap_or(false) {
                hide_companion(app, &companion);
            }
        }
    }
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
            remember_foreground();
            let _ = win.set_focus();
            let _ = app.emit("panel-shown", ());
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
