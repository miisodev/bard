//! Where each window opens: exactly where it was last left, at the size it was
//! left, or centred on the screen under the pointer the first time.
//!
//! Every window (the panel, Settings, Draw) keeps its own rectangle under
//! `geometry:<label>`. A rectangle is only ever written from a window that is
//! on screen as a normal window: never while minimised (Windows parks those at
//! -32000, -32000), fullscreen or maximised, and never one that no longer
//! overlaps a monitor. The panel once saved its minimised position and then
//! reopened invisibly on every launch after it.

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// How much of a window must lie on some monitor for its saved rectangle to be
/// trusted: enough to see it and grab it.
const MIN_VISIBLE: i64 = 120;

/// Does at least a `MIN_VISIBLE` square of `rect` overlap one of `monitors`?
pub fn visible_enough(rect: Rect, monitors: &[Rect]) -> bool {
    monitors.iter().any(|m| {
        let left = (rect.x as i64).max(m.x as i64);
        let top = (rect.y as i64).max(m.y as i64);
        let right = (rect.x as i64 + rect.width as i64).min(m.x as i64 + m.width as i64);
        let bottom = (rect.y as i64 + rect.height as i64).min(m.y as i64 + m.height as i64);
        right - left >= MIN_VISIBLE && bottom - top >= MIN_VISIBLE
    })
}

/// `size` centred in `area`, shrunk to fit it.
pub fn centred(area: Rect, width: u32, height: u32) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) as i32 / 2,
        y: area.y + (area.height - height) as i32 / 2,
        width,
        height,
    }
}

fn key(label: &str) -> String {
    format!("geometry:{label}")
}

fn monitor_rects(app: &AppHandle) -> Vec<Rect> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Rect {
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
        })
        .collect()
}

fn saved(app: &AppHandle, label: &str) -> Option<Rect> {
    let raw = crate::db::get_setting(app, &key(label))?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some(Rect {
        x: value.get("x")?.as_i64()? as i32,
        y: value.get("y")?.as_i64()? as i32,
        width: value.get("width")?.as_u64()? as u32,
        height: value.get("height")?.as_u64()? as u32,
    })
}

/// Put the window where it was left, or centre it at its configured logical
/// size on the monitor under the pointer. Call before showing it.
pub fn restore(app: &AppHandle, win: &WebviewWindow, logical_width: f64, logical_height: f64) {
    if let Some(rect) = saved(app, win.label()) {
        if rect.width >= 200 && rect.height >= 200 && visible_enough(rect, &monitor_rects(app)) {
            let _ = win.set_size(PhysicalSize::new(rect.width, rect.height));
            let _ = win.set_position(PhysicalPosition::new(rect.x, rect.y));
            return;
        }
    }
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        let _ = win.center();
        return;
    };
    let work = monitor.work_area();
    let scale = monitor.scale_factor();
    let area = Rect {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
    };
    let rect = centred(
        area,
        (logical_width * scale).round() as u32,
        (logical_height * scale).round() as u32,
    );
    let _ = win.set_size(PhysicalSize::new(rect.width, rect.height));
    let _ = win.set_position(PhysicalPosition::new(rect.x, rect.y));
}

/// Record where the window is, if it is somewhere worth returning to.
pub fn remember(app: &AppHandle, win: &WebviewWindow) {
    if !crate::db::is_ready(app)
        || !win.is_visible().unwrap_or(false)
        || win.is_minimized().unwrap_or(true)
        || win.is_maximized().unwrap_or(false)
        || win.is_fullscreen().unwrap_or(false)
    {
        return;
    }
    let (Ok(position), Ok(size)) = (win.outer_position(), win.inner_size()) else {
        return;
    };
    let rect = Rect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    if !visible_enough(rect, &monitor_rects(app)) {
        return;
    }
    let value = serde_json::json!({
        "x": rect.x,
        "y": rect.y,
        "width": rect.width,
        "height": rect.height,
    });
    crate::db::set_setting(app, &key(win.label()), &value.to_string());
}

/// Record a window's rectangle a moment after it stops moving or resizing,
/// rather than on every one of the dozens of events a drag produces.
pub fn remember_soon(app: &AppHandle, label: &str) {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static GENERATIONS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    let generations = GENERATIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let generation = {
        let mut map = generations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = map.entry(label.to_string()).or_insert(0);
        *entry += 1;
        *entry
    };
    let app = app.clone();
    let label = label.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        let current = generations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&label)
            .copied();
        if current != Some(generation) {
            return;
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(win) = handle.get_webview_window(&label) {
                remember(&handle, &win);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };

    #[test]
    fn a_minimised_window_position_is_not_visible() {
        let parked = Rect {
            x: -32000,
            y: -32000,
            width: 340,
            height: 420,
        };
        assert!(!visible_enough(parked, &[SCREEN]));
    }

    #[test]
    fn a_window_mostly_on_screen_is_visible() {
        let on = Rect {
            x: 1700,
            y: 900,
            width: 380,
            height: 620,
        };
        assert!(visible_enough(on, &[SCREEN]));
        let sliver = Rect {
            x: 1850,
            y: 100,
            width: 380,
            height: 620,
        };
        assert!(!visible_enough(sliver, &[SCREEN]));
    }

    #[test]
    fn a_second_monitor_counts() {
        let right = Rect {
            x: 1920,
            y: 0,
            width: 2560,
            height: 1440,
        };
        let there = Rect {
            x: 2400,
            y: 300,
            width: 720,
            height: 620,
        };
        assert!(visible_enough(there, &[SCREEN, right]));
    }

    #[test]
    fn centring_fits_the_area() {
        assert_eq!(
            centred(SCREEN, 720, 620),
            Rect {
                x: 600,
                y: 230,
                width: 720,
                height: 620
            }
        );
        assert_eq!(centred(SCREEN, 4000, 3000), SCREEN);
    }
}
