//! Bard's global shortcuts: one table of actions, one chord each, the same on
//! every platform, and changeable by the person.
//!
//! Each platform gets its native, consent-respecting mechanism rather than a
//! keyboard hook:
//! - Windows: `RegisterHotKey`. The system swallows the chord, so the app in
//!   front never sees the key, and a chord another app already owns fails
//!   visibly instead of silently doing nothing.
//! - X11: `XGrabKey` on the root window, with detectable auto-repeat so a held
//!   chord is one press and one release.
//! - Wayland: the GlobalShortcuts desktop portal (`linux_portal.rs`). The
//!   desktop asks the person once, and owns the bindings from then on.
//!
//! Dictation is the one action with a release: held, it is push-to-talk;
//! tapped, it toggles.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Action {
    Main,
    Draw,
    Settings,
    Guide,
    Dictate,
    Slot1,
    Slot2,
    Slot3,
    Cycle,
    Preview,
    Pause,
}

pub const ACTIONS: [Action; 11] = [
    Action::Main,
    Action::Dictate,
    Action::Slot1,
    Action::Slot2,
    Action::Slot3,
    Action::Cycle,
    Action::Preview,
    Action::Pause,
    Action::Draw,
    Action::Settings,
    Action::Guide,
];

impl Action {
    pub fn id(self) -> &'static str {
        match self {
            Action::Main => "main",
            Action::Draw => "draw",
            Action::Settings => "settings",
            Action::Guide => "guide",
            Action::Dictate => "dictate",
            Action::Slot1 => "slot-1",
            Action::Slot2 => "slot-2",
            Action::Slot3 => "slot-3",
            Action::Cycle => "cycle",
            Action::Preview => "preview",
            Action::Pause => "pause",
        }
    }

    pub fn from_id(id: &str) -> Option<Action> {
        ACTIONS.iter().copied().find(|action| action.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::Main => "Show or hide Bard",
            Action::Draw => "Open Draw",
            Action::Settings => "Open Settings",
            Action::Guide => "Show the shortcut guide",
            Action::Dictate => "Dictate (hold to talk, tap to toggle)",
            Action::Slot1 => "Copy item 1",
            Action::Slot2 => "Copy item 2",
            Action::Slot3 => "Copy item 3",
            Action::Cycle => "Cycle through items 1–3",
            Action::Preview => "Preview the clipboard",
            Action::Pause => "Pause or resume clipboard capture",
        }
    }

    pub fn default_chord(self) -> &'static str {
        match self {
            Action::Main => "Alt+Shift+V",
            Action::Draw => "Alt+Shift+D",
            Action::Settings => "Alt+Shift+Comma",
            Action::Guide => "Alt+Shift+Slash",
            Action::Dictate => "Alt+Shift+Space",
            Action::Slot1 => "Alt+Shift+1",
            Action::Slot2 => "Alt+Shift+2",
            Action::Slot3 => "Alt+Shift+3",
            Action::Cycle => "Alt+Shift+C",
            Action::Preview => "Alt+Shift+P",
            Action::Pause => "Alt+Shift+X",
        }
    }

    fn setting_key(self) -> String {
        format!("shortcut:{}", self.id())
    }
}

/// A modifier set plus one key, written `Ctrl+Alt+Shift+Super+Key`.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    /// A key name from [`KEYS`].
    pub key: &'static str,
}

/// Every key a chord may end in: its canonical name, how it is shown, its
/// Windows virtual-key code and its X11 keysym.
const KEYS: &[(&str, &str, u32, u32)] = &[
    ("A", "A", 0x41, 0x61),
    ("B", "B", 0x42, 0x62),
    ("C", "C", 0x43, 0x63),
    ("D", "D", 0x44, 0x64),
    ("E", "E", 0x45, 0x65),
    ("F", "F", 0x46, 0x66),
    ("G", "G", 0x47, 0x67),
    ("H", "H", 0x48, 0x68),
    ("I", "I", 0x49, 0x69),
    ("J", "J", 0x4A, 0x6a),
    ("K", "K", 0x4B, 0x6b),
    ("L", "L", 0x4C, 0x6c),
    ("M", "M", 0x4D, 0x6d),
    ("N", "N", 0x4E, 0x6e),
    ("O", "O", 0x4F, 0x6f),
    ("P", "P", 0x50, 0x70),
    ("Q", "Q", 0x51, 0x71),
    ("R", "R", 0x52, 0x72),
    ("S", "S", 0x53, 0x73),
    ("T", "T", 0x54, 0x74),
    ("U", "U", 0x55, 0x75),
    ("V", "V", 0x56, 0x76),
    ("W", "W", 0x57, 0x77),
    ("X", "X", 0x58, 0x78),
    ("Y", "Y", 0x59, 0x79),
    ("Z", "Z", 0x5A, 0x7a),
    ("0", "0", 0x30, 0x30),
    ("1", "1", 0x31, 0x31),
    ("2", "2", 0x32, 0x32),
    ("3", "3", 0x33, 0x33),
    ("4", "4", 0x34, 0x34),
    ("5", "5", 0x35, 0x35),
    ("6", "6", 0x36, 0x36),
    ("7", "7", 0x37, 0x37),
    ("8", "8", 0x38, 0x38),
    ("9", "9", 0x39, 0x39),
    ("Space", "Space", 0x20, 0x20),
    ("Comma", ",", 0xBC, 0x2c),
    ("Period", ".", 0xBE, 0x2e),
    ("Slash", "/", 0xBF, 0x2f),
    ("Semicolon", ";", 0xBA, 0x3b),
    ("Quote", "'", 0xDE, 0x27),
    ("BracketLeft", "[", 0xDB, 0x5b),
    ("BracketRight", "]", 0xDD, 0x5d),
    ("Backslash", "\\", 0xDC, 0x5c),
    ("Backquote", "`", 0xC0, 0x60),
    ("Minus", "-", 0xBD, 0x2d),
    ("Equal", "=", 0xBB, 0x3d),
    ("Enter", "Enter", 0x0D, 0xff0d),
    ("Tab", "Tab", 0x09, 0xff09),
    ("Insert", "Insert", 0x2D, 0xff63),
    ("Delete", "Delete", 0x2E, 0xffff),
    ("Home", "Home", 0x24, 0xff50),
    ("End", "End", 0x23, 0xff57),
    ("PageUp", "Page Up", 0x21, 0xff55),
    ("PageDown", "Page Down", 0x22, 0xff56),
    ("Up", "↑", 0x26, 0xff52),
    ("Down", "↓", 0x28, 0xff54),
    ("Left", "←", 0x25, 0xff51),
    ("Right", "→", 0x27, 0xff53),
    ("F1", "F1", 0x70, 0xffbe),
    ("F2", "F2", 0x71, 0xffbf),
    ("F3", "F3", 0x72, 0xffc0),
    ("F4", "F4", 0x73, 0xffc1),
    ("F5", "F5", 0x74, 0xffc2),
    ("F6", "F6", 0x75, 0xffc3),
    ("F7", "F7", 0x76, 0xffc4),
    ("F8", "F8", 0x77, 0xffc5),
    ("F9", "F9", 0x78, 0xffc6),
    ("F10", "F10", 0x79, 0xffc7),
    ("F11", "F11", 0x7A, 0xffc8),
    ("F12", "F12", 0x7B, 0xffc9),
];

