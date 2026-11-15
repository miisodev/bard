//! Wayland integration through consent-based desktop portals: global
//! shortcuts and start-at-login. No raw keyboard device access, no keyboard
//! injection, no compositor-specific helpers. The one thing read outside the
//! portal is GNOME's own key bindings, to say when it kept a chord it
//! granted (see `shortcuts::desktop_conflicts`).
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut, Shortcut};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

use crate::shortcuts::{self, Action};

static SHORTCUTS_READY: AtomicBool = AtomicBool::new(false);
static CONFIGURING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// The live portal and session, kept to open the desktop's own shortcut
/// editor later.
type Live = std::sync::Arc<(GlobalShortcuts, ashpd::desktop::Session<GlobalShortcuts>)>;
static LIVE: tokio::sync::Mutex<Option<Live>> = tokio::sync::Mutex::const_new(None);

/// Set when the person turned the desktop's request down, so Bard does not
/// ask again on every launch. Settings › Shortcuts can ask again.
pub const DECLINED_SETTING: &str = "portal_shortcuts_declined";
/// The version of Bard's default keys the desktop was last asked for.
const KEYS_SETTING: &str = "portal_keys";
const KEYS_VERSION: &str = "4";

pub fn shortcuts_ready() -> bool {
    SHORTCUTS_READY.load(Ordering::Acquire)
}

/// What the desktop bound, keyed by Bard's action id.
fn triggers(shortcuts: &[Shortcut]) -> HashMap<String, String> {
    shortcuts
        .iter()
        .map(|s| {
            (
                action_of(s.id()).to_string(),
                s.trigger_description().to_string(),
            )
        })
        .collect()
}

/// The id Bard asks the desktop to bind an action under.
///
/// A desktop remembers the keys it gave an id and ignores a new suggestion
/// for it, so a chord changed in Bard could never reach the desktop under the
/// same id. When Bard wants keys the desktop does not have for an action, the
/// action is asked for under an id that carries the chord (`dictate.alt-shift-
/// space`): new to the desktop, so it offers Bard's keys for the person to
/// confirm. The id in use is remembered so later launches ask for the same
/// one and are not asked anything.
fn portal_id(app: &AppHandle, action: Action) -> String {
    crate::db::get_setting(app, &format!("portal_id:{}", action.id()))
        .filter(|id| action_of(id) == action.id())
        .unwrap_or_else(|| action.id().to_string())
}

fn action_of(portal_id: &str) -> &str {
    portal_id.split('.').next().unwrap_or(portal_id)
}

/// Point every action whose desktop keys differ from Bard's at a new id, so
/// the next bind asks the desktop for Bard's keys. Returns whether any moved.
fn adopt_new_ids(app: &AppHandle) -> bool {
    let state = shortcuts::state(app);
    let mut moved = false;
    for row in &state.rows {
        let Some(action) = Action::from_id(row.id) else {
            continue;
        };
        if row.chord.is_empty() || shortcuts::same_chord(&row.chord, &row.display) {
            continue;
        }
        let id = format!(
            "{}.{}",
            action.id(),
            row.chord.to_ascii_lowercase().replace('+', "-")
        );
        if portal_id(app, action) != id {
            crate::db::set_setting(app, &format!("portal_id:{}", action.id()), &id);
            moved = true;
        }
    }
    moved
}

/// Connect the shortcuts at launch.
///
/// A desktop that met an earlier Bard still holds that version's keys. Once,
/// it is asked for this version's: the desktop shows them for the person to
/// confirm, and whatever they choose is left alone from then on.
pub async fn connect_at_launch(app: AppHandle) -> Result<(), String> {
    configure_shortcuts(app.clone()).await?;
    if crate::db::get_setting(&app, KEYS_SETTING).as_deref() != Some(KEYS_VERSION) {
        crate::db::set_setting(&app, KEYS_SETTING, KEYS_VERSION);
        if adopt_new_ids(&app) {
            return rebind(app).await;
        }
    }
    Ok(())
}

/// Ask the desktop again after the table changed in Bard: the old session is
/// closed and a new one binds the table as it now stands.
pub async fn rebind(app: AppHandle) -> Result<(), String> {
    adopt_new_ids(&app);
    if let Some(live) = LIVE.lock().await.take() {
        let _ = live.1.close().await;
    }
    SHORTCUTS_READY.store(false, Ordering::Release);
    configure_shortcuts(app).await
}

/// Record what the desktop bound, and which of those chords it also keeps
/// for itself.
async fn publish(triggers: HashMap<String, String>) {
    let asked = triggers.clone();
    let conflicts =
        tauri::async_runtime::spawn_blocking(move || shortcuts::desktop_conflicts(&asked))
            .await
            .unwrap_or_default();
    shortcuts::set_portal_triggers(triggers, conflicts);
}

