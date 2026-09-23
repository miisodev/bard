mod audio;
mod model;
mod overlay;
mod vad;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub use model::{
    cancel_download, delete_model, download_model, model_ready, model_status, models, ModelInfo,
    ModelStatus,
};

pub fn reset_cancel() {
    cancel_download_reset();
}

pub fn is_cancel_pending() -> bool {
    cancel_pending()
}

use model::{cancel_download_reset, cancel_pending};

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DictState {
    Idle,
    Recording,
    Transcribing,
}

static STATE: Mutex<DictState> = Mutex::new(DictState::Idle);
static OWNER: Mutex<Option<u64>> = Mutex::new(None);
static OPERATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static OPERATION_CANCELLED: AtomicBool = AtomicBool::new(false);

fn begin_operation() -> u64 {
    OPERATION_CANCELLED.store(false, Ordering::SeqCst);
    OPERATION_SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1
}

fn operation_cancelled(operation: u64) -> bool {
    OPERATION_CANCELLED.load(Ordering::SeqCst)
        || OPERATION_SEQUENCE.load(Ordering::SeqCst) != operation
}

fn cancel_operation() {
    OPERATION_CANCELLED.store(true, Ordering::SeqCst);
}

fn state_lock() -> MutexGuard<'static, DictState> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn owner_lock() -> MutexGuard<'static, Option<u64>> {
    OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn clear_owner() {
    *owner_lock() = None;
}

pub fn init(app: AppHandle) {
    model::start_unload_watcher(app);
}

pub fn set_overlay_position(app: &AppHandle, position: &str) -> Result<(), String> {
    overlay::save_position(app, position)
}

pub fn mic_ready() -> bool {
    audio::mic_ready()
}

pub fn list_input_devices() -> Vec<String> {
    audio::list_input_devices()
}

/// Start a take owned by one Ctrl+Ctrl gesture. The token prevents a late raw
/// release from stopping a take that was started by the tray, a command, or a
/// later gesture.
pub fn gesture_press(app: &AppHandle, token: u64) {
    let action = {
        let state = state_lock();
        match *state {
            DictState::Idle => {
                *owner_lock() = Some(token);
                Some(true)
            }
            DictState::Recording => Some(false),
            DictState::Transcribing => None,
        }
    };
    match action {
        Some(true) => {
            start(app);
        }
        Some(false) => stop(app),
        None => {}
    }
}

pub fn gesture_release(app: &AppHandle, token: u64, held_for: Duration) {
    let owns_take = *owner_lock() == Some(token);
    if !owns_take {
        return;
    }
    if held_for >= crate::gesture::DICTATION_HOLD_LIMIT {
        stop(app);
    } else {
        // A short tap leaves the recording locked. A later complete Ctrl+Ctrl
        // gesture will stop it; a stale release cannot do so accidentally.
        clear_owner();
    }
}

pub fn start(app: &AppHandle) -> bool {
    {
        let mut state = state_lock();
        if *state != DictState::Idle {
            return false;
        }
        if !model_ready(app) {
            clear_owner();
            let _ = app.emit(
                "dictation-error",
                "Voice model not downloaded yet. Open Settings to install it.",
            );
            return false;
        }
        if !audio::mic_ready() {
            clear_owner();
            let _ = app.emit("dictation-error", "No microphone found.");
            return false;
        }
        *state = DictState::Recording;
    }
    crate::crash::record_event("dictation:start-requested");

    overlay::show(app, "listening");
    if let Err(error) = audio::start(app.clone()) {
        crate::crash::record_event("dictation:audio-start-failed");
        overlay::hide(app);
        *state_lock() = DictState::Idle;
        clear_owner();
        let _ = app.emit("dictation-error", error);
        return false;
    }
    true
}

async fn wait_for_operation_cancel(operation: u64) {
    while !operation_cancelled(operation) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn bounded_postprocess(
    local: &str,
    config: &crate::cloud::Config,
) -> crate::cloud::PostprocessResult<String> {
    match tokio::time::timeout(
        Duration::from_secs(20),
        crate::cloud::postprocess(local, config, Duration::from_secs(15)),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(crate::cloud::Error::Provider(
            crate::cloud::ProviderError::Transport {
                kind: crate::cloud::TransportKind::Timeout,
            },
        )),
    }
}

fn postprocess_transcript(app: &AppHandle, local: String, operation: u64) -> String {
    let config = crate::cloud::Config::from_setting_values(
        crate::db::get_setting(app, crate::cloud::ENABLED_SETTING_KEY),
        crate::db::get_setting(app, crate::cloud::MODEL_IDS_SETTING_KEY),
    );
    if !config.enabled || operation_cancelled(operation) {
        return local;
    }

    overlay::show(app, "finishing");
    let result: Option<crate::cloud::PostprocessResult<String>> =
        match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime.block_on(async {
                tokio::select! {
                    result = bounded_postprocess(&local, &config) => Some(result),
                    _ = wait_for_operation_cancel(operation) => None,
                }
            }),
            Err(_) => Some(Err(crate::cloud::Error::Provider(
                crate::cloud::ProviderError::Transport {
                    kind: crate::cloud::TransportKind::Request,
                },
            ))),
        };
    let Some(result) = result else {
        return local;
    };
    if operation_cancelled(operation) {
        return local;
    }
    match result {
        Ok(cleaned) => cleaned,
        Err(error) => {
            let _ = app.emit("dictation-cloud-warning", error.to_string());
            local
        }
    }
}

pub fn stop(app: &AppHandle) {
    let mut state = state_lock();
    if *state != DictState::Recording {
        return;
    }
    *state = DictState::Transcribing;
    clear_owner();
    crate::crash::record_event("dictation:stop-requested");
    let operation = begin_operation();
    let app = app.clone();
    std::thread::spawn(move || {
        let recording = audio::stop();
        if let Some(error) = recording.error {
            overlay::hide(&app);
            *state_lock() = DictState::Idle;
            let _ = app.emit("dictation-error", error);
            return;
        }
        let mut samples = recording.samples;
        if samples.is_empty() || !recording.speech_detected {
            overlay::hide(&app);
            *state_lock() = DictState::Idle;
            let _ = app.emit("dictation-error", "Nothing heard.");
            return;
        }
        if samples.len() < 16000 {
            samples.resize(20000, 0.0);
        }
        if operation_cancelled(operation) {
            overlay::hide(&app);
            *state_lock() = DictState::Idle;
            let _ = app.emit("dictation-cancelled", ());
            return;
        }
        overlay::show(&app, "transcribing");
        let outcome = model::transcribe(&app, &samples).map(clean_text);
        if operation_cancelled(operation) {
            overlay::hide(&app);
            *state_lock() = DictState::Idle;
            let _ = app.emit("dictation-cancelled", ());
            return;
        }
        match outcome {
            Ok(text) if !text.is_empty() => {
                let text = postprocess_transcript(&app, text, operation);
                if operation_cancelled(operation) {
                    overlay::hide(&app);
                    *state_lock() = DictState::Idle;
                    let _ = app.emit("dictation-cancelled", ());
                    return;
                }
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
                    paste_chord(&app);
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
        *state_lock() = DictState::Idle;
    });
}

pub fn cancel(app: &AppHandle) {
    let state = *state_lock();
    if state == DictState::Recording {
        clear_owner();
        audio::cancel();
        cancel_operation();
        overlay::hide(app);
        *state_lock() = DictState::Idle;
        let _ = app.emit("dictation-cancelled", ());
    } else if state == DictState::Transcribing {
        model::cancel_transcription();
        cancel_operation();
        overlay::show(app, "cancelling");
        let _ = app.emit("dictation-cancel-requested", ());
    }
}

pub fn is_recording() -> bool {
    *state_lock() == DictState::Recording
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
    for f in [
        "um, ", "um ", "uh, ", "uh ", "uhm, ", "uhm ", "hmm, ", "hmm ", "er, ", "er ", "ahem, ",
    ] {
        if t.to_lowercase().starts_with(f) {
            return t[f.len()..].trim_start().to_string();
        }
    }
    t.to_string()
}

fn paste_chord(app: &AppHandle) {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let _synthetic_input = crate::capture::SyntheticInputGuard::enter();
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
        crate::monitor::note_own_write(app);
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(old);
        }
    }
}
