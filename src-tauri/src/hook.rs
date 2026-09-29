//! Bard's own low-level keyboard and mouse hooks.
//!
//! This replaces `rdev::listen`, which did two things on every key press, in
//! every application, inside the hook callback:
//!
//! - `AttachThreadInput` to the foreground window's thread, to read its keyboard
//!   state. Attaching merges the two threads' input state — focus, active window
//!   and **caret** — for the duration, which is what made Notepad's caret jump
//!   and flicker while Bard was running.
//! - `ToUnicodeEx` to name the key. That call consumes a pending dead key from
//!   the kernel's keyboard state, so accented characters on international
//!   layouts could come out wrong in whatever app was being typed into.
//!
//! Bard only needs to know which physical key went up or down, which the hook's
//! `KBDLLHOOKSTRUCT` already says. Nothing here reads, translates or attaches to
//! anything, and the callbacks do no work beyond forwarding a small event.
//!
//! `rdev` also discarded `GetMessage`'s return value and reported a hook that
//! ended as `Ok(())`. [`listen`] returns why its message loop ended, so the
//! re-arming worker in `capture.rs` can record it.

use rdev::{Button, Event, EventType, Key};

/// Why [`listen`] returned. It only returns when something went wrong.
#[derive(Debug)]
pub enum ListenEnd {
    /// `SetWindowsHookExW` refused a hook; the value is `GetLastError`.
    HookRefused(&'static str, u32),
    /// `GetMessageW` failed; the value is `GetLastError`.
    MessageLoopFailed(u32),
    /// The thread received `WM_QUIT`.
    Quit,
}

impl std::fmt::Display for ListenEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HookRefused(which, code) => write!(f, "{which}-hook-refused:{code}"),
            Self::MessageLoopFailed(code) => write!(f, "message-loop-failed:{code}"),
            Self::Quit => f.write_str("quit"),
        }
    }
}

/// Maps a virtual-key code from a low-level hook to the key Bard reasons about.
/// Low-level hooks always report the side-specific codes for Shift, Control and
/// Alt, so there is no generic-modifier case to resolve. Every other key is
/// `Unknown(vk)`: distinct per key, which is all the coordinator needs to pair a
/// press with its release.
pub fn key_from_vk(vk: u32) -> Key {
    match vk {
        0xA0 => Key::ShiftLeft,
        0xA1 => Key::ShiftRight,
        0xA2 => Key::ControlLeft,
        0xA3 => Key::ControlRight,
        0xA4 => Key::Alt,
        0xA5 => Key::AltGr,
        0x1B => Key::Escape,
        0x31 => Key::Num1,
        0x32 => Key::Num2,
        0x33 => Key::Num3,
        0x61 => Key::Kp1,
        0x62 => Key::Kp2,
        0x63 => Key::Kp3,
        other => Key::Unknown(other),
    }
}

/// Translates one low-level mouse message. Movement is dropped here: nothing in
/// Bard uses it, and forwarding it meant a channel send per pixel of travel.
pub fn mouse_event(message: u32, mouse_data: u32) -> Option<EventType> {
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_RBUTTONDOWN: u32 = 0x0204;
    const WM_RBUTTONUP: u32 = 0x0205;
    const WM_MBUTTONDOWN: u32 = 0x0207;
    const WM_MBUTTONUP: u32 = 0x0208;
    const WM_MOUSEWHEEL: u32 = 0x020A;
    const WM_XBUTTONDOWN: u32 = 0x020B;
    const WM_XBUTTONUP: u32 = 0x020C;
    const WM_MOUSEHWHEEL: u32 = 0x020E;
    let x_button = || Button::Unknown((mouse_data >> 16) as u8);
    match message {
        WM_LBUTTONDOWN => Some(EventType::ButtonPress(Button::Left)),
        WM_LBUTTONUP => Some(EventType::ButtonRelease(Button::Left)),
        WM_RBUTTONDOWN => Some(EventType::ButtonPress(Button::Right)),
        WM_RBUTTONUP => Some(EventType::ButtonRelease(Button::Right)),
        WM_MBUTTONDOWN => Some(EventType::ButtonPress(Button::Middle)),
        WM_MBUTTONUP => Some(EventType::ButtonRelease(Button::Middle)),
        WM_XBUTTONDOWN => Some(EventType::ButtonPress(x_button())),
        WM_XBUTTONUP => Some(EventType::ButtonRelease(x_button())),
        WM_MOUSEWHEEL => Some(EventType::Wheel {
            delta_x: 0,
            delta_y: 1,
        }),
        WM_MOUSEHWHEEL => Some(EventType::Wheel {
            delta_x: 1,
            delta_y: 0,
        }),
        _ => None,
    }
}

#[cfg(windows)]
mod native {
    use super::{key_from_vk, mouse_event, ListenEnd};
    use rdev::{Event, EventType};
    use std::cell::RefCell;
    use std::time::SystemTime;

    #[repr(C)]
    struct KeyboardHookInfo {
        vk_code: u32,
        scan_code: u32,
        flags: u32,
        time: u32,
        extra_info: usize,
    }

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct MouseHookInfo {
        pt: Point,
        mouse_data: u32,
        flags: u32,
        time: u32,
        extra_info: usize,
    }

    #[repr(C)]
    struct Message {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: Point,
        private: u32,
    }

    const WH_KEYBOARD_LL: i32 = 13;
    const WH_MOUSE_LL: i32 = 14;
    const HC_ACTION: i32 = 0;
    const WM_KEYDOWN: usize = 0x0100;
    const WM_KEYUP: usize = 0x0101;
    const WM_SYSKEYDOWN: usize = 0x0104;
    const WM_SYSKEYUP: usize = 0x0105;

