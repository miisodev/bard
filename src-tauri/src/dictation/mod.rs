mod audio;
mod model;
mod overlay;
mod vad;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

const POSTPROCESS_REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const POSTPROCESS_DEADLINE: Duration = Duration::from_secs(60);

/// The floor on how long a take may run before the watchdog calls it a wedge.
///
/// Refine is separately bounded at 60 s, so this has to be comfortably more than
/// that or the watchdog would preempt a slow but working refinement. What it
/// also has to cover, on the first take after an install, is a SHA-256 over a
/// 1.58 GB model file and then loading it — cold, from a spinning disk, on a
/// slow machine, that is tens of seconds. 210 s leaves roughly two and a half
/// times that worst case for everything, and is still a real bound: a five-second
/// take that has not produced text in three and a half minutes is not slow, it
/// is stuck.
const TAKE_DEADLINE_FLOOR: Duration = Duration::from_secs(210);

/// What the audio in a take is itself allowed to cost, in seconds of wall clock
/// per second of audio.
///
/// Linear, because transcription cost is linear in audio length. The factor is
/// deliberately far above the worst case that exists: the largest model Bard
/// ships is 1.7 B parameters at Q6_K, and a 20-minute take on a 4-core laptop
/// with AVX2 — where per-ISA dispatch gives the best answer — takes four to
/// seven minutes, about 2.5x realtime. Three times realtime leaves no headroom
/// over that, which is the point: a ceiling that fires on a slow machine throws
/// away a transcript the user just spent twenty minutes speaking, and that is
/// strictly worse than a wait.
const TAKE_REALTIME_FACTOR: u64 = 3;

/// How often the watchdog looks. Cheap, and it only exists to notice a wedge.
const TAKE_WATCHDOG_TICK: Duration = Duration::from_millis(250);

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
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
/// The newest operation the watchdog gave up on, or 0 for none.
///
/// A worker whose operation is at or before this has already had its failure
/// reported and its state reset, so it must not touch the overlay or the state
/// machine when it finally unwinds: a newer take may own them by then. Only ever
/// increases, so a worker never has to be told it was superseded.
static TIMED_OUT_OPERATION: AtomicU64 = AtomicU64::new(0);

fn begin_operation() -> u64 {
    OPERATION_CANCELLED.store(false, Ordering::SeqCst);
    OPERATION_SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1
}

fn operation_cancelled(operation: u64) -> bool {
    OPERATION_CANCELLED.load(Ordering::SeqCst)
        || OPERATION_SEQUENCE.load(Ordering::SeqCst) != operation
}

fn operation_reported(operation: u64) -> bool {
    TIMED_OUT_OPERATION.load(Ordering::SeqCst) >= operation
}

fn cancel_operation() {
    OPERATION_CANCELLED.store(true, Ordering::SeqCst);
}

/// Lock order is always STATE then OWNER, in every function that takes both. The
/// two are never held the other way round, so they cannot deadlock.
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
    overlay::start_guard(&app);
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

/// What one half of a Ctrl+Ctrl gesture does to the state machine.
///
/// These are the rules that stop a stale raw release from cancelling a take that
/// belongs to somebody else, and they used to live inline in two functions that
/// needed an `AppHandle`, a licence, a microphone and a downloaded model to
/// reach — which is to say they were untestable, and they are exactly the kind of
/// thing that regresses silently.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GestureOutcome {
    /// Claim the take for this token and start recording.
    Start,
    /// End the take that is recording.
    Stop,
    /// Leave the take recording but give up ownership, so a later release of
    /// this same gesture cannot end it. A short tap locks the recording on.
    Disown,
    /// Touch nothing at all.
    Ignore,
}

/// A press is deliberately not conditional on who owns the take. Ctrl+Ctrl has
/// to be able to stop a take the tray started, which no gesture owns, and a
/// second Control is how a held take ends. The release is the half that can
/// arrive late and stale, so that is the half that checks the token.
fn gesture_press_outcome(state: DictState) -> GestureOutcome {
    match state {
        DictState::Idle => GestureOutcome::Start,
        DictState::Recording => GestureOutcome::Stop,
        DictState::Transcribing => GestureOutcome::Ignore,
    }
}

