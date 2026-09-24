use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::Duration;

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
            remember_rect(&win);
        }
    });
}

/*
  The overlay is modal while a take is live. It never takes focus — the words
  land wherever you were typing — and while it listens, transcribes or refines,
  a click anywhere else is held back and its × flashes instead: the × is the
  one way out, and a finished take puts the pill away itself after pasting.
  Once the take is over and the pill is only reporting (an error, or
  "Cancelling"), a click anywhere puts it away and goes through as normal.

  Clicks on the pill itself never reach its webview either. A click into a
  WebView2 moves focus into it, and focus moved into a window that must not
  take focus lands on another of Bard's windows instead — measured: the hidden
  panel, which is where the transcript was then pasted. So the guard hit-tests
  the × itself and acts on it directly.
*/

const HIDDEN: u8 = 0;
const ACTIVE: u8 = 1;
const STALE: u8 = 2;

const LISTENING: u8 = 1;
const WORKING: u8 = 2;

/// The × hit area in CSS pixels within the 260 × 60 overlay window: the
/// right end of the 212 px pill (`.overlay-pill` in styles.css), generous on
/// purpose. The two must move together.
const CLOSE_LEFT: f64 = 192.0;
const CLOSE_RIGHT: f64 = 240.0;
const CLOSE_TOP: f64 = 10.0;
const CLOSE_BOTTOM: f64 = 50.0;

/// How long a report stays up when nobody dismisses it.
const REPORT_LINGER: Duration = Duration::from_millis(3200);
/// A watchdog on the modal hold. Listening ends by your own hand — Ctrl, Esc
/// or the × — but transcribing and refining end on their own, and if either
/// ever stalled, clicks must not stay held behind it. Refining alone is
/// bounded at 60 s.
const WORKING_HOLD_LIMIT_MS: u64 = 90_000;

static MODE: AtomicU8 = AtomicU8::new(HIDDEN);
static PHASE: AtomicU8 = AtomicU8::new(0);
static PHASE_STARTED_MS: AtomicU64 = AtomicU64::new(0);
/// Bumped by every show and hide. A queued update carries the value it was
/// issued with and gives way to anything issued after it, so a late
/// "Cancelling" can no longer bring the pill back after the take has ended.
static GENERATION: AtomicU64 = AtomicU64::new(0);
static RECT: [AtomicI32; 4] = [
    AtomicI32::new(0),
    AtomicI32::new(0),
    AtomicI32::new(0),
    AtomicI32::new(0),
];
/// Buttons whose press was held back, so their release is held back too and
/// no application ever sees half a click.
static HELD_BUTTONS: AtomicU8 = AtomicU8::new(0);
/// The overlay's scale factor × 1000, for the × hit test.
static SCALE_MILLI: AtomicU32 = AtomicU32::new(1000);
static SIGNALS: OnceLock<Sender<Signal>> = OnceLock::new();

enum Signal {
    Nudge,
    Dismiss,
    Cancel,
}

/// Show a live phase of the take: "listening", "transcribing", "finishing"
/// or "cancelling".
pub fn show(app: &AppHandle, state: &str) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let mode = if state == "cancelling" { STALE } else { ACTIVE };
    let phase = if state == "listening" {
        LISTENING
    } else {
        WORKING
    };
    let state = state.to_owned();
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let Some(win) = task_app.get_webview_window("overlay") else {
            return;
        };
        present(&task_app, &win);
        if PHASE.swap(phase, Ordering::SeqCst) != phase {
            PHASE_STARTED_MS.store(crate::db::now_ms() as u64, Ordering::SeqCst);
        }
        MODE.store(mode, Ordering::SeqCst);
        let _ = task_app.emit("dictation-overlay", state);
    });
}

/// Report why a take ended without text. The report puts itself away after a
/// few seconds, or at the first click anywhere.
pub fn show_report(app: &AppHandle, message: &str) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    MODE.store(HIDDEN, Ordering::SeqCst);
    let message = message.to_owned();
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let Some(win) = task_app.get_webview_window("overlay") else {
            return;
        };
        present(&task_app, &win);
        PHASE.store(0, Ordering::SeqCst);
        MODE.store(STALE, Ordering::SeqCst);
        let _ = task_app.emit("dictation-overlay-report", message);
    });
    let timer_app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(REPORT_LINGER);
        if GENERATION.load(Ordering::SeqCst) == generation {
            hide(&timer_app);
        }
    });
}

