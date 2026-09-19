mod audio;
mod model;
mod overlay;
mod vad;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub use model::{
    ModelInfo, ModelStatus, cancel_download, delete_model, download_model, model_ready,
    model_status, models,
};

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DictState {
    Idle,
    Recording,
    Transcribing,
}

static STATE: Mutex<DictState> = Mutex::new(DictState::Idle);
static PRESSED_AT: Mutex<Option<std::time::Instant>> = Mutex::new(None);
static ESC_SHORTCUT: Mutex<Option<tauri_plugin_global_shortcut::Shortcut>> = Mutex::new(None);

pub fn init(app: AppHandle) {
    model::start_unload_watcher(app);
}

pub fn mic_ready() -> bool {
    audio::mic_ready()
}

pub fn list_input_devices() -> Vec<String> {
    audio::list_input_devices()
}

pub fn toggle(app: &AppHandle) {
    match *STATE.lock().unwrap() {
        DictState::Idle => {
            *PRESSED_AT.lock().unwrap() = Some(std::time::Instant::now());
            start(app);
        }
        DictState::Recording => stop(app),
        DictState::Transcribing => {}
    }
}

pub fn release(app: &AppHandle) {
    let held = PRESSED_AT
        .lock()
        .unwrap()
        .map(|t| t.elapsed().as_millis() > 300)
        .unwrap_or(true);
    if held {
        stop(app);
    }
    *PRESSED_AT.lock().unwrap() = None;
}

fn register_esc_cancel(app: &AppHandle) {
    use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
    let mgr = app.global_shortcut();
    let shortcut = Shortcut::new(Some(Modifiers::empty()), Code::Escape);
    let _ = mgr.on_shortcut(shortcut, |app, _s, _e| {
        cancel(app);
    });
    *ESC_SHORTCUT.lock().unwrap() = Some(shortcut);
}

fn unregister_esc_cancel(app: &AppHandle) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    if let Some(s) = ESC_SHORTCUT.lock().unwrap().take() {
        let _ = app.global_shortcut().unregister(s);
    }
}

pub fn start(app: &AppHandle) {
    let mut state = STATE.lock().unwrap();
    if *state != DictState::Idle {
        return;
    }
    if !model_ready(app) {
        let _ = app.emit(
            "dictation-error",
            "Voice model not downloaded yet. Open Settings to install it.",
        );
        return;
    }
    if !audio::mic_ready() {
        let _ = app.emit("dictation-error", "No microphone found.");
        return;
    }
    *state = DictState::Recording;
    overlay::show(app, "listening");
    register_esc_cancel(app);
    audio::start(app.clone());
}

pub fn stop(app: &AppHandle) {
    let mut state = STATE.lock().unwrap();
    if *state != DictState::Recording {
        return;
    }
    *state = DictState::Transcribing;
    unregister_esc_cancel(app);
    let app = app.clone();
    std::thread::spawn(move || {
        let mut samples = audio::stop();
        if !samples.is_empty() && samples.len() < 16000 {
            samples.resize(20000, 0.0);
        }
        overlay::show(&app, "transcribing");
        let outcome = model::transcribe(&app, &samples).map(clean_text);
        match outcome {
            Ok(text) if !text.is_empty() => {
                let panel_focused = match app.get_webview_window("main") {
                    Some(win) => {
                        win.is_visible().unwrap_or(false) && win.is_focused().unwrap_or(false)
                    }
                    None => false,
                };
                if panel_focused {
                    let _ = crate::paste::set_text(&app, &text);
                    crate::monitor::capture_text(&app, &text);
                    let _ = app.emit("dictation-panel", text);
                } else {
                    let _ = crate::paste::set_text(&app, &text);
                    paste_chord();
                    let _ = app.emit("dictation-pasted", ());
                }
            }
            Ok(_) => {
                let _ = app.emit("dictation-error", "Nothing heard.");
            }
            Err(e) => {
                let _ = app.emit("dictation-error", e);
            }
        }
        overlay::hide(&app);
        *STATE.lock().unwrap() = DictState::Idle;
    });
}

pub fn cancel(app: &AppHandle) {
    if *STATE.lock().unwrap() == DictState::Recording {
        unregister_esc_cancel(app);
        audio::cancel();
        overlay::hide(app);
        *STATE.lock().unwrap() = DictState::Idle;
        let _ = app.emit("dictation-cancelled", ());
    }
}

pub fn is_recording() -> bool {
    *STATE.lock().unwrap() == DictState::Recording
}

fn clean_text(raw: String) -> String {
    let mut t = raw.trim().to_string();
    t = strip_fillers(&t);
    let chars = t.chars().collect::<Vec<_>>();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut run: u32 = 0;
    let mut prev: Option<char> = None;
    for c in chars {
        if Some(c) == prev {
            run += 1;
            if run >= 3 {
                continue;
            }
        } else {
            run = 0;
        }
        out.push(c);
        prev = Some(c);
    }
    let mut res: String = out.into_iter().collect();
    while res.ends_with(' ') || res.ends_with('\n') {
        res.pop();
    }
    if !res.is_empty() {
        res.push(' ');
    }
    res
}

fn strip_fillers(t: &str) -> String {
    for f in ["um, ", "um ", "uh, ", "uh ", "uhm, ", "uhm ", "hmm, ", "hmm ", "er, ", "er ", "ahem, "] {
        if t.to_lowercase().starts_with(f) {
            return t[f.len()..].trim_start().to_string();
        }
    }
    t.to_string()
}

fn paste_chord() {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let snapshot = arboard::Clipboard::new()
        .ok()
        .and_then(|mut c| c.get_text().ok());
    std::thread::sleep(std::time::Duration::from_millis(60));
    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        return;
    };
    let _ = enigo.key(Key::Control, Direction::Press);
    std::thread::sleep(std::time::Duration::from_millis(30));
    let _ = enigo.key(Key::Unicode('v'), Direction::Click);
    std::thread::sleep(std::time::Duration::from_millis(30));
    let _ = enigo.key(Key::Control, Direction::Release);
    std::thread::sleep(std::time::Duration::from_millis(60));
    if let Some(old) = snapshot {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(old);
        }
    }
}