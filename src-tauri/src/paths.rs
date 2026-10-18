//! One place for the directories Bard resolves from the environment.
//!
//! Nothing here may ask the Tauri runtime where its data lives: the database
//! is opened before the builder's `setup()` runs, and window events fire even
//! earlier. Windows keeps the location the bundle identifier has always
//! produced, `%LOCALAPPDATA%\com.miisodev.bard`, byte for byte, and
//! deliberately stays off roaming profiles. Linux follows the XDG
//! base-directory spec: `$XDG_DATA_HOME` (or `~/.local/share`) holds the
//! database, images and models; `$XDG_STATE_HOME` (or `~/.local/state`) holds
//! the logs and the clean-exit marker.

#[cfg(any(not(windows), test))]
use std::ffi::OsStr;
use std::path::PathBuf;

/// The folder Bard owns under whichever base the platform names.
const APP_DIRECTORY: &str = "com.miisodev.bard";

/// The database, images and models: the files Bard must keep across a
/// reinstall.
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join(APP_DIRECTORY))
    }
    #[cfg(not(windows))]
    {
        xdg_app_dir(
            std::env::var_os("XDG_DATA_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
            ".local/share",
        )
    }
}

/// Logs and the clean-exit marker: run-state a reinstall may throw away,
/// unlike the database.
pub fn state_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        // This state has always lived in the data directory on Windows.
        data_dir()
    }
    #[cfg(not(windows))]
    {
        xdg_app_dir(
            std::env::var_os("XDG_STATE_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
            ".local/state",
        )
    }
}

/// The crash and lifecycle logs under [`state_dir`]. Never fails: the panic
/// hook has always had a temp-directory fallback, so a report is still
/// written on a machine whose environment names no home at all.
pub fn logs_dir() -> PathBuf {
    #[cfg(windows)]
    {
        // `APPDATA` is the fallback because a profile with no local app-data
        // directory still resolves there, and that is where the crash reader
        // already looks for these logs.
        std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("APPDATA"))
            .map(|base| PathBuf::from(base).join(APP_DIRECTORY).join("logs"))
            .unwrap_or_else(|| std::env::temp_dir().join(APP_DIRECTORY).join("logs"))
    }
    #[cfg(not(windows))]
    {
        state_dir()
            .map(|state| state.join("logs"))
            .unwrap_or_else(|| std::env::temp_dir().join(APP_DIRECTORY).join("logs"))
    }
}

/// XDG base-directory resolution, pure over its inputs so the same tests can
/// run on every platform: an absolute `$XDG_*` value wins; an empty, relative
/// or missing value falls back to `$HOME/<fallback>`; with neither set the
/// answer is `None`. Linux is the only platform that resolves with it, so on
/// Windows it exists solely for its tests.
#[cfg(any(not(windows), test))]
fn xdg_app_dir(base: Option<&OsStr>, home: Option<&OsStr>, fallback: &str) -> Option<PathBuf> {
    if let Some(base) = base.filter(|value| !value.is_empty()) {
        let base = PathBuf::from(base);
        if base.is_absolute() {
            return Some(base.join(APP_DIRECTORY));
        }
    }
    home.filter(|value| !value.is_empty())
        .map(|home| PathBuf::from(home).join(fallback).join(APP_DIRECTORY))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absolute_xdg_base_wins_over_home() {
        let base = std::env::temp_dir();
        assert_eq!(
            xdg_app_dir(
                Some(base.as_os_str()),
                Some(OsStr::new("/home/miiso")),
                ".local/share"
            ),
            Some(base.join(APP_DIRECTORY))
        );
    }

    #[test]
    fn empty_relative_or_missing_bases_fall_back_to_home() {
        let home = OsStr::new("/home/miiso");
        let expected = Some(PathBuf::from(home).join(".local/share").join(APP_DIRECTORY));
        for base in [
            None,
            Some(OsStr::new("")),
            Some(OsStr::new("relative/place")),
        ] {
            assert_eq!(xdg_app_dir(base, Some(home), ".local/share"), expected);
        }
    }

    #[test]
    fn no_base_and_no_home_resolves_to_nothing() {
        assert_eq!(xdg_app_dir(None, None, ".local/share"), None);
        assert_eq!(
            xdg_app_dir(Some(OsStr::new("")), None, ".local/share"),
            None
        );
    }
}