pub fn hide(app: &AppHandle) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    // Release the hold at once rather than when the main thread gets to it.
    MODE.store(HIDDEN, Ordering::SeqCst);
    PHASE.store(0, Ordering::SeqCst);
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Some(win) = task_app.get_webview_window("overlay") {
            native::hide(&win);
        }
        let _ = task_app.emit("dictation-overlay", "idle");
    });
}

/// The × on a report, or a click elsewhere: put a finished pill away. A live
/// take is only ever ended by cancelling it.
pub fn dismiss(app: &AppHandle) {
    if MODE.load(Ordering::SeqCst) != ACTIVE {
        hide(app);
    }
}

/// Start the click guard: one low-level mouse hook on its own thread, and a
/// worker that turns its verdicts into overlay updates. The hook itself only
/// reads atomics and posts a signal, so it never keeps the pointer waiting.
pub fn start_guard(app: &AppHandle) {
    let (sender, receiver) = mpsc::channel::<Signal>();
    if SIGNALS.set(sender).is_err() {
        return;
    }
    let worker_app = app.clone();
    std::thread::spawn(move || {
        for signal in receiver {
            match signal {
                Signal::Nudge => {
                    let _ = worker_app.emit("dictation-overlay-nudge", ());
                }
                Signal::Dismiss => dismiss(&worker_app),
                Signal::Cancel => super::cancel(&worker_app),
            }
        }
    });
    native::run_mouse_hook();
}

fn present(app: &AppHandle, win: &tauri::WebviewWindow) {
    // Placed when it appears and left there for the whole take: a pill that
    // followed the pointer to another monitor between phases read as the
    // take itself moving away.
    if !native::is_visible(win) {
        position(app, win);
    }
    remember_rect(win);
    native::show_without_focus(win);
}

fn remember_rect(win: &tauri::WebviewWindow) {
    let (Ok(position), Ok(size)) = (win.outer_position(), win.outer_size()) else {
        return;
    };
    RECT[0].store(position.x, Ordering::SeqCst);
    RECT[1].store(position.y, Ordering::SeqCst);
    RECT[2].store(position.x + size.width as i32, Ordering::SeqCst);
    RECT[3].store(position.y + size.height as i32, Ordering::SeqCst);
    let scale = win.scale_factor().unwrap_or(1.0).clamp(0.5, 8.0);
    SCALE_MILLI.store((scale * 1000.0).round() as u32, Ordering::SeqCst);
}

fn inside_overlay(x: i32, y: i32) -> bool {
    x >= RECT[0].load(Ordering::SeqCst)
        && y >= RECT[1].load(Ordering::SeqCst)
        && x < RECT[2].load(Ordering::SeqCst)
        && y < RECT[3].load(Ordering::SeqCst)
}

fn on_close(x: i32, y: i32) -> bool {
    let scale = SCALE_MILLI.load(Ordering::SeqCst) as f64 / 1000.0;
    let css_x = (x - RECT[0].load(Ordering::SeqCst)) as f64 / scale;
    let css_y = (y - RECT[1].load(Ordering::SeqCst)) as f64 / scale;
    (CLOSE_LEFT..CLOSE_RIGHT).contains(&css_x) && (CLOSE_TOP..CLOSE_BOTTOM).contains(&css_y)
}

fn holding_clicks() -> bool {
    if MODE.load(Ordering::SeqCst) != ACTIVE {
        return false;
    }
    PHASE.load(Ordering::SeqCst) == LISTENING
        || (crate::db::now_ms() as u64).saturating_sub(PHASE_STARTED_MS.load(Ordering::SeqCst))
            < WORKING_HOLD_LIMIT_MS
}

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Pass,
    Hold,
    HoldAndNudge,
    HoldAndCancel,
    HoldAndDismiss,
    PassAndDismiss,
}