fn gesture_release_outcome(
    state: DictState,
    owner: Option<u64>,
    token: u64,
    held_for: Duration,
) -> GestureOutcome {
    // The ownership check comes first and is unconditional. A raw key release
    // from an earlier gesture, or from a gesture that never owned anything, must
    // not be able to stop a take that a later gesture or the tray is running.
    if owner != Some(token) {
        return GestureOutcome::Ignore;
    }
    match state {
        // Nothing is running. `stop` has already cleared the owner by the time
        // it reaches Transcribing, so these two are unreachable in practice; they
        // are handled rather than assumed.
        DictState::Idle | DictState::Transcribing => GestureOutcome::Ignore,
        DictState::Recording => {
            if held_for >= crate::gesture::DICTATION_HOLD_LIMIT {
                GestureOutcome::Stop
            } else {
                // A short tap leaves the recording locked. A later complete
                // Ctrl+Ctrl gesture will stop it; a stale release cannot.
                GestureOutcome::Disown
            }
        }
    }
}

/// Start a take owned by one Ctrl+Ctrl gesture. The token prevents a late raw
/// release from stopping a take that was started by the tray, a command, or a
/// later gesture.
pub fn gesture_press(app: &AppHandle, token: u64) {
    let outcome = {
        // The decision and the claim are one critical section: a second press
        // arriving at the same moment must see the take as already claimed, not
        // as Idle and up for grabs.
        let state = state_lock();
        let outcome = gesture_press_outcome(*state);
        if outcome == GestureOutcome::Start {
            *owner_lock() = Some(token);
        }
        outcome
    };
    match outcome {
        GestureOutcome::Start => {
            start(app);
        }
        GestureOutcome::Stop => stop(app),
        GestureOutcome::Disown | GestureOutcome::Ignore => {}
    }
}

pub fn gesture_release(app: &AppHandle, token: u64, held_for: Duration) {
    let outcome = {
        let state = *state_lock();
        let owner = *owner_lock();
        gesture_release_outcome(state, owner, token, held_for)
    };
    match outcome {
        GestureOutcome::Stop => stop(app),
        GestureOutcome::Disown => clear_owner(),
        GestureOutcome::Start | GestureOutcome::Ignore => {}
    }
}

