use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

/// Settings' and Draw's size on their first open, in logical pixels.
pub const COMPANION_WIDTH: f64 = 720.0;
pub const COMPANION_HEIGHT: f64 = 620.0;

/// Put a companion away, remembering where it was. Fullscreen is left first,
/// so the next open starts from the window's own geometry, not the monitor's.
pub fn hide_companion(app: &AppHandle, win: &tauri::WebviewWindow) {
    crate::geometry::remember(app, win);
    if win.is_fullscreen().unwrap_or(false) {
        let _ = win.set_fullscreen(false);
    }
    let _ = win.hide();
    companions_changed(app);
}

/// Tell the panel which companions are open, so its Draw and Settings
/// buttons show as pressed while their window is out.
pub fn companions_changed(app: &AppHandle) {
    let open = |label: &str| {
        app.get_webview_window(label)
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false)
    };
    let _ = app.emit_to(
        "main",
        "companions",
        serde_json::json!({ "draw": open("draw"), "settings": open("settings") }),
    );
}

pub fn focus_panel(app: &tauri::AppHandle) {
    if let Some(panel) = app.get_webview_window("main") {
        if panel.is_visible().unwrap_or(false) {
            let _ = panel.set_focus();
        }
    }
}

/// Set when the panel is shown deliberately. The auto-hide checks it and
/// stands down, because a window that has just been summoned goes through a
/// burst of focus changes while it takes the foreground — and hiding on one of
/// those is why summoning sometimes appeared to do nothing and needed a second
/// try.
static SHOWN_AT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How long after a deliberate show the auto-hide keeps out of the way.
const SETTLE_MS: u64 = 700;

fn mark_shown() {
    SHOWN_AT.store(
        crate::db::now_ms() as u64,
        std::sync::atomic::Ordering::SeqCst,
    );
}

fn just_shown() -> bool {
    let at = SHOWN_AT.load(std::sync::atomic::Ordering::SeqCst);
    at > 0 && (crate::db::now_ms() as u64).saturating_sub(at) < SETTLE_MS
}

/// Window operations are a Windows-main-thread responsibility. The global
/// input listener and tray callbacks are not on that thread, so every public
/// entry point queues the actual work instead of calling Tauri directly.
pub fn show_now(app: &tauri::AppHandle) {
    remember_foreground();
    mark_shown();
    let app = app.clone();
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || show_now_inner(&task_app));
}

fn show_now_inner(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    // A summon in flight is recorded before anything else: a slide-out still
    // waiting on the main thread sees this and leaves the window alone.
    SUMMONS.fetch_add(1, Ordering::SeqCst);
    // Unminimise first, then place. A minimised window's rectangle is not the
    // window's to change — Windows answers SetWindowPos about the restored
    // rectangle — so the shelf is brought back to normal before it is anchored.
    let _ = win.unminimize();
    let setup = crate::db::get_setting(app, "onboarding_completed").as_deref() != Some("1");
    if setup {
        // Setup is a normal desktop window. In particular it must not inherit
        // the shelf's full-monitor width and 292px height on first launch.
        if !win.is_decorated().unwrap_or(false) {
            let _ = win.set_title("Set up Bard");
            let _ = win.set_decorations(true);
            let _ = win.set_shadow(true);
            let _ = win.set_always_on_top(false);
            let _ = win.set_skip_taskbar(false);
            let _ = win.set_resizable(true);
            let _ = win.set_min_size(Some(tauri::LogicalSize::new(480.0, 440.0)));
            crate::geometry::restore(app, &win, 620.0, 680.0);
        }
    } else {
        if win.is_decorated().unwrap_or(false) {
            let _ = win.unmaximize();
            let _ = win.set_title("Bard");
            let _ = win.set_min_size(None::<tauri::LogicalSize<f64>>);
            let _ = win.set_resizable(false);
            let _ = win.set_decorations(false);
            let _ = win.set_shadow(false);
            let _ = win.set_skip_taskbar(true);
            let _ = win.set_always_on_top(true);
        }
        crate::geometry::anchor(app, &win, crate::geometry::shelf_edge(app));
    }
    let _ = win.show();
    let _ = win.set_focus();
    let _ = app.emit("panel-shown", ());
    companions_changed(app);
    // A review sheet may share the screen: when both would sit on the same
    // side, the sheet steps to the other one rather than being covered.
    crate::dictation::overlay::dodge_for_shelf(app, crate::geometry::shelf_edge(app));
}

/// Whether the shelf is on screen. Used by the review sheet's placement rule
/// when a take finishes with the panel already open.
pub fn is_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|win| win.is_visible().ok())
        .unwrap_or(false)
}

