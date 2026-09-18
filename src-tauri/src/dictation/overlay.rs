use tauri::{AppHandle, Emitter, Manager};

pub fn show(app: &AppHandle, state: &str) {
    if let Some(win) = app.get_webview_window("overlay") {
        position(app, &win);
        let _ = win.show();
        let _ = app.emit("dictation-overlay", state);
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.hide();
    }
}

fn position(app: &AppHandle, win: &tauri::WebviewWindow) {
    let Some(monitor) = app
        .cursor_position()
        .ok()
        .and_then(|_| win.current_monitor().unwrap_or(None))
    else {
        return;
    };
    let m = monitor.position();
    let size = monitor.size();
    let outer = win.outer_size().unwrap_or_default();
    let scale = win.scale_factor().unwrap_or(1.0);
    let w = outer.width as f64 * scale;
    let h = outer.height as f64 * scale;
    let x = m.x + ((size.width as f64 - w) / 2.0) as i32;
    let y = m.y + (size.height as f64 - h - 40.0 * scale) as i32;
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
}