pub fn start(app: &AppHandle) -> bool {
    if let Err(error) = crate::license::require(app) {
        // The gesture claimed the take before it got here, so give it back
        // rather than leaving a token with no take behind it.
        clear_owner();
        let _ = app.emit("license-restricted", error);
        return false;
    }
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
            overlay::show_report(app, "No voice model yet");
            return false;
        }
        if !audio::mic_ready() {
            clear_owner();
            let _ = app.emit("dictation-error", "No microphone found.");
            overlay::show_report(app, "No microphone found");
            return false;
        }
        *state = DictState::Recording;
    }
    crate::crash::record_event("dictation:start-requested");

    overlay::show(app, "listening");
    if let Err(error) = audio::start(app.clone()) {
        crate::crash::record_event("dictation:audio-start-failed");
        overlay::show_report(app, "Microphone unavailable");
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
        POSTPROCESS_DEADLINE,
        crate::cloud::postprocess(local, config, POSTPROCESS_REQUEST_TIMEOUT),
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

/// How long a take of `samples` may run before it is treated as wedged.
///
/// Two parts: a floor for everything that is not the audio — backend init, a
/// cold SHA-256 over 1.58 GB, reading the model off disk, and the separately
/// bounded 60 s refinement — and a per-second-of-audio allowance on top, so a
/// twenty-minute take is not judged by the clock of a two-second one.
fn take_deadline(samples: usize) -> Duration {
    let audio_seconds = (samples / audio::TARGET_RATE as usize) as u64;
    Duration::from_secs(
        TAKE_DEADLINE_FLOOR
            .as_secs()
            .max(audio_seconds.saturating_mul(TAKE_REALTIME_FACTOR)),
    )
}

/// Put a take away because it was cancelled, unless somebody already reported it.
fn finish_cancelled(app: &AppHandle, operation: u64) {
    if operation_reported(operation) {
        // The watchdog has already shown the failure and put the state back. A
        // later take may already own the overlay, so this worker touches
        // neither it nor the state machine, and says nothing: the user was told
        // once, by the path that knows why.
        return;
    }
    overlay::hide(app);
    *state_lock() = DictState::Idle;
    let _ = app.emit("dictation-cancelled", ());
}

/// Give up on a take that has run past its ceiling.
///
/// This exists because nothing else did. `STATE` stayed `Transcribing` for ever
/// if native inference never returned, and nothing in the product could get out
/// of that but a restart. The ceiling is deliberately generous — see
/// `TAKE_DEADLINE_FLOOR` — because a false positive here throws away a
/// transcript, and a long wait does not.
fn start_take_watchdog(app: &AppHandle, operation: u64, deadline: Duration) {
    let app = app.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        loop {
            // The take finishing is the normal way out: the worker itself clears
            // Transcribing. A cancel is the other: it is already being reported
            // by whoever requested it, and reporting it twice with two different
            // explanations is worse than reporting it once.
            if *state_lock() != DictState::Transcribing || operation_cancelled(operation) {
                return;
            }
            if started.elapsed() >= deadline {
                break;
            }
            std::thread::sleep(TAKE_WATCHDOG_TICK);
        }
        // Close the gap between the last poll and here. A take that finished in
        // it has already pasted its text, and a failure report printed over that
        // is a lie the user can see on screen.
        if *state_lock() != DictState::Transcribing || operation_cancelled(operation) {
            return;
        }

        crate::crash::record_event("dictation:transcribe-timeout");
        log::warn!(
            "dictation gave up on operation {operation} after {}s of audio",
            deadline.as_secs()
        );
        // Published before the state changes, so a worker that is still blocked
        // in native inference can tell that its teardown is no longer its job.
        TIMED_OUT_OPERATION.fetch_max(operation, Ordering::SeqCst);
        model::cancel_transcription();
        cancel_operation();
        overlay::show_report(&app, "Transcription timed out");
        let _ = app.emit(
            "dictation-error",
            "Transcription took longer than expected and was stopped. A shorter take, or a smaller model, will finish.",
        );
        *state_lock() = DictState::Idle;
    });
}

