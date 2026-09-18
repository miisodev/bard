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

pub fn show_now(app: &AppHandle) {
    remember_foreground();
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    position(&win);
    let _ = win.show();
    let _ = win.unminimize();
    let _ = win.set_focus();
    let _ = app.emit("panel-shown", ());
}

pub fn hide_now(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
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