/// Bumped by every summon. A slide-out carries the value it started with and
/// stands down if the shelf was summoned again while it waited, because the
/// window on screen belongs to the new summon.
static SUMMONS: AtomicU64 = AtomicU64::new(0);

/// A one-shot signal: a flag and a condition variable, opened once and closed
/// when the next round begins.
struct Gate {
    open: Mutex<bool>,
    cv: Condvar,
}

impl Gate {
    const fn new() -> Gate {
        Gate {
            open: Mutex::new(false),
            cv: Condvar::new(),
        }
    }

    fn close(&self) {
        *self
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = false;
    }

    fn open(&self) {
        *self
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        self.cv.notify_all();
    }

    /// Wait up to `timeout` for the gate to open. `true` when it did.
    fn wait(&self, timeout: Duration) -> bool {
        let open = self
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *open {
            return true;
        }
        let (open, _) = self
            .cv
            .wait_timeout_while(open, timeout, |open| !*open)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *open
    }
}

/// The frontend's `shelf_hidden` ack: the slide-out animation is over, and the
/// window may leave.
static SHELF_ACK: Gate = Gate::new();

/// The hide itself: the window is away, parked, and the foreground is back.
static SHELF_HIDDEN: Gate = Gate::new();

/// How long a hide waits for the frontend's ack. The slide is a couple of
/// hundred milliseconds; longer than this means the renderer is not listening,
/// and the hide goes ahead without it.
const SHELF_ACK_TIMEOUT: Duration = Duration::from_millis(450);

/// The frontend has finished sliding the shelf out; from here it can hide.
pub fn shelf_hidden_ack() {
    SHELF_ACK.open();
}

/// Wait until the shelf is actually away — hidden, parked, and the foreground
/// back with the app it was summoned over. `false` means the wait ran out; the
/// caller carries on, because a paste that never happens is worse than one
/// that lands a moment early.
pub fn wait_hidden(timeout: Duration) -> bool {
    SHELF_HIDDEN.wait(timeout)
}

pub fn hide_now(app: &tauri::AppHandle) {
    // Re-arm both handshakes before the work is queued: a `paste_back` calling
    // this and then waiting must wait for *this* hide, not the one before it.
    SHELF_ACK.close();
    SHELF_HIDDEN.close();
    let app = app.clone();
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || hide_now_inner(&task_app, None));
}

/// Take the shelf away.
///
/// It used to vanish in one call. It now slides out, which means the window has
/// to stay on screen for the length of the animation, so the hide is a
/// handshake: `shelf-hiding` goes out, the frontend answers with the
/// `shelf_hidden` invoke, and only then is the window hidden, parked clear of
/// the work area and the foreground handed back. An ack that never arrives
/// costs the wait and nothing else.
///
/// The wait runs on its own thread because the ack is a Tauri command and
/// commands are handled on the main thread: waiting for one there would be
/// waiting for the thread that has to deliver it.
///
/// `blur_generation` is the focus generation an auto-hide was scheduled with,
/// or `None` for a hide someone asked for. Coming back to the shelf during the
/// slide cancels an auto-hide and completes a deliberate one.
fn hide_now_inner(app: &tauri::AppHandle, blur_generation: Option<u64>) {
    let Some(win) = app.get_webview_window("main") else {
        SHELF_HIDDEN.open();
        return;
    };
    if !win.is_visible().unwrap_or(false) {
        // Already away: nothing to slide out, so nothing to wait for.
        SHELF_HIDDEN.open();
        return;
    }
    if crate::db::get_setting(app, "onboarding_completed").as_deref() != Some("1") {
        // A closed setup window stays at its normal position for the next
        // tray/launcher open; parking it like the shelf would strand it offscreen.
        let _ = win.hide();
        SHELF_HIDDEN.open();
        return;
    }
    SHELF_ACK.close();
    SHELF_HIDDEN.close();
    let summons = SUMMONS.load(Ordering::SeqCst);
    let _ = app.emit_to("main", "shelf-hiding", ());
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = SHELF_ACK.wait(SHELF_ACK_TIMEOUT);
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || finish_hide(&handle, summons, blur_generation));
    });
}

/// The second half of the slide-out handshake, back on the main thread.
fn finish_hide(app: &tauri::AppHandle, summons: u64, blur_generation: Option<u64>) {
    if let Some(generation) = blur_generation {
        if FOCUS_GENERATION.load(Ordering::SeqCst) != generation {
            // Someone came back to the shelf while it was sliding away, which
            // is exactly what the blur's grace period exists to protect.
            SHELF_HIDDEN.open();
            return;
        }
    }
    if SUMMONS.load(Ordering::SeqCst) != summons {
        // Summoned again while the slide was running. The animation is going
        // the wrong way, but the window stays where the new summon put it.
        SHELF_HIDDEN.open();
        return;
    }
    let Some(win) = app.get_webview_window("main") else {
        SHELF_HIDDEN.open();
        return;
    };
    let _ = win.hide();
    crate::geometry::park(app, &win, crate::geometry::shelf_edge(app));
    restore_foreground();
    SHELF_HIDDEN.open();
}

