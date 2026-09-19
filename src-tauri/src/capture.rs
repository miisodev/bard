use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use rdev::{Event, EventType, Key};
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub const SENTINEL: &str = "\u{200B}bard::capturing\u{200B}";
const TAP_WINDOW: Duration = Duration::from_millis(400);
const HOLD_LIMIT: Duration = Duration::from_millis(500);
static CAPTURING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Left,
    Right,
}

pub fn start(app: AppHandle) {
    let thread_app = app.clone();
    thread::spawn(move || {
        let mut pressed: Option<(Side, Instant)> = None;
        let mut dirty = false;
        let mut last_tap: Option<(Side, Instant)> = None;
        let callback = move |event: Event| match event.event_type {
            EventType::KeyPress(Key::ShiftLeft) => {
                if pressed.is_none() {
                    pressed = Some((Side::Left, Instant::now()));
                    dirty = false;
                }
            }
            EventType::KeyPress(Key::ShiftRight) => {
                if pressed.is_none() {
                    pressed = Some((Side::Right, Instant::now()));
                    dirty = false;
                }
            }
            EventType::KeyPress(_) => {
                dirty = true;
                last_tap = None;
            }
            EventType::KeyRelease(key) => {
                let side = match key {
                    Key::ShiftLeft => Some(Side::Left),
                    Key::ShiftRight => Some(Side::Right),
                    _ => None,
                };
                if let Some(side) = side {
                    if let Some((pressed_side, at)) = pressed {
                        pressed = None;
                        if pressed_side == side && !dirty && at.elapsed() < HOLD_LIMIT {
                            if let Some((last_side, last_at)) = last_tap {
                                if last_side == side && at.duration_since(last_at) < TAP_WINDOW {
                                    last_tap = None;
                                    match side {
                                        Side::Left => capture_selection(&thread_app),
                                        Side::Right => crate::panel::toggle_now(&thread_app),
                                    }
                                    return;
                                }
                            }
                            last_tap = Some((side, at));
                        } else {
                            last_tap = None;
                        }
                    }
                }
            }
            _ => {}
        };
        if let Err(e) = rdev::listen(callback) {
            log::warn!("rdev listen failed: {e:?}");
        }
    });
    register_shortcuts(&app);
}

fn parse_keybind(s: &str) -> Option<Shortcut> {
    let mut modifiers = Modifiers::empty();
    let mut code = None;
    for part in s.to_lowercase().split('+') {
        match part.trim() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            "super" | "cmd" | "meta" | "win" => modifiers |= Modifiers::SUPER,
            "space" => code = Some(Code::Space),
            "c" => code = Some(Code::KeyC),
            "v" => code = Some(Code::KeyV),
            "a" => code = Some(Code::KeyA),
            "b" => code = Some(Code::KeyB),
            "enter" | "return" => code = Some(Code::Enter),
            "escape" | "esc" => code = Some(Code::Escape),
            "tab" => code = Some(Code::Tab),
            "backspace" => code = Some(Code::Backspace),
            "delete" | "del" => code = Some(Code::Delete),
            "insert" => code = Some(Code::Insert),
            "home" => code = Some(Code::Home),
            "end" => code = Some(Code::End),
            "pageup" => code = Some(Code::PageUp),
            "pagedown" => code = Some(Code::PageDown),
            "up" => code = Some(Code::ArrowUp),
            "down" => code = Some(Code::ArrowDown),
            "left" => code = Some(Code::ArrowLeft),
            "right" => code = Some(Code::ArrowRight),
            "f1" => code = Some(Code::F1),
            "f2" => code = Some(Code::F2),
            "f3" => code = Some(Code::F3),
            "f4" => code = Some(Code::F4),
            "f5" => code = Some(Code::F5),
            "f6" => code = Some(Code::F6),
            "f7" => code = Some(Code::F7),
            "f8" => code = Some(Code::F8),
            "f9" => code = Some(Code::F9),
            "f10" => code = Some(Code::F10),
            "f11" => code = Some(Code::F11),
            "f12" => code = Some(Code::F12),
            _ => {}
        }
    }
    code.map(|c| Shortcut::new(Some(modifiers), c))
}

pub fn repair_keybinds(app: &AppHandle) {
    const DEFAULTS: [(&str, &str); 3] = [
        ("capture", "Ctrl+Shift+C"),
        ("panel", "Ctrl+Shift+Space"),
        ("dictate", "Ctrl+Space"),
    ];
    let raw = crate::db::get_setting(app, "keybinds").unwrap_or_default();
    let mut map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&raw).unwrap_or_default();
    let mut changed = raw.is_empty() || raw == "{}";
    for (key, fallback) in DEFAULTS {
        let valid = map
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| parse_keybind(s).is_some())
            .unwrap_or(false);
        if !valid {
            map.insert(key.to_string(), serde_json::json!(fallback));
            changed = true;
        }
    }
    if changed {
        crate::db::set_setting(app, "keybinds", &serde_json::Value::Object(map).to_string());
    }
}