/// Where a button event landed, as the guard sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Elsewhere,
    Pill,
    Close,
}

/// What to do with one button event. Pure, so the whole policy is tested.
/// `mode` is what the pill shows; `holding` is whether a live take may still
/// hold clicks elsewhere (false once the watchdog has run out).
fn judge(mode: u8, holding: bool, target: Target, pressed: bool, press_was_held: bool) -> Verdict {
    if !pressed {
        return if press_was_held {
            Verdict::Hold
        } else {
            Verdict::Pass
        };
    }
    match (mode, target) {
        (HIDDEN, _) => Verdict::Pass,
        (ACTIVE, Target::Close) => Verdict::HoldAndCancel,
        (ACTIVE, Target::Pill) => Verdict::HoldAndNudge,
        (ACTIVE, Target::Elsewhere) if holding => Verdict::HoldAndNudge,
        (ACTIVE, Target::Elsewhere) => Verdict::Pass,
        (_, Target::Elsewhere) => Verdict::PassAndDismiss,
        (_, _) => Verdict::HoldAndDismiss,
    }
}

/// Called by the hook for every mouse button event; true swallows it.
fn on_button(button: u8, pressed: bool, x: i32, y: i32) -> bool {
    let press_was_held = HELD_BUTTONS.load(Ordering::SeqCst) & button != 0;
    let target = if !inside_overlay(x, y) {
        Target::Elsewhere
    } else if on_close(x, y) {
        Target::Close
    } else {
        Target::Pill
    };
    let verdict = judge(
        MODE.load(Ordering::SeqCst),
        holding_clicks(),
        target,
        pressed,
        press_was_held,
    );
    let signal = |signal: Signal| {
        if let Some(sender) = SIGNALS.get() {
            let _ = sender.send(signal);
        }
    };
    let hold = |signal: Option<Signal>| {
        HELD_BUTTONS.fetch_or(button, Ordering::SeqCst);
        if let Some(signal) = signal {
            let _ = SIGNALS.get().map(|sender| sender.send(signal));
        }
        true
    };
    match verdict {
        Verdict::Pass => false,
        Verdict::Hold => {
            HELD_BUTTONS.fetch_and(!button, Ordering::SeqCst);
            true
        }
        Verdict::HoldAndNudge => hold(Some(Signal::Nudge)),
        Verdict::HoldAndCancel => hold(Some(Signal::Cancel)),
        Verdict::HoldAndDismiss => hold(Some(Signal::Dismiss)),
        Verdict::PassAndDismiss => {
            signal(Signal::Dismiss);
            false
        }
    }
}

#[cfg(windows)]
mod native {
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct MouseHookInfo {
        pt: Point,
        mouse_data: u32,
        flags: u32,
        time: u32,
        extra_info: usize,
    }

    #[repr(C)]
    struct Message {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: Point,
        private: u32,
    }

    const WH_MOUSE_LL: i32 = 14;
    const HC_ACTION: i32 = 0;
    const SW_HIDE: i32 = 0;
    const HWND_TOPMOST: isize = -1;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_SHOWWINDOW: u32 = 0x0040;