    #[link(name = "user32")]
    extern "system" {
        fn SetWindowsHookExW(
            id: i32,
            hook: unsafe extern "system" fn(i32, usize, isize) -> isize,
            module: isize,
            thread: u32,
        ) -> isize;
        fn UnhookWindowsHookEx(hook: isize) -> i32;
        fn CallNextHookEx(hook: isize, code: i32, wparam: usize, lparam: isize) -> isize;
        fn GetMessageW(message: *mut Message, hwnd: isize, first: u32, last: u32) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> isize;
        fn GetLastError() -> u32;
    }

    type Callback = Box<dyn FnMut(Event)>;

    thread_local! {
        /// The callback for the hooks installed on this thread. Low-level hooks
        /// are called on the thread that installed them, from inside its message
        /// loop, so a thread-local is both sufficient and race-free.
        static CALLBACK: RefCell<Option<Callback>> = const { RefCell::new(None) };
    }

    fn deliver(event_type: EventType) {
        CALLBACK.with(|slot| {
            // `try_borrow_mut`: a hook callback is never re-entered on the same
            // thread in practice, but if it were, dropping one event is better
            // than a panic inside a system callback.
            if let Ok(mut slot) = slot.try_borrow_mut() {
                if let Some(callback) = slot.as_mut() {
                    callback(Event {
                        time: SystemTime::now(),
                        name: None,
                        event_type,
                    });
                }
            }
        });
    }

    unsafe extern "system" fn keyboard_proc(code: i32, wparam: usize, lparam: isize) -> isize {
        if code == HC_ACTION && lparam != 0 {
            let info = &*(lparam as *const KeyboardHookInfo);
            let key = key_from_vk(info.vk_code);
            match wparam {
                WM_KEYDOWN | WM_SYSKEYDOWN => deliver(EventType::KeyPress(key)),
                WM_KEYUP | WM_SYSKEYUP => deliver(EventType::KeyRelease(key)),
                _ => {}
            }
        }
        CallNextHookEx(0, code, wparam, lparam)
    }

    unsafe extern "system" fn mouse_proc(code: i32, wparam: usize, lparam: isize) -> isize {
        if code == HC_ACTION && lparam != 0 {
            let info = &*(lparam as *const MouseHookInfo);
            if let Some(event_type) = mouse_event(wparam as u32, info.mouse_data) {
                deliver(event_type);
            }
        }
        CallNextHookEx(0, code, wparam, lparam)
    }

    pub fn listen(callback: Box<dyn FnMut(Event)>) -> ListenEnd {
        CALLBACK.with(|slot| *slot.borrow_mut() = Some(callback));
        let end = unsafe {
            let module = GetModuleHandleW(std::ptr::null());
            let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, keyboard_proc, module, 0);
            if keyboard == 0 {
                ListenEnd::HookRefused("keyboard", GetLastError())
            } else {
                let mouse = SetWindowsHookExW(WH_MOUSE_LL, mouse_proc, module, 0);
                let end = if mouse == 0 {
                    ListenEnd::HookRefused("mouse", GetLastError())
                } else {
                    let mut message = std::mem::zeroed::<Message>();
                    let end = loop {
                        match GetMessageW(&mut message, 0, 0, 0) {
                            0 => break ListenEnd::Quit,
                            -1 => break ListenEnd::MessageLoopFailed(GetLastError()),
                            _ => {}
                        }
                    };
                    UnhookWindowsHookEx(mouse);
                    end
                };
                UnhookWindowsHookEx(keyboard);
                end
            }
        };
        CALLBACK.with(|slot| *slot.borrow_mut() = None);
        end
    }
}

/// Installs both hooks on the calling thread and runs its message loop, calling
/// `callback` for every key and button event. Blocks until the loop ends, and
/// returns why it did.
#[cfg(windows)]
pub fn listen<F: FnMut(Event) + 'static>(callback: F) -> ListenEnd {
    native::listen(Box::new(callback))
}

#[cfg(not(windows))]
pub fn listen<F: FnMut(Event) + 'static>(callback: F) -> ListenEnd {
    let _ = rdev::listen(callback);
    ListenEnd::Quit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_map_to_their_sides() {
        assert_eq!(key_from_vk(0xA0), Key::ShiftLeft);
        assert_eq!(key_from_vk(0xA1), Key::ShiftRight);
        assert_eq!(key_from_vk(0xA2), Key::ControlLeft);
        assert_eq!(key_from_vk(0xA3), Key::ControlRight);
        assert_eq!(key_from_vk(0xA4), Key::Alt);
        assert_eq!(key_from_vk(0xA5), Key::AltGr);
    }

    #[test]
    fn item_slots_map_from_both_rows() {
        assert_eq!(key_from_vk(0x31), Key::Num1);
        assert_eq!(key_from_vk(0x63), Key::Kp3);
    }

    #[test]
    fn other_keys_stay_distinct() {
        assert_eq!(key_from_vk(0x41), Key::Unknown(0x41));
        assert_ne!(key_from_vk(0x41), key_from_vk(0x42));
    }

    #[test]
    fn mouse_movement_is_not_forwarded() {
        assert_eq!(mouse_event(0x0200, 0), None);
        assert_eq!(
            mouse_event(0x0201, 0),
            Some(EventType::ButtonPress(Button::Left))
        );
        assert_eq!(
            mouse_event(0x020B, 1 << 16),
            Some(EventType::ButtonPress(Button::Unknown(1)))
        );
    }
}