fn key_entry(name: &str) -> Option<&'static (&'static str, &'static str, u32, u32)> {
    KEYS.iter().find(|(canonical, shown, _, _)| {
        canonical.eq_ignore_ascii_case(name) || (*shown != *canonical && *shown == name)
    })
}

impl Chord {
    pub fn parse(text: &str) -> Option<Chord> {
        let mut chord = Chord {
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
            key: "",
        };
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let (key, modifiers) = parts.split_last()?;
        for modifier in modifiers {
            match modifier.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => chord.ctrl = true,
                "alt" | "option" => chord.alt = true,
                "shift" => chord.shift = true,
                "super" | "win" | "meta" | "logo" | "cmd" => chord.meta = true,
                _ => return None,
            }
        }
        chord.key = key_entry(key)?.0;
        Some(chord)
    }

    /// A global chord needs a modifier, unless it is a function key: a bare
    /// letter would be taken from every app on the desktop. Shift alone is
    /// typing, not a shortcut.
    pub fn is_usable(&self) -> bool {
        let function_key = self.key.starts_with('F') && self.key.len() > 1;
        function_key || self.ctrl || self.alt || self.meta
    }

    pub fn canonical(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.meta {
            parts.push("Super");
        }
        parts.push(self.key);
        parts.join("+")
    }

    /// How the chord is written for people, using the platform's name for
    /// the logo key.
    pub fn display(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.meta {
            parts.push(if cfg!(windows) { "Win" } else { "Super" });
        }
        parts.push(key_entry(self.key).map(|k| k.1).unwrap_or(self.key));
        parts.join("+")
    }

    /// The xdg shortcut-spec trigger the portal is offered.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn portal_trigger(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("CTRL".into());
        }
        if self.alt {
            parts.push("ALT".into());
        }
        if self.shift {
            parts.push("SHIFT".into());
        }
        if self.meta {
            parts.push("LOGO".into());
        }
        let key = match self.key {
            "Space" => "space".to_string(),
            "Comma" => "comma".into(),
            "Period" => "period".into(),
            "Slash" => "slash".into(),
            "Semicolon" => "semicolon".into(),
            "Quote" => "apostrophe".into(),
            "BracketLeft" => "bracketleft".into(),
            "BracketRight" => "bracketright".into(),
            "Backslash" => "backslash".into(),
            "Backquote" => "grave".into(),
            "Minus" => "minus".into(),
            "Equal" => "equal".into(),
            "Enter" => "Return".into(),
            "PageUp" => "Prior".into(),
            "PageDown" => "Next".into(),
            other if other.len() == 1 => other.to_ascii_lowercase(),
            other => other.to_string(),
        };
        parts.push(key);
        parts.join("+")
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn vk(&self) -> u32 {
        key_entry(self.key).map(|k| k.2).unwrap_or(0)
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn keysym(&self) -> u32 {
        key_entry(self.key).map(|k| k.3).unwrap_or(0)
    }
}

/// The chord each action answers to now: the person's choice, else the
/// default. `None` means the person cleared it.
pub fn chord_for(app: &AppHandle, action: Action) -> Option<Chord> {
    match crate::db::get_setting(app, &action.setting_key()) {
        Some(stored) if stored.is_empty() => None,
        Some(stored) => Chord::parse(&stored).or_else(|| Chord::parse(action.default_chord())),
        None => Chord::parse(action.default_chord()),
    }
}

pub fn bindings(app: &AppHandle) -> Vec<(Action, Chord)> {
    ACTIONS
        .iter()
        .filter_map(|action| chord_for(app, *action).map(|chord| (*action, chord)))
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Registered and listening.
    Ready,
    /// Another application owns the chord.
    Taken,
    /// Cleared by the person.
    Off,
    /// Waiting for the desktop (Wayland portal) to grant it.
    Pending,
}