    #[link(name = "user32")]
    extern "system" {
        fn ShowWindow(hwnd: isize, command: i32) -> i32;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn SetWindowPos(
            hwnd: isize,
            insert_after: isize,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
        fn SetWindowsHookExW(
            id: i32,
            hook: unsafe extern "system" fn(i32, usize, isize) -> isize,
            module: isize,
            thread: u32,
        ) -> isize;
        fn CallNextHookEx(hook: isize, code: i32, wparam: usize, lparam: isize) -> isize;
        fn GetMessageW(message: *mut Message, hwnd: isize, first: u32, last: u32) -> i32;
    }

    fn handle(win: &tauri::WebviewWindow) -> Option<isize> {
        win.hwnd().ok().map(|hwnd| hwnd.0 as isize)
    }

    pub fn is_visible(win: &tauri::WebviewWindow) -> bool {
        handle(win).is_some_and(|hwnd| unsafe { IsWindowVisible(hwnd) } != 0)
    }

    /// Shown and raised above other topmost windows without being activated.
    /// Tauri's show() activates on every call after the first, which took
    /// focus from the field the words were meant for. Visibility stays out of
    /// Tauri's hands entirely, so its record never disagrees with the window.
    pub fn show_without_focus(win: &tauri::WebviewWindow) {
        if let Some(hwnd) = handle(win) {
            unsafe {
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
    }

    pub fn hide(win: &tauri::WebviewWindow) {
        if let Some(hwnd) = handle(win) {
            unsafe {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    unsafe extern "system" fn mouse_hook(code: i32, wparam: usize, lparam: isize) -> isize {
        if code == HC_ACTION && lparam != 0 {
            let message = wparam as u32;
            let button = match message {
                0x0201 | 0x0202 => Some((1, message == 0x0201)),
                0x0204 | 0x0205 => Some((2, message == 0x0204)),
                0x0207 | 0x0208 => Some((4, message == 0x0207)),
                0x020B | 0x020C => Some((8, message == 0x020B)),
                _ => None,
            };
            if let Some((button, pressed)) = button {
                let info = &*(lparam as *const MouseHookInfo);
                if super::on_button(button, pressed, info.pt.x, info.pt.y) {
                    return 1;
                }
            }
        }
        CallNextHookEx(0, code, wparam, lparam)
    }

    pub fn run_mouse_hook() {
        std::thread::spawn(|| unsafe {
            if SetWindowsHookExW(WH_MOUSE_LL, mouse_hook, 0, 0) == 0 {
                log::warn!("the dictation click guard could not start");
                return;
            }
            let mut message = std::mem::zeroed::<Message>();
            while GetMessageW(&mut message, 0, 0, 0) > 0 {}
        });
    }
}

#[cfg(not(windows))]
mod native {
    pub fn is_visible(win: &tauri::WebviewWindow) -> bool {
        win.is_visible().unwrap_or(false)
    }
    pub fn show_without_focus(win: &tauri::WebviewWindow) {
        let _ = win.show();
    }
    pub fn hide(win: &tauri::WebviewWindow) {
        let _ = win.hide();
    }
    pub fn run_mouse_hook() {}
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
    fn a_live_take_holds_every_click_and_only_the_close_button_ends_it() {
        use Target::*;
        // Elsewhere: held, and the × flashes. Its release is held too.
        assert_eq!(
            judge(ACTIVE, true, Elsewhere, true, false),
            Verdict::HoldAndNudge
        );
        assert_eq!(judge(ACTIVE, true, Elsewhere, false, true), Verdict::Hold);
        // On the pill: held — the webview never takes a click, or focus.
        assert_eq!(
            judge(ACTIVE, true, Pill, true, false),
            Verdict::HoldAndNudge
        );
        // On the ×: the take is cancelled.
        assert_eq!(
            judge(ACTIVE, true, Close, true, false),
            Verdict::HoldAndCancel
        );
        // A stalled take stops holding clicks elsewhere, but its × still works.
        assert_eq!(judge(ACTIVE, false, Elsewhere, true, false), Verdict::Pass);
        assert_eq!(
            judge(ACTIVE, false, Close, true, false),
            Verdict::HoldAndCancel
        );
        // A release whose press went through is never swallowed.
        assert_eq!(judge(ACTIVE, true, Elsewhere, false, false), Verdict::Pass);
    }

    #[test]
    fn a_report_is_put_away_by_any_click_and_a_hidden_pill_touches_nothing() {
        use Target::*;
        assert_eq!(
            judge(STALE, false, Elsewhere, true, false),
            Verdict::PassAndDismiss
        );
        assert_eq!(
            judge(STALE, false, Pill, true, false),
            Verdict::HoldAndDismiss
        );
        assert_eq!(
            judge(STALE, false, Close, true, false),
            Verdict::HoldAndDismiss
        );
        assert_eq!(judge(HIDDEN, false, Close, true, false), Verdict::Pass);
        assert_eq!(judge(HIDDEN, false, Elsewhere, true, false), Verdict::Pass);
        // A press held while live keeps its release held after the take ends.
        assert_eq!(judge(HIDDEN, false, Elsewhere, false, true), Verdict::Hold);
    }

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