pub fn register_shortcuts(app: &AppHandle) {
    let manager = app.global_shortcut();
    let _ = manager.unregister_all();

    let keybinds: serde_json::Value =
        serde_json::from_str(&crate::db::get_setting(app, "keybinds").unwrap_or_default())
            .unwrap_or_else(|_| serde_json::json!({}));
    let get = |k: &str, d: &str| -> Option<Shortcut> {
        keybinds
            .get(k)
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| Some(d.to_string()))
            .and_then(|s| parse_keybind(&s))
    };

    let capture = get("capture", "Ctrl+Shift+C");
    let panel = get("panel", "Ctrl+Shift+Space");
    let dictate = get("dictate", "Ctrl+Space");

    if let Some(s) = capture {
        let _ = manager.on_shortcut(s, |app, _s, event| {
            if event.state() == ShortcutState::Pressed {
                capture_selection(app);
            }
        });
    }
    if let Some(s) = panel {
        let _ = manager.on_shortcut(s, |app, _s, event| {
            if event.state() == ShortcutState::Pressed {
                crate::panel::toggle_now(app);
            }
        });
    }
    if let Some(s) = dictate {
        let _ = manager.on_shortcut(s, |app, _s, event| {
            if event.state() == ShortcutState::Pressed {
                crate::dictation::toggle(app);
            } else {
                crate::dictation::release(app);
            }
        });
    }
}

pub fn capture_clipboard(app: &AppHandle) {
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
    let old = cb.get_text().ok();
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
                return;
            }
        }
    }
    restore(app, old);
}

fn restore(app: &AppHandle, old: Option<String>) {
    let mut cb = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(_) => return,
    };
    match old {
        Some(text) => {
            let _ = cb.set_text(text);
        }
        None => {
            let _ = cb.clear();
        }
    }
    let _ = app.emit("capture-failed", ());
}

fn send_copy() {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        return;
    };
    let _ = enigo.key(Key::Control, Direction::Press);
    let _ = enigo.key(Key::Unicode('c'), Direction::Click);
    let _ = enigo.key(Key::Control, Direction::Release);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::Code;

    fn mods(s: &str) -> Modifiers {
        parse_keybind(s).map(|sh| sh.mods).unwrap_or(Modifiers::empty())
    }

    fn code(s: &str) -> Option<Code> {
        parse_keybind(s).map(|sh| sh.key)
    }

    #[test]
    fn parses_defaults() {
        assert_eq!(code("Ctrl+Shift+C"), Some(Code::KeyC));
        assert!(mods("Ctrl+Shift+C").contains(Modifiers::CONTROL | Modifiers::SHIFT));
        assert_eq!(code("Ctrl+Shift+Space"), Some(Code::Space));
        assert_eq!(code("Ctrl+Space"), Some(Code::Space));
        assert!(mods("Ctrl+Space").contains(Modifiers::CONTROL));
        assert!(!mods("Ctrl+Space").contains(Modifiers::SHIFT));
    }

    #[test]
    fn parses_case_and_whitespace() {
        assert_eq!(code("ctrl+shift+c"), Some(Code::KeyC));
        assert_eq!(code("  ctrl   +  space "), Some(Code::Space));
        assert_eq!(code("CTRL+ALT+V"), Some(Code::KeyV));
    }

    #[test]
    fn parses_special_keys() {
        assert_eq!(code("Escape"), Some(Code::Escape));
        assert_eq!(code("esc"), Some(Code::Escape));
        assert_eq!(code("Enter"), Some(Code::Enter));
        assert_eq!(code("Tab"), Some(Code::Tab));
        assert_eq!(code("Backspace"), Some(Code::Backspace));
        assert_eq!(code("Delete"), Some(Code::Delete));
        assert_eq!(code("PageUp"), Some(Code::PageUp));
        assert_eq!(code("Home"), Some(Code::Home));
    }

    #[test]
    fn parses_function_keys() {
        assert_eq!(code("F1"), Some(Code::F1));
        assert_eq!(code("Ctrl+F5"), Some(Code::F5));
        assert_eq!(code("f12"), Some(Code::F12));
    }

    #[test]
    fn modifier_combos() {
        assert!(mods("Ctrl+Alt+Shift+A").contains(
            Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT
        ));
        assert!(mods("Alt+Space").contains(Modifiers::ALT));
        assert!(mods("Super+Space").contains(Modifiers::SUPER));
        assert_eq!(code("Super+Space"), Some(Code::Space));
    }

    #[test]
    fn unknown_tokens_yield_none() {
        assert!(parse_keybind("Ctrl+Banana").is_none());
        assert!(parse_keybind("Banana").is_none());
        assert!(parse_keybind("").is_none());
        assert!(parse_keybind("+++").is_none());
    }

    #[test]
    fn stress_fuzz_none_panic() {
        let corpus = [
            "Ctrl+Shift+C", "ctrl+shift+c", "Alt+F4", "Shift+F10", "Ctrl+Alt+Delete",
            "Escape", "esc", "Space", "Ctrl+Space", "Ctrl+Shift+Space", "F1", "F12",
            "PageDown", "ArrowDown", "Insert", "Home", "End", "", " ", "+++", "Ctrl+++C",
            "ctrl+super+shift+alt+space", "Control+Option+Meta", "zzz", "#+c", "Ctrl + Shift + C",
        ];
        for s in corpus {
            let _ = parse_keybind(s);
        }
    }
}