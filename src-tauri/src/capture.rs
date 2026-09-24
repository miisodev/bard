use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

use rdev::{Button, Event, EventType, Key};
use tauri::{AppHandle, Emitter};

use crate::gesture::{GestureAction, GestureKey, GestureState};

pub const SENTINEL: &str = "\u{200B}bard::capturing\u{200B}";
static CAPTURING: AtomicBool = AtomicBool::new(false);
static SYNTHETIC_INPUT: AtomicBool = AtomicBool::new(false);
static GESTURE_RESET: AtomicBool = AtomicBool::new(false);

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

pub struct SyntheticInputGuard;

impl SyntheticInputGuard {
    pub fn enter() -> Self {
        SYNTHETIC_INPUT.store(true, Ordering::SeqCst);
        GESTURE_RESET.store(true, Ordering::SeqCst);
        Self
    }
}

impl Drop for SyntheticInputGuard {
    fn drop(&mut self) {
        SYNTHETIC_INPUT.store(false, Ordering::SeqCst);
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
            }
        }
        GestureAction::TogglePanel => crate::panel::toggle_now(app),
        GestureAction::ToggleDraw => crate::commands::draw_gesture(app),
        GestureAction::ToggleSettings => crate::commands::settings_gesture(app),
        GestureAction::PasteItem { slot } => {
            if let Err(error) = crate::commands::paste_item(app, slot) {
                log::warn!("item paste failed: {error}");
            }
        }
        GestureAction::DictationPress { token } => crate::dictation::gesture_press(app, token),
        GestureAction::DictationRelease { token, held } => {
            crate::dictation::gesture_release(app, token, held)
        }
    }
}

/// Start the single raw-input coordinator. The rdev hook only forwards events
/// to a normal worker; all timing, state, and application actions happen off
/// the low-level callback thread.
pub fn start(app: AppHandle) {
    let (sender, receiver) = mpsc::channel::<Event>();
    let hook_sender = sender.clone();
    thread::spawn(move || {
        if let Err(error) = rdev::listen(move |event| {
            // Bard's own Ctrl+C/Ctrl+V synthesis must never be interpreted as
            // a physical Ctrl tap. The worker also checks this flag in case an
            // event is already queued behind the hook.
            if !SYNTHETIC_INPUT.load(Ordering::SeqCst) {
                let _ = hook_sender.send(event);
            }
        }) {
            crate::crash::record_event("input:listener-failed");
            log::warn!("rdev listen failed: {error:?}");
        }
    });

    let worker_app = app.clone();
    thread::spawn(move || {
        let mut gestures = GestureState::new();
        let mut foreign_keys: HashSet<Key> = HashSet::new();
        let mut foreign_buttons: HashSet<Button> = HashSet::new();

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
                    for action in gestures.poll(Instant::now()) {
                        handle_gesture(action, &worker_app);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
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
    if SYNTHETIC_INPUT.load(Ordering::SeqCst) {
        gestures.reset();
        foreign_keys.clear();
        foreign_buttons.clear();
        return;
    }

    let now = Instant::now();
    match event.event_type {
        EventType::KeyPress(key) => {
            if key == Key::Escape && crate::dictation::is_recording() {
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
    if CAPTURING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    thread::spawn(move || {
        do_capture(&app);
        CAPTURING.store(false, Ordering::SeqCst);
    });
}

fn do_capture(app: &AppHandle) {
    let mut cb = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(_) => return,
    };
    let old = snapshot_clipboard(&mut cb);
    if cb.set_text(SENTINEL.to_string()).is_err() {
        return;
    }
    thread::sleep(Duration::from_millis(60));
    send_copy();
    for _ in 0..14 {
        thread::sleep(Duration::from_millis(50));
        if let Ok(text) = cb.get_text() {
            if text != SENTINEL && !text.trim().is_empty() {
                crate::monitor::capture_text(app, &text);
                restore_snapshot(app, &old);
                return;
            }
        } else if let Ok(image) = cb.get_image() {
            if crate::monitor::capture_image(app, &image) {
                restore_snapshot(app, &old);
                return;
            }
        }
    }
    restore_snapshot(app, &old);
    let _ = app.emit("capture-failed", ());
}

fn send_copy() {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let _guard = SyntheticInputGuard::enter();
    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        return;
    };
    let _ = enigo.key(Key::Control, Direction::Press);
    let _ = enigo.key(Key::Unicode('c'), Direction::Click);
    let _ = enigo.key(Key::Control, Direction::Release);
    // Let the low-level hook drain Bard's synthetic events before the guard
    // re-enables gesture recognition.
    thread::sleep(Duration::from_millis(40));
}
