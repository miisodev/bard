//! Wayland integration through consent-based desktop portals: global
//! shortcuts and start-at-login. No raw keyboard device access, no keyboard
//! injection, no compositor-specific helpers or shell commands.
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

pub fn shortcuts_ready() -> bool {
    SHORTCUTS_READY.load(Ordering::Acquire)
}

fn triggers(shortcuts: &[Shortcut]) -> HashMap<String, String> {
    shortcuts
        .iter()
        .map(|s| (s.id().to_string(), s.trigger_description().to_string()))
        .collect()
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
    let requested: Vec<NewShortcut> = shortcuts::ACTIONS
        .iter()
        .map(|action| {
            let trigger = shortcuts::chord_for(&app, *action).map(|c| c.portal_trigger());
            NewShortcut::new(action.id(), action.label()).preferred_trigger(trigger.as_deref())
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
    shortcuts::set_portal_triggers(triggers(bound.shortcuts()));
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
                    if let Some(action) = Action::from_id(event.shortcut_id()) {
                        let app = app.clone();
                        // Window and clipboard work must never block the signal reader.
                        tauri::async_runtime::spawn_blocking(move || shortcuts::press(&app, action));
                    }
                }
                event = deactivated.next() => {
                    let Some(event) = event else { break; };
                    if !mine(event.session_handle().as_str()) { continue; }
                    if let Some(action) = Action::from_id(event.shortcut_id()) {
                        let app = app.clone();
                        tauri::async_runtime::spawn_blocking(move || shortcuts::release(&app, action));
                    }
                }
                event = changed.next() => {
                    let Some(event) = event else { break; };
                    if !mine(event.session_handle().as_str()) { continue; }
                    shortcuts::set_portal_triggers(triggers(event.shortcuts()));
                    let _ = app.emit("shortcuts-changed", ());
                }
                _ = closed.next() => break,
            }
        }
        SHORTCUTS_READY.store(false, Ordering::Release);
        *LIVE.lock().await = None;
        shortcuts::clear_portal();
        let _ = app.emit("shortcuts-changed", ());
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
