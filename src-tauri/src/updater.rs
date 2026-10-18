//! User-invoked update checks. The updater is never silent: the frontend
//! asks, shows the signed release metadata, and asks again before install.

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    pub published_at: Option<String>,
}

fn info(update: &tauri_plugin_updater::Update) -> UpdateInfo {
    UpdateInfo {
        version: update.version.clone(),
        notes: update.body.clone(),
        published_at: update.date.map(|date| date.to_string()),
    }
}

/// The Store copy is updated by the Microsoft Store; installing the direct
/// channel's NSIS package from inside it would leave a second, unpackaged Bard.
const STORE_UPDATES: &str = "Bard updates through the Microsoft Store.";

/// The updater feed serves the Windows NSIS installer only, and a Linux copy
/// (deb, rpm or Flatpak) is updated by its package manager. The plugin is
/// registered on every platform — it does nothing until asked — so the gate
/// lives here, in front of the network.
const PACKAGE_MANAGER_UPDATES: &str = "Bard updates through your package manager on this platform.";

pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    if crate::packaging::is_packaged() {
        return Err(STORE_UPDATES.into());
    }
    if !cfg!(windows) {
        return Err(PACKAGE_MANAGER_UPDATES.into());
    }
    let updater = app
        .updater_builder()
        .build()
        .map_err(|error| format!("Update service unavailable: {error}"))?;
    let update = updater
        .check()
        .await
        .map_err(|error| format!("Update check failed: {error}"))?;
    Ok(update.as_ref().map(info))
}

pub async fn install(app: &AppHandle) -> Result<(), String> {
    if crate::packaging::is_packaged() {
        return Err(STORE_UPDATES.into());
    }
    if !cfg!(windows) {
        return Err(PACKAGE_MANAGER_UPDATES.into());
    }
    let updater = app
        .updater_builder()
        .build()
        .map_err(|error| format!("Update service unavailable: {error}"))?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|error| format!("Update check failed: {error}"))?
    else {
        return Err("Bard is already up to date.".into());
    };
    let payload = info(&update);
    let _ = app.emit("update-ready", payload);
    // Windows installers terminate the app as part of installation. Mark the
    // exit before handing control to the updater so the guardian does not
    // interpret the deliberate update exit as a crash and relaunch Bard.
    crate::guardian::mark_clean_exit();
    update
        .download_and_install(
            |chunk, total| {
                let _ = app.emit(
                    "update-progress",
                    serde_json::json!({ "chunk": chunk, "total": total }),
                );
            },
            || {
                let _ = app.emit("update-downloaded", ());
            },
        )
        .await
        .map_err(|error| format!("Update installation failed: {error}"))
}
