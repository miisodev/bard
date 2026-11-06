//! The foreground window, in one place.
//!
//! Three features need it and they must agree: the panel remembers the window
//! it was summoned over so hiding can give the foreground back, dictation
//! remembers the window a take started in so a confirmed review can give the
//! foreground back before the paste chord, and Draw remembers the window a
//! sketch is made over so a send can bring it forward before its own paste.
//! The helpers used to live inside `panel.rs`, where dictation could not reach
//! them without a copy — and a second copy of a Win32 foreground query is
//! exactly the kind of thing that drifts.

/// The window the person is working in right now, or 0 when there is none.
pub fn current() -> isize {
    native::current()
}

/// Bring `hwnd` back to the foreground. 0 — nothing was remembered — is
/// deliberately ignored: `SetForegroundWindow(0)` cannot do anything but
/// distract from the caller's own decision about what to do next.
pub fn focus(hwnd: isize) {
    if hwnd != 0 {
        native::focus(hwnd);
    }
}

/// Does this window belong to us?
///
/// The window-specific form of [`foreground_is_ours`], for a handle that has
/// been remembered rather than queried: Draw carries the window it was opened
/// over for as long as a sketch is made, and by delivery time it must not be
/// one of Bard's own.
pub fn is_ours(hwnd: isize) -> bool {
    native::is_ours(hwnd)
}

/// Does the foreground window belong to us?
///
/// The obvious version of this asks each of our own windows `is_focused()`,
/// and it does not work: queried during a focus transition it answers about a
/// window that is merely the most recently active, so a foreign window taking
/// focus reads as "one of ours is focused" and the panel never hides. Asking
/// the OS which process owns the foreground window has no such ambiguity, and
/// it covers every window we might grow later without a list to maintain.
pub fn foreground_is_ours() -> bool {
    native::foreground_is_ours()
}

#[cfg(windows)]
mod native {
    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> isize;
        fn SetForegroundWindow(hwnd: isize) -> i32;
        fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    }

    pub fn current() -> isize {
        unsafe { GetForegroundWindow() }
    }

    pub fn focus(hwnd: isize) {
        unsafe {
            SetForegroundWindow(hwnd);
        }
    }

    pub fn is_ours(hwnd: isize) -> bool {
        if hwnd == 0 {
            return false;
        }
        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        pid == std::process::id()
    }

    pub fn foreground_is_ours() -> bool {
        is_ours(current())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use x11rb::{
        connection::Connection,
        protocol::xproto::{AtomEnum, ConnectionExt, CreateWindowAux, WindowClass},
        wrapper::ConnectionExt as _,
        COPY_DEPTH_FROM_PARENT, CURRENT_TIME,
    };

    #[test]
    #[ignore = "requires an isolated X11 display; run under xvfb-run"]
    fn x11_foreground_tracks_and_focuses_real_windows() {
        let (connection, screen) = x11rb::connect(None).unwrap();
        let root = connection.setup().roots[screen].root;
        let window = connection.generate_id().unwrap();
        connection
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                window,
                root,
                0,
                0,
                100,
                80,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap()
            .check()
            .unwrap();
        connection.map_window(window).unwrap().check().unwrap();
        let active = connection
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        let pid = connection
            .intern_atom(false, b"_NET_WM_PID")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        connection
            .change_property32(
                x11rb::protocol::xproto::PropMode::REPLACE,
                root,
                active,
                AtomEnum::WINDOW,
                &[window],
            )
            .unwrap()
            .check()
            .unwrap();
        connection
            .change_property32(
                x11rb::protocol::xproto::PropMode::REPLACE,
                window,
                pid,
                AtomEnum::CARDINAL,
                &[std::process::id()],
            )
            .unwrap()
            .check()
            .unwrap();
        connection.flush().unwrap();
        assert_eq!(current(), window as isize);
        assert!(is_ours(window as isize));
        assert!(foreground_is_ours());
        connection
            .set_input_focus(
                x11rb::protocol::xproto::InputFocus::POINTER_ROOT,
                root,
                CURRENT_TIME,
            )
            .unwrap()
            .check()
            .unwrap();
        focus(window as isize);
        assert_eq!(
            connection.get_input_focus().unwrap().reply().unwrap().focus,
            window
        );
        assert!(!is_ours(0));
    }
}

#[cfg(target_os = "linux")]
mod native {
    use x11rb::{
        connection::Connection,
        protocol::xproto::{
            AtomEnum, ClientMessageData, ClientMessageEvent, ConfigureWindowAux, ConnectionExt,
            EventMask, InputFocus, StackMode,
        },
        rust_connection::RustConnection,
        CURRENT_TIME,
    };

    fn connection() -> Option<(RustConnection, usize)> {
        // Xwayland cannot identify or activate a native Wayland window.
        if crate::desktop::is_wayland() {
            return None;
        }
        x11rb::connect(None).ok()
    }

    fn property(
        connection: &RustConnection,
        window: u32,
        name: &[u8],
        kind: AtomEnum,
    ) -> Option<u32> {
        let atom = connection.intern_atom(false, name).ok()?.reply().ok()?.atom;
        let reply = connection
            .get_property(false, window, atom, kind, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        let value = reply.value32()?.next();
        value
    }

    pub fn current() -> isize {
        let Some((connection, screen)) = connection() else {
            return 0;
        };
        property(
            &connection,
            connection.setup().roots[screen].root,
            b"_NET_ACTIVE_WINDOW",
            AtomEnum::WINDOW,
        )
        .unwrap_or(0) as isize
    }

    pub fn focus(hwnd: isize) {
        let Ok(window) = u32::try_from(hwnd) else {
            return;
        };
        let Some((connection, screen)) = connection() else {
            return;
        };
        let root = connection.setup().roots[screen].root;
        if let Ok(cookie) = connection.intern_atom(false, b"_NET_ACTIVE_WINDOW") {
            if let Ok(atom) = cookie.reply() {
                let event = ClientMessageEvent::new(
                    32,
                    window,
                    atom.atom,
                    ClientMessageData::from([1, CURRENT_TIME, 0, 0, 0]),
                );
                if let Ok(cookie) = connection.send_event(
                    false,
                    root,
                    EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                    event,
                ) {
                    let _ = cookie.check();
                }
            }
        }
        // Also works without an EWMH window manager. Checked requests make a
        // target closed during handoff a normal failure, never an Xlib abort.
        if let Ok(cookie) = connection.configure_window(
            window,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        ) {
            let _ = cookie.check();
        }
        if let Ok(cookie) = connection.set_input_focus(InputFocus::PARENT, window, CURRENT_TIME) {
            let _ = cookie.check();
        }
        let _ = connection.flush();
    }

    pub fn is_ours(hwnd: isize) -> bool {
        let Ok(window) = u32::try_from(hwnd) else {
            return false;
        };
        if window == 0 {
            return false;
        }
        let Some((connection, _)) = connection() else {
            return false;
        };
        property(&connection, window, b"_NET_WM_PID", AtomEnum::CARDINAL)
            == Some(std::process::id())
    }

    pub fn foreground_is_ours() -> bool {
        is_ours(current())
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod native {
    pub fn current() -> isize {
        0
    }
    pub fn focus(_hwnd: isize) {}
    pub fn is_ours(_hwnd: isize) -> bool {
        false
    }
    pub fn foreground_is_ours() -> bool {
        false
    }
}
