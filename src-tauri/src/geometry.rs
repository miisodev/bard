//! Where each window opens: exactly where it was last left, at the size it was
//! left, or centred on the screen under the pointer the first time.
//!
//! Every window (the panel, Settings, Draw) keeps its own rectangle under
//! `geometry:<label>`. A rectangle is only ever written from a window that is
//! on screen as a normal window: never while minimised (Windows parks those at
//! -32000, -32000), fullscreen or maximised, and never one that no longer
//! overlaps a monitor. The panel once saved its minimised position and then
//! reopened invisibly on every launch after it.
//!
//! The panel is the exception to the remembering: it is a shelf docked to one
//! of the screen's four edges, so it has no rectangle of its own to return to.
//! [`anchor`] gives it the band its edge dictates on every show, from the work
//! area of the monitor under the pointer (or the one it docked to last), and
//! [`park`] moves it back out of the work area when it goes away. Settings and
//! Draw are still windows a person moves, and still remember where they were.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow};

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

/// The setting holding the shelf's edge: "top", "right", "bottom" or "left".
pub const EDGE_SETTING: &str = "shelf_edge";

/// The setting holding the name of the monitor the shelf docked to last.
pub const MONITOR_SETTING: &str = "shelf_monitor";

/// How deep the shelf is on a horizontal edge, in logical pixels.
pub const SHELF_THICKNESS_H: f64 = 250.0;

/// How wide it is on a vertical edge, in logical pixels.
pub const SHELF_THICKNESS_V: f64 = 440.0;

/// The screen edge the shelf is docked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Top,
    Right,
    Bottom,
    Left,
}

impl Edge {
    /// The edge a stored or commanded value names, with the bottom as the
    /// answer for anything else: a setting written by an older or newer
    /// version must never leave the shelf without a home.
    pub fn parse(raw: &str) -> Edge {
        Edge::checked(raw).unwrap_or(Edge::Bottom)
    }

    /// The edge a value names, or nothing — for input that has to be refused
    /// rather than guessed at.
    pub fn checked(raw: &str) -> Option<Edge> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "top" => Some(Edge::Top),
            "right" => Some(Edge::Right),
            "bottom" => Some(Edge::Bottom),
            "left" => Some(Edge::Left),
            _ => None,
        }
    }

    /// The value written to the setting.
    pub fn as_str(self) -> &'static str {
        match self {
            Edge::Top => "top",
            Edge::Right => "right",
            Edge::Bottom => "bottom",
            Edge::Left => "left",
        }
    }
}

/// One monitor, reduced to what docking needs. A plain struct rather than
/// [`Monitor`] because which monitor the shelf lands on has to be testable
/// without a running app.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    /// The monitor's own top-left corner, which is also its identity here.
    pub at: (i32, i32),
    pub size: (u32, u32),
    /// The monitor minus the taskbar and any other appbar.
    pub work: Rect,
    pub scale: f64,
    pub name: Option<String>,
    pub primary: bool,
}

impl Screen {
    fn of(monitor: &Monitor, primary: bool) -> Screen {
        let work = monitor.work_area();
        Screen {
            at: (monitor.position().x, monitor.position().y),
            size: (monitor.size().width, monitor.size().height),
            work: Rect {
                x: work.position.x,
                y: work.position.y,
                width: work.size.width,
                height: work.size.height,
            },
            scale: monitor.scale_factor(),
            name: monitor.name().cloned(),
            primary,
        }
    }

    fn is(&self, monitor: &Monitor) -> bool {
        self.at == (monitor.position().x, monitor.position().y)
            && self.size == (monitor.size().width, monitor.size().height)
    }
}

/// Where the shelf belongs: the screen under the pointer, else the screen it
/// docked to last, else the primary.
pub fn choose_screen<'a>(
    screens: &'a [Screen],
    cursor: Option<usize>,
    stored: Option<&str>,
) -> Option<&'a Screen> {
    cursor
        .and_then(|index| screens.get(index))
        .or_else(|| {
            stored.and_then(|name| {
                screens
                    .iter()
                    .find(|screen| screen.name.as_deref() == Some(name))
            })
        })
        .or_else(|| screens.iter().find(|screen| screen.primary))
        .or_else(|| screens.first())
}

