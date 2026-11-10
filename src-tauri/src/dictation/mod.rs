mod audio;
pub(crate) mod model;
mod vad;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::feedback::{self, Note};

/// Each attempt to refine a transcript gets this long to answer, and the
/// whole refinement this long in total; the local words are kept either way.
const POSTPROCESS_REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const POSTPROCESS_DEADLINE: Duration = Duration::from_secs(60);

/// The least time a take may spend between the end of recording and its
/// transcript before it is treated as wedged. It covers backend init, a cold
/// hash of a large model, loading it, and the separately bounded refinement.
const TAKE_DEADLINE_FLOOR: Duration = Duration::from_secs(210);

/// Seconds of transcription allowed per second of audio beyond the floor.
const TAKE_REALTIME_FACTOR: u64 = 3;

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
static OPERATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static OPERATION_CANCELLED: AtomicBool = AtomicBool::new(false);
/// The newest operation the watchdog has already reported as timed out.
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

fn state_lock() -> MutexGuard<'static, DictState> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn init(app: AppHandle) {
    // The take path's first verify is a full hash of the model; start it now,
    // on its own thread, so the first take does not wait for it.
    model::spawn_verification_warmup(app.clone());
    model::start_unload_watcher(app);
}

pub fn mic_ready() -> bool {
    audio::mic_ready()
}

pub fn list_input_devices() -> Vec<String> {
    audio::list_input_devices()
}

fn report(app: &AppHandle, text: &str, detail: Option<String>) {
    let mut note = Note::error(text);
    note.detail = detail;
    feedback::notify(app, note);
    let _ = app.emit("dictation-ended", ());
}

/// Start listening. Returns whether a take started.
pub fn start(app: &AppHandle) -> bool {
    if let Err(error) = crate::license::require(app) {
        let _ = app.emit("license-restricted", error.clone());
        feedback::notify(app, Note::error("Bard needs a licence to dictate"));
        return false;
    }
    {
        let state = state_lock();
        // Idle is the only state a new take can start from.
        if *state != DictState::Idle {
            return false;
        }
    }
    // The first check hashes the whole model file, which on a cold start is
    // seconds of work. It must not run under the state lock: every state query
    // and the overlay read that lock, and a take that is starting is not yet a
    // take. The check used to be missing entirely and the take paid for it only
    // after recording, so a corrupt model cost a whole take; the price of the
    // gate is that it is paid before the microphone opens instead.
    if let Err(error) = model::ensure_model_ready(app) {
        report(app, "Voice model not ready", Some(error));
        return false;
    }
    {
        let mut state = state_lock();
        // Checked again because two starts can pass the check above together;
        // only the one that arrives here first may own the take.
        if *state != DictState::Idle {
            return false;
        }
        if !audio::mic_ready() {
            drop(state);
            report(app, "No microphone found", None);
            return false;
        }
        *state = DictState::Recording;
    }
    crate::crash::record_event("dictation:start-requested");
    feedback::notify(app, Note::dictation("listen", "Listening", true));
    if let Err(error) = audio::start(app.clone()) {
        crate::crash::record_event("dictation:audio-start-failed");
        *state_lock() = DictState::Idle;
        report(app, "Microphone unavailable", Some(error));
        return false;
    }
    crate::shortcuts::grab_escape(true);
    let _ = app.emit("dictation-state", "listening");
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

    crate::feedback::notify(
        app,
        crate::feedback::Note::dictation("work", "Refining", false),
    );
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
        // The watchdog has already shown the failure and put the state back.
        return;
    }
    *state_lock() = DictState::Idle;
    feedback::notify(app, Note::info("info", "Dictation cancelled"));
    let _ = app.emit("dictation-ended", ());
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
        crate::shortcuts::grab_escape(false);
        crate::feedback::notify(
            &app,
            crate::feedback::Note::error("Transcription timed out"),
        );
        let _ = app.emit(
            "dictation-error",
            "Transcription took longer than expected and was stopped. A shorter take, or a smaller model, will finish.",
        );
        *state_lock() = DictState::Idle;
    });
}

/// Stop listening and turn the take into text on the clipboard.
///
/// Every finished transcript goes to the clipboard and into history (origin
/// `dictation`, under its own retention lifespan), so a copy made before the
/// person pastes cannot lose what they said. Bard never pastes it for them.
pub fn stop(app: &AppHandle) {
    let mut state = state_lock();
    if *state != DictState::Recording {
        return;
    }
    *state = DictState::Transcribing;
    drop(state);
    crate::shortcuts::grab_escape(false);
    crate::crash::record_event("dictation:stop-requested");
    let operation = begin_operation();
    let app = app.clone();
    std::thread::spawn(move || {
        let recording = audio::stop();
        if let Some(error) = recording.error {
            *state_lock() = DictState::Idle;
            report(&app, "Recording failed", Some(error));
            return;
        }
        if let Some(warning) = recording.warning {
            // Capture outran the buffer and discarded its oldest unread audio.
            // The take is still worth transcribing, but this is the only way the
            // person can know the transcript does not cover everything they
            // said, so it is said on screen and in the log rather than dropped.
            crate::crash::record_event("dictation:audio-overrun");
            let _ = app.emit("dictation-audio-warning", warning.clone());
            feedback::notify(
                &app,
                Note::info("info", "Some audio was skipped").with_detail(warning),
            );
        }
        let mut samples = recording.samples;
        if samples.is_empty() || !recording.speech_detected {
            *state_lock() = DictState::Idle;
            report(&app, "Nothing heard", None);
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
        feedback::notify(&app, Note::dictation("work", "Transcribing", true));
        let _ = app.emit("dictation-state", "transcribing");
        let options = transcript_options(&app);
        let outcome = model::transcribe(&app, &samples).map(|text| clean_text(text, options));
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
                deliver(&app, &text);
            }
            Ok(_) => report(&app, "Nothing heard", None),
            Err(error) => report(&app, "Transcription failed", Some(error)),
        }
        *state_lock() = DictState::Idle;
        let _ = app.emit("dictation-ended", ());
    });
}

