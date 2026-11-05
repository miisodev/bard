use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use rdev::{Button, Event, EventType, Key};
use tauri::{AppHandle, Emitter};

use crate::gesture::{GestureAction, GestureKey, GestureState};

pub const SENTINEL: &str = "\u{200B}bard::capturing\u{200B}";
static SYNTHETIC_INPUT: AtomicBool = AtomicBool::new(false);
/// How many synthetic-input guards are live. See [`SyntheticInputGuard`].
static SYNTHETIC_DEPTH: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// How many of the current burst's synthetic keys the hook has dropped. Zeroed
/// when a burst begins, and counted where the drop happens, so a caller that
/// has to hold the guard can wait for the burst's keys rather than for a clock
/// — see [`wait_for_synthetic`].
static SYNTHETIC_DROPPED: AtomicU64 = AtomicU64::new(0);
static GESTURE_RESET: AtomicBool = AtomicBool::new(false);

/// True only while the raw-input hook thread is inside `hook::listen`. The worker
/// reads it to tell a hook that ended from a channel that closed for another
/// reason, and it is the difference between a diagnosable dead input stack and
/// an app that is simply unresponsive with nothing in any log.
static LISTENER_ALIVE: AtomicBool = AtomicBool::new(false);
/// Counts hook starts, so a re-arm names its attempt in the lifecycle log and a
/// hot respawn loop is visible rather than silent.
static LISTENER_ATTEMPT: AtomicU64 = AtomicU64::new(0);
/// Counts every raw event the hook has forwarded. The worker's heartbeat records
/// the running total, so a hook that is still installed but no longer delivering
/// is distinguishable from a hook that ended — the two produce identical silence
/// and only one of them is fixed by re-arming.
static EVENT_COUNT: AtomicU64 = AtomicU64::new(0);

/// How often the worker notes that input is flowing. The heartbeat is only
/// written when the count has moved since the last one, so an idle Bard writes
/// nothing and a Bard in use writes about one line a minute.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);

struct ClipboardSnapshot {
    text: Option<String>,
    html: Option<String>,
    image: Option<arboard::ImageData<'static>>,
    files: Option<Vec<PathBuf>>,
}

fn snapshot_clipboard(clipboard: &mut arboard::Clipboard) -> ClipboardSnapshot {
    ClipboardSnapshot {
        text: clipboard.get_text().ok(),
        html: clipboard.get().html().ok(),
        image: clipboard.get_image().ok().map(|image| image.to_owned_img()),
        files: clipboard.get().file_list().ok(),
    }
}

fn restore_snapshot(app: &AppHandle, snapshot: &ClipboardSnapshot) {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(clipboard) => clipboard,
        Err(_) => return,
    };
    let restored = if let Some(text) = snapshot.text.as_ref() {
        if let Some(html) = snapshot.html.as_ref() {
            clipboard.set().html(html.clone(), Some(text.clone()))
        } else {
            clipboard.set_text(text.clone())
        }
    } else if let Some(image) = snapshot.image.as_ref() {
        clipboard.set_image(image.clone())
    } else if let Some(files) = snapshot.files.as_ref() {
        clipboard.set().file_list(files)
    } else {
        clipboard.clear()
    };
    if restored.is_ok() {
        crate::monitor::note_own_write(app);
    }
}

/// Suppresses gesture interpretation while Bard synthesises its own keystrokes.
///
/// A depth counter rather than a boolean, because these bursts genuinely overlap:
/// a selection capture can begin while a dictation paste chord is still being
/// synthesised. With a plain flag the inner guard's `Drop` cleared the
/// suppression while the outer burst was still running, so Bard's own Ctrl+C
/// became visible to the coordinator and could be read as a Ctrl tap. In the
/// other direction, while the flag was set the hook discarded *every* event, so
/// genuine user input during a paste was lost outright for up to 180ms.
pub struct SyntheticInputGuard {
    outermost: bool,
}

