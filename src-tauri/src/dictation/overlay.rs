use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

pub const POSITION_SETTING_KEY: &str = "dictation_overlay_position";
pub const DEFAULT_POSITION: &str = "bottom-middle";
const EDGE_MARGIN: f64 = 40.0;

/// The pill's size in logical pixels. `tauri.conf.json` creates the window at
/// this size, and `present` re-asserts it on every pill show because the review
/// sheet is the same window at the size below.
const PILL_WIDTH: f64 = 260.0;
const PILL_HEIGHT: f64 = 60.0;

/// The review sheet's size in logical pixels. The sheet's own stylesheet lays
/// out to these numbers; the two move together.
const REVIEW_WIDTH: f64 = 420.0;
const REVIEW_HEIGHT: f64 = 300.0;

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
/// The review sheet: the one mode in which the overlay is allowed to take
/// focus, and the one in which the click guard lets every button event through
/// to whoever it belongs to.
const REVIEW: u8 = 3;

const LISTENING: u8 = 1;
const WORKING: u8 = 2;

/// The × hit area in CSS pixels within the 260 × 60 overlay window: the
/// right end of the 212 px pill (`.overlay-pill` in styles.css), generous on
/// purpose. The two must move together.
// The whole guard below — the × hit test, the click hold, the verdicts — is
// the Windows low-level mouse hook's machinery. On Linux it is inert until
// the platform input slice replaces it; the allowances are per item so real
// dead code elsewhere still fails the gate.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const CLOSE_LEFT: f64 = 192.0;
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const CLOSE_RIGHT: f64 = 240.0;
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const CLOSE_TOP: f64 = 10.0;
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const CLOSE_BOTTOM: f64 = 50.0;

/// How long a report stays up when nobody dismisses it.
const REPORT_LINGER: Duration = Duration::from_millis(3200);
/// A watchdog on the modal hold. Listening ends by your own hand — Ctrl, Esc
/// or the × — but transcribing and refining end on their own, and if either
/// ever stalled, clicks must not stay held behind it. Refining alone is
/// bounded at 60 s.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
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
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
static HELD_BUTTONS: AtomicU8 = AtomicU8::new(0);
/// The overlay's scale factor × 1000, for the × hit test.
static SCALE_MILLI: AtomicU32 = AtomicU32::new(1000);
static SIGNALS: OnceLock<Sender<Signal>> = OnceLock::new();

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
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

/// Turn the pill into the review sheet: 420 × 300 at the anchored edge, shown
/// and focused once, with the finished take handed to its webview as
/// `dictation-review`.
///
/// The click guard stands down entirely for the sheet — `MODE` goes `REVIEW`,
/// so every button event is passed through. The sheet is a dialog now: it is
/// meant to be clicked, its fields typed in, and a click beside it must not
/// dismiss the take.
pub fn show_review(app: &AppHandle, text: &str) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    MODE.store(REVIEW, Ordering::SeqCst);
    PHASE.store(0, Ordering::SeqCst);
    let text = text.to_owned();
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        // The generation covers a close that overtook this show; the state
        // check covers a cancel that landed in the same instant. Either way
        // the sheet must not appear over a take that no longer exists.
        if GENERATION.load(Ordering::SeqCst) != generation || !super::is_reviewing() {
            return;
        }
        let Some(win) = task_app.get_webview_window("overlay") else {
            return;
        };
        review_present(&task_app, &win);
        let _ = task_app.emit("dictation-review", serde_json::json!({ "text": text }));
        let _ = task_app.emit("dictation-overlay", "review");
    });
}

