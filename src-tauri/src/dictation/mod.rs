mod audio;
mod model;
mod overlay;
mod vad;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub use model::{ModelInfo, ModelStatus, delete_model, download_model, model_ready, model_status, models};

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DictState {
    Idle,
    Recording,
    Transcribing,
}

static STATE: Mutex<DictState> = Mutex::new(DictState::Idle);
static PRESSED_AT: Mutex<Option<std::time::Instant>> = Mutex::new(None);

pub fn init(app: AppHandle) {
    let _ = app;
}

pub fn mic_ready() -> bool {
    audio::mic_ready()
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
    audio::start(app.clone());
}

pub fn stop(app: &AppHandle) {
    let mut state = STATE.lock().unwrap();
    if *state != DictState::Recording {
        return;
    }
    *state = DictState::Transcribing;
    let app = app.clone();
    std::thread::spawn(move || {
        let samples = audio::stop();
        overlay::show(&app, "transcribing");
        match model::transcribe(&app, &samples) {
            Ok(text) if !text.trim().is_empty() => {
                let panel_focused = match app.get_webview_window("main") {
                    Some(win) => {
                        win.is_visible().unwrap_or(false) && win.is_focused().unwrap_or(false)
                    }
                    None => false,
                };
                if panel_focused {
                    let _ = crate::paste::set_text(&app, &text);
                    let _ = app.emit("dictation-result", text);
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

fn paste_chord() {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    std::thread::sleep(std::time::Duration::from_millis(60));
    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        return;
    };
    let _ = enigo.key(Key::Control, Direction::Press);
    std::thread::sleep(std::time::Duration::from_millis(30));
    let _ = enigo.key(Key::Unicode('v'), Direction::Click);
    std::thread::sleep(std::time::Duration::from_millis(30));
    let _ = enigo.key(Key::Control, Direction::Release);
}

pub fn cancel(app: &AppHandle) {
    audio::cancel();
    overlay::hide(app);
    *STATE.lock().unwrap() = DictState::Idle;
}

pub fn is_recording() -> bool {
    *STATE.lock().unwrap() == DictState::Recording
}