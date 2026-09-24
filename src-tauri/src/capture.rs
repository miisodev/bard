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
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};

use crate::gesture::{GestureAction, GestureKey, GestureState};

pub const SENTINEL: &str = "\u{200B}bard::capturing\u{200B}";
/// Chords start unassigned. The defaults are the fixed double-tap gestures —
/// Shift × 2 captures, Ctrl × 2 dictates — which follow the product's modifier
/// convention and take no key combination away from other applications. A
/// chord is for someone who wants one, and they record it themselves.
const DEFAULT_CAPTURE: &str = "";
const DEFAULT_PANEL: &str = "";
/// Written as defaults by keybinds version 2 and never chosen by a person.
/// Version 3 releases them, so Ctrl+Shift+C reaches terminals and DevTools again.
const LEGACY_DEFAULTS: [(&str, &str); 2] =
    [("capture", "Ctrl+Shift+C"), ("panel", "Ctrl+Shift+Space")];
const KEYBINDS_VERSION: &str = "3";
static CAPTURING: AtomicBool = AtomicBool::new(false);
static SHORTCUT_RECORDING: AtomicBool = AtomicBool::new(false);
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
        GestureAction::DictationPress { token } => crate::dictation::gesture_press(app, token),
        GestureAction::DictationRelease { token, held } => {
            crate::dictation::gesture_release(app, token, held)
        }
    }
}

pub fn set_shortcut_recording(active: bool) {
    SHORTCUT_RECORDING.store(active, Ordering::SeqCst);
}

fn shortcut_recording() -> bool {
    SHORTCUT_RECORDING.load(Ordering::SeqCst)
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

    if let Err(error) = register_shortcuts(&app) {
        crate::crash::record_event("input:shortcut-registration-failed");
        log::warn!("global shortcut registration failed: {error}");
    }
}

