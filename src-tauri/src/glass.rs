use tauri::{AppHandle, Manager};

pub fn apply_all(app: &AppHandle, theme: &str) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let (r, g, b, a) = match theme {
        "light" => (245, 246, 248, 170),
        "dark" => (18, 20, 26, 190),
        _ => match win.theme().unwrap_or(tauri::Theme::Dark) {
            tauri::Theme::Light => (245, 246, 248, 170),
            tauri::Theme::Dark => (18, 20, 26, 190),
            _ => (18, 20, 26, 190),
        },
    };
    let _ = window_vibrancy::apply_acrylic(&win, Some((r, g, b, a)));
}