/// End the review: hide the sheet, give up the ability to take focus, and put
/// the pill's own size back on the window.
///
/// The hide is deliberately doubled. tao's own keeps tao's model of the window
/// true — its `show()` is a no-op while its VISIBLE flag is set, so the flag has
/// to go back to false with the window or the next review would never appear.
/// The pill's native hide guarantees the window actually leaves even when
/// tao's model had not caught up with a native show (the review never reached
/// its tao show, say a cancel in the same instant).
pub fn close_review(app: &AppHandle) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    MODE.store(HIDDEN, Ordering::SeqCst);
    PHASE.store(0, Ordering::SeqCst);
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Some(win) = task_app.get_webview_window("overlay") {
            // Both hides, and both are load-bearing. tao's keeps tao's model of
            // the window true — its `show()` is a no-op while its VISIBLE flag
            // is set, so the flag has to go back to false with the window. The
            // native one guarantees the window actually leaves: if the review
            // never got to its tao show (a cancel in the same instant), tao
            // still believes the window is hidden from the native pill shows,
            // and its own hide would do nothing at all.
            let _ = win.hide();
            native::hide(&win);
            let _ = win.set_focusable(false);
            let _ = win.set_size(tauri::LogicalSize::new(PILL_WIDTH, PILL_HEIGHT));
        }
        let _ = task_app.emit("dictation-overlay", "idle");
    });
}

/// The × on a report, or a click elsewhere: put a finished pill away. A live
/// take is only ever ended by cancelling it, and the review sheet closes only
/// through its own Confirm, Copy or Dismiss.
pub fn dismiss(app: &AppHandle) {
    if MODE.load(Ordering::SeqCst) == REVIEW {
        return;
    }
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
    //
    // The pill's own size is re-asserted on every show, before the anchor is
    // computed: the review sheet is this same window at 420 × 300, and a next
    // take must not inherit the sheet's geometry from any close path that was
    // skipped. The anchor is then worked out for the size the window really is.
    let _ = win.set_size(tauri::LogicalSize::new(PILL_WIDTH, PILL_HEIGHT));
    if !native::is_visible(win) {
        position(app, win);
    }
    remember_rect(win);
    native::show_without_focus(win);
}

