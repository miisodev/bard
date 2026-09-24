//! How this copy of Bard was installed.
//!
//! Bard ships two ways: a direct NSIS download from miiso.dev, and an MSIX
//! package through the Microsoft Store. The Store copy runs with a package
//! identity, is updated by the Store and is started at login by the startup
//! task its manifest declares — so the direct channel's updater and Run-key
//! autostart stand down there instead of fighting the platform.

use std::sync::OnceLock;

/// True when this process runs from an MSIX package (the Microsoft Store copy).
pub fn is_packaged() -> bool {
    static PACKAGED: OnceLock<bool> = OnceLock::new();
    *PACKAGED.get_or_init(detect_package)
}

pub fn channel() -> &'static str {
    if is_packaged() {
        "store"
    } else {
        "direct"
    }
}

#[cfg(windows)]
fn detect_package() -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentPackageFullName(length: *mut u32, name: *mut u16) -> i32;
    }
    const APPMODEL_ERROR_NO_PACKAGE: i32 = 15700;
    let mut length = 0u32;
    // Asked with an empty buffer, a packaged process answers with the buffer
    // size it needs; an unpackaged one answers "no package".
    let status = unsafe { GetCurrentPackageFullName(&mut length, std::ptr::null_mut()) };
    status != APPMODEL_ERROR_NO_PACKAGE
}

#[cfg(not(windows))]
fn detect_package() -> bool {
    false
}

/// A startup task cannot pass the `--autostart` flag the direct channel uses,
/// so a packaged launch within two minutes of this session's logon is taken
/// to be the login start. A workaround, and named as one: opening Bard by hand
/// in those first two minutes leaves the panel in the tray rather than on
/// screen, where one gesture brings it up.
#[cfg(windows)]
pub fn launched_near_logon() -> bool {
    #[repr(C)]
    struct WtsInfoW {
        state: i32,
        session_id: u32,
        incoming_bytes: u32,
        outgoing_bytes: u32,
        incoming_frames: u32,
        outgoing_frames: u32,
        incoming_compressed_bytes: u32,
        outgoing_compressed_bytes: u32,
        win_station_name: [u16; 32],
        domain: [u16; 17],
        user_name: [u16; 21],
        connect_time: i64,
        disconnect_time: i64,
        last_input_time: i64,
        logon_time: i64,
        current_time: i64,
    }
    #[link(name = "wtsapi32")]
    extern "system" {
        fn WTSQuerySessionInformationW(
            server: isize,
            session: u32,
            class: i32,
            buffer: *mut *mut u16,
            bytes: *mut u32,
        ) -> i32;
        fn WTSFreeMemory(memory: *mut core::ffi::c_void);
    }
    const WTS_CURRENT_SESSION: u32 = u32::MAX;
    const WTS_SESSION_INFO: i32 = 24;
    const WINDOW_100NS: i64 = 120 * 10_000_000;

    let mut buffer: *mut u16 = std::ptr::null_mut();
    let mut bytes = 0u32;
    let ok = unsafe {
        WTSQuerySessionInformationW(
            0,
            WTS_CURRENT_SESSION,
            WTS_SESSION_INFO,
            &mut buffer,
            &mut bytes,
        )
    };
    if ok == 0 || buffer.is_null() {
        return false;
    }
    let near = if bytes as usize >= std::mem::size_of::<WtsInfoW>() {
        let info = unsafe { &*(buffer as *const WtsInfoW) };
        let elapsed = info.current_time.saturating_sub(info.logon_time);
        info.logon_time > 0 && (0..=WINDOW_100NS).contains(&elapsed)
    } else {
        false
    };
    unsafe { WTSFreeMemory(buffer.cast()) };
    near
}

#[cfg(not(windows))]
pub fn launched_near_logon() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unpackaged_test_process_reports_the_direct_channel() {
        assert!(!is_packaged());
        assert_eq!(channel(), "direct");
    }

    #[test]
    fn logon_query_does_not_fault() {
        let _ = launched_near_logon();
    }
}
