use tauri::{AppHandle, Emitter, Manager};

pub const POSITION_SETTING_KEY: &str = "dictation_overlay_position";
pub const DEFAULT_POSITION: &str = "bottom-middle";
const EDGE_MARGIN: f64 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Vertical {
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Horizontal {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Screen {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowSize {
    width: u32,
    height: u32,
}

fn parse_position(value: &str) -> Option<(Vertical, Horizontal)> {
    let (vertical, horizontal) = value.split_once('-')?;
    let vertical = match vertical {
        "top" => Vertical::Top,
        "bottom" => Vertical::Bottom,
        _ => return None,
    };
    let horizontal = match horizontal {
        "left" => Horizontal::Left,
        "middle" => Horizontal::Middle,
        "right" => Horizontal::Right,
        _ => return None,
    };
    Some((vertical, horizontal))
}

pub fn normalize_position(value: &str) -> Option<&'static str> {
    match value {
        "top-left" => Some("top-left"),
        "top-middle" => Some("top-middle"),
        "top-right" => Some("top-right"),
        "bottom-left" => Some("bottom-left"),
        "bottom-middle" => Some("bottom-middle"),
        "bottom-right" => Some("bottom-right"),
        _ => None,
    }
}

pub fn save_position(app: &AppHandle, value: &str) -> Result<(), String> {
    let normalized = normalize_position(value)
        .ok_or_else(|| "Choose a valid recording overlay position.".to_string())?;
    crate::db::set_setting_checked(app, POSITION_SETTING_KEY, normalized)?;
    reposition(app);
    Ok(())
}

pub fn reposition(app: &AppHandle) {
    let app = app.clone();
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(win) = task_app.get_webview_window("overlay") {
            position(&task_app, &win);
        }
    });
}

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
    let cursor_monitor = app
        .cursor_position()
        .ok()
        .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten());
    let Some(monitor) = cursor_monitor
        .or_else(|| win.current_monitor().ok().flatten())
        .or_else(|| win.primary_monitor().ok().flatten())
    else {
        return;
    };
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let outer = win.outer_size().unwrap_or_default();
    let scale = win.scale_factor().unwrap_or(1.0);
    let value = crate::db::get_setting(app, POSITION_SETTING_KEY)
        .unwrap_or_else(|| DEFAULT_POSITION.to_string());
    let (vertical, horizontal) = parse_position(&value).unwrap_or_else(|| {
        parse_position(DEFAULT_POSITION).expect("default overlay position is valid")
    });
    let (x, y) = position_for(
        Screen {
            x: monitor_position.x,
            y: monitor_position.y,
            width: monitor_size.width,
            height: monitor_size.height,
        },
        WindowSize {
            width: outer.width,
            height: outer.height,
        },
        scale,
        (vertical, horizontal),
    );
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
}

fn position_for(
    screen: Screen,
    window: WindowSize,
    scale: f64,
    (vertical, horizontal): (Vertical, Horizontal),
) -> (i32, i32) {
    let width = window.width as f64;
    let height = window.height as f64;
    let available_width = (screen.width as f64 - width).max(0.0);
    let available_height = (screen.height as f64 - height).max(0.0);
    let margin_x = (EDGE_MARGIN * scale).min(available_width / 2.0);
    let margin_y = (EDGE_MARGIN * scale).min(available_height / 2.0);

    let x = match horizontal {
        Horizontal::Left => screen.x as f64 + margin_x,
        Horizontal::Middle => screen.x as f64 + available_width / 2.0,
        Horizontal::Right => screen.x as f64 + available_width - margin_x,
    };
    let y = match vertical {
        Vertical::Top => screen.y as f64 + margin_y,
        Vertical::Bottom => screen.y as f64 + available_height - margin_y,
    };
    (x.round() as i32, y.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_exactly_six_positions() {
        let values = [
            "top-left",
            "top-middle",
            "top-right",
            "bottom-left",
            "bottom-middle",
            "bottom-right",
        ];
        for value in values {
            assert_eq!(normalize_position(value), Some(value));
        }
        assert!(normalize_position("middle-left").is_none());
        assert!(normalize_position("bottom-center").is_none());
        assert!(normalize_position("top").is_none());
    }

    #[test]
    fn calculates_all_six_positions_inside_the_monitor() {
        let screen = Screen {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let window = WindowSize {
            width: 260,
            height: 60,
        };
        for (value, expected) in [
            ("top-left", (40, 40)),
            ("top-middle", (830, 40)),
            ("top-right", (1620, 40)),
            ("bottom-left", (40, 980)),
            ("bottom-middle", (830, 980)),
            ("bottom-right", (1620, 980)),
        ] {
            let position = parse_position(value).expect("valid position");
            assert_eq!(position_for(screen, window, 1.0, position), expected);
        }
    }

    #[test]
    fn clamps_a_window_larger_than_the_monitor() {
        let (vertical, horizontal) = parse_position("bottom-right").expect("valid position");
        let screen = Screen {
            x: 100,
            y: 50,
            width: 200,
            height: 100,
        };
        let window = WindowSize {
            width: 260,
            height: 120,
        };
        assert_eq!(
            position_for(screen, window, 1.0, (vertical, horizontal)),
            (100, 50)
        );
    }
}
