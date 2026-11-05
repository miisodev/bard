//! Wayland integration through consent-based desktop portals. No raw keyboard
//! device access, compositor-specific helpers or shell commands are used.
use ashpd::desktop::{
    global_shortcuts::{GlobalShortcuts, NewShortcut},
    remote_desktop::{DeviceType, KeyState, RemoteDesktop, SelectDevicesOptions},
};
use futures_util::StreamExt;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

static SHORTCUTS_READY: AtomicBool = AtomicBool::new(false);
static CONFIGURING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static INPUT: Mutex<Option<tokio::sync::mpsc::Sender<InputRequest>>> = Mutex::new(None);

pub fn shortcuts_ready() -> bool {
    SHORTCUTS_READY.load(Ordering::Acquire)
}
pub fn input_ready() -> bool {
    INPUT
        .lock()
        .map(|v| v.as_ref().is_some_and(|s| !s.is_closed()))
        .unwrap_or(false)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    Shelf,
    Capture,
    Dictate,
    Draw,
    Settings,
    Cancel,
}
fn action(id: &str) -> Option<Action> {
    match id {
        "shelf" => Some(Action::Shelf),
        "capture" => Some(Action::Capture),
        "dictate" => Some(Action::Dictate),
        "draw" => Some(Action::Draw),
        "settings" => Some(Action::Settings),
        "cancel" => Some(Action::Cancel),
        _ => None,
    }
}

fn dispatch(app: &AppHandle, action: Action) {
    match action {
        Action::Shelf => crate::panel::toggle_now(app),
        Action::Capture => crate::capture::capture_selection(app),
        Action::Draw => crate::commands::open_draw(app.clone()),
        Action::Settings => crate::commands::open_settings(app.clone()),
        Action::Cancel => crate::dictation::cancel(app),
        Action::Dictate => {
            if crate::dictation::is_recording() {
                crate::dictation::stop(app);
            } else {
                let _ = crate::dictation::start(app);
            }
        }
    }
}

pub async fn configure_shortcuts(app: AppHandle) -> Result<(), String> {
    let _guard = CONFIGURING.lock().await;
    if shortcuts_ready() {
        return Ok(());
    }
    let proxy = GlobalShortcuts::new().await.map_err(|_| "This desktop does not provide global shortcuts. You can still open Bard from the application launcher or tray.".to_string())?;
    let session = proxy
        .create_session(Default::default())
        .await
        .map_err(|e| e.to_string())?;
    // Subscribe before binding: the first activation must not fall into the
    // gap between the consent response and installing the signal listener.
    let mut activated = proxy.receive_activated().await.map_err(|e| e.to_string())?;
    let shortcuts = [
        NewShortcut::new("shelf", "Show or hide Bard").preferred_trigger("CTRL+ALT+b"),
        NewShortcut::new("capture", "Capture the current selection")
            .preferred_trigger("CTRL+ALT+c"),
        NewShortcut::new("dictate", "Start or stop dictation").preferred_trigger("CTRL+ALT+d"),
        NewShortcut::new("draw", "Open Bard Draw").preferred_trigger("CTRL+ALT+n"),
        NewShortcut::new("settings", "Open Bard Settings").preferred_trigger("CTRL+ALT+comma"),
        NewShortcut::new("cancel", "Cancel dictation").preferred_trigger("CTRL+ALT+Escape"),
    ];
    proxy
        .bind_shortcuts(&session, &shortcuts, None, Default::default())
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|_| {
            "Shortcuts were not enabled. You can try again when you are ready.".to_string()
        })?;
    let own_session = serde_json::to_value(&session).map_err(|e| e.to_string())?;
    SHORTCUTS_READY.store(true, Ordering::Release);
    crate::db::set_setting(&app, "desktop_shortcuts_enabled", "1");
    crate::commands::refresh(&app);
    tauri::async_runtime::spawn(async move {
        // Keep both the proxy and session alive until permission is revoked.
        let _proxy = proxy;
        if let Ok(mut closed) = session.receive_closed().await {
            loop {
                tokio::select! {
                    event = activated.next() => {
                        let Some(event) = event else { break; };
                        if Some(event.session_handle().as_str()) != own_session.as_str() { continue; }
                        if let Some(action) = action(event.shortcut_id()) {
                            let app = app.clone();
                            // Input/capture work must never block the signal reader.
                            tauri::async_runtime::spawn_blocking(move || dispatch(&app, action));
                        }
                    }
                    _ = closed.next() => break,
                }
            }
        }
        SHORTCUTS_READY.store(false, Ordering::Release);
        crate::commands::refresh(&app);
        let _ = app.emit_to("main", "desktop-integration-changed", ());
    });
    Ok(())
}

