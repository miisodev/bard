use tauri::{AppHandle, Emitter, Manager};

/// The recording overlay is a native Tauri window. It is reached from the raw
/// keyboard listener, the audio callback, and the transcription worker, none of
/// which are the Windows UI thread. Queue every window operation explicitly;
/// calling `show()`/`hide()` directly from those threads can deadlock the
/// WebView and make the whole process appear crashed.
pub fn show(app: &AppHandle, state: &str) {
    let app = app.clone();
    let state = state.to_owned();
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(win) = task_app.get_webview_window("overlay") {
            position(&task_app, &win);
            let _ = win.show();
            let _ = task_app.emit("dictation-overlay", state);
        }
    });
}

pub fn hide(app: &AppHandle) {
    let app = app.clone();
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(win) = task_app.get_webview_window("overlay") {
            let _ = win.hide();
        }
        let _ = task_app.emit("dictation-overlay", "idle");
    });
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