pub fn stop(app: &AppHandle) {
    let mut state = state_lock();
    if *state != DictState::Recording {
        return;
    }
    *state = DictState::Transcribing;
    clear_owner();
    crate::crash::record_event("dictation:stop-requested");
    // Sampled here, at the moment the take ends, rather than after the
    // transcript is produced. Transcription plus optional cloud refinement can
    // take a minute, and reading focus at the end means an alt-tab during that
    // minute changes where the words go: a transcript meant for Notepad gets
    // filed in Bard's timeline instead. That is the privacy direction of the
    // error, and the person dictating cannot see it happen.
    let panel_focused = panel_has_focus(app);
    let operation = begin_operation();
    let app = app.clone();
    std::thread::spawn(move || {
        let recording = audio::stop();
        if let Some(error) = recording.error {
            overlay::show_report(&app, "Recording failed");
            *state_lock() = DictState::Idle;
            let _ = app.emit("dictation-error", error);
            return;
        }
        let mut samples = recording.samples;
        if samples.is_empty() || !recording.speech_detected {
            overlay::show_report(&app, "Nothing heard");
            *state_lock() = DictState::Idle;
            let _ = app.emit("dictation-error", "Nothing heard.");
            return;
        }
        if samples.len() < 16000 {
            samples.resize(20000, 0.0);
        }
        if operation_cancelled(operation) {
            finish_cancelled(&app, operation);
            return;
        }
        start_take_watchdog(&app, operation, take_deadline(samples.len()));
        overlay::show(&app, "transcribing");
        let outcome = model::transcribe(&app, &samples).map(clean_text);
        if operation_cancelled(operation) {
            finish_cancelled(&app, operation);
            return;
        }
        match outcome {
            Ok(text) if !text.is_empty() => {
                let text = postprocess_transcript(&app, text, operation);
                if operation_cancelled(operation) {
                    finish_cancelled(&app, operation);
                    return;
                }
                if let Err(error) = crate::paste::set_text(&app, &text) {
                    overlay::show_report(&app, "Couldn't paste");
                    let _ = app.emit("dictation-error", error);
                    *state_lock() = DictState::Idle;
                    return;
                }
                match delivery(panel_focused) {
                    Delivery::Timeline => {
                        let item_id =
                            crate::db::insert_item_with_origin(&app, &text, "text", None, "dictation");
                        crate::db::set_active_item(&app, Some(item_id));
                        let _ = crate::db::cleanup_origin(&app, "dictation");
                        let _ = app.emit("dictation-panel", text);
                    }
                    Delivery::Foreground => {
                        // The words go to the field you were typing in, and that
                        // is the end of it: no timeline row, no active item, no
                        // copy of what you said kept anywhere.
                        paste_chord(&app);
                        let _ = app.emit("dictation-pasted", ());
                    }
                }
            }
            Ok(_) => {
                overlay::show_report(&app, "Nothing heard");
                *state_lock() = DictState::Idle;
                let _ = app.emit("dictation-error", "Nothing heard.");
                return;
            }
            Err(e) => {
                overlay::show_report(&app, "Transcription failed");
                *state_lock() = DictState::Idle;
                let _ = app.emit("dictation-error", e);
                return;
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
    } else {
        // Nothing is running: the × on a report puts the pill away.
        overlay::dismiss(app);
    }
}

/// True only while the microphone is actually open and taking audio. This is
/// what the escape key is gated on today, which is why Escape could not reach
/// `cancel` during a transcription.
pub fn is_recording() -> bool {
    *state_lock() == DictState::Recording
}

/// True while a take owns Bard: recording, transcribing, or finishing.
///
/// This is the whole of the busy window. `DictState` has no `Finishing` — the
/// "finishing" the pill shows is the refinement, which runs while the state is
/// still `Transcribing` — so the predicate is "not Idle".
///
/// The signature the capture lane wants is exactly this, with no arguments:
/// `crate::dictation::is_busy()`. It replaces `is_recording()` in the escape
/// check at `capture.rs:284`, so Escape reaches `cancel` in both phases;
/// `cancel` already branches correctly for each. A short Ctrl+Ctrl gesture that
/// stops a take, and a push-to-talk release, should use the same predicate:
/// there is nothing to stop once it is false.
///
/// Deliberately conservative: it stays true until the state machine leaves
/// `Transcribing`, so a caller that polls it never races a take that is about to
/// start.
pub fn is_busy() -> bool {
    busy(*state_lock())
}

fn busy(state: DictState) -> bool {
    !matches!(state, DictState::Idle)
}

/// Where a finished transcript is allowed to go.
///
/// This is the product's privacy claim in code. "Anything else has focus → the
/// clipboard, then the paste chord; no timeline copy is created." The insert used
/// to run unconditionally, outside the focus check, so dictating into Notepad
/// quietly filed a second copy in Bard's history. In a local-first product that
/// sentence is the trust argument, so it is expressed as a decision with exactly
/// two arms and a test.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Delivery {
    /// Bard's own panel had focus: the user is entering the text into Bard, so
    /// it is filed like any other item and shown in the panel.
    Timeline,
    /// Something else had focus: the text goes to that window and nowhere else.
    Foreground,
}

/// Whether Bard's own panel is the window the person is typing into right now.
fn panel_has_focus(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_some_and(|window| {
        window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false)
    })
}

fn delivery(panel_focused: bool) -> Delivery {
    if panel_focused {
        Delivery::Timeline
    } else {
        Delivery::Foreground
    }
}

/// The transcript, exactly as the engine recognised it.
///
/// This used to squash every run of three or more identical characters down to
/// two, unconditionally, on every take, before storage and before the paste.
/// That turned `1000000` into `10000`, `...` into `..`, `===` into `==` and
/// `sooo` into `soo`, with no setting, nothing on screen, and no way to tell
/// afterwards. Silent corruption of the one thing dictation exists to produce.
/// It has no defensible purpose and it is gone.
///
/// It also used to strip a leading `um `, `uh `, `hmm ` or `er `. Filler removal
/// is what the blueprint puts in a Phase 2 settings row, as a visible opt-in
/// setting; its documented default is off, so applying it here with no setting
/// and no disclosure could only ever be wrong. Removing it is exactly shipping
/// the setting off, without a database read on the take path and without a
/// half-built row, and the row can add it back in one place. It was not harmless
/// either: it ate the word "um" when it was meant, ate the German "er", and
/// stripped one filler out of eleven, so it never did the job it claimed.
fn clean_text(raw: String) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut text = String::with_capacity(trimmed.len() + 1);
    text.push_str(trimmed);
    // A trailing space, so dictated text never fuses with the word it was
    // dictated after. Added, never taken away.
    text.push(' ');
    text
}