/// The review sheet's window presentation: sized, placed, then shown and
/// focused exactly once.
fn review_present(app: &AppHandle, win: &tauri::WebviewWindow) {
    // tao's focus steal can synthesise an Alt press/release — `SendInput` is
    // literally how tao forces the foreground — and Alt is one of Bard's
    // gesture keys: Alt × 2 opens the drawing canvas, AltGr × 2 opens Settings.
    // A stray tap would arm that gesture and a pair would fire it. The
    // synthetic guard makes the raw-input hook drop Alt and AltGr for the
    // length of the steal (see the filter in `capture.rs`), and entering it
    // also resets the gesture machine, so nothing the steal produces can be
    // read as a shortcut. The sleep below holds the guard over the send.
    let _synthetic = crate::capture::SyntheticInputGuard::enter();

    let _ = win.set_size(tauri::LogicalSize::new(REVIEW_WIDTH, REVIEW_HEIGHT));
    // If the shelf is already up while the take finishes, the sheet opens on
    // the other side of it rather than on top of it.
    let anchor = if crate::panel::is_visible(app) {
        dodge_anchor(current_anchor(app), crate::geometry::shelf_edge(app))
    } else {
        current_anchor(app)
    };
    place_sheet(app, win, anchor);
    remember_rect(win);
    // Focus, exactly once per review, in this order:
    //
    // tao's `set_focus` is a no-op until tao's own VISIBLE flag is set, and the
    // pill is shown with `SetWindowPos` from under tao — so only the `show()`
    // below tells tao the window is up. Its first ever show of a window created
    // with `focus: false` uses `SW_SHOWNOACTIVATE`; that is expected, and the
    // `set_focus` after it runs `force_window_active`. If it fails anyway, the
    // window is left focusable and topmost, so a click activates it: the sheet
    // is fully mouse-operable and there is no retry loop.
    let _ = win.set_focusable(true);
    let _ = win.show();
    let _ = win.set_focus();
    // Let the hook see — and drop — the injected Alt while the guard is still
    // up. The sleep is on the main thread on purpose: the steal just happened
    // here, and a guard that lifts before the hook runs would drop nothing.
    std::thread::sleep(Duration::from_millis(60));
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

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn inside_overlay(x: i32, y: i32) -> bool {
    x >= RECT[0].load(Ordering::SeqCst)
        && y >= RECT[1].load(Ordering::SeqCst)
        && x < RECT[2].load(Ordering::SeqCst)
        && y < RECT[3].load(Ordering::SeqCst)
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn on_close(x: i32, y: i32) -> bool {
    let scale = SCALE_MILLI.load(Ordering::SeqCst) as f64 / 1000.0;
    let css_x = (x - RECT[0].load(Ordering::SeqCst)) as f64 / scale;
    let css_y = (y - RECT[1].load(Ordering::SeqCst)) as f64 / scale;
    (CLOSE_LEFT..CLOSE_RIGHT).contains(&css_x) && (CLOSE_TOP..CLOSE_BOTTOM).contains(&css_y)
}

/// Whether a live take may still hold clicks elsewhere. `elapsed_ms` is how long
/// the phase on screen has been showing. Pure, so the watchdog's effect on the
/// desktop's responsiveness is testable.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn holds_clicks(mode: u8, phase: u8, elapsed_ms: u64) -> bool {
    mode == ACTIVE && (phase == LISTENING || elapsed_ms < WORKING_HOLD_LIMIT_MS)
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn holding_clicks() -> bool {
    holds_clicks(
        MODE.load(Ordering::SeqCst),
        PHASE.load(Ordering::SeqCst),
        (crate::db::now_ms() as u64).saturating_sub(PHASE_STARTED_MS.load(Ordering::SeqCst)),
    )
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
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
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
enum Target {
    Elsewhere,
    Pill,
    Close,
}

/// What to do with one button event. Pure, so the whole policy is tested.
/// `mode` is what the pill shows; `holding` is whether a live take may still
/// hold clicks elsewhere (false once the watchdog has run out).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
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
        // The review sheet is a dialog. Every click — on the sheet, on its ×,
        // on whatever sits behind it — is the person's to make; the guard
        // swallows nothing and dismisses nothing.
        (REVIEW, _) => Verdict::Pass,
        (ACTIVE, Target::Close) => Verdict::HoldAndCancel,
        (ACTIVE, Target::Pill) => Verdict::HoldAndNudge,
        (ACTIVE, Target::Elsewhere) if holding => Verdict::HoldAndNudge,
        (ACTIVE, Target::Elsewhere) => Verdict::Pass,
        (_, Target::Elsewhere) => Verdict::PassAndDismiss,
        (_, _) => Verdict::HoldAndDismiss,
    }
}

/// Called by the hook for every mouse button event; true swallows it.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
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

/// The monitor the overlay belongs on: the one under the pointer when it
/// appears, falling back to where the window already is, then the primary.
fn monitor_for(app: &AppHandle, win: &tauri::WebviewWindow) -> Option<tauri::Monitor> {
    app.cursor_position()
        .ok()
        .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
        .or_else(|| win.current_monitor().ok().flatten())
        .or_else(|| win.primary_monitor().ok().flatten())
}

/// The stored anchor, or the default when nothing valid is stored.
fn current_anchor(app: &AppHandle) -> (Vertical, Horizontal) {
    let value = crate::db::get_setting(app, POSITION_SETTING_KEY)
        .unwrap_or_else(|| DEFAULT_POSITION.to_string());
    parse_position(&value).unwrap_or_else(|| {
        parse_position(DEFAULT_POSITION).expect("default overlay position is valid")
    })
}

fn position(app: &AppHandle, win: &tauri::WebviewWindow) {
    let Some(monitor) = monitor_for(app, win) else {
        return;
    };
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let outer = win.outer_size().unwrap_or_default();
    let scale = win.scale_factor().unwrap_or(1.0);
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
        current_anchor(app),
    );
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Place the sheet at an explicit anchor. Split out so a live reposition (the
/// shelf dodge below) uses the exact same maths as the first placement.
fn place_sheet(app: &AppHandle, win: &tauri::WebviewWindow, anchor: (Vertical, Horizontal)) {
    let Some(monitor) = monitor_for(app, win) else {
        return;
    };
    let work = monitor.work_area();
    let outer = win.outer_size().unwrap_or_default();
    let scale = win.scale_factor().unwrap_or(1.0);
    let (x, y) = sheet_position_for(
        Screen {
            x: work.position.x,
            y: work.position.y,
            width: work.size.width,
            height: work.size.height,
        },
        WindowSize {
            width: outer.width,
            height: outer.height,
        },
        scale,
        anchor,
    );
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Which anchor the sheet takes when the shelf would sit under it: the
/// opposite side of the axis the shelf occupies, or nothing when they are
/// already apart. Pure so the rule has tests; the move it serves is live only
/// — the stored pill anchor is never changed.
fn dodge_anchor(
    (vertical, horizontal): (Vertical, Horizontal),
    shelf: crate::geometry::Edge,
) -> (Vertical, Horizontal) {
    use crate::geometry::Edge;
    match shelf {
        Edge::Bottom if vertical == Vertical::Bottom => (Vertical::Top, horizontal),
        Edge::Top if vertical == Vertical::Top => (Vertical::Bottom, horizontal),
        Edge::Left if horizontal == Horizontal::Left => (vertical, Horizontal::Right),
        Edge::Right if horizontal == Horizontal::Right => (vertical, Horizontal::Left),
        _ => (vertical, horizontal),
    }
}

/// Move the review sheet aside when the shelf shows over it. Called from the
/// shelf's show path; a no-op unless a review is open, and only ever a live
/// reposition — the next take still opens the pill and the sheet at the
/// stored anchor.
pub fn dodge_for_shelf(app: &AppHandle, shelf: crate::geometry::Edge) {
    if MODE.load(Ordering::SeqCst) != REVIEW {
        return;
    }
    let anchor = dodge_anchor(current_anchor(app), shelf);
    if anchor == current_anchor(app) {
        return;
    }
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if MODE.load(Ordering::SeqCst) != REVIEW {
            return;
        }
        if let Some(win) = task_app.get_webview_window("overlay") {
            place_sheet(&task_app, &win, anchor);
            remember_rect(&win);
        }
    });
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

/// The sheet's placement rule: the six pill anchors against the work-area
/// rectangle, with `position_for`'s own clamp when the work area is smaller
/// than the sheet. Named and separate so the choice of rectangle is a thing
/// with tests, not a comment on one call site.
fn sheet_position_for(
    work_area: Screen,
    window: WindowSize,
    scale: f64,
    anchor: (Vertical, Horizontal),
) -> (i32, i32) {
    position_for(work_area, window, scale, anchor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_review_sheet_dodges_the_shelf() {
        use crate::geometry::Edge;
        // Same side: the sheet crosses to the opposite one.
        assert_eq!(
            dodge_anchor((Vertical::Bottom, Horizontal::Middle), Edge::Bottom),
            (Vertical::Top, Horizontal::Middle)
        );
        assert_eq!(
            dodge_anchor((Vertical::Top, Horizontal::Middle), Edge::Top),
            (Vertical::Bottom, Horizontal::Middle)
        );
        assert_eq!(
            dodge_anchor((Vertical::Bottom, Horizontal::Left), Edge::Left),
            (Vertical::Bottom, Horizontal::Right)
        );
        assert_eq!(
            dodge_anchor((Vertical::Bottom, Horizontal::Right), Edge::Right),
            (Vertical::Bottom, Horizontal::Left)
        );
        // Already apart: nothing moves.
        assert_eq!(
            dodge_anchor((Vertical::Top, Horizontal::Middle), Edge::Bottom),
            (Vertical::Top, Horizontal::Middle)
        );
        assert_eq!(
            dodge_anchor((Vertical::Bottom, Horizontal::Middle), Edge::Top),
            (Vertical::Bottom, Horizontal::Middle)
        );
        assert_eq!(
            dodge_anchor((Vertical::Bottom, Horizontal::Middle), Edge::Left),
            (Vertical::Bottom, Horizontal::Middle)
        );
    }

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
    fn the_review_sheet_passes_every_click_through() {
        use Target::*;
        // Every target, every press, every held combination: the sheet is a
        // dialog and the guard is not in its way. The single exception is the
        // release of a press that was held back before the take ended — that
        // stays held so the application never sees half a click, exactly as it
        // does after every other ending.
        for target in [Elsewhere, Pill, Close] {
            for pressed in [false, true] {
                for press_was_held in [false, true] {
                    let expected = if !pressed && press_was_held {
                        Verdict::Hold
                    } else {
                        Verdict::Pass
                    };
                    assert_eq!(
                        judge(REVIEW, false, target, pressed, press_was_held),
                        expected,
                        "target {target:?} pressed {pressed} held {press_was_held}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_stalled_take_stops_holding_the_desktop_after_ninety_seconds() {
        // Whatever the transcription watchdog allows, this is the bound on how
        // long the user's own clicks are swallowed. A take that stalls must not
        // turn into a desktop that will not respond to you.
        assert_eq!(WORKING_HOLD_LIMIT_MS, 90_000);
        // Listening holds indefinitely, because only the user ends it.
        assert!(holds_clicks(ACTIVE, LISTENING, 0));
        assert!(holds_clicks(ACTIVE, LISTENING, 10 * 60_000));
        // Working holds until the watchdog, then stops.
        assert!(holds_clicks(ACTIVE, WORKING, 0));
        assert!(holds_clicks(ACTIVE, WORKING, WORKING_HOLD_LIMIT_MS - 1));
        assert!(!holds_clicks(ACTIVE, WORKING, WORKING_HOLD_LIMIT_MS));
        // A report holds nothing.
        assert!(!holds_clicks(STALE, WORKING, 0));
        assert!(!holds_clicks(HIDDEN, LISTENING, 0));
        // Neither does the review sheet: it is a dialog, and its buttons are
        // the person's to click.
        assert!(!holds_clicks(REVIEW, LISTENING, 0));
        assert!(!holds_clicks(REVIEW, WORKING, 0));
        // And the click hold always ends sooner than the take ceiling, so a
        // wedged take never becomes a wedged desktop.
        assert!(
            crate::dictation::TAKE_DEADLINE_FLOOR > Duration::from_millis(WORKING_HOLD_LIMIT_MS)
        );
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

    #[test]
    fn places_the_review_sheet_at_every_anchor() {
        // The pill's six anchors, with the sheet's own 420 × 300 geometry.
        let work = Screen {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let sheet = WindowSize {
            width: 420,
            height: 300,
        };
        for (value, expected) in [
            ("top-left", (40, 40)),
            ("top-middle", (750, 40)),
            ("top-right", (1460, 40)),
            ("bottom-left", (40, 740)),
            ("bottom-middle", (750, 740)),
            ("bottom-right", (1460, 740)),
        ] {
            let anchor = parse_position(value).expect("valid position");
            assert_eq!(
                sheet_position_for(work, sheet, 1.0, anchor),
                expected,
                "{value}"
            );
        }
    }

    #[test]
    fn clamps_the_review_sheet_onto_a_small_screen() {
        // A work area smaller than the sheet itself: every anchor still
        // resolves to an on-screen origin — the work area's own corner —
        // rather than to a negative or an off-screen edge.
        let work = Screen {
            x: 100,
            y: 50,
            width: 380,
            height: 260,
        };
        let sheet = WindowSize {
            width: 420,
            height: 300,
        };
        for value in [
            "top-left",
            "top-middle",
            "top-right",
            "bottom-left",
            "bottom-middle",
            "bottom-right",
        ] {
            let anchor = parse_position(value).expect("valid position");
            assert_eq!(
                sheet_position_for(work, sheet, 1.0, anchor),
                (100, 50),
                "{value}"
            );
        }
    }

    #[test]
    fn the_review_sheet_uses_the_work_area_not_the_monitor() {
        // A 40 px taskbar along the bottom: with the work area the sheet's
        // bottom edge sits on it; with the monitor's own height the same
        // anchor puts the sheet 40 px lower, half of it behind the taskbar.
        let monitor = Screen {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let work = Screen {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        };
        let sheet = WindowSize {
            width: 420,
            height: 300,
        };
        let anchor = parse_position("bottom-left").expect("valid position");
        assert_eq!(sheet_position_for(work, sheet, 1.0, anchor), (40, 700));
        assert_eq!(position_for(monitor, sheet, 1.0, anchor), (40, 740));
    }
}
