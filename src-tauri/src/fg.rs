//! The foreground window, in one place.
//!
//! Two features need it and they must agree: the panel remembers the window it
//! was summoned over so hiding can give the foreground back, and dictation
//! remembers the window a take started in so a confirmed review can give the
//! foreground back before the paste chord. The helpers used to live inside
//! `panel.rs`, where dictation could not reach them without a copy — and a
//! second copy of a Win32 foreground query is exactly the kind of thing that
//! drifts.

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

    pub fn foreground_is_ours() -> bool {
        let hwnd = current();
        if hwnd == 0 {
            return false;
        }
        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        pid == std::process::id()
    }
}

#[cfg(not(windows))]
mod native {
    pub fn current() -> isize {
        0
    }
    pub fn focus(_hwnd: isize) {}
    pub fn foreground_is_ours() -> bool {
        false
    }
}
