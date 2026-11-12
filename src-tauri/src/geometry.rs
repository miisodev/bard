//! Where each window opens: exactly where it was last left, at the size it was
//! left, or centred on the screen under the pointer the first time.
//!
//! Every window (the main window, Settings, Draw) keeps its own rectangle
//! under `geometry:<label>`. A rectangle is only ever written from a window
//! that is on screen as a normal window: never while minimised (Windows parks
//! those at -32000, -32000), fullscreen or maximised, and never one that no
//! longer overlaps a monitor.
//!
//! The main window can also dock beside the left or right edge of the work
//! area, full height less a small gap, at a width the person chose. [`dock`] places it there on every
//! show, on the monitor under the pointer.

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

/// Which edge the main window docks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// The gap a docked window keeps from the edges of the work area, in logical
/// pixels. Flush against the screen it was a slab with no edge of its own;
/// set in a little, it is the same rounded surface as the floating window,
/// and looks the same whatever panels or docks the desktop draws around it.
pub const DOCK_GAP: f64 = 8.0;

/// The docked rectangle: the height of the work area, beside one edge, set in
/// by `gap` on the three sides it shares with the screen.
pub fn dock_rect(work: Rect, side: Side, width: u32, gap: u32) -> Rect {
    // A work area too small to spare the gap gives it up.
    let gap = if work.width > 200 + gap * 2 && work.height > 200 + gap * 2 {
        gap
    } else {
        0
    };
    let room = work.width - gap * 2;
    let width = width.clamp(200, room.max(200)).min(room);
    let x = match side {
        Side::Left => work.x + gap as i32,
        Side::Right => work.x + work.width as i32 - gap as i32 - width as i32,
    };
    Rect {
        x,
        y: work.y + gap as i32,
        width,
        height: work.height - gap * 2,
    }
}

/// Dock `win` to `side` of the work area under the pointer, `logical_width`
/// wide.
pub fn dock(app: &AppHandle, win: &WebviewWindow, side: Side, logical_width: f64) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
        .or_else(|| win.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let work = monitor.work_area();
    let area = Rect {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
    };
    let scale = monitor.scale_factor();
    let rect = dock_rect(
        area,
        side,
        (logical_width * scale).round() as u32,
        (DOCK_GAP * scale).round() as u32,
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

/// How long a window must be still before its rectangle is written.
const SETTLE_MS: u64 = 400;

/// One window's pending write.
struct Pending {
    /// Bumped by every move and resize event.
    generation: u64,
    /// Whether a worker is already waiting for this window to settle.
    running: bool,
}

/// The pending writes, one entry per window label.
fn pending() -> &'static std::sync::Mutex<std::collections::HashMap<String, Pending>> {
    static PENDING: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, Pending>>,
    > = std::sync::OnceLock::new();
    PENDING.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Record a window's rectangle a moment after it stops moving or resizing,
/// rather than on every one of the dozens of events a drag produces.
///
/// One worker waits per window, not one per event. A drag produces hundreds of
/// events, and a thread each meant hundreds of threads all waking 400 ms later
/// to find out they had nothing to do. The worker simply keeps waiting while
/// the events keep coming, and writes once, when they stop.
pub fn remember_soon(app: &AppHandle, label: &str) {
    let mut map = pending()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = map.entry(label.to_string()).or_insert(Pending {
        generation: 0,
        running: false,
    });
    entry.generation += 1;
    if entry.running {
        // A worker is already waiting for this window; it will see the new
        // generation and start its clock again.
        return;
    }
    entry.running = true;
    drop(map);

    let app = app.clone();
    let label = label.to_string();
    std::thread::spawn(move || {
        let mut seen = 0u64;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(SETTLE_MS));
            let current = {
                let map = pending()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                map.get(&label).map(|entry| entry.generation)
            };
            match current {
                // The window moved again while the worker waited: nothing has
                // settled yet, so the clock starts again from the last event.
                Some(generation) if generation != seen => {
                    seen = generation;
                    continue;
                }
                None => return,
                _ => {}
            }
            {
                let mut map = pending()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(entry) = map.get_mut(&label) {
                    entry.running = false;
                }
            }
            let handle = app.clone();
            let target = label.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(win) = handle.get_webview_window(&target) {
                    remember(&handle, &win);
                }
            });
            return;
        }
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

    #[test]
    fn a_docked_window_fills_the_height_beside_its_edge() {
        let work = Rect {
            x: 0,
            y: 32,
            width: 1920,
            height: 1008,
        };
        assert_eq!(
            dock_rect(work, Side::Left, 400, 8),
            Rect {
                x: 8,
                y: 40,
                width: 400,
                height: 992
            }
        );
        assert_eq!(
            dock_rect(work, Side::Right, 400, 8),
            Rect {
                x: 1512,
                y: 40,
                width: 400,
                height: 992
            }
        );
    }

    #[test]
    fn a_dock_width_is_kept_inside_the_work_area() {
        let work = Rect {
            x: 1920,
            y: 0,
            width: 300,
            height: 800,
        };
        let rect = dock_rect(work, Side::Right, 900, 8);
        assert_eq!(rect.width, 284);
        assert_eq!(rect.x, 1928);
        assert_eq!(dock_rect(work, Side::Left, 10, 8).width, 200);
        // No room for a gap: the window takes what there is.
        let tight = Rect { width: 210, ..work };
        assert_eq!(dock_rect(tight, Side::Left, 400, 8).x, 1920);
    }
}