pub async fn configure_shortcuts(app: AppHandle) -> Result<(), String> {
    let _guard = CONFIGURING.lock().await;
    if shortcuts_ready() {
        return Ok(());
    }
    let proxy = GlobalShortcuts::new().await.map_err(|_| {
        "This desktop does not offer global shortcuts. Open Bard from the tray or the app menu."
            .to_string()
    })?;
    let session = proxy
        .create_session(Default::default())
        .await
        .map_err(|e| e.to_string())?;
    // Subscribe before binding: the first press must not fall into the gap
    // between the consent response and installing the signal listener.
    let mut activated = proxy.receive_activated().await.map_err(|e| e.to_string())?;
    let mut deactivated = proxy
        .receive_deactivated()
        .await
        .map_err(|e| e.to_string())?;
    let mut changed = proxy
        .receive_shortcuts_changed()
        .await
        .map_err(|e| e.to_string())?;
    // A shortcut the person turned off is not asked for at all.
    let requested: Vec<NewShortcut> = shortcuts::ACTIONS
        .iter()
        .filter_map(|action| {
            let trigger = shortcuts::chord_for(&app, *action)?.portal_trigger();
            Some(
                NewShortcut::new(portal_id(&app, *action), action.label())
                    .preferred_trigger(Some(trigger.as_str())),
            )
        })
        .collect();
    let bound = proxy
        .bind_shortcuts(&session, &requested, None, Default::default())
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|_| {
            crate::db::set_setting(&app, DECLINED_SETTING, "1");
            "Shortcuts were not enabled. You can allow them in Settings › Shortcuts.".to_string()
        })?;
    publish(triggers(bound.shortcuts())).await;
    shortcuts::report_conflicts(&app);
    crate::db::set_setting(&app, DECLINED_SETTING, "0");
    let own_session = serde_json::to_value(&session).map_err(|e| e.to_string())?;
    SHORTCUTS_READY.store(true, Ordering::Release);
    let _ = app.emit("shortcuts-changed", ());
    let live = std::sync::Arc::new((proxy, session));
    *LIVE.lock().await = Some(live.clone());
    tauri::async_runtime::spawn(async move {
        let Ok(mut closed) = live.1.receive_closed().await else {
            return;
        };
        let mine = |handle: &str| Some(handle) == own_session.as_str();
        loop {
            tokio::select! {
                event = activated.next() => {
                    let Some(event) = event else { break; };
                    if !mine(event.session_handle().as_str()) { continue; }
                    if let Some(action) = Action::from_id(action_of(event.shortcut_id())) {
                        let app = app.clone();
                        // Window and clipboard work must never block the signal reader.
                        tauri::async_runtime::spawn_blocking(move || shortcuts::press(&app, action));
                    }
                }
                event = deactivated.next() => {
                    let Some(event) = event else { break; };
                    if !mine(event.session_handle().as_str()) { continue; }
                    if let Some(action) = Action::from_id(action_of(event.shortcut_id())) {
                        let app = app.clone();
                        tauri::async_runtime::spawn_blocking(move || shortcuts::release(&app, action));
                    }
                }
                event = changed.next() => {
                    let Some(event) = event else { break; };
                    if !mine(event.session_handle().as_str()) { continue; }
                    publish(triggers(event.shortcuts())).await;
                    let _ = app.emit("shortcuts-changed", ());
                }
                _ = closed.next() => break,
            }
        }
        // Only the session that is still the live one clears the state: after
        // a rebind this loop belongs to the closed session, and the new one's
        // triggers are already in place.
        let mut slot = LIVE.lock().await;
        if slot
            .as_ref()
            .is_some_and(|current| std::sync::Arc::ptr_eq(current, &live))
        {
            *slot = None;
            SHORTCUTS_READY.store(false, Ordering::Release);
            shortcuts::clear_portal();
            let _ = app.emit("shortcuts-changed", ());
        }
    });
    Ok(())
}

/// Open the desktop's own editor for Bard's shortcuts (portal version 2).
pub async fn edit_shortcuts(app: AppHandle) -> Result<(), String> {
    if !shortcuts_ready() {
        return configure_shortcuts(app).await;
    }
    let Some(live) = LIVE.lock().await.clone() else {
        return Err("Shortcuts are not connected yet.".into());
    };
    let (proxy, session) = (&live.0, &live.1);
    if proxy.version() < 2 {
        return Err(
            "This desktop changes app shortcuts in its own Settings, under Keyboard.".into(),
        );
    }
    proxy
        .configure_shortcuts(session, None, Default::default())
        .await
        .map_err(|e| e.to_string())
}

pub async fn set_autostart(enabled: bool) -> Result<(), String> {
    let response = ashpd::desktop::background::Background::request()
        .reason("Keep Bard ready to keep what you copy and to dictate when you sign in")
        .auto_start(enabled)
        .command(["bard", "--autostart"])
        .dbus_activatable(false)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|e| e.to_string())?;
    if response.auto_start() != enabled {
        return Err("The desktop did not allow this startup change.".into());
    }
    Ok(())
}
