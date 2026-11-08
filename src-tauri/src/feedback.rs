//! Every action and error Bard takes is told to the person through two quiet
//! channels: a small pill at the top middle of the screen and a soft sound.
//! Each channel has its own switch in Settings; both, either or neither.
//!
//! The one exception is deliberate: while the microphone is open the pill
//! shows regardless of the switch, because a live microphone is never a
//! notification someone can opt out of seeing.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::AppHandle;

pub const PILL_SETTING: &str = "feedback_pill";
pub const SOUND_SETTING: &str = "feedback_sound";

static PILL_ON: AtomicBool = AtomicBool::new(true);
static SOUND_ON: AtomicBool = AtomicBool::new(true);

/// Read both switches once the database is open. Absent means on.
pub fn init(app: &AppHandle) {
    PILL_ON.store(
        crate::db::get_setting(app, PILL_SETTING).as_deref() != Some("0"),
        Ordering::SeqCst,
    );
    SOUND_ON.store(
        crate::db::get_setting(app, SOUND_SETTING).as_deref() != Some("0"),
        Ordering::SeqCst,
    );
}

pub fn pill_on() -> bool {
    PILL_ON.load(Ordering::SeqCst)
}

pub fn sound_on() -> bool {
    SOUND_ON.load(Ordering::SeqCst)
}

pub fn set_channels(app: &AppHandle, pill: bool, sound: bool) {
    PILL_ON.store(pill, Ordering::SeqCst);
    SOUND_ON.store(sound, Ordering::SeqCst);
    crate::db::set_setting(app, PILL_SETTING, if pill { "1" } else { "0" });
    crate::db::set_setting(app, SOUND_SETTING, if sound { "1" } else { "0" });
}

/// One thing worth telling the person.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    /// The icon and the sound: copy, select, preview, listen, work, done,
    /// pause, resume, error or info.
    pub kind: String,
    /// The headline, a few words.
    pub text: String,
    /// A one-line excerpt of what it was about, when there is one.
    pub detail: Option<String>,
    /// For a slot: which one (1–3) and how many are filled.
    pub slot: Option<u8>,
    pub slots: Option<u8>,
    /// The clipboard as it is now, for the preview shortcut.
    pub preview: Option<crate::clip::Preview>,
    /// Stays until replaced (dictation states) instead of timing out.
    pub sticky: bool,
    /// How long a non-sticky pill stays, in milliseconds.
    pub hold_ms: u64,
    /// Whether this note makes a sound. A dictation phase change inside one
    /// take is shown but not sounded twice.
    #[serde(skip)]
    pub sound: bool,
    /// Shown even with the pill switched off (a live microphone).
    #[serde(skip)]
    pub always_show: bool,
}

const SHORT: u64 = 1_500;
const READ: u64 = 3_400;
const PREVIEW: u64 = 4_500;

/// The first line of some text, trimmed to a pill's width.
pub fn excerpt(text: &str, max: usize) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let mut out: String = line.chars().take(max).collect();
    if line.chars().count() > max || text.trim().lines().count() > 1 {
        out.push('…');
    }
    out
}

impl Note {
    fn new(kind: &str, text: impl Into<String>, hold_ms: u64) -> Note {
        Note {
            kind: kind.into(),
            text: text.into(),
            detail: None,
            slot: None,
            slots: None,
            preview: None,
            sticky: false,
            hold_ms,
            sound: true,
            always_show: false,
        }
    }

    pub fn info(kind: &str, text: impl Into<String>) -> Note {
        Note::new(kind, text, SHORT)
    }

    pub fn error(text: impl Into<String>) -> Note {
        Note::new("error", text, READ)
    }

    /// Something the person copied was kept.
    pub fn copied(text: &str) -> Note {
        let mut note = Note::new("copy", "Copied", SHORT);
        note.detail = Some(excerpt(text, 64));
        note
    }

    /// Bard put an item on the clipboard.
    pub fn selected(slot: Option<u8>, slots: Option<u8>, detail: String) -> Note {
        let mut note = Note::new("select", "On your clipboard", SHORT + 300);
        note.detail = Some(detail);
        note.slot = slot;
        note.slots = slots;
        note
    }

    pub fn preview(preview: crate::clip::Preview) -> Note {
        let mut note = Note::new("preview", "Clipboard", PREVIEW);
        note.preview = Some(preview);
        note
    }

    /// A dictation phase: listening, working, finishing.
    pub fn dictation(kind: &str, text: &str, sound: bool) -> Note {
        let mut note = Note::new(kind, text, 0);
        note.sticky = true;
        note.sound = sound;
        note.always_show = kind == "listen";
        note
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Note {
        self.detail = Some(detail.into());
        self
    }

    pub fn hold(mut self, hold_ms: u64) -> Note {
        self.hold_ms = hold_ms;
        self
    }
}

/// Tell the person, on whichever channels they have switched on.
pub fn notify(app: &AppHandle, note: Note) {
    if note.sound && sound_on() {
        crate::sound::play(crate::sound::Cue::from_kind(&note.kind));
    }
    if pill_on() || note.always_show {
        crate::pill::show(app, note);
    } else if note.kind != "listen" {
        // A pill left over from a live take must not outlive it.
        crate::pill::hide_sticky(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpt_takes_the_first_real_line() {
        assert_eq!(excerpt("\n  hello world  \nsecond", 64), "hello world…");
        assert_eq!(excerpt("short", 64), "short");
        assert_eq!(excerpt("abcdef", 3), "abc…");
        assert_eq!(excerpt("   ", 10), "");
    }

    #[test]
    fn only_a_live_microphone_overrides_the_pill_switch() {
        assert!(Note::dictation("listen", "Listening", true).always_show);
        assert!(!Note::dictation("work", "Transcribing", false).always_show);
        assert!(!Note::copied("x").always_show);
    }
}
