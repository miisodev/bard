//! Bringing a summoned window to the front.
//!
//! Bard's windows are asked for from the tray, a global shortcut or another
//! Bard window, and a summoned window that opens behind the app in use has
//! not really opened. Asking the toolkit for focus is not enough on Linux:
//! the request carries the time of Bard's last input, which is older than
//! whatever the person just did elsewhere, so the window manager treats it
//! as focus stealing and leaves the window underneath. The window manager's
//! own activation message, sent the way a taskbar sends it, is honoured.

use tauri::WebviewWindow;

/// Focus `win` and put it above the other applications' windows. Call it
/// after showing the window.
pub fn bring(win: &WebviewWindow) {
    let _ = win.set_focus();
    #[cfg(target_os = "linux")]
    x11::activate_soon(win);
}

/// Take `win` out of the window manager's hands, as a tooltip or a
/// notification is: it is not focused, placed, animated, listed or tiled, and
/// compositor effects meant for application windows leave it alone. Call it
/// before the window is first shown.
///
/// The status pill is drawn in a transparent window larger than itself. As an
/// ordinary window, a desktop that blurs or tints what lies behind every
/// application window did so behind the whole transparent rectangle, which
/// showed as a large rounded square behind the pill.
pub fn unmanage(win: &WebviewWindow) {
    #[cfg(target_os = "linux")]
    x11::unmanage(win);
    #[cfg(not(target_os = "linux"))]
    let _ = win;
}

#[cfg(target_os = "linux")]
mod x11 {
    use std::time::Duration;

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use tauri::WebviewWindow;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt as _, EventMask};

    /// The X window behind `win`, when Bard is running on X11 or XWayland.
    pub(crate) fn xid(win: &WebviewWindow) -> Option<u32> {
        match win.window_handle().ok()?.as_raw() {
            RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok(),
            RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
            _ => None,
        }
    }

    /// `_NET_ACTIVE_WINDOW` from a pager (source 2), which window managers
    /// obey without weighing it against the last input elsewhere.
    fn activate(window: u32) -> Option<()> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?.root;
        let atom = conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let event = ClientMessageEvent::new(32, window, atom, [2u32, 0, 0, 0, 0]);
        conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )
        .ok()?;
        conn.flush().ok()
    }

    pub fn unmanage(win: &WebviewWindow) -> Option<()> {
        use x11rb::protocol::xproto::{AtomEnum, ChangeWindowAttributesAux, PropMode};
        use x11rb::wrapper::ConnectionExt as _;
        let window = xid(win)?;
        let (conn, _) = x11rb::connect(None).ok()?;
        let atom = |name: &[u8]| Some(conn.intern_atom(false, name).ok()?.reply().ok()?.atom);
        let kind = atom(b"_NET_WM_WINDOW_TYPE")?;
        let notification = atom(b"_NET_WM_WINDOW_TYPE_NOTIFICATION")?;
        conn.change_window_attributes(
            window,
            &ChangeWindowAttributesAux::new().override_redirect(1),
        )
        .ok()?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            kind,
            AtomEnum::ATOM,
            &[notification],
        )
        .ok()?;
        conn.flush().ok()
    }

    /// A window that was hidden is only mapped once the event loop gets to
    /// it, and a message about an unmapped window is ignored. Ask once it
    /// has had time to map, and once more if that was still too early.
    pub fn activate_soon(win: &WebviewWindow) {
        let Some(window) = xid(win) else {
            return;
        };
        let win = win.clone();
        std::thread::spawn(move || {
            for wait in [90, 220] {
                std::thread::sleep(Duration::from_millis(wait));
                if !win.is_visible().unwrap_or(false) || win.is_focused().unwrap_or(false) {
                    return;
                }
                let _ = activate(window);
            }
        });
    }
}