fn statuses() -> &'static Mutex<HashMap<Action, Status>> {
    static STATUS: OnceLock<Mutex<HashMap<Action, Status>>> = OnceLock::new();
    STATUS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_status(action: Action, status: Status) {
    if let Ok(mut map) = statuses().lock() {
        map.insert(action, status);
    }
}

fn status(action: Action) -> Status {
    statuses()
        .lock()
        .ok()
        .and_then(|map| map.get(&action).copied())
        .unwrap_or(Status::Pending)
}

/// What the portal reports each shortcut is actually bound to on Wayland.
static PORTAL_TRIGGERS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
/// Shortcut id → what the desktop itself already does with that chord.
static PORTAL_CONFLICTS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

#[cfg(target_os = "linux")]
pub fn set_portal_triggers(triggers: HashMap<String, String>, conflicts: HashMap<String, String>) {
    if let Ok(mut slot) = PORTAL_CONFLICTS.lock() {
        *slot = Some(conflicts.clone());
    }
    if let Ok(mut slot) = PORTAL_TRIGGERS.lock() {
        *slot = Some(triggers.clone());
        for action in ACTIONS {
            // A shortcut is Ready only when the portal actually reported it.
            // An empty or partial list means the desktop did not bind that
            // chord, and the person must be told the truth: not bound, not
            // shown as a working shortcut that does nothing.
            let reported = slot
                .as_ref()
                .and_then(|triggers| triggers.get(action.id()))
                .is_some_and(|trigger| !trigger.is_empty());
            set_status(
                action,
                if conflicts.contains_key(action.id()) {
                    Status::Taken
                } else if reported {
                    Status::Ready
                } else {
                    Status::Pending
                },
            );
        }
    }
}

/// A chord reduced to its modifiers and key, however it is spelled:
/// `Ctrl+Alt+D`, `<Primary><Alt>d` and `CTRL+ALT+d` are one chord.
#[cfg(target_os = "linux")]
fn combo(text: &str) -> Option<(u8, String)> {
    let mut mods = 0u8;
    let mut key = None;
    for token in text
        .split(|c: char| matches!(c, '+' | '<' | '>') || c.is_whitespace())
        .filter(|token| !token.is_empty())
    {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "primary" => mods |= 1,
            "alt" | "mod1" => mods |= 2,
            "shift" => mods |= 4,
            "super" | "meta" | "logo" | "mod4" | "win" => mods |= 8,
            "," => key = Some("comma".to_string()),
            "." => key = Some("period".to_string()),
            "/" => key = Some("slash".to_string()),
            other => key = Some(other.to_string()),
        }
    }
    key.map(|key| (mods, key))
}

/// One line of `gsettings list-recursively`: the action's name and the
/// chords bound to it. `schema show-desktop ['<Primary><Alt>d', '<Super>d']`.
#[cfg(target_os = "linux")]
fn gsettings_line(line: &str) -> Option<(String, Vec<(u8, String)>)> {
    let mut parts = line.splitn(3, ' ');
    let (_schema, name, value) = (parts.next()?, parts.next()?, parts.next()?);
    let chords: Vec<_> = value
        .split('\'')
        .skip(1)
        .step_by(2)
        .filter_map(combo)
        .collect();
    if chords.is_empty() {
        return None;
    }
    Some((name.replace('-', " "), chords))
}

/// The chords GNOME keeps for itself, each with the name of what it does.
/// The desktop grants an app a chord it already uses without a word, and
/// then never delivers it; this is how Bard finds out. Other desktops, and
/// a GNOME without `gsettings`, report nothing.
#[cfg(target_os = "linux")]
fn desktop_bindings() -> HashMap<(u8, String), String> {
    let mut found = HashMap::new();
    let gnome = std::env::var("XDG_CURRENT_DESKTOP")
        .map(|value| value.to_ascii_uppercase().contains("GNOME"))
        .unwrap_or(false);
    if !gnome {
        return found;
    }
    for schema in [
        "org.gnome.desktop.wm.keybindings",
        "org.gnome.shell.keybindings",
        "org.gnome.mutter.keybindings",
        "org.gnome.mutter.wayland.keybindings",
        "org.gnome.settings-daemon.plugins.media-keys",
    ] {
        let Ok(output) = std::process::Command::new("gsettings")
            .args(["list-recursively", schema])
            .output()
        else {
            return found;
        };
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some((name, chords)) = gsettings_line(line) {
                for chord in chords {
                    found.entry(chord).or_insert_with(|| name.clone());
                }
            }
        }
    }
    found
}

/// Which of the desktop-assigned `triggers` the desktop also uses itself:
/// shortcut id → the desktop's name for what the chord does.
#[cfg(target_os = "linux")]
pub fn desktop_conflicts(triggers: &HashMap<String, String>) -> HashMap<String, String> {
    conflicts_among(triggers, &desktop_bindings())
}

#[cfg(target_os = "linux")]
fn conflicts_among(
    triggers: &HashMap<String, String>,
    system: &HashMap<(u8, String), String>,
) -> HashMap<String, String> {
    triggers
        .iter()
        .filter_map(|(id, trigger)| {
            let name = system.get(&combo(trigger)?)?;
            Some((id.clone(), name.clone()))
        })
        .collect()
}