impl SyntheticInputGuard {
    pub fn enter() -> Self {
        let outermost = SYNTHETIC_DEPTH.fetch_add(1, Ordering::SeqCst) == 0;
        SYNTHETIC_INPUT.store(true, Ordering::SeqCst);
        // Only reset the gesture machine on the outermost entry: a nested burst
        // is part of the same synthetic event, and clearing mid-sequence would
        // drop the pending state the outer one established.
        if outermost {
            SYNTHETIC_DROPPED.store(0, Ordering::SeqCst);
            GESTURE_RESET.store(true, Ordering::SeqCst);
        }
        Self { outermost }
    }
}

impl Drop for SyntheticInputGuard {
    fn drop(&mut self) {
        if SYNTHETIC_DEPTH.fetch_sub(1, Ordering::SeqCst) == 1 && self.outermost {
            SYNTHETIC_INPUT.store(false, Ordering::SeqCst);
        }
    }
}

fn gesture_key(key: Key) -> Option<GestureKey> {
    match key {
        Key::ShiftLeft => Some(GestureKey::LeftShift),
        Key::ShiftRight => Some(GestureKey::RightShift),
        Key::ControlLeft => Some(GestureKey::LeftControl),
        Key::ControlRight => Some(GestureKey::RightControl),
        Key::Alt => Some(GestureKey::LeftAlt),
        Key::AltGr => Some(GestureKey::RightAlt),
        _ => None,
    }
}

fn item_slot(key: Key) -> Option<u8> {
    match key {
        Key::Num1 | Key::Kp1 => Some(1),
        Key::Num2 | Key::Kp2 => Some(2),
        Key::Num3 | Key::Kp3 => Some(3),
        _ => None,
    }
}

fn foreign_buttons_down(buttons: &HashSet<Button>) -> bool {
    !buttons.is_empty()
}

fn handle_gesture(action: GestureAction, app: &AppHandle) {
    let label = match action {
        GestureAction::Capture => "gesture:capture",
        GestureAction::TogglePanel => "gesture:panel",
        GestureAction::ToggleDraw => "gesture:draw",
        GestureAction::ToggleSettings => "gesture:settings",
        GestureAction::PasteItem { .. } => "gesture:paste-item",
        GestureAction::DictationPress { .. } => "gesture:dictation-press",
        GestureAction::DictationRelease { .. } => "gesture:dictation-release",
    };
    crate::crash::record_event(label);
    match action {
        GestureAction::Capture => {
            if crate::db::get_setting(app, "capture_double_shift").as_deref() != Some("0") {
                capture_selection(app);
            } else {
                // The gesture still fires and still logged, then did nothing at
                // all, with no indication that capture had been switched off.
                // The Shortcuts pane still lists the gesture, so the user had no
                // way to connect the two.
                let _ = app.emit(
                    "capture-failed",
                    "Double-Shift capture is off. Turn it back on in Settings › Capture.",
                );
            }
        }
        GestureAction::TogglePanel => crate::panel::toggle_now(app),
        GestureAction::ToggleDraw => crate::commands::draw_gesture(app),
        GestureAction::ToggleSettings => crate::commands::settings_gesture(app),
        GestureAction::PasteItem { slot } => {
            if let Err(error) = crate::commands::paste_item(app, slot) {
                // A failed paste used to be one `log::warn!` to a sink that
                // never existed. The gesture is global and the user is looking
                // at another window, so the only place they will see the failure
                // is the panel.
                log::warn!("item paste failed: {error}");
                let _ = app.emit_to("main", "capture-failed", error);
            }
        }
        GestureAction::DictationPress { token } => crate::dictation::gesture_press(app, token),
        GestureAction::DictationRelease { token, held } => {
            crate::dictation::gesture_release(app, token, held)
        }
    }
}

/// Suppress only the keys Bard itself synthesises, rather than every event.
///
/// The blanket discard meant that for the whole duration of a Ctrl+C or Ctrl+V
/// burst — up to 180ms — a person's real typing was thrown away, so a keypress
/// could simply vanish. Bard sends Control for its copy and paste chords, and
/// scoping the filter to those keys removed the loss without letting Bard's own
/// chords be read as gestures. Alt joined the list with the review sheet: tao
/// forces the foreground with a synthetic Alt tap (`SendInput`, in
/// `force_window_active`), and Alt is a gesture key — one stray tap dirties the
/// gesture machine and two open the drawing canvas or Settings. Control and Alt
/// are both only ever dropped while a synthetic burst is live.
fn is_synthetic_key(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::KeyPress(Key::ControlLeft | Key::ControlRight | Key::Alt | Key::AltGr)
            | EventType::KeyRelease(Key::ControlLeft | Key::ControlRight | Key::Alt | Key::AltGr)
    )
}

