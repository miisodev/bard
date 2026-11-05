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