/// Bumped on every focus change. A pending auto-hide carries the value it was
/// scheduled with and abandons itself if anything has happened since, which is
/// what makes a refocus — or a second blur — cancel it cleanly.
static FOCUS_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Grace period before an unfocused panel hides. Long enough to absorb the
/// focus flicker of opening one of our own windows or a native file dialog,
/// short enough that the panel is gone by the time you have looked away.
const HIDE_GRACE_MS: u64 = 140;

/// The panel is summoned, so it dismisses itself once it is no longer what you
/// are using — including when it never lost focus in the webview's opinion,
/// which is why this is driven by the native window event rather than a blur
/// listener in JS.
pub fn on_focus_change(app: &AppHandle, focused: bool) {
    use std::sync::atomic::Ordering;

    let generation = FOCUS_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if focused {
        return;
    }

    // Focus events arrive while the window is still being created, before
    // setup() has opened the database. Reading a setting here panics the app
    // before it has drawn anything.
    if !crate::db::is_ready(app) {
        return;
    }

    // Onboarding is a first-run flow, not a summoned panel: hiding it the
    // instant the window loses focus would strand a new user with nothing on
    // screen and no idea how to get it back.
    if crate::db::get_setting(app, "onboarding_completed") != Some("1".into()) {
        return;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(HIDE_GRACE_MS));
        if FOCUS_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        if just_shown() {
            return;
        }
        // Settings and the drawing canvas are ours; opening one should not
        // yank the panel away behind it.
        if crate::fg::foreground_is_ours() {
            return;
        }
        // Window operations have to happen on the main thread on Windows.
        // Called straight from this timer thread, hide() returns Ok and does
        // nothing at all.
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || hide_now_inner(&handle, Some(generation)));
    });
}

pub fn toggle_now(app: &AppHandle) {
    let app = app.clone();
    let task_app = app.clone();
    let _ = app.run_on_main_thread(move || toggle_now_inner(&task_app));
}

fn toggle_now_inner(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    if win.is_visible().unwrap_or(false) {
        if win.is_focused().unwrap_or(false) {
            hide_now_inner(app, None);
        } else {
            // Refocusing an already-visible panel. Deliberately does *not* emit
            // `panel-shown`: the panel's composer asserts focus when it hears that,
            // so announcing a refocus would throw away whatever the person was
            // doing inside it — a half-typed inline edit, a card selection, the
            // search field — and send their next keystrokes to the composer.
            SUMMONS.fetch_add(1, Ordering::SeqCst);
            remember_foreground();
            let _ = win.set_focus();
        }
    } else {
        remember_foreground();
        mark_shown();
        show_now_inner(app);
    }
}

/// The window the panel was summoned over, so hiding can give it back. This is
/// the panel's own memory; `crate::fg` owns the native calls and the question
/// of whether the foreground is ours.
static LAST_HWND: AtomicIsize = AtomicIsize::new(0);

fn remember_foreground() {
    LAST_HWND.store(crate::fg::current(), Ordering::SeqCst);
}

/// The window the panel was summoned over, for a feature that outlives the
/// panel's own focus: a sketch drawn while the panel is open goes back to the
/// app the panel came from. 0 when nothing is remembered.
pub fn remembered_foreground() -> isize {
    LAST_HWND.load(Ordering::SeqCst)
}

fn restore_foreground() {
    crate::fg::focus(LAST_HWND.load(Ordering::SeqCst));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_gate_does_not_open_on_its_own() {
        let gate = Gate::new();
        assert!(!gate.wait(Duration::from_millis(20)));
        gate.open();
        assert!(gate.wait(Duration::from_millis(20)));
        // A new round closes it again: a stale open must not release a later
        // waiter before its own handshake is done.
        gate.close();
        assert!(!gate.wait(Duration::from_millis(20)));
    }

    #[test]
    fn a_waiting_hide_is_woken_by_the_ack() {
        let gate = std::sync::Arc::new(Gate::new());
        let opener = std::sync::Arc::clone(&gate);
        let ack = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            opener.open();
        });
        // The wait is bounded, but the ack gets there long before the bound.
        assert!(gate.wait(Duration::from_millis(2_000)));
        ack.join().unwrap();
    }
}