/// History first, then the clipboard: whatever happens to the clipboard, the
/// words are kept.
fn deliver(app: &AppHandle, text: &str) {
    let stored =
        crate::db::insert_item_with_origin(app, text.trim_end(), "text", None, "dictation");
    if stored != crate::db::INSERT_FAILED {
        crate::db::set_active_item(app, Some(stored));
        let _ = crate::db::cleanup_origin(app, "dictation");
        crate::commands::refresh(app);
    }
    match crate::clip::set_text(app, text) {
        Ok(()) => feedback::notify(
            app,
            Note::info("done", "Transcript copied")
                .with_detail(feedback::excerpt(text, 64))
                .hold(2_600),
        ),
        Err(_) if stored != crate::db::INSERT_FAILED => feedback::notify(
            app,
            Note::error("Couldn't copy the transcript")
                .with_detail("It is saved in Bard's history"),
        ),
        Err(_) => feedback::notify(app, Note::error("Couldn't copy or save the transcript")),
    }
}

pub fn cancel(app: &AppHandle) {
    let state = *state_lock();
    crate::shortcuts::grab_escape(false);
    match state {
        DictState::Recording => {
            audio::cancel();
            cancel_operation();
            *state_lock() = DictState::Idle;
            feedback::notify(app, Note::info("info", "Dictation cancelled"));
            let _ = app.emit("dictation-ended", ());
        }
        DictState::Transcribing => {
            model::cancel_transcription();
            cancel_operation();
            feedback::notify(app, Note::dictation("work", "Cancelling", false));
        }
        DictState::Idle => {}
    }
}

/// True only while the microphone is open and taking audio.
pub fn is_recording() -> bool {
    *state_lock() == DictState::Recording
}

/// True while a take owns Bard: recording or transcribing (refinement runs
/// inside Transcribing).
pub fn is_busy() -> bool {
    busy(*state_lock())
}

fn busy(state: DictState) -> bool {
    !matches!(state, DictState::Idle)
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
fn clean_text(raw: String, options: TranscriptOptions) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let without_fillers = if options.fillers {
        remove_fillers(trimmed)
    } else {
        trimmed.to_string()
    };
    let trimmed = without_fillers.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut text = String::with_capacity(trimmed.len() + 1);
    text.push_str(trimmed);
    // A trailing space, so dictated text never fuses with the word it was
    // dictated after. Added, never taken away - unless the person asked for
    // the transcript to end exactly where they stopped speaking.
    if options.trailing_space {
        text.push(' ');
    }
    text
}

/// What the person chose for the finished transcript.
#[derive(Clone, Copy)]
pub struct TranscriptOptions {
    /// End the transcript with a space, so it never fuses with what is typed
    /// next. On by default.
    pub trailing_space: bool,
    /// Take the filler words out. Off by default: a filler is sometimes the
    /// word someone meant.
    pub fillers: bool,
}

/// The setting names, read from the same store as every other choice.
const TRAILING_SPACE_SETTING: &str = "dictation_trailing_space";
const FILLERS_SETTING: &str = "dictation_fillers";
/// How long a silence has to last before the take ends on its own. Zero means
/// never: the take ends when the person stops it.
pub const SILENCE_SETTING: &str = "dictation_silence_ms";

/// How long the silence has to be, in milliseconds, as the person chose it.
pub fn silence_auto_stop_ms(app: &AppHandle) -> u64 {
    crate::db::get_setting(app, SILENCE_SETTING)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(audio::SILENCE_AUTO_STOP_MS)
}

pub fn transcript_options(app: &AppHandle) -> TranscriptOptions {
    TranscriptOptions {
        // The trailing space is the default, so only an explicit "0" turns it off.
        trailing_space: crate::db::get_setting(app, TRAILING_SPACE_SETTING).as_deref() != Some("0"),
        fillers: crate::db::get_setting(app, FILLERS_SETTING).as_deref() == Some("1"),
    }
}