struct InputRequest {
    key: i32,
    deadline: Instant,
    reply: std::sync::mpsc::Sender<Result<(), String>>,
}

/// Request only keyboard emulation. No screen, pointer or audio is shared.
/// The desktop presents its own permission dialog; declining is recoverable.
pub async fn configure_input(app: AppHandle) -> Result<(), String> {
    let _guard = CONFIGURING.lock().await;
    if input_ready() {
        return Ok(());
    }
    let proxy = RemoteDesktop::new().await.map_err(|_| "This desktop does not provide keyboard access for paste-back. Your copied items remain available for manual paste.".to_string())?;
    let session = proxy
        .create_session(Default::default())
        .await
        .map_err(|e| e.to_string())?;
    proxy
        .select_devices(
            &session,
            SelectDevicesOptions::default().set_devices(Some(DeviceType::Keyboard.into())),
        )
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|e| e.to_string())?;
    let result = proxy
        .start(&session, None, Default::default())
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|_| {
            "Keyboard access was not enabled. You can still copy items and paste them yourself."
                .to_string()
        })?;
    if !result.devices().contains(DeviceType::Keyboard) {
        return Err(
            "Keyboard access was not granted. Copy still works; paste into your app with Ctrl+V."
                .into(),
        );
    }
    let (sender, mut receiver) = tokio::sync::mpsc::channel::<InputRequest>(8);
    *INPUT
        .lock()
        .map_err(|_| "Keyboard access could not start.".to_string())? = Some(sender);
    crate::commands::refresh(&app);
    tauri::async_runtime::spawn(async move {
        if let Ok(mut closed) = session.receive_closed().await {
            loop {
                tokio::select! {
                    request = receiver.recv() => {
                        let Some(request) = request else { break; };
                        if Instant::now() > request.deadline { continue; }
                        let result = chord(&proxy, &session, request.key).await.map_err(|_| "Bard could not send the shortcut. Your text is on the clipboard; paste it with Ctrl+V.".to_string());
                        let _ = request.reply.send(result);
                    }
                    _ = closed.next() => break,
                }
            }
        }
        if let Ok(mut input) = INPUT.lock() {
            *input = None;
        }
        crate::commands::refresh(&app);
    });
    Ok(())
}

async fn chord(
    proxy: &RemoteDesktop,
    session: &ashpd::desktop::Session<RemoteDesktop>,
    key: i32,
) -> ashpd::Result<()> {
    const CONTROL_L: i32 = 0xffe3;
    proxy
        .notify_keyboard_keysym(session, CONTROL_L, KeyState::Pressed, Default::default())
        .await?;
    let press = proxy
        .notify_keyboard_keysym(session, key, KeyState::Pressed, Default::default())
        .await;
    // Release both keys even when a portal call failed: a partial chord must
    // never leave Ctrl pressed in the user's next application.
    let release = proxy
        .notify_keyboard_keysym(session, key, KeyState::Released, Default::default())
        .await;
    let control = proxy
        .notify_keyboard_keysym(session, CONTROL_L, KeyState::Released, Default::default())
        .await;
    press?;
    release?;
    control
}

/// Called only from capture/paste workers. A bounded queue and deadline prevent
/// delayed input being injected into an unrelated application later.
pub fn send_chord(key: char) -> Result<(), String> {
    let sender = INPUT
        .lock()
        .ok()
        .and_then(|v| v.clone())
        .ok_or("Enable keyboard access in Settings › Shortcuts, or paste with Ctrl+V.")?;
    let (reply, result) = std::sync::mpsc::channel();
    sender
        .try_send(InputRequest {
            key: key as i32,
            deadline: Instant::now() + Duration::from_secs(1),
            reply,
        })
        .map_err(|_| "Keyboard access is busy. Try again.".to_string())?;
    result.recv_timeout(Duration::from_secs(2)).map_err(|_| {
        "Keyboard access did not respond. Your copied content is still available.".to_string()
    })?
}

pub async fn set_autostart(enabled: bool) -> Result<(), String> {
    let response = ashpd::desktop::background::Background::request()
        .reason("Keep Bard ready to capture and dictate when you sign in")
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_declared_shortcuts_can_dispatch_actions() {
        assert_eq!(action("shelf"), Some(Action::Shelf));
        assert_eq!(action("capture"), Some(Action::Capture));
        assert_eq!(action("dictate"), Some(Action::Dictate));
        assert_eq!(action("quit"), None);
        assert_eq!(action(""), None);
    }
}