/// A physical coordinate, kept inside what Windows can represent.
fn fitting(value: i64) -> i32 {
    value.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// The thickness of the shelf across its edge, clamped to the work area so a
/// docked shelf can never hang off the screen it is docked to.
fn thickness(logical: f64, along: u32, scale: f64) -> u32 {
    ((logical * scale).round() as u32).clamp(1, along.max(1))
}

/// The shelf's rectangle when docked to `edge` on the work area `area`, in
/// physical pixels at `scale`. The thickness is a logical measurement — the
/// shelf is a band of content, so it should be as deep on a 4K monitor as on a
/// 1080p one — while its length is exactly the work area, which is what keeps
/// it clear of the taskbar.
pub fn shelf_rect(area: Rect, edge: Edge, scale: f64) -> Rect {
    let (width, height) = match edge {
        Edge::Top | Edge::Bottom => (area.width, thickness(SHELF_THICKNESS_H, area.height, scale)),
        Edge::Left | Edge::Right => (thickness(SHELF_THICKNESS_V, area.width, scale), area.height),
    };
    let x = match edge {
        Edge::Right => area.x as i64 + area.width as i64 - width as i64,
        _ => area.x as i64,
    };
    let y = match edge {
        Edge::Bottom => area.y as i64 + area.height as i64 - height as i64,
        _ => area.y as i64,
    };
    Rect {
        x: fitting(x),
        y: fitting(y),
        width,
        height,
    }
}

/// Where the shelf's window goes when it is not on screen: just past the work
/// area's docking edge, so nothing it contains can be clicked, hovered or
/// tabbed to even if the window is shown without being anchored first.
pub fn parked_rect(area: Rect, edge: Edge, width: u32, height: u32) -> Rect {
    let (x, y) = match edge {
        Edge::Bottom => (area.x as i64, area.y as i64 + area.height as i64),
        Edge::Top => (area.x as i64, area.y as i64 - height as i64),
        Edge::Left => (area.x as i64 - width as i64, area.y as i64),
        Edge::Right => (area.x as i64 + area.width as i64, area.y as i64),
    };
    Rect {
        x: fitting(x),
        y: fitting(y),
        width,
        height,
    }
}

/// The edge the shelf is docked to, as stored; the bottom when nothing is.
pub fn shelf_edge(app: &AppHandle) -> Edge {
    Edge::parse(&crate::db::get_setting(app, EDGE_SETTING).unwrap_or_default())
}

/// Dock the shelf: give it the band its edge dictates on the monitor the
/// pointer is on and put it there. The monitor is kept so the shelf comes back
/// to the screen it was used on rather than the one the pointer happens to be
/// over at the next launch.
pub fn anchor(app: &AppHandle, win: &WebviewWindow, edge: Edge) {
    let Some(screen) = screen_for(app, true) else {
        return;
    };
    place(win, shelf_rect(screen.work, edge, screen.scale));
    if let Some(name) = screen.name.as_deref() {
        if crate::db::get_setting(app, MONITOR_SETTING).as_deref() != Some(name) {
            crate::db::set_setting(app, MONITOR_SETTING, name);
        }
    }
}

/// Move the shelf out of the work area. It is hidden as well; this is what
/// keeps the window from sitting invisibly over whatever is at that edge. The
/// monitor comes from the setting, not the pointer, which may be anywhere by
/// the time a slide is over.
pub fn park(app: &AppHandle, win: &WebviewWindow, edge: Edge) {
    let Some(screen) = screen_for(app, false) else {
        return;
    };
    let docked = shelf_rect(screen.work, edge, screen.scale);
    place(
        win,
        parked_rect(screen.work, edge, docked.width, docked.height),
    );
}

fn place(win: &WebviewWindow, rect: Rect) {
    let _ = win.set_size(PhysicalSize::new(rect.width, rect.height));
    let _ = win.set_position(PhysicalPosition::new(rect.x, rect.y));
}

fn screens(app: &AppHandle) -> Vec<Screen> {
    let primary = app.primary_monitor().ok().flatten();
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|monitor| {
            let is_primary = primary.as_ref().is_some_and(|primary| {
                primary.position() == monitor.position() && primary.size() == monitor.size()
            });
            Screen::of(monitor, is_primary)
        })
        .collect()
}

/// The screen the pointer is on, by index into `screens`.
fn cursor_screen(app: &AppHandle, screens: &[Screen]) -> Option<usize> {
    let point = app.cursor_position().ok()?;
    let monitor = app.monitor_from_point(point.x, point.y).ok().flatten()?;
    screens.iter().position(|screen| screen.is(&monitor))
}

