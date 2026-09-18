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
            "enter" | "return" => code = Some(Code::Enter),
            "escape" | "esc" => code = Some(Code::Escape),
            _ => {}
        }
    }
    code.map(|c| Shortcut::new(Some(modifiers), c))
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