fn process_raw_event(
    event: Event,
    gestures: &mut GestureState,
    foreign_keys: &mut HashSet<Key>,
    foreign_buttons: &mut HashSet<Button>,
    app: &AppHandle,
) {
    if SYNTHETIC_INPUT.load(Ordering::SeqCst) || shortcut_recording() {
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
            } else if foreign_keys.insert(key) {
                gestures.set_foreign_down(true);
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

fn is_function_key(code: Code) -> bool {
    matches!(
        code,
        Code::F1
            | Code::F2
            | Code::F3
            | Code::F4
            | Code::F5
            | Code::F6
            | Code::F7
            | Code::F8
            | Code::F9
            | Code::F10
            | Code::F11
            | Code::F12
    )
}

fn parse_keybind(value: &str) -> Option<Shortcut> {
    let normalized = value
        .trim()
        .to_lowercase()
        .replace("option", "alt")
        .replace("cmd", "super")
        .replace("win", "super")
        .replace("control", "ctrl");
    let shortcut: Shortcut = normalized.parse().ok()?;
    if shortcut.mods.is_empty() && !is_function_key(shortcut.key) {
        return None;
    }
    Some(shortcut)
}

type KeybindMap = serde_json::Map<String, serde_json::Value>;

fn keybind_map(raw: &str) -> KeybindMap {
    serde_json::from_str(raw).unwrap_or_default()
}

fn default_keybind_map() -> KeybindMap {
    serde_json::json!({
        "capture": DEFAULT_CAPTURE,
        "panel": DEFAULT_PANEL,
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

fn binding_text(map: &KeybindMap, key: &str) -> String {
    map.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

/// An empty binding is unassigned, not invalid.
fn binding(map: &KeybindMap, key: &str, label: &str) -> Result<Option<Shortcut>, String> {
    let value = binding_text(map, key);
    if value.is_empty() {
        return Ok(None);
    }
    parse_keybind(&value)
        .map(Some)
        .ok_or_else(|| format!("{label} needs a valid shortcut."))
}

fn assigned_bindings(map: &KeybindMap) -> Result<(Option<Shortcut>, Option<Shortcut>), String> {
    let capture = binding(map, "capture", "Capture selection")?;
    let panel = binding(map, "panel", "Toggle panel")?;
    if let (Some(capture), Some(panel)) = (&capture, &panel) {
        if capture.id() == panel.id() {
            return Err("Capture and panel cannot use the same shortcut.".into());
        }
    }
    Ok((capture, panel))
}

pub fn normalize_keybinds(raw: &str) -> Result<KeybindMap, String> {
    let map = keybind_map(raw);
    assigned_bindings(&map)?;
    let mut normalized = KeybindMap::new();
    normalized.insert(
        "capture".into(),
        serde_json::json!(binding_text(&map, "capture")),
    );
    normalized.insert(
        "panel".into(),
        serde_json::json!(binding_text(&map, "panel")),
    );
    Ok(normalized)
}

fn normalized_or_default(raw: &str) -> KeybindMap {
    normalize_keybinds(raw).unwrap_or_else(|_| default_keybind_map())
}

fn release_legacy_defaults(raw: &str) -> String {
    let mut map = keybind_map(raw);
    for (key, legacy) in LEGACY_DEFAULTS {
        let is_legacy = map
            .get(key)
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.trim().eq_ignore_ascii_case(legacy));
        if is_legacy {
            map.insert(key.into(), serde_json::json!(""));
        }
    }
    serde_json::Value::Object(map).to_string()
}

pub fn repair_keybinds(app: &AppHandle) {
    let stored = crate::db::get_setting(app, "keybinds").unwrap_or_default();
    let current_version =
        crate::db::get_setting(app, "keybinds_version").as_deref() == Some(KEYBINDS_VERSION);
    let source = if current_version {
        stored.clone()
    } else {
        release_legacy_defaults(&stored)
    };
    let normalized_json = serde_json::Value::Object(normalized_or_default(&source)).to_string();
    if stored != normalized_json {
        let _ = crate::db::set_setting_checked(app, "keybinds", &normalized_json);
    }
    if !current_version {
        let _ = crate::db::set_setting_checked(app, "keybinds_version", KEYBINDS_VERSION);
    }
}

fn register_map(
    manager: &tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>,
    map: &KeybindMap,
) -> Result<(), String> {
    let (capture, panel) = assigned_bindings(map)?;
    if let Some(capture) = capture {
        manager
            .on_shortcut(capture, |app, _shortcut, event| {
                if !shortcut_recording() && event.state() == ShortcutState::Pressed {
                    capture_selection(app);
                }
            })
            .map_err(|error| format!("Could not register capture shortcut: {error}"))?;
    }
    if let Some(panel) = panel {
        manager
            .on_shortcut(panel, |app, _shortcut, event| {
                if !shortcut_recording() && event.state() == ShortcutState::Pressed {
                    crate::panel::toggle_now(app);
                }
            })
            .map_err(|error| format!("Could not register panel shortcut: {error}"))?;
    }
    Ok(())
}

fn shortcuts_in(map: &KeybindMap) -> Vec<Shortcut> {
    ["capture", "panel"]
        .iter()
        .filter_map(|key| {
            map.get(*key)
                .and_then(|value| value.as_str())
                .and_then(parse_keybind)
        })
        .collect()
}

fn unregister_map(
    manager: &tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>,
    map: &KeybindMap,
) -> Result<(), String> {
    let shortcuts = shortcuts_in(map);
    if shortcuts.is_empty() {
        return Ok(());
    }
    manager
        .unregister_multiple(shortcuts)
        .map_err(|error| error.to_string())
}

fn unregister_and_register(app: &AppHandle, map: &KeybindMap) -> Result<(), String> {
    let manager = app.global_shortcut();
    let old = normalized_or_default(&crate::db::get_setting(app, "keybinds").unwrap_or_default());
    unregister_map(manager, &old)?;
    register_map(manager, map)
}

fn restore_registration(app: &AppHandle, raw: &str) {
    let manager = app.global_shortcut();
    let previous = normalized_or_default(raw);
    let _ = unregister_map(
        manager,
        &normalized_or_default(&crate::db::get_setting(app, "keybinds").unwrap_or_default()),
    );
    let _ = register_map(manager, &previous);
}

/// Validate, register, and persist a candidate map as one user-visible action.
/// If registration or the database write fails, the previous map is restored
/// before the error reaches Settings.
pub fn apply_keybinds(app: &AppHandle, json: &str) -> Result<(), String> {
    let candidate = normalize_keybinds(json)?;
    let old_raw = crate::db::get_setting(app, "keybinds").unwrap_or_default();
    let manager = app.global_shortcut();
    let previous = normalized_or_default(&old_raw);
    unregister_map(manager, &previous)?;
    if let Err(error) = register_map(manager, &candidate) {
        restore_registration(app, &old_raw);
        return Err(error);
    }
    let candidate_json = serde_json::Value::Object(candidate).to_string();
    if let Err(error) = crate::db::set_setting_checked(app, "keybinds", &candidate_json) {
        restore_registration(app, &old_raw);
        return Err(error);
    }
    Ok(())
}

pub fn register_shortcuts(app: &AppHandle) -> Result<(), String> {
    let raw = crate::db::get_setting(app, "keybinds").unwrap_or_default();
    let map = normalized_or_default(&raw);
    unregister_and_register(app, &map)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    fn mods(value: &str) -> Modifiers {
        parse_keybind(value)
            .map(|shortcut| shortcut.mods)
            .unwrap_or_default()
    }

    fn code(value: &str) -> Option<Code> {
        parse_keybind(value).map(|shortcut| shortcut.key)
    }

    #[test]
    fn parses_defaults() {
        assert_eq!(code("Ctrl+Shift+C"), Some(Code::KeyC));
        assert!(mods("Ctrl+Shift+C").contains(Modifiers::CONTROL | Modifiers::SHIFT));
        assert_eq!(code("Ctrl+Shift+Space"), Some(Code::Space));
    }

    #[test]
    fn parses_case_and_whitespace() {
        assert_eq!(code("ctrl+shift+c"), Some(Code::KeyC));
        assert_eq!(code("  ctrl   +  space "), Some(Code::Space));
        assert_eq!(code("CTRL+ALT+V"), Some(Code::KeyV));
    }

    #[test]
    fn parses_special_and_function_keys() {
        assert_eq!(code("Ctrl+Escape"), Some(Code::Escape));
        assert_eq!(code("Ctrl+Enter"), Some(Code::Enter));
        assert_eq!(code("Ctrl+ArrowDown"), Some(Code::ArrowDown));
        assert_eq!(code("F1"), Some(Code::F1));
        assert_eq!(code("Ctrl+F5"), Some(Code::F5));
    }

    #[test]
    fn unsafe_or_unknown_bindings_yield_none() {
        assert!(parse_keybind("Ctrl+Banana").is_none());
        assert!(parse_keybind("A").is_none());
        assert!(parse_keybind("Enter").is_none());
        assert!(parse_keybind("").is_none());
        assert!(parse_keybind("+++").is_none());
    }

    #[test]
    fn normal_map_has_no_control_control_sentinel() {
        let map = normalize_keybinds(
            r#"{"capture":"Ctrl+Shift+C","panel":"Ctrl+Shift+Space","dictate":"Ctrl+Ctrl"}"#,
        )
        .expect("map");
        assert!(!map.contains_key("dictate"));
    }

    #[test]
    fn chords_default_to_unassigned() {
        let map = normalize_keybinds("{}").expect("empty map");
        assert_eq!(map.get("capture").and_then(|v| v.as_str()), Some(""));
        assert_eq!(map.get("panel").and_then(|v| v.as_str()), Some(""));
        assert!(shortcuts_in(&map).is_empty());
    }

    #[test]
    fn legacy_defaults_are_released_but_chosen_chords_kept() {
        let released =
            release_legacy_defaults(r#"{"capture":"ctrl+shift+c","panel":"Ctrl+Alt+Space"}"#);
        let map = normalize_keybinds(&released).expect("map");
        assert_eq!(map.get("capture").and_then(|v| v.as_str()), Some(""));
        assert_eq!(
            map.get("panel").and_then(|v| v.as_str()),
            Some("Ctrl+Alt+Space")
        );
    }

    #[test]
    fn duplicate_normal_bindings_are_rejected() {
        let error = normalize_keybinds(r#"{"capture":"Ctrl+Shift+C","panel":"ctrl+shift+c"}"#)
            .expect_err("duplicate");
        assert!(error.contains("same shortcut"));
    }

    #[test]
    fn stress_fuzz_none_panic() {
        let corpus = [
            "Ctrl+Shift+C",
            "ctrl+shift+c",
            "Alt+F4",
            "Shift+F10",
            "Ctrl+Alt+Delete",
            "Escape",
            "esc",
            "Space",
            "Ctrl+Space",
            "Ctrl+Shift+Space",
            "F1",
            "F12",
            "PageDown",
            "ArrowDown",
            "Insert",
            "Home",
            "End",
            "",
            " ",
            "+++",
            "Ctrl+++C",
            "ctrl+super+shift+alt+space",
            "Control+Option+Meta",
            "zzz",
            "#+c",
            "Ctrl + Shift + C",
        ];
        for value in corpus {
            let _ = parse_keybind(value);
        }
    }
}