/// Whether this event is one of Bard's own synthetic keystrokes and must not
/// reach the gesture machine. `burst` is the synthetic-input flag itself,
/// passed in rather than read here so the decision is a pure function.
fn drops_synthetic(burst: bool, event_type: &EventType) -> bool {
    burst && is_synthetic_key(event_type)
}

/// How often [`wait_for_synthetic`] looks at the count while it waits.
const SYNTHETIC_POLL: Duration = Duration::from_millis(2);

/// Wait, up to `bound`, for the hook to have dropped `expected` of this burst's
/// synthetic keys, and report whether it did.
///
/// A guard that drops every Alt and Control it sees is worth exactly as long as
/// the synthetic keys need it: hold it longer and a real press that arrives in
/// the meantime is discarded with them. Waiting for the burst's own keys — the
/// hook counts them as it drops them — lifts the guard a millisecond or two
/// after the tap has passed, where a fixed sleep held it over the whole window
/// whether the tap had been and gone or never came. The bound is what a burst
/// that sends nothing at all costs, and it is the ceiling, not the wait.
pub fn wait_for_synthetic(expected: u64, bound: Duration) -> bool {
    let started = Instant::now();
    loop {
        if SYNTHETIC_DROPPED.load(Ordering::SeqCst) >= expected {
            return true;
        }
        if started.elapsed() >= bound {
            return false;
        }
        thread::sleep(SYNTHETIC_POLL);
    }
}

/// Start the raw-input hook. `sender` is moved in rather than cloned so the hook
/// thread holds the only sender: when the thread ends, the channel closes and the
/// worker sees it. A clone kept anywhere else would hide the failure forever.
///
/// Every exit path records, because the failure this exists to catch is silent
/// by construction. The hook can end three ways — an error, a clean return, or the
/// process tearing down — and the first two used to leave no trace anywhere.
fn spawn_listener(sender: mpsc::Sender<Event>) {
    thread::spawn(move || {
        let hook_sender = sender.clone();
        LISTENER_ALIVE.store(true, Ordering::Release);
        let end = crate::hook::listen(move |event| {
            EVENT_COUNT.fetch_add(1, Ordering::Relaxed);
            let burst = SYNTHETIC_INPUT.load(Ordering::SeqCst);
            if drops_synthetic(burst, &event.event_type) {
                // Counted where the drop happens, so a caller holding the guard
                // can tell a tap that has passed from one still in flight.
                SYNTHETIC_DROPPED.fetch_add(1, Ordering::SeqCst);
            } else {
                let _ = hook_sender.send(event);
            }
        });
        LISTENER_ALIVE.store(false, Ordering::Release);
        crate::crash::record_event(&format!("input:listener-ended:{end}"));
    });
}

/// How long to wait before re-arming a dead hook. Doubling from two seconds to a
/// one-minute ceiling recovers a transient failure in seconds and cannot spin if
/// the hook fails immediately and every time.
const LISTENER_RETRY_MIN: Duration = Duration::from_secs(2);
const LISTENER_RETRY_MAX: Duration = Duration::from_secs(60);

/// A hook that stayed up at least this long counts as working, and resets the
/// backoff. A hook that dies sooner is treated as still broken, so a permanently
/// failing one settles at the one-minute ceiling rather than respawning every
/// two seconds and filling the lifecycle log with noise that evicts the evidence
/// for everything else.
const HEALTHY_LISTENER_RUN: Duration = Duration::from_secs(30);