/// The screen the shelf anchors to (`cursor_first`) or went away on.
fn screen_for(app: &AppHandle, cursor_first: bool) -> Option<Screen> {
    let screens = screens(app);
    let cursor = if cursor_first {
        cursor_screen(app, &screens)
    } else {
        None
    };
    let stored = crate::db::get_setting(app, MONITOR_SETTING);
    choose_screen(&screens, cursor, stored.as_deref()).cloned()
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

    /// 1920 × 1080 with a 40 px taskbar along the bottom: the common case the
    /// shelf has to keep clear of.
    const WORK: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };

    fn screen(name: &str, at: (i32, i32), primary: bool) -> Screen {
        Screen {
            at,
            size: (1920, 1080),
            work: Rect {
                x: at.0,
                y: at.1,
                width: 1920,
                height: 1040,
            },
            scale: 1.0,
            name: Some(name.to_string()),
            primary,
        }
    }

    fn two_screens() -> Vec<Screen> {
        vec![
            screen("left", (-1920, 0), false),
            screen("primary", (0, 0), true),
        ]
    }

    #[test]
    fn the_shelf_docks_to_each_edge_inside_the_work_area() {
        assert_eq!(
            shelf_rect(WORK, Edge::Bottom, 1.0),
            Rect {
                x: 0,
                y: 790,
                width: 1920,
                height: 250
            }
        );
        assert_eq!(
            shelf_rect(WORK, Edge::Top, 1.0),
            Rect {
                x: 0,
                y: 0,
                width: 1920,
                height: 250
            }
        );
        assert_eq!(
            shelf_rect(WORK, Edge::Left, 1.0),
            Rect {
                x: 0,
                y: 0,
                width: 440,
                height: 1040
            }
        );
        assert_eq!(
            shelf_rect(WORK, Edge::Right, 1.0),
            Rect {
                x: 1480,
                y: 0,
                width: 440,
                height: 1040
            }
        );
    }

    #[test]
    fn a_docked_shelf_respects_a_second_monitors_origin() {
        // 2560 × 1440 to the right of the primary, work area 1380 tall.
        let second = Rect {
            x: 1920,
            y: 0,
            width: 2560,
            height: 1380,
        };
        assert_eq!(
            shelf_rect(second, Edge::Bottom, 1.0),
            Rect {
                x: 1920,
                y: 1130,
                width: 2560,
                height: 250
            }
        );
        assert_eq!(
            shelf_rect(second, Edge::Right, 1.0),
            Rect {
                x: 4040,
                y: 0,
                width: 440,
                height: 1380
            }
        );
    }

    #[test]
    fn the_shelf_is_measured_in_logical_pixels() {
        assert_eq!(shelf_rect(WORK, Edge::Bottom, 1.5).height, 375);
        assert_eq!(shelf_rect(WORK, Edge::Right, 2.0).width, 880);
    }

    #[test]
    fn a_tiny_work_area_still_contains_the_shelf() {
        let small = Rect {
            x: 0,
            y: 0,
            width: 320,
            height: 200,
        };
        assert_eq!(
            shelf_rect(small, Edge::Top, 1.0),
            Rect {
                x: 0,
                y: 0,
                width: 320,
                height: 200
            }
        );
        assert_eq!(
            shelf_rect(small, Edge::Left, 2.0),
            Rect {
                x: 0,
                y: 0,
                width: 320,
                height: 200
            }
        );
    }

    #[test]
    fn a_parked_shelf_is_outside_the_work_area() {
        // The window keeps its docked size and moves just past the edge, so no
        // part of it is left hovering over the desktop.
        for (edge, expected) in [
            (
                Edge::Top,
                Rect {
                    x: 0,
                    y: -250,
                    width: 1920,
                    height: 250,
                },
            ),
            (
                Edge::Bottom,
                Rect {
                    x: 0,
                    y: 1040,
                    width: 1920,
                    height: 250,
                },
            ),
            (
                Edge::Left,
                Rect {
                    x: -440,
                    y: 0,
                    width: 440,
                    height: 1040,
                },
            ),
            (
                Edge::Right,
                Rect {
                    x: 1920,
                    y: 0,
                    width: 440,
                    height: 1040,
                },
            ),
        ] {
            let docked = shelf_rect(WORK, edge, 1.0);
            let parked = parked_rect(WORK, edge, docked.width, docked.height);
            assert_eq!(parked, expected);
            assert!(!visible_enough(parked, &[SCREEN]));
        }
    }

    #[test]
    fn an_edge_reads_and_writes_as_a_string() {
        for edge in [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left] {
            assert_eq!(Edge::parse(edge.as_str()), edge);
            let json = serde_json::to_string(&edge).expect("an edge is a string");
            assert_eq!(json, format!("\"{}\"", edge.as_str()));
            assert_eq!(
                serde_json::from_str::<Edge>(&json).expect("and reads back"),
                edge
            );
        }
        assert_eq!(Edge::parse(" Top "), Edge::Top);
        assert_eq!(Edge::parse(""), Edge::Bottom);
        assert_eq!(Edge::parse("sideways"), Edge::Bottom);
        assert_eq!(Edge::checked("left"), Some(Edge::Left));
        assert_eq!(Edge::checked("diagonal"), None);
    }

    #[test]
    fn the_pointer_leads_the_monitor_choice() {
        let screens = two_screens();
        let named = |screen: &Screen| screen.name.clone();

        // Under the pointer wins, even when another monitor was used last.
        let chosen = choose_screen(&screens, Some(0), Some("primary")).unwrap();
        assert_eq!(named(chosen), Some("left".into()));
        // Otherwise the screen it docked to last, then the primary.
        let chosen = choose_screen(&screens, None, Some("left")).unwrap();
        assert_eq!(named(chosen), Some("left".into()));
        let chosen = choose_screen(&screens, None, None).unwrap();
        assert_eq!(named(chosen), Some("primary".into()));
        // A stored name for a monitor that is gone falls back to the primary,
        // and a pointer index that no longer exists falls back to the name.
        let chosen = choose_screen(&screens, None, Some("gone")).unwrap();
        assert_eq!(named(chosen), Some("primary".into()));
        let chosen = choose_screen(&screens, Some(9), Some("left")).unwrap();
        assert_eq!(named(chosen), Some("left".into()));
        assert!(choose_screen(&[], None, Some("primary")).is_none());
    }
}