/// Say once, at the top of the screen, that a shortcut cannot work.
#[cfg(target_os = "linux")]
pub fn report_conflicts(app: &AppHandle) {
    let rows: Vec<ShortcutRow> = state(app)
        .rows
        .into_iter()
        .filter(|row| row.conflict.is_some())
        .collect();
    let Some(first) = rows.first() else {
        return;
    };
    let text = if rows.len() == 1 {
        format!(
            "{} is your desktop's “{}”",
            first.display,
            first.conflict.as_deref().unwrap_or_default()
        )
    } else {
        format!("{} shortcuts are used by your desktop", rows.len())
    };
    crate::feedback::notify(
        app,
        crate::feedback::Note::error(text).with_detail("Change it in Settings › Shortcuts"),
    );
}

#[cfg(target_os = "linux")]
pub fn clear_portal() {
    if let Ok(mut slot) = PORTAL_TRIGGERS.lock() {
        *slot = None;
    }
    if let Ok(mut slot) = PORTAL_CONFLICTS.lock() {
        *slot = None;
    }
    for action in ACTIONS {
        set_status(action, Status::Pending);
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutRow {
    pub id: &'static str,
    pub label: &'static str,
    /// The stored chord, `Alt+Shift+V`; empty when cleared.
    pub chord: String,
    /// The chord as people read it, or the desktop's own description.
    pub display: String,
    pub default_chord: &'static str,
    pub status: Status,
    /// What the desktop itself does with this chord, when it keeps it.
    pub conflict: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutState {
    /// "windows", "x11" or "portal".
    pub backend: &'static str,
    /// Bard can rebind here; on Wayland the desktop does.
    pub editable: bool,
    pub rows: Vec<ShortcutRow>,
}

pub fn backend() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if crate::desktop::is_wayland() {
        "portal"
    } else {
        "x11"
    }
}

pub fn state(app: &AppHandle) -> ShortcutState {
    let portal = PORTAL_TRIGGERS.lock().ok().and_then(|slot| slot.clone());
    let conflicts = PORTAL_CONFLICTS.lock().ok().and_then(|slot| slot.clone());
    let rows = ACTIONS
        .iter()
        .map(|action| {
            let chord = chord_for(app, *action);
            let mut display = chord.as_ref().map(Chord::display).unwrap_or_default();
            if let Some(trigger) = portal.as_ref().and_then(|p| p.get(action.id())) {
                if !trigger.is_empty() {
                    display = trigger.clone();
                }
            }
            ShortcutRow {
                id: action.id(),
                label: action.label(),
                chord: chord.as_ref().map(Chord::canonical).unwrap_or_default(),
                display,
                default_chord: action.default_chord(),
                status: if chord.is_none() {
                    Status::Off
                } else {
                    status(*action)
                },
                conflict: conflicts
                    .as_ref()
                    .and_then(|found| found.get(action.id()).cloned()),
            }
        })
        .collect();
    ShortcutState {
        backend: backend(),
        editable: backend() != "portal",
        rows,
    }
}

// ---------------------------------------------------------------------------
// What each action does.

/// When the dictation chord went down, if that press started the take.
static DICTATE_PRESSED: Mutex<Option<Instant>> = Mutex::new(None);

/// Held at least this long, a dictation chord is push-to-talk.
pub const HOLD_TO_TALK: Duration = Duration::from_millis(350);

pub fn press(app: &AppHandle, action: Action) {
    crate::crash::record_event(&format!("shortcut:{}", action.id()));
    match action {
        Action::Main => crate::panel::toggle_now(app),
        Action::Draw => crate::commands::draw_shortcut(app),
        Action::Settings => crate::commands::settings_shortcut(app),
        Action::Guide => crate::panel::show_guide(app),
        Action::Dictate => {
            if crate::dictation::is_recording() {
                *DICTATE_PRESSED.lock().unwrap_or_else(|p| p.into_inner()) = None;
                crate::dictation::stop(app);
            } else if crate::dictation::is_busy() {
                crate::feedback::notify(
                    app,
                    crate::feedback::Note::info("info", "Still transcribing"),
                );
            } else if crate::dictation::start(app) {
                *DICTATE_PRESSED.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
            }
        }
        Action::Slot1 => crate::commands::select_slot(app, 1),
        Action::Slot2 => crate::commands::select_slot(app, 2),
        Action::Slot3 => crate::commands::select_slot(app, 3),
        Action::Cycle => crate::commands::cycle_slot(app),
        Action::Preview => {
            let preview = crate::clip::preview();
            let items = crate::db::slot_items(app);
            let active = crate::db::get_setting(app, "active_item_id")
                .and_then(|value| value.parse::<i64>().ok());
            let mut note = crate::feedback::Note::preview(preview)
                .with_rail(crate::commands::slot_rail(&items));
            note.slot = items
                .iter()
                .position(|item| Some(item.id) == active)
                .map(|index| index as u8 + 1);
            note.slots = Some(items.len() as u8);
            crate::feedback::notify(app, note);
        }
        Action::Pause => {
            crate::monitor::toggle_paused(app);
        }
    }
}

pub fn release(app: &AppHandle, action: Action) {
    if action != Action::Dictate {
        return;
    }
    let pressed = DICTATE_PRESSED
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take();
    if let Some(at) = pressed {
        if at.elapsed() >= HOLD_TO_TALK && crate::dictation::is_recording() {
            crate::dictation::stop(app);
        }
    }
}

/// Run an action off the listener's thread: a listener that blocks on a
/// window or the clipboard stops hearing the next chord.
fn spawn_press(app: &AppHandle, action: Action) {
    let app = app.clone();
    std::thread::spawn(move || press(&app, action));
}

// ---------------------------------------------------------------------------
// Starting, changing and the Escape key.

pub fn start(app: &AppHandle) {
    #[cfg(windows)]
    windows::start(app);
    #[cfg(target_os = "linux")]
    if !crate::desktop::is_wayland() && !x11::start(app) {
        crate::crash::record_event("shortcut:x11-unavailable");
    }
}

/// Re-apply every binding after the person changed one.
pub fn reload(app: &AppHandle) {
    #[cfg(windows)]
    windows::reload();
    #[cfg(target_os = "linux")]
    if !crate::desktop::is_wayland() {
        x11::apply(app);
    }
    let _ = app.emit("shortcuts-changed", ());
}

/// While a take records, Escape cancels it. Bard takes Escape only for that
/// long, and only where it can (not under the Wayland portal).
pub fn grab_escape(on: bool) {
    #[cfg(windows)]
    windows::escape(on);
    #[cfg(target_os = "linux")]
    if !crate::desktop::is_wayland() {
        x11::escape(on);
    }
    let _ = on;
}

/// Tell the person once about chords another app already holds.
fn report_taken(app: &AppHandle) {
    let taken: Vec<String> = ACTIONS
        .iter()
        .filter(|action| status(**action) == Status::Taken)
        .filter_map(|action| chord_for(app, *action).map(|c| c.display()))
        .collect();
    if taken.is_empty() {
        return;
    }
    crate::feedback::notify(
        app,
        crate::feedback::Note::error(if taken.len() == 1 {
            format!("{} is used by another app", taken[0])
        } else {
            format!("{} shortcuts are used by other apps", taken.len())
        })
        .with_detail("Change them in Settings › Shortcuts"),
    );
}

// ---------------------------------------------------------------------------
// Commands.

#[tauri::command]
pub fn get_shortcuts(app: AppHandle) -> ShortcutState {
    state(&app)
}

/// Bind `id` to `chord`, or clear it with an empty string.
#[tauri::command]
pub fn set_shortcut(app: AppHandle, id: String, chord: String) -> Result<ShortcutState, String> {
    let action = Action::from_id(&id).ok_or("Bard has no such shortcut.")?;
    let stored = if chord.trim().is_empty() {
        String::new()
    } else {
        let parsed = Chord::parse(&chord).ok_or("Bard cannot use that key.")?;
        if !parsed.is_usable() {
            return Err(
                "Add Ctrl, Alt or Super so the shortcut does not take a key from typing.".into(),
            );
        }
        if let Some((other, _)) = bindings(&app)
            .into_iter()
            .find(|(other, existing)| *other != action && *existing == parsed)
        {
            return Err(format!(
                "{} already uses {}.",
                other.label(),
                parsed.display()
            ));
        }
        parsed.canonical()
    };
    crate::db::set_setting_checked(&app, &action.setting_key(), &stored)?;
    reload(&app);
    Ok(state(&app))
}

#[tauri::command]
pub fn reset_shortcuts(app: AppHandle) -> Result<ShortcutState, String> {
    for action in ACTIONS {
        crate::db::delete_setting(&app, &action.setting_key());
    }
    reload(&app);
    Ok(state(&app))
}

// ---------------------------------------------------------------------------
// Windows.

#[cfg(windows)]
mod windows {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[repr(C)]
    struct Msg {
        hwnd: *mut std::ffi::c_void,
        message: u32,
        w_param: usize,
        l_param: isize,
        time: u32,
        point: [i32; 2],
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterHotKey(hwnd: *mut std::ffi::c_void, id: i32, modifiers: u32, vk: u32) -> i32;
        fn UnregisterHotKey(hwnd: *mut std::ffi::c_void, id: i32) -> i32;
        fn GetMessageW(msg: *mut Msg, hwnd: *mut std::ffi::c_void, min: u32, max: u32) -> i32;
        fn PeekMessageW(
            msg: *mut Msg,
            hwnd: *mut std::ffi::c_void,
            min: u32,
            max: u32,
            remove: u32,
        ) -> i32;
        fn PostThreadMessageW(thread: u32, message: u32, w_param: usize, l_param: isize) -> i32;
        fn GetAsyncKeyState(vk: i32) -> i16;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    const MOD_ALT: u32 = 0x0001;
    const MOD_CONTROL: u32 = 0x0002;
    const MOD_SHIFT: u32 = 0x0004;
    const MOD_WIN: u32 = 0x0008;
    const MOD_NOREPEAT: u32 = 0x4000;
    const WM_HOTKEY: u32 = 0x0312;
    const WM_APP: u32 = 0x8000;
    const RELOAD: u32 = WM_APP + 1;
    const ESCAPE_ON: u32 = WM_APP + 2;
    const ESCAPE_OFF: u32 = WM_APP + 3;
    const ESCAPE_ID: i32 = 0x0B00;
    const VK_ESCAPE: u32 = 0x1B;
    const PM_NOREMOVE: u32 = 0;

    static THREAD: AtomicU32 = AtomicU32::new(0);

    pub fn start(app: &AppHandle) {
        let app = app.clone();
        std::thread::Builder::new()
            .name("bard-shortcuts".into())
            .spawn(move || run(app))
            .ok();
    }

    pub fn reload() {
        post(RELOAD);
    }

    pub fn escape(on: bool) {
        post(if on { ESCAPE_ON } else { ESCAPE_OFF });
    }

    fn post(message: u32) {
        let thread = THREAD.load(Ordering::SeqCst);
        if thread != 0 {
            unsafe {
                PostThreadMessageW(thread, message, 0, 0);
            }
        }
    }

    fn modifiers(chord: &Chord) -> u32 {
        let mut flags = MOD_NOREPEAT;
        if chord.ctrl {
            flags |= MOD_CONTROL;
        }
        if chord.alt {
            flags |= MOD_ALT;
        }
        if chord.shift {
            flags |= MOD_SHIFT;
        }
        if chord.meta {
            flags |= MOD_WIN;
        }
        flags
    }

    fn apply(app: &AppHandle, registered: &mut Vec<(i32, Action, u32)>) {
        for (id, _, _) in registered.drain(..) {
            unsafe {
                UnregisterHotKey(std::ptr::null_mut(), id);
            }
        }
        for action in ACTIONS {
            set_status(action, Status::Off);
        }
        for (index, (action, chord)) in bindings(app).into_iter().enumerate() {
            let id = index as i32 + 1;
            let ok =
                unsafe { RegisterHotKey(std::ptr::null_mut(), id, modifiers(&chord), chord.vk()) }
                    != 0;
            if ok {
                registered.push((id, action, chord.vk()));
                set_status(action, Status::Ready);
            } else {
                crate::crash::record_event(&format!("shortcut:taken:{}", action.id()));
                set_status(action, Status::Taken);
            }
        }
    }

    fn run(app: AppHandle) {
        let mut message: Msg = unsafe { std::mem::zeroed() };
        // Create this thread's message queue before anyone can post to it.
        unsafe {
            PeekMessageW(
                &mut message,
                std::ptr::null_mut(),
                WM_APP,
                WM_APP,
                PM_NOREMOVE,
            );
        }
        THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
        let mut registered: Vec<(i32, Action, u32)> = Vec::new();
        apply(&app, &mut registered);
        report_taken(&app);
        loop {
            let got = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
            if got <= 0 {
                crate::crash::record_event("shortcut:loop-ended");
                return;
            }
            match message.message {
                WM_HOTKEY => {
                    let id = message.w_param as i32;
                    if id == ESCAPE_ID {
                        let app = app.clone();
                        std::thread::spawn(move || crate::dictation::cancel(&app));
                        continue;
                    }
                    let Some((_, action, vk)) = registered
                        .iter()
                        .find(|(known, _, _)| *known == id)
                        .copied()
                    else {
                        continue;
                    };
                    if action == Action::Dictate {
                        let app = app.clone();
                        std::thread::spawn(move || {
                            press(&app, action);
                            // RegisterHotKey reports the press only; the
                            // release is read from the key itself.
                            let started = Instant::now();
                            while unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0
                                && started.elapsed() < Duration::from_secs(60 * 30)
                            {
                                std::thread::sleep(Duration::from_millis(15));
                            }
                            release(&app, action);
                        });
                    } else {
                        spawn_press(&app, action);
                    }
                }
                RELOAD => apply(&app, &mut registered),
                ESCAPE_ON => unsafe {
                    RegisterHotKey(std::ptr::null_mut(), ESCAPE_ID, MOD_NOREPEAT, VK_ESCAPE);
                },
                ESCAPE_OFF => unsafe {
                    UnregisterHotKey(std::ptr::null_mut(), ESCAPE_ID);
                },
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// X11.

#[cfg(target_os = "linux")]
mod x11 {
    use super::*;
    use std::sync::Arc;

    use x11rb::connection::Connection;
    use x11rb::protocol::xkb::{self, ConnectionExt as _};
    use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, Keycode, ModMask, Window};
    use x11rb::protocol::Event;
    use x11rb::rust_connection::RustConnection;

    struct Ctl {
        conn: Arc<RustConnection>,
        root: Window,
        grabs: Mutex<Vec<(Keycode, u16, Action)>>,
        escape: Mutex<Option<Keycode>>,
    }

    static CTL: OnceLock<Ctl> = OnceLock::new();

    /// The modifiers a chord is matched on. Caps Lock and Num Lock are
    /// ignored, so a chord works whichever of them is on.
    const RELEVANT: u16 = 0x0001 | 0x0004 | 0x0008 | 0x0040; // Shift, Control, Mod1, Mod4
    const LOCK: u16 = 0x0002;
    const NUM_LOCK: u16 = 0x0010;

    fn mask(chord: &Chord) -> u16 {
        let mut mask = 0;
        if chord.shift {
            mask |= 0x0001;
        }
        if chord.ctrl {
            mask |= 0x0004;
        }
        if chord.alt {
            mask |= 0x0008;
        }
        if chord.meta {
            mask |= 0x0040;
        }
        mask
    }

    pub fn start(app: &AppHandle) -> bool {
        let Ok((conn, screen)) = x11rb::connect(None) else {
            return false;
        };
        let root = conn.setup().roots[screen].root;
        // Without this a held chord arrives as press/release pairs at the
        // repeat rate, and push-to-talk would end on the first repeat.
        let has_xkb = conn
            .xkb_use_extension(1, 0)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| r.supported);
        if has_xkb {
            let _ = conn.xkb_per_client_flags(
                xkb::ID::USE_CORE_KBD.into(),
                xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
                xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
                xkb::BoolCtrl::from(0u32),
                xkb::BoolCtrl::from(0u32),
                xkb::BoolCtrl::from(0u32),
            );
        }
        let conn = Arc::new(conn);
        if CTL
            .set(Ctl {
                conn: conn.clone(),
                root,
                grabs: Mutex::new(Vec::new()),
                escape: Mutex::new(None),
            })
            .is_err()
        {
            return true;
        }
        apply(app);
        report_taken(app);
        let app = app.clone();
        std::thread::Builder::new()
            .name("bard-shortcuts".into())
            .spawn(move || run(app, conn))
            .is_ok()
    }

    fn keycode(conn: &RustConnection, keysym: u32) -> Option<Keycode> {
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let mapping = conn
            .get_keyboard_mapping(min, max - min + 1)
            .ok()?
            .reply()
            .ok()?;
        let per = mapping.keysyms_per_keycode as usize;
        if per == 0 {
            return None;
        }
        mapping
            .keysyms
            .chunks(per)
            .position(|syms| syms.iter().take(2).any(|s| *s == keysym))
            .map(|index| min + index as u8)
    }

    fn grab(ctl: &Ctl, code: Keycode, mask: u16) -> bool {
        let mut ok = true;
        for extra in [0, LOCK, NUM_LOCK, LOCK | NUM_LOCK] {
            let granted = ctl
                .conn
                .grab_key(
                    false,
                    ctl.root,
                    ModMask::from(mask | extra),
                    code,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )
                .ok()
                .and_then(|cookie| cookie.check().ok())
                .is_some();
            ok &= granted;
        }
        ok
    }

    fn ungrab(ctl: &Ctl, code: Keycode, mask: u16) {
        for extra in [0, LOCK, NUM_LOCK, LOCK | NUM_LOCK] {
            let _ = ctl
                .conn
                .ungrab_key(code, ctl.root, ModMask::from(mask | extra));
        }
    }

    pub fn apply(app: &AppHandle) {
        let Some(ctl) = CTL.get() else {
            return;
        };
        let mut grabs = ctl.grabs.lock().unwrap_or_else(|p| p.into_inner());
        for (code, mask, _) in grabs.drain(..) {
            ungrab(ctl, code, mask);
        }
        for action in ACTIONS {
            set_status(action, Status::Off);
        }
        for (action, chord) in bindings(app) {
            let Some(code) = keycode(&ctl.conn, chord.keysym()) else {
                set_status(action, Status::Taken);
                continue;
            };
            let mask = mask(&chord);
            if grab(ctl, code, mask) {
                grabs.push((code, mask, action));
                set_status(action, Status::Ready);
            } else {
                ungrab(ctl, code, mask);
                crate::crash::record_event(&format!("shortcut:taken:{}", action.id()));
                set_status(action, Status::Taken);
            }
        }
        let _ = ctl.conn.flush();
    }

    pub fn escape(on: bool) {
        let Some(ctl) = CTL.get() else {
            return;
        };
        let mut escape = ctl.escape.lock().unwrap_or_else(|p| p.into_inner());
        if on && escape.is_none() {
            if let Some(code) = keycode(&ctl.conn, 0xff1b) {
                if grab(ctl, code, 0) {
                    *escape = Some(code);
                } else {
                    ungrab(ctl, code, 0);
                }
            }
        } else if !on {
            if let Some(code) = escape.take() {
                ungrab(ctl, code, 0);
            }
        }
        let _ = ctl.conn.flush();
    }

    fn lookup(code: Keycode, state: u16) -> Option<Action> {
        let ctl = CTL.get()?;
        let grabs = ctl.grabs.lock().ok()?;
        grabs
            .iter()
            .find(|(c, m, _)| *c == code && *m == state & RELEVANT)
            .map(|(_, _, a)| *a)
    }

    fn is_escape(code: Keycode) -> bool {
        CTL.get()
            .and_then(|ctl| ctl.escape.lock().ok().map(|e| *e == Some(code)))
            .unwrap_or(false)
    }

    fn run(app: AppHandle, conn: Arc<RustConnection>) {
        let mut held: Option<(Keycode, Action)> = None;
        loop {
            let event = match conn.wait_for_event() {
                Ok(event) => event,
                Err(_) => {
                    crate::crash::record_event("shortcut:x11-connection-lost");
                    return;
                }
            };
            match event {
                Event::KeyPress(key) => {
                    if is_escape(key.detail) {
                        let app = app.clone();
                        std::thread::spawn(move || crate::dictation::cancel(&app));
                        continue;
                    }
                    let state = u16::from(key.state);
                    if let Some(action) = lookup(key.detail, state) {
                        // Detectable auto-repeat sends repeats as presses
                        // without releases; a held chord is one press.
                        if held.is_some_and(|(code, _)| code == key.detail) {
                            continue;
                        }
                        held = Some((key.detail, action));
                        spawn_press(&app, action);
                    }
                }
                Event::KeyRelease(key) => {
                    if let Some((code, action)) = held {
                        if code == key.detail {
                            held = None;
                            let app = app.clone();
                            std::thread::spawn(move || release(&app, action));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    #[cfg(test)]
    pub mod test_support {
        use super::*;
        use x11rb::protocol::xtest::ConnectionExt as _;

        pub fn grab_and_press(text: &str) {
            let chord = Chord::parse(text).expect("chord");
            let (conn, screen) = x11rb::connect(None).expect("X server");
            let root = conn.setup().roots[screen].root;
            let ctl = Ctl {
                conn: Arc::new(conn),
                root,
                grabs: Mutex::new(Vec::new()),
                escape: Mutex::new(None),
            };
            let code = keycode(&ctl.conn, chord.keysym()).expect("keycode for the key");
            assert!(grab(&ctl, code, mask(&chord)), "the chord can be grabbed");
            ctl.conn.flush().expect("flush");
            // A second client presses the chord, as a person would.
            let (other, _) = x11rb::connect(None).expect("second client");
            let alt = keycode(&other, 0xffe9).expect("Alt_L");
            let shift = keycode(&other, 0xffe1).expect("Shift_L");
            for (kind, key) in [
                (2u8, alt),
                (2, shift),
                (2, code),
                (3, code),
                (3, shift),
                (3, alt),
            ] {
                other
                    .xtest_fake_input(kind, key, x11rb::CURRENT_TIME, root, 0, 0, 0)
                    .expect("fake input");
            }
            other.flush().expect("flush");
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                match ctl.conn.poll_for_event().expect("event") {
                    Some(Event::KeyPress(press)) if press.detail == code => {
                        assert_eq!(u16::from(press.state) & RELEVANT, mask(&chord));
                        return;
                    }
                    Some(_) => {}
                    None => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "the grab never saw the press"
                        );
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod x11_integration {
    use super::x11::test_support;

    /// Run under a real X server (CI: xvfb-run): Alt+Shift+V can be grabbed
    /// on the root window, and a synthetic press of it reaches the grab.
    #[test]
    #[ignore = "needs an X server"]
    fn x11_integration_a_grabbed_chord_is_delivered() {
        test_support::grab_and_press("Alt+Shift+V");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_and_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for action in ACTIONS {
            let chord = Chord::parse(action.default_chord())
                .unwrap_or_else(|| panic!("{}", action.default_chord()));
            assert!(chord.is_usable());
            assert_eq!(chord.canonical(), action.default_chord());
            assert!(
                seen.insert(chord),
                "duplicate default {}",
                action.default_chord()
            );
        }
    }

    #[test]
    fn chords_round_trip_and_normalise() {
        let chord = Chord::parse("shift+alt+v").unwrap();
        assert_eq!(chord.canonical(), "Alt+Shift+V");
        assert_eq!(chord.portal_trigger(), "ALT+SHIFT+v");
        assert_eq!(
            Chord::parse("Ctrl+Super+Comma").unwrap().portal_trigger(),
            "CTRL+LOGO+comma"
        );
        assert_eq!(
            Chord::parse("Alt+Shift+Space").unwrap().display(),
            "Alt+Shift+Space"
        );
        assert_eq!(
            Chord::parse("Alt+Shift+Slash").unwrap().display(),
            "Alt+Shift+/"
        );
        assert!(Chord::parse("Alt+Hyper+V").is_none());
        assert!(Chord::parse("Alt+Shift+").is_none());
    }

    #[test]
    fn a_bare_key_or_shift_alone_is_not_a_global_shortcut() {
        assert!(!Chord::parse("V").unwrap().is_usable());
        assert!(!Chord::parse("Shift+V").unwrap().is_usable());
        assert!(Chord::parse("F9").unwrap().is_usable());
        assert!(Chord::parse("Ctrl+V").unwrap().is_usable());
    }

    #[test]
    fn every_action_has_an_id_that_finds_it() {
        for action in ACTIONS {
            assert_eq!(Action::from_id(action.id()), Some(action));
        }
        assert_eq!(Action::from_id("capture"), None);
    }
}

/// The portal's answer decides which shortcuts are Ready; it does not need an
/// X server, only the trigger map the desktop sends.
#[cfg(all(test, target_os = "linux"))]
mod portal_triggers {
    use super::*;

    #[test]
    fn a_chord_is_the_same_however_it_is_spelled() {
        assert_eq!(combo("Ctrl+Alt+D"), combo("<Primary><Alt>d"));
        assert_eq!(combo("CTRL+ALT+d"), combo("<Control><Alt>d"));
        assert_eq!(combo("Shift+Alt+,"), combo("<Shift><Alt>comma"));
        assert_ne!(combo("Ctrl+Alt+D"), combo("Ctrl+Shift+D"));
        assert_eq!(combo("<Shift><Alt>"), None);
    }

    #[test]
    fn a_chord_the_desktop_keeps_is_named() {
        let (name, chords) = gsettings_line(
            "org.gnome.desktop.wm.keybindings show-desktop ['<Primary><Super>d', '<Primary><Alt>d', '<Super>d']",
        )
        .expect("a bound action");
        assert_eq!(name, "show desktop");
        assert_eq!(chords.len(), 3);
        assert!(gsettings_line("org.gnome.desktop.wm.keybindings lower @as []").is_none());

        let system: HashMap<_, _> = chords.into_iter().map(|c| (c, name.clone())).collect();
        let mut triggers = HashMap::new();
        triggers.insert("dictate".to_string(), "Ctrl+Alt+D".to_string());
        triggers.insert("main".to_string(), "Shift+Alt+V".to_string());
        let found = conflicts_among(&triggers, &system);
        assert_eq!(
            found.get("dictate").map(String::as_str),
            Some("show desktop")
        );
        assert!(!found.contains_key("main"));
    }

    #[test]
    fn only_portal_reported_shortcuts_are_ready() {
        let mut reported = HashMap::new();
        reported.insert("main".to_string(), "ALT+SHIFT+v".to_string());
        // An empty trigger is a report of nothing: treated as unreported.
        reported.insert("draw".to_string(), String::new());
        set_portal_triggers(reported, HashMap::new());

        assert_eq!(status(Action::Main), Status::Ready);
        assert_eq!(
            status(Action::Draw),
            Status::Pending,
            "an empty trigger is not a report"
        );
        assert_eq!(
            status(Action::Pause),
            Status::Pending,
            "an unreported shortcut is not bound"
        );

        // The desktop's changed event re-reports the whole table: what it
        // omits falls back to pending, never stays ready.
        set_portal_triggers(HashMap::new(), HashMap::new());
        for action in ACTIONS {
            assert_eq!(
                status(action),
                Status::Pending,
                "{} after an empty report",
                action.id()
            );
        }
    }
}