/// Start the single raw-input coordinator. The hook only forwards events
/// to a normal worker; all timing, state, and application actions happen off
/// the low-level callback thread.
///
/// The worker owns the channel and therefore the hook's lifetime. If the hook
/// ends for any reason, the worker notices the closure, records it, and re-arms a
/// fresh hook rather than exiting and leaving Bard permanently deaf to its own
/// shortcuts. On 2026-09-25 the installed 0.1.2 did exactly that: the hook
/// returned, the worker hit `Disconnected` and fell out of its loop, and the app
/// sat for thirteen hours with every window hidden and nothing in any log.
pub fn start(app: AppHandle) {
    let worker_app = app.clone();
    thread::spawn(move || {
        let mut backoff = LISTENER_RETRY_MIN;
        // When the hook that just died was last armed. A run that lasted long
        // enough to count as healthy resets the backoff; one that died
        // immediately lets it keep doubling.
        let mut previous_armed_at: Option<Instant> = None;

        loop {
            let attempt = LISTENER_ATTEMPT.fetch_add(1, Ordering::AcqRel) + 1;
            let (sender, receiver) = mpsc::channel::<Event>();
            spawn_listener(sender);
            let armed_at = Instant::now();
            if attempt == 1 {
                crate::crash::record_event("input:listener-started");
            } else {
                crate::crash::record_event(&format!("input:listener-restarted:{attempt}"));
                // The backoff is reset only when the *previous* hook actually
                // worked. Resetting it unconditionally meant the sleep was always
                // the 2-second floor and the 60-second ceiling was dead code: a
                // hook that fails immediately and permanently — a blocked or
                // denied low-level hook, a conflicting one from another product —
                // respawned every two seconds forever, writing two lifecycle lines
                // each time and rotating away the evidence for everything else
                // inside about ninety minutes.
                if previous_armed_at.is_some_and(|since| {
                    armed_at.saturating_duration_since(since) > HEALTHY_LISTENER_RUN
                }) {
                    backoff = LISTENER_RETRY_MIN;
                }
            }

            let mut gestures = GestureState::new();
            let mut foreign_keys: HashSet<Key> = HashSet::new();
            let mut foreign_buttons: HashSet<Button> = HashSet::new();
            let mut next_heartbeat = Instant::now() + HEARTBEAT_INTERVAL;
            let mut last_count = EVENT_COUNT.load(Ordering::Relaxed);

            loop {
                if GESTURE_RESET.swap(false, Ordering::AcqRel) {
                    gestures.reset();
                    foreign_keys.clear();
                    foreign_buttons.clear();
                }
                match receiver.recv_timeout(Duration::from_millis(20)) {
                    Ok(event) => process_raw_event(
                        event,
                        &mut gestures,
                        &mut foreign_keys,
                        &mut foreign_buttons,
                        &worker_app,
                    ),
                    Err(RecvTimeoutError::Timeout) => {
                        // If the coordinator has been disarmed by a lost
                        // release, the adapter's physical-key sets still hold the
                        // stale entries. Clearing them alongside the state keeps
                        // the two in agreement; clearing only the state would let
                        // the next foreign key press re-disarm it immediately.
                        if gestures.expire_stale_foreign(Instant::now()) {
                            foreign_keys.clear();
                            foreign_buttons.clear();
                            crate::crash::record_event("input:foreign-state-recovered");
                        }
                        for action in gestures.poll(Instant::now()) {
                            handle_gesture(action, &worker_app);
                        }
                        let now = Instant::now();
                        if now >= next_heartbeat {
                            let count = EVENT_COUNT.load(Ordering::Relaxed);
                            if count != last_count {
                                crate::crash::record_event(&format!("input:alive:{count}"));
                            }
                            last_count = count;
                            next_heartbeat = now + HEARTBEAT_INTERVAL;
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }

            // The channel closed. `LISTENER_ALIVE` is cleared by the hook thread
            // *before* it drops the sender, so a genuine end always reads false
            // here — a true reading means the channel closed for some other
            // reason and the hook never got to record why.
            if LISTENER_ALIVE.load(Ordering::Acquire) {
                crate::crash::record_event("input:hook-vanished");
            }
            previous_armed_at = Some(armed_at);
            crate::crash::record_event("input:worker-rearming");
            // Gesture state is rebuilt fresh on every re-arm. The sets below are
            // the coordinator's whole memory of which keys are physically down,
            // and one stranded entry makes every later gesture key press look
            // "dirty" — so the coordinator is permanently disarmed with nothing in
            // any log. Re-arming with a clean machine is what makes the recovery
            // actually recover.
            thread::sleep(backoff);
            backoff = (backoff * 2).min(LISTENER_RETRY_MAX);
        }
    });
}

fn process_raw_event(
    event: Event,
    gestures: &mut GestureState,
    foreign_keys: &mut HashSet<Key>,
    foreign_buttons: &mut HashSet<Button>,
    app: &AppHandle,
) {
    // Deliberately no blanket early-return on SYNTHETIC_INPUT here.
    //
    // The hook already scopes its suppression to the Control key, so Bard's own
    // Ctrl+C and Ctrl+V never reach this function — and a blanket discard would
    // throw away every *genuine* keystroke that arrived during the same 40-180ms
    // window, on the two most-used actions in the product. A keypress could simply
    // vanish.
    //
    // The gesture machine is still reset across a synthetic burst, but by
    // `GESTURE_RESET`, which `SyntheticInputGuard::enter` sets on the outermost
    // entry and which the worker's own loop consumes before it calls in here.
    let now = Instant::now();
    match event.event_type {
        EventType::KeyPress(key) => {
            // `is_busy`, not `is_recording`: Escape has to cancel a take that is
            // still transcribing too. Gating on `Recording` only left the
            // keyboard unable to stop a wedged transcription, and the overlay's
            // own close button is unreachable while the click guard holds every
            // click in the pill and the window cannot take focus. `cancel`
            // already branches correctly per phase.
            if key == Key::Escape && crate::dictation::is_busy() {
                gestures.invalidate();
                crate::dictation::cancel(app);
            }
            if let Some(key) = gesture_key(key) {
                for action in gestures.press(key, now) {
                    handle_gesture(action, app);
                }
            } else {
                let slot_result = item_slot(key).map(|slot| gestures.consume_item_slot(slot, now));
                let consumed = match slot_result {
                    Some(crate::gesture::ItemSlotResult::Paste(slot)) => {
                        handle_gesture(crate::gesture::GestureAction::PasteItem { slot }, app);
                        true
                    }
                    Some(crate::gesture::ItemSlotResult::Cancelled) => true,
                    Some(crate::gesture::ItemSlotResult::TogglePanel) => {
                        handle_gesture(crate::gesture::GestureAction::TogglePanel, app);
                        false
                    }
                    Some(crate::gesture::ItemSlotResult::Unchanged) | None => false,
                };
                if !consumed && foreign_keys.insert(key) {
                    gestures.set_foreign_down(true);
                }
            }
        }
        EventType::KeyRelease(key) => {
            if let Some(key) = gesture_key(key) {
                for action in gestures.release(key, now) {
                    handle_gesture(action, app);
                }
            } else if foreign_keys.remove(&key) {
                gestures.set_foreign_down(
                    !foreign_keys.is_empty() || foreign_buttons_down(foreign_buttons),
                );
            }
        }
        EventType::ButtonPress(button) => {
            if foreign_buttons.insert(button) {
                gestures.set_foreign_down(true);
            }
        }
        EventType::ButtonRelease(button) => {
            if foreign_buttons.remove(&button) {
                gestures.set_foreign_down(
                    !foreign_keys.is_empty() || foreign_buttons_down(foreign_buttons),
                );
            }
        }
        EventType::Wheel { .. } => gestures.invalidate(),
        _ => {}
    }

    for action in gestures.poll(now) {
        handle_gesture(action, app);
    }
}

pub fn capture_clipboard(app: &AppHandle) {
    if let Err(error) = crate::license::require(app) {
        let _ = app.emit("license-restricted", error);
        return;
    }
    let mut cb = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(_) => return,
    };
    if let Ok(text) = cb.get_text() {
        let t = text.trim().to_string();
        if !t.is_empty() && t != SENTINEL {
            crate::monitor::capture_text(app, &t);
        }
    }
}

pub fn capture_selection(app: &AppHandle) {
    if let Err(error) = crate::license::require(app) {
        let _ = app.emit("license-restricted", error);
        return;
    }
    // The slot is a deadline, not just a flag. `do_capture` sleeps in 50 ms steps
    // and returns on its own, so a normal capture is done in under a second — but
    // a thread that never returns (a wedged clipboard, a filter driver holding the
    // clipboard open) would hold the slot true for the rest of the session, and
    // every later gesture would keep firing and doing nothing. Observed once in
    // testing: five taps, five `gesture:capture` lines, no outcome, no insert.
    if let Some(since) = CAPTURE_SLOT.held_for(Instant::now()) {
        if since < CAPTURE_SLOT_LIMIT {
            return;
        }
        crate::crash::record_event("capture:previous-capture-stuck");
        let _ = app.emit(
            "capture-failed",
            "The last capture did not finish. Try again in a moment.",
        );
    }
    CAPTURE_SLOT.claim(Instant::now());
    let app = app.clone();
    thread::spawn(move || {
        // The slot is released by a guard rather than by a statement after the
        // call, so a panic inside `do_capture` cannot leave it held.
        let _guard = CapturingGuard;
        do_capture(&app);
    });
}

/// Releases the in-flight selection-capture slot.
struct CapturingGuard;

impl Drop for CapturingGuard {
    fn drop(&mut self) {
        CAPTURE_SLOT.release();
    }
}

/// The single in-flight selection-capture slot.
struct CaptureSlot(Mutex<Option<Instant>>);

impl CaptureSlot {
    fn held_for(&self, now: Instant) -> Option<Duration> {
        let guard = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.map(|since| now.saturating_duration_since(since))
    }

    fn claim(&self, now: Instant) {
        let mut guard = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(now);
    }

    fn release(&self) {
        let mut guard = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
    }
}

/// How long a selection capture may hold the slot before it is assumed stuck.
const CAPTURE_SLOT_LIMIT: Duration = Duration::from_secs(5);

static CAPTURE_SLOT: CaptureSlot = CaptureSlot(Mutex::new(None));

fn do_capture(app: &AppHandle) {
    // Every exit restores the person's real clipboard. The old code returned
    // early on a clipboard-open or sentinel-write failure, leaving the sentinel
    // in the clipboard and the user's actual contents gone — an unrecoverable
    // loss of whatever they had copied, caused by Bard failing at a capture.
    let mut cb = match arboard::Clipboard::new() {
        Ok(clipboard) => clipboard,
        Err(error) => {
            crate::crash::record_event("capture:clipboard-unavailable");
            log::warn!("could not open the clipboard for capture: {error}");
            let _ = app.emit(
                "capture-failed",
                "Bard could not read the clipboard. Another app may be holding it.",
            );
            return;
        }
    };
    let old = snapshot_clipboard(&mut cb);
    if let Err(error) = cb.set_text(SENTINEL.to_string()) {
        crate::crash::record_event("capture:sentinel-write-failed");
        log::warn!("could not place the capture sentinel: {error}");
        // Nothing was overwritten, so the clipboard is already intact; say why
        // nothing happened rather than returning in silence.
        let _ = app.emit(
            "capture-failed",
            "Bard could not reach the clipboard. Try again in a moment.",
        );
        return;
    }
    thread::sleep(Duration::from_millis(60));
    if let Err(error) = send_copy() {
        restore_snapshot(app, &old);
        let _ = app.emit("capture-failed", error);
        return;
    }
    for _ in 0..14 {
        thread::sleep(Duration::from_millis(50));
        if let Ok(text) = cb.get_text() {
            if text != SENTINEL && !text.trim().is_empty() {
                crate::monitor::capture_text(app, &text);
                restore_snapshot(app, &old);
                return;
            }
        } else if let Ok(image) = cb.get_image() {
            if crate::monitor::capture_image(app, image) {
                restore_snapshot(app, &old);
                return;
            }
        }
    }
    restore_snapshot(app, &old);
    // The most common first-run mistake is having nothing selected. The sentinel
    // machinery worked perfectly and this event was emitted with an empty
    // payload that nothing in the frontend listened for, so the gesture appeared
    // to do nothing at all. It now carries a sentence written for a person.
    crate::crash::record_event("capture:no-selection");
    let _ = app.emit(
        "capture-failed",
        "Nothing was selected. Select some text or an image, then try again.",
    );
}

fn send_copy() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if crate::desktop::is_wayland() {
        return crate::linux_portal::send_chord('c');
    }
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let _guard = SyntheticInputGuard::enter();
    let mut enigo = Enigo::new(&Settings::default()).map_err(|_| {
        "Bard could not reach the selection. Copy it first, then capture the clipboard.".to_string()
    })?;
    let _ = enigo.key(Key::Control, Direction::Press);
    let _ = enigo.key(Key::Unicode('c'), Direction::Click);
    let _ = enigo.key(Key::Control, Direction::Release);
    // Let the low-level hook drain Bard's synthetic events before the guard
    // re-enables gesture recognition.
    thread::sleep(Duration::from_millis(40));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    static BURST_TEST: Mutex<()> = Mutex::new(());

    #[test]
    fn overlapping_bursts_can_finish_in_either_order() {
        let _serial = BURST_TEST.lock().unwrap();
        let first = SyntheticInputGuard::enter();
        let second = SyntheticInputGuard::enter();
        drop(first);
        assert!(SYNTHETIC_INPUT.load(Ordering::SeqCst));
        drop(second);
        assert!(!SYNTHETIC_INPUT.load(Ordering::SeqCst));
    }

    #[test]
    fn every_key_bard_synthesises_is_dropped_during_a_burst() {
        // Control, for every copy and paste chord.
        for key in [Key::ControlLeft, Key::ControlRight] {
            assert!(is_synthetic_key(&EventType::KeyPress(key)), "{key:?} press");
            assert!(
                is_synthetic_key(&EventType::KeyRelease(key)),
                "{key:?} release"
            );
        }
        // Alt, for the review sheet's focus steal.
        for key in [Key::Alt, Key::AltGr] {
            assert!(is_synthetic_key(&EventType::KeyPress(key)), "{key:?} press");
            assert!(
                is_synthetic_key(&EventType::KeyRelease(key)),
                "{key:?} release"
            );
        }
    }

    #[test]
    fn a_persons_own_keys_keep_reaching_the_gesture_machine() {
        // The loss this filter must never reintroduce: real typing during a
        // synthetic burst. Shift and ordinary keys are still forwarded.
        for key in [Key::ShiftLeft, Key::Unknown(0x41), Key::Escape, Key::Num1] {
            assert!(!is_synthetic_key(&EventType::KeyPress(key)), "{key:?}");
        }
        assert!(!is_synthetic_key(&EventType::Wheel {
            delta_x: 0,
            delta_y: 1,
        }));
        assert!(!is_synthetic_key(&EventType::ButtonPress(Button::Left)));
    }

    #[test]
    fn a_synthetic_key_is_only_dropped_while_a_burst_is_live() {
        // Outside a burst nothing is filtered at all; inside one, exactly the
        // synthesised keys are, and real typing still gets through.
        let control = EventType::KeyPress(Key::ControlLeft);
        let alt = EventType::KeyRelease(Key::Alt);
        let typed = EventType::KeyPress(Key::Unknown(0x41));
        assert!(!drops_synthetic(false, &control));
        assert!(!drops_synthetic(false, &alt));
        assert!(!drops_synthetic(false, &typed));
        assert!(drops_synthetic(true, &control));
        assert!(drops_synthetic(true, &alt));
        assert!(!drops_synthetic(true, &typed));
    }

    /// The review sheet holds the guard while tao's Alt tap is in flight. The
    /// wait has to end the moment the tap has been seen, and it has to end on
    /// its own when no tap was sent at all — a steal that hangs would leave the
    /// sheet's own presentation unfinished.
    #[test]
    fn waiting_for_a_burst_ends_on_the_keys_or_on_the_bound() {
        let _serial = BURST_TEST.lock().unwrap();
        let _guard = SyntheticInputGuard::enter();
        // Nothing to wait for is answered at once, without spending the bound.
        let started = Instant::now();
        assert!(wait_for_synthetic(0, Duration::from_millis(200)));
        assert!(started.elapsed() < Duration::from_millis(100));

        // A tap that never comes costs the bound and no more.
        let started = Instant::now();
        assert!(!wait_for_synthetic(2, Duration::from_millis(40)));
        assert!(started.elapsed() >= Duration::from_millis(40));

        // The tap itself: counted where the drop happens, and answered from
        // there without waiting out the bound.
        SYNTHETIC_DROPPED.fetch_add(2, Ordering::SeqCst);
        let started = Instant::now();
        assert!(wait_for_synthetic(2, Duration::from_millis(200)));
        assert!(started.elapsed() < Duration::from_millis(100));
    }
}