fn put_clipboard_back(app: &AppHandle, previous: Option<String>) {
    let Some(previous) = previous else {
        return;
    };
    crate::monitor::note_own_write(app);
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(previous);
    }
}

fn paste_chord(app: &AppHandle) {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let _synthetic_input = crate::capture::SyntheticInputGuard::enter();
    let previous = arboard::Clipboard::new()
        .ok()
        .and_then(|mut c| c.get_text().ok());
    std::thread::sleep(Duration::from_millis(60));
    // The clipboard is put back however this ends. Bailing out with the
    // transcript still on it would leave a copy of what the user just said in
    // the one place every other application can read without asking.
    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        put_clipboard_back(app, previous);
        return;
    };
    let _ = enigo.key(Key::Control, Direction::Press);
    std::thread::sleep(Duration::from_millis(30));
    let _ = enigo.key(Key::Unicode('v'), Direction::Click);
    std::thread::sleep(Duration::from_millis(30));
    let _ = enigo.key(Key::Control, Direction::Release);
    std::thread::sleep(Duration::from_millis(60));
    put_clipboard_back(app, previous);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every case the old squash mangled, plus the shapes a transcript actually
    /// takes: nothing, only whitespace, one character, Latin, CJK, and mixed.
    const TEXT_CASES: &[(&str, &str)] = &[
        // Numbers, hex, and repeated punctuation — all silently destroyed before.
        ("1000000", "1000000 "),
        ("00000", "00000 "),
        ("0x00FF00FF", "0x00FF00FF "),
        ("...", "... "),
        ("===", "=== "),
        ("----", "---- "),
        ("!!!!!", "!!!!! "),
        ("aaaaaaaaaaaaaaaa", "aaaaaaaaaaaaaaaa "),
        // Ordinary words with a real stutter in them.
        ("sooo", "sooo "),
        ("sooooo good", "sooooo good "),
        ("a", "a "),
        ("I", "I "),
        // Leading fillers. The blueprint's Phase 2 row removes these, opt-in.
        ("um so we ship", "um so we ship "),
        ("uh, I mean", "uh, I mean "),
        ("hmm okay", "hmm okay "),
        ("er ist gut", "er ist gut "),
        ("um", "um "),
        // Nothing at all.
        ("", ""),
        (" ", ""),
        ("   \t\n  ", ""),
        // Whitespace, trimmed at both ends and never more than one space at the
        // end.
        ("  hello  ", "hello "),
        ("\n\nwrapped\n", "wrapped "),
        ("a b", "a b "),
        // Anything but ASCII: trimming and the copy must not split a character.
        ("café", "café "),
        ("  café  ", "café "),
        ("naïve résumé", "naïve résumé "),
        ("東京は日本の首都です", "東京は日本の首都です "),
        ("  東京  ", "東京 "),
        // Mixed scripts, which is ordinary dictation in a bilingual dictation app.
        ("hello 東京 ok", "hello 東京 ok "),
        ("commit 1234567890abcdef 東京", "commit 1234567890abcdef 東京 "),
        ("don't", "don't "),
        ("\"quoted\"", "\"quoted\" "),
        ("version 1.0.0 build 20260926", "version 1.0.0 build 20260926 "),
    ];

    #[test]
    fn clean_text_preserves_every_character_it_was_given() {
        for (input, expected) in TEXT_CASES {
            assert_eq!(&clean_text((*input).to_string()), expected, "input {input:?}");
        }
    }

    #[test]
    fn clean_text_removes_no_characters_a_user_dictated() {
        // The invariant behind the whole fix: the only difference between what
        // the engine returned and what will be pasted is surrounding whitespace
        // and one trailing space. Character for character, it is the same text.
        for (input, _) in TEXT_CASES {
            let expected = input.trim();
            let produced = clean_text((*input).to_string());
            let produced = produced.strip_suffix(' ').unwrap_or(&produced);
            assert_eq!(produced, expected, "input {input:?}");
            assert_eq!(
                produced.chars().count(),
                expected.chars().count(),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn a_transcript_with_nothing_speech_like_in_it_is_still_empty() {
        // `stop` treats an empty result as "Nothing heard", so this is load
        // bearing: whitespace must not become a pasteable space.
        for input in ["", " ", "\t", "\n", " \t\r\n ", "\u{a0}"] {
            assert_eq!(clean_text(input.to_string()), "", "input {input:?}");
        }
    }

    #[test]
    fn a_stale_release_cannot_stop_a_take_it_does_not_own() {
        // The headline property. A raw key release from an earlier gesture, or
        // from a gesture that never owned anything, must reach nothing.
        for state in [DictState::Idle, DictState::Recording, DictState::Transcribing] {
            for owner in [None, Some(1_u64), Some(7)] {
                for held in [
                    Duration::ZERO,
                    Duration::from_millis(1),
                    crate::gesture::DICTATION_HOLD_LIMIT,
                    Duration::from_secs(30),
                ] {
                    assert_eq!(
                        gesture_release_outcome(state, owner, 2, held),
                        GestureOutcome::Ignore,
                        "state {state:?} owner {owner:?} held {held:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn an_owning_release_ends_the_take_only_if_it_was_held() {
        let limit = crate::gesture::DICTATION_HOLD_LIMIT;
        // Short tap: the recording stays open and ownership is given up, so a
        // second release of the same gesture cannot end it either.
        assert_eq!(
            gesture_release_outcome(DictState::Recording, Some(1), 1, Duration::ZERO),
            GestureOutcome::Disown
        );
        assert_eq!(
            gesture_release_outcome(
                DictState::Recording,
                Some(1),
                1,
                limit - Duration::from_millis(1)
            ),
            GestureOutcome::Disown
        );
        // Held to the limit, inclusive: push-to-talk ends the take.
        assert_eq!(
            gesture_release_outcome(DictState::Recording, Some(1), 1, limit),
            GestureOutcome::Stop
        );
        assert_eq!(
            gesture_release_outcome(
                DictState::Recording,
                Some(1),
                1,
                limit + Duration::from_secs(10)
            ),
            GestureOutcome::Stop
        );
    }

    #[test]
    fn a_disowned_take_can_only_be_stopped_by_the_next_press() {
        // Disowning is what a short tap does. The owner is now None, so the
        // same gesture's release is inert, and only a fresh press ends it.
        let mut owner = Some(1_u64);
        let release = gesture_release_outcome(
            DictState::Recording,
            owner,
            1,
            Duration::ZERO,
        );
        assert_eq!(release, GestureOutcome::Disown);
        owner = None;
        assert_eq!(
            gesture_release_outcome(DictState::Recording, owner, 1, Duration::ZERO),
            GestureOutcome::Ignore
        );
        assert_eq!(
            gesture_press_outcome(DictState::Recording),
            GestureOutcome::Stop
        );
    }

    #[test]
    fn a_press_starts_from_idle_and_stops_from_recording() {
        assert_eq!(gesture_press_outcome(DictState::Idle), GestureOutcome::Start);
        assert_eq!(
            gesture_press_outcome(DictState::Recording),
            GestureOutcome::Stop
        );
        // Nothing can start or stop a transcription from a gesture; the × and
        // Escape are the only ways, and they are not this.
        assert_eq!(
            gesture_press_outcome(DictState::Transcribing),
            GestureOutcome::Ignore
        );
    }

    #[test]
    fn a_press_can_stop_a_take_that_no_gesture_owns() {
        // The tray starts takes, and it has no token. If a press required
        // ownership, a tray-started take could never be stopped by Ctrl+Ctrl.
        for owner in [None, Some(1), Some(2)] {
            let _ = owner;
            assert_eq!(
                gesture_press_outcome(DictState::Recording),
                GestureOutcome::Stop
            );
        }
    }

    #[test]
    fn a_release_that_lands_after_the_take_ended_touches_nothing() {
        // `stop` clears the owner before it reaches Transcribing, so these are
        // unreachable — handled rather than assumed.
        assert_eq!(
            gesture_release_outcome(DictState::Idle, Some(1), 1, Duration::from_secs(9)),
            GestureOutcome::Ignore
        );
        assert_eq!(
            gesture_release_outcome(
                DictState::Transcribing,
                Some(1),
                1,
                Duration::from_secs(9)
            ),
            GestureOutcome::Ignore
        );
    }

    #[test]
    fn the_busy_window_covers_every_phase_of_a_take() {
        assert!(!busy(DictState::Idle));
        assert!(busy(DictState::Recording));
        // "Finishing" is the pill's word for the refinement, and the refinement
        // runs while the state is still Transcribing.
        assert!(busy(DictState::Transcribing));
        // Recording and Transcribing are the two states Escape has to reach.
        assert!(busy(DictState::Recording) || busy(DictState::Transcribing));
    }

    #[test]
    fn a_transcript_only_reaches_the_timeline_when_bard_had_focus() {
        assert_eq!(delivery(true), Delivery::Timeline);
        assert_eq!(delivery(false), Delivery::Foreground);
        // The two arms are the whole claim. `Delivery::Foreground` is the arm
        // that used to file a copy anyway.
        assert_ne!(delivery(true), delivery(false));
    }

    #[test]
    fn the_take_ceiling_outlasts_a_slow_cold_start() {
        // A one-second take: the floor, which has to cover backend init, a
        // SHA-256 over 1.58 GB, reading the model, and the separately bounded
        // 60 s refinement, on a machine doing none of that quickly.
        assert_eq!(take_deadline(16_000), TAKE_DEADLINE_FLOOR);
        assert!(TAKE_DEADLINE_FLOOR > POSTPROCESS_DEADLINE * 3);
        assert!(TAKE_DEADLINE_FLOOR > POSTPROCESS_REQUEST_TIMEOUT * 4);
    }

    #[test]
    fn the_take_ceiling_scales_with_the_audio_it_has_to_read() {
        // Four and a half times slower than realtime is the floor; beyond that
        // the allowance is three seconds per second of audio.
        let short = take_deadline(16_000);
        let long = take_deadline(audio::TARGET_RATE as usize * 60 * 20);
        assert_eq!(long, Duration::from_secs(20 * 60 * 3));
        assert!(long > short);
        // Monotonic in the take length, and never below the floor.
        let mut previous = Duration::ZERO;
        for minutes in 0..=20 {
            let deadline = take_deadline(audio::TARGET_RATE as usize * 60 * minutes);
            assert!(deadline >= previous, "{minutes} minutes went backwards");
            assert!(deadline >= TAKE_DEADLINE_FLOOR);
            previous = deadline;
        }
    }

    #[test]
    fn a_reported_take_is_not_reported_again() {
        assert!(!operation_reported(1), "nothing has timed out yet");
        TIMED_OUT_OPERATION.fetch_max(5, Ordering::SeqCst);
        // A worker at or before the reported operation must keep its hands off
        // the overlay, because a later take may already own it.
        for operation in 1..=5 {
            assert!(operation_reported(operation), "operation {operation}");
        }
        assert!(!operation_reported(6));
    }
}