/// The filler words, as whole words.
///
/// The list is deliberately short and deliberately excludes "er" and "ah":
/// both are ordinary words in German, Danish, Dutch and several other
/// languages, and a transcript in one of those languages must not lose them.
/// The old cleaner ate the German "er", and a cleaner that eats a word
/// somebody meant is worse than one that leaves a filler in.
const FILLERS: &[&str] = &[
    "um", "uh", "umm", "ummm", "uhm", "erm", "hmm", "hm", "mm", "mhm",
];

/// Take the filler words out, and change nothing else.
///
/// Every character that is not part of a filler word is kept exactly where it
/// was: the whitespace around a removed word stays, so the rest of the
/// transcript is byte for byte what was said.
fn remove_fillers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        // The whitespace before the next word, and the word itself.
        let gap_len = rest.len() - rest.trim_start().len();
        let (gap, after_gap) = rest.split_at(gap_len);
        let word_len = after_gap
            .find(char::is_whitespace)
            .unwrap_or(after_gap.len());
        let (word, after) = after_gap.split_at(word_len);
        // The word without the punctuation around it: "Um," and "um," are
        // both the filler.
        let bare = word.trim_matches(|c: char| !c.is_alphanumeric());
        if !bare.is_empty()
            && FILLERS
                .iter()
                .any(|filler| bare.eq_ignore_ascii_case(filler))
        {
            // The punctuation that belongs to the sentence stays - a comma
            // after a pause is the writer's, not the filler's - and the
            // filler's own marks go with it. The whitespace that followed the
            // filler goes too, so the words around it keep the spacing they
            // had rather than gaining a gap.
            let sentence: String = word
                .chars()
                .rev()
                .take_while(|c| !c.is_alphanumeric())
                .filter(|c| ",.;:!?".contains(*c))
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            out.push_str(&sentence);
            rest = after;
            continue;
        }
        out.push_str(gap);
        out.push_str(word);
        rest = after;
    }
    out
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
        (
            "commit 1234567890abcdef 東京",
            "commit 1234567890abcdef 東京 ",
        ),
        ("don't", "don't "),
        ("\"quoted\"", "\"quoted\" "),
        (
            "version 1.0.0 build 20260926",
            "version 1.0.0 build 20260926 ",
        ),
    ];

    #[test]
    fn clean_text_preserves_every_character_it_was_given() {
        for (input, expected) in TEXT_CASES {
            assert_eq!(
                &clean_text(
                    (*input).to_string(),
                    TranscriptOptions {
                        trailing_space: true,
                        fillers: false
                    }
                ),
                expected,
                "input {input:?}"
            );
        }
    }

    #[test]
    fn clean_text_removes_no_characters_a_user_dictated() {
        // The invariant behind the whole fix: the only difference between what
        // the engine returned and what will be pasted is surrounding whitespace
        // and one trailing space. Character for character, it is the same text.
        for (input, _) in TEXT_CASES {
            let expected = input.trim();
            let produced = clean_text(
                (*input).to_string(),
                TranscriptOptions {
                    trailing_space: true,
                    fillers: false,
                },
            );
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
            assert_eq!(
                clean_text(
                    input.to_string(),
                    TranscriptOptions {
                        trailing_space: true,
                        fillers: false,
                    },
                ),
                "",
                "input {input:?}"
            );
        }
    }

    #[test]
    fn the_trailing_space_can_be_turned_off() {
        let options = TranscriptOptions {
            trailing_space: false,
            fillers: false,
        };
        assert_eq!(clean_text("hello ".into(), options), "hello");
        assert_eq!(
            clean_text("hello".into(), options),
            "hello",
            "the space is added, never taken away"
        );
    }

    #[test]
    fn fillers_are_removed_only_when_asked_and_only_as_whole_words() {
        let on = TranscriptOptions {
            trailing_space: true,
            fillers: true,
        };
        let off = TranscriptOptions {
            trailing_space: true,
            fillers: false,
        };
        assert_eq!(clean_text("well um, hello".into(), on), "well, hello ");
        assert_eq!(clean_text("well um, hello".into(), off), "well um, hello ");
        // A word that merely contains a filler is not one.
        assert_eq!(clean_text("summit umbrella".into(), on), "summit umbrella ");
        // "er" and "ah" are ordinary words in several languages, so they are
        // not on the list.
        assert_eq!(clean_text("er ah".into(), on), "er ah ");
    }

    #[test]
    fn removing_a_filler_leaves_the_rest_of_the_text_exactly_where_it_was() {
        let on = TranscriptOptions {
            trailing_space: true,
            fillers: true,
        };
        assert_eq!(clean_text("a um b".into(), on), "a b ");
        assert_eq!(clean_text("a  um  b".into(), on), "a  b ");
        // The comma belongs to the sentence, not to the filler.
        assert_eq!(clean_text("well um, hello".into(), on), "well, hello ");
        assert_eq!(clean_text("um".into(), on), "");
        assert_eq!(clean_text("um ".into(), on), "");
    }

    #[test]
    fn the_busy_window_covers_every_phase_of_a_take() {
        assert!(!busy(DictState::Idle));
        assert!(busy(DictState::Recording));
        assert!(busy(DictState::Transcribing));
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
