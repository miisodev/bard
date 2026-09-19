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