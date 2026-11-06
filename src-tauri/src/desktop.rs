//! Desktop capabilities follow the session, rather than the presence of DISPLAY.
//! A Wayland session commonly also has DISPLAY for Xwayland; X11 hooks there
//! still cannot see or control native Wayland applications.

use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopStatus {
    pub session: &'static str,
    pub shortcuts_ready: bool,
    pub input_ready: bool,
}

pub fn is_wayland() -> bool {
    cfg!(target_os = "linux")
        && session_kind(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
        ) == "wayland"
}

/// Open Bard's windows through XWayland on a Wayland session.
///
/// The shelf is docked to a screen edge, and Wayland lets no client place its
/// own window: GNOME's compositor ignores the position and centres it, and it
/// does not implement the layer-shell protocol docks use elsewhere. Under
/// XWayland the compositor honours the position, so the shelf docks where its
/// setting says. Only the window toolkit changes: [`is_wayland`] still reads
/// the session, so shortcuts, capture and paste-back keep using the desktop's
/// portals, and focus tracking stays off.
///
/// Skipped when XWayland is absent (`DISPLAY` unset, as in a Flatpak without
/// the X11 socket), when the user already chose a backend, or when
/// `BARD_NATIVE_WAYLAND=1` asks for native Wayland windows.
#[cfg(target_os = "linux")]
pub fn prefer_xwayland_windows() {
    let var = |name: &str| std::env::var(name).ok();
    if should_prefer_xwayland(
        var("XDG_SESSION_TYPE").as_deref(),
        var("WAYLAND_DISPLAY").as_deref(),
        var("DISPLAY").as_deref(),
        var("GDK_BACKEND").as_deref(),
        var("BARD_NATIVE_WAYLAND").as_deref(),
    ) {
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

fn should_prefer_xwayland(
    session: Option<&str>,
    wayland_display: Option<&str>,
    display: Option<&str>,
    gdk_backend: Option<&str>,
    native: Option<&str>,
) -> bool {
    session_kind(session, wayland_display) == "wayland"
        && display.is_some_and(|value| !value.is_empty())
        && gdk_backend.is_none_or(str::is_empty)
        && native != Some("1")
}

fn session_kind(kind: Option<&str>, wayland_display: Option<&str>) -> &'static str {
    if kind.is_some_and(|value| value.eq_ignore_ascii_case("wayland"))
        || (kind != Some("x11") && wayland_display.is_some_and(|value| !value.is_empty()))
    {
        "wayland"
    } else {
        "x11"
    }
}

pub fn status() -> DesktopStatus {
    #[cfg(target_os = "linux")]
    if is_wayland() {
        return DesktopStatus {
            session: "wayland",
            shortcuts_ready: crate::linux_portal::shortcuts_ready(),
            input_ready: crate::linux_portal::input_ready(),
        };
    }
    DesktopStatus {
        session: if cfg!(windows) { "windows" } else { "x11" },
        shortcuts_ready: true,
        input_ready: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_never_falls_back_to_x11_hooks() {
        assert_eq!(session_kind(Some("wayland"), None), "wayland");
        assert_eq!(session_kind(None, Some("wayland-0")), "wayland");
        assert_eq!(session_kind(Some("x11"), Some("wayland-0")), "x11");
        assert_eq!(session_kind(None, Some("")), "x11");
    }
}

#[cfg(test)]
mod xwayland_tests {
    use super::should_prefer_xwayland;

    #[test]
    fn xwayland_is_used_only_on_wayland_with_an_x_server_and_no_override() {
        assert!(should_prefer_xwayland(
            Some("wayland"),
            Some("wayland-0"),
            Some(":0"),
            None,
            None
        ));
        assert!(!should_prefer_xwayland(
            Some("x11"),
            None,
            Some(":0"),
            None,
            None
        ));
        assert!(!should_prefer_xwayland(
            Some("wayland"),
            Some("wayland-0"),
            None,
            None,
            None
        ));
        assert!(!should_prefer_xwayland(
            Some("wayland"),
            Some("wayland-0"),
            Some(":0"),
            Some("wayland"),
            None
        ));
        assert!(!should_prefer_xwayland(
            Some("wayland"),
            Some("wayland-0"),
            Some(":0"),
            None,
            Some("1")
        ));
    }
}
