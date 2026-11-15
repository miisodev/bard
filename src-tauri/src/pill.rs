//! The status pill: a small, transparent, click-through window that sits at
//! the top middle of the monitor under the pointer and says what Bard just
//! did. It never takes focus and never intercepts a click, so it can appear
//! over anything without getting in the way.
//!
//! The window is a fixed transparent canvas; the pill itself is drawn at its
//! top centre by `pill.html` and grows downward for a preview. The page fills
//! the canvas and keeps the pill's shadow inside it. Timing lives
//! here, not in the webview, so a note always goes away on schedule.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::feedback::Note;

pub const LABEL: &str = "pill";

/// The canvas, in logical pixels. Wide enough for a preview card; the empty
/// part is transparent and lets every click through.
const WIDTH: f64 = 480.0;
const HEIGHT: f64 = 300.0;
/// Distance from the top of the work area to the canvas. The page keeps its
/// own margin above the pill, so that the shadow has room to fade out.
const TOP_MARGIN: f64 = 0.0;
/// How long the webview's exit animation takes before the window hides.
const EXIT: Duration = Duration::from_millis(220);

static GENERATION: AtomicU64 = AtomicU64::new(0);
static STICKY: AtomicBool = AtomicBool::new(false);

pub fn show(app: &AppHandle, note: Note) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    STICKY.store(note.sticky, Ordering::SeqCst);
    let sticky = note.sticky;
    let hold = Duration::from_millis(note.hold_ms.max(600));
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(win) = handle.get_webview_window(LABEL) else {
            return;
        };
        let visible = win.is_visible().unwrap_or(false);
        if !visible {
            // Before the first map: a window the manager has already taken
            // charge of stays an application window.
            static UNMANAGED: std::sync::Once = std::sync::Once::new();
            UNMANAGED.call_once(|| crate::front::unmanage(&win));
            place(&handle, &win);
        }
        let _ = handle.emit_to(LABEL, "pill", &note);
        if !visible {
            let _ = win.show();
            let _ = win.set_always_on_top(true);
            // After show, never before: on Linux the click-through input shape
            // is set on the native window, which GTK only creates when the
            // window is first shown, and tao unwraps it (a panic, and under
            // `panic = "abort"` a dead app, on the first copy).
            let _ = win.set_ignore_cursor_events(true);
        }
    });
    if !sticky {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(hold);
            retire(&app, generation);
        });
    }
}

/// Put the pill away now (a cancelled take, a closed preview).
pub fn hide(app: &AppHandle) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || retire(&app, generation));
}

/// Put away a sticky pill (a dictation phase) only; a timed note keeps its
/// own schedule.
pub fn hide_sticky(app: &AppHandle) {
    if STICKY.swap(false, Ordering::SeqCst) {
        hide(app);
    }
}

fn retire(app: &AppHandle, generation: u64) {
    if GENERATION.load(Ordering::SeqCst) != generation {
        return;
    }
    let _ = app.emit_to(LABEL, "pill-out", ());
    std::thread::sleep(EXIT);
    if GENERATION.load(Ordering::SeqCst) != generation {
        return;
    }
    STICKY.store(false, Ordering::SeqCst);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Some(win) = handle.get_webview_window(LABEL) {
            let _ = win.hide();
        }
    });
}

/// Top centre of the work area on the monitor under the pointer.
fn place(app: &AppHandle, win: &tauri::WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let _ = win.set_size(tauri::LogicalSize::new(WIDTH, HEIGHT));
    let work = monitor.work_area();
    let (x, y) = top_centre(
        (work.position.x, work.position.y),
        (work.size.width, work.size.height),
        scale,
    );
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
}

fn top_centre(origin: (i32, i32), size: (u32, u32), scale: f64) -> (i32, i32) {
    let width = (WIDTH * scale).round() as i64;
    let x = origin.0 as i64 + (size.0 as i64 - width) / 2;
    let y = origin.1 as i64 + (TOP_MARGIN * scale).round() as i64;
    (x as i32, y as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pill_is_centred_at_the_top_of_the_work_area() {
        assert_eq!(top_centre((0, 0), (1920, 1040), 1.0), (720, 0));
        assert_eq!(
            top_centre((1920, 32), (2560, 1408), 2.0),
            (1920 + 1280 - 480, 32)
        );
    }
}
