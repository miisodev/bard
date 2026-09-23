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

pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
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
