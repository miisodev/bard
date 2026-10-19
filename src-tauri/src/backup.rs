//! Backup and restore: the whole local library in one folder.
//!
//! A backup is deliberately plain — a copy of `bard.db`, a copy of the
//! `images` folder, and a `manifest.json` that says what the folder is and
//! what it holds. Restoring stages everything beside the live data, verifies
//! the staged copy, and only then swaps it in, so a failure at any point
//! leaves the live data exactly as it was.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::db::{self, Db};

const MANIFEST_NAME: &str = "manifest.json";
const MANIFEST_FORMAT: &str = "bard-backup";
const MANIFEST_VERSION: u32 = 1;
const DATABASE_NAME: &str = "bard.db";
const IMAGES_DIR: &str = "images";
/// Staging lives beside the live data so the swap is a set of renames on one
/// filesystem, which Windows performs atomically and cannot half-complete.
const STAGING_DIR: &str = "restore-staging";

/// What a backup holds. The images count is the number of files actually
/// copied; the rest are read from the database being backed up.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupCounts {
    pub items: i64,
    pub sections: i64,
    pub packs: i64,
    pub attachments: i64,
    pub images: usize,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    /// Always `bard-backup`; a folder holding anything else is refused.
    pub format: String,
    pub format_version: u32,
    pub app_version: String,
    pub channel: String,
    /// Milliseconds since the Unix epoch, the same clock as every stored time.
    pub created_at: i64,
    pub counts: BackupCounts,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub path: String,
    pub counts: BackupCounts,
}

/// What a restore would do, shown before the restore is allowed to happen.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreview {
    pub path: String,
    pub manifest: BackupManifest,
    pub live_items: i64,
    pub live_sections: i64,
    /// True when this Bard has no items and no sections yet.
    pub fresh: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase", tag = "outcome")]
pub enum RestoreOutcome {
    /// The restore asks before changing anything at all, whoever is asking.
    NeedsConfirmation { preview: RestorePreview },
    Restored {
        path: String,
        counts: BackupCounts,
        /// The caller shows "restart Bard" rather than assuming the live
        /// session picked everything up.
        restart_required: bool,
        /// The previous database and images are kept beside the live data so a
        /// mistake stays recoverable; this is the replaced database's path,
        /// whose sibling `images.replaced-…` folder holds the old images.
        previous: String,
    },
}

/// Write a backup of the database and images into a new timestamped folder
/// under `destination`.
#[tauri::command]
pub fn backup_now(app: AppHandle, destination: String) -> Result<BackupSummary, String> {
    let destination = PathBuf::from(destination);
    if !destination.is_dir() {
        return Err("Choose an existing folder for the backup.".into());
    }
    let data_dir =
        crate::paths::data_dir().ok_or("Bard could not find its local application-data folder.")?;
    let counts = live_counts(&app);
    let summary = create_backup(&data_dir, &destination, counts)?;
    crate::crash::record_event("backup:created");
    Ok(summary)
}

/// Restore the database and images from a backup folder.
///
/// `confirmed` is the caller's answer to the restore question: the first call
/// reports what the backup holds and changes nothing, and only a confirmed
/// call performs the swap. A fresh Bard must go through the same question —
/// replacing an empty library by accident is still replacing something.
#[tauri::command]
pub fn restore_from_backup(
    app: AppHandle,
    source: String,
    confirmed: bool,
) -> Result<RestoreOutcome, String> {
    let source = PathBuf::from(source);
    let manifest = read_manifest(&source)?;
    validate_database(&source.join(DATABASE_NAME))?;
    let live = live_counts(&app);
    let fresh = live.items == 0 && live.sections == 0;
    if !confirmed {
        return Ok(RestoreOutcome::NeedsConfirmation {
            preview: RestorePreview {
                path: source.to_string_lossy().to_string(),
                manifest,
                live_items: live.items,
                live_sections: live.sections,
                fresh,
            },
        });
    }
    let restored = perform_restore(&app, &source, &manifest.counts)?;
    Ok(RestoreOutcome::Restored {
        path: source.to_string_lossy().to_string(),
        counts: restored.counts,
        restart_required: true,
        previous: restored.previous,
    })
}

/// The testable half of `backup_now`: directories in, a summary out. Every
/// failure removes the partial folder, so a folder that exists is a backup.
pub fn create_backup(
    data_dir: &Path,
    destination: &Path,
    live: BackupCounts,
) -> Result<BackupSummary, String> {
    let source_db = data_dir.join(DATABASE_NAME);
    if !source_db.is_file() {
        return Err("Bard has no history file to back up yet.".into());
    }
    let folder = unique_backup_folder(destination, db::now_ms());
    fs::create_dir_all(&folder)
        .map_err(|error| format!("Could not create the backup folder: {error}"))?;
    let staged = (|| -> Result<BackupCounts, String> {
        snapshot_database(&source_db, &folder.join(DATABASE_NAME))?;
        let images = copy_tree(&data_dir.join(IMAGES_DIR), &folder.join(IMAGES_DIR))?;
        Ok(BackupCounts { images, ..live })
    })();
    let counts = match staged {
        Ok(counts) => counts,
        Err(error) => {
            let _ = fs::remove_dir_all(&folder);
            return Err(error);
        }
    };
    let manifest = BackupManifest {
        format: MANIFEST_FORMAT.into(),
        format_version: MANIFEST_VERSION,
        app_version: env!("CARGO_PKG_VERSION").into(),
        channel: crate::packaging::channel().into(),
        created_at: db::now_ms(),
        counts: counts.clone(),
    };
    if let Err(error) = write_manifest(&folder, &manifest) {
        let _ = fs::remove_dir_all(&folder);
        return Err(error);
    }
    Ok(BackupSummary {
        path: folder.to_string_lossy().to_string(),
        counts,
    })
}

/// Read and validate a backup's manifest. This is the first thing a restore
/// does: a folder without a Bard manifest is refused before anything is read
/// from it or written anywhere.
pub fn read_manifest(folder: &Path) -> Result<BackupManifest, String> {
    let path = folder.join(MANIFEST_NAME);
    if !path.is_file() {
        return Err(
            "That folder is not a Bard backup: it has no manifest.json. Choose the folder a \
             backup was written to."
                .into(),
        );
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("The backup manifest could not be read: {error}"))?;
    let manifest: BackupManifest = serde_json::from_str(&text).map_err(|_| {
        "That folder is not a Bard backup: its manifest.json could not be read.".to_string()
    })?;
    if manifest.format != MANIFEST_FORMAT {
        return Err("That folder is not a Bard backup.".into());
    }
    if manifest.format_version > MANIFEST_VERSION {
        return Err(
            "This backup was written by a newer Bard; update Bard before restoring it.".into(),
        );
    }
    Ok(manifest)
}

/// Open a backup's or staged database and check it reads. A file that fails
/// this is never swapped in.
pub fn validate_database(path: &Path) -> Result<(), String> {
    // Refuse a missing file explicitly: `Connection::open` would create an
    // empty one and every check below would pass against nothing.
    if !path.is_file() {
        return Err("That folder is not a Bard backup: it has no history file.".into());
    }
    let connection = rusqlite::Connection::open(path)
        .map_err(|error| format!("The backup's history file could not be opened: {error}"))?;
    let verdict: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| format!("The backup's history file could not be read: {error}"))?;
    if verdict != "ok" {
        return Err("The backup's history file is damaged; nothing was restored.".into());
    }
    connection
        .query_row("SELECT COUNT(*) FROM items", [], |row| row.get::<_, i64>(0))
        .map_err(|_| {
            "That folder is not a Bard backup: its history file has no Bard data.".to_string()
        })?;
    Ok(())
}

struct Restored {
    counts: BackupCounts,
    previous: String,
}

/// Stage, verify, swap. Nothing written here can leave the live data half
/// old and half new: the staged copy is complete and checked before the first
/// rename, and a failed swap renames everything back.
fn perform_restore(
    app: &AppHandle,
    source: &Path,
    expected: &BackupCounts,
) -> Result<Restored, String> {
    let data_dir =
        crate::paths::data_dir().ok_or("Bard could not find its local application-data folder.")?;
    if same_folder(source, &data_dir) {
        return Err("That folder is Bard's live data folder, not a backup.".into());
    }
    let staging = data_dir.join(STAGING_DIR);
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)
        .map_err(|error| format!("Could not prepare the restore: {error}"))?;
    // Stage first: a full copy of the database and images beside the live
    // data, so the swap itself is only renames and the expensive work happens
    // while the live database is still fully in service.
    let staged_db = staging.join(DATABASE_NAME);
    fs::copy(source.join(DATABASE_NAME), &staged_db)
        .map_err(|error| format!("Could not copy the backup's history file: {error}"))?;
    let images = copy_tree(&source.join(IMAGES_DIR), &staging.join(IMAGES_DIR))?;
    if let Err(error) = validate_database(&staged_db) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    // The swap runs under the database lock. The live connection is closed
    // first — Windows will not rename a file another handle holds open — and
    // every reader blocks on the lock until the restored connection is in
    // place, so no window can query the database while it is between files.
    let state = app.state::<Db>();
    let mut guard = state
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let placeholder = rusqlite::Connection::open_in_memory()
        .map_err(|error| format!("Could not prepare the restore: {error}"))?;
    let previous_connection = std::mem::replace(&mut *guard, placeholder);
    drop(previous_connection);

    let stamp = db::now_ms();
    let swap = swap_in(&data_dir, &staging, stamp);
    // Whatever happened, the guard gets a working connection before it is
    // released; on a failed swap it is the original file, untouched.
    let (connection, notice) = db::open_or_recover(&data_dir.join(DATABASE_NAME));
    *guard = connection;
    drop(guard);
    if let Some(notice) = notice {
        log::warn!("the restored database reported: {notice}");
    }
    let previous = match swap {
        Ok(previous) => previous,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    let _ = fs::remove_dir_all(&staging);
    crate::crash::record_event("backup:restored");
    crate::commands::refresh(app);
    Ok(Restored {
        counts: BackupCounts {
            images,
            ..expected.clone()
        },
        previous,
    })
}

/// Move the staged database and images into place, keeping what they replaced
/// beside them. Every rename that happened is rolled back when a later one
/// fails, so the live folder is never a mix of two libraries.
fn swap_in(data_dir: &Path, staging: &Path, stamp: i64) -> Result<String, String> {
    let live_db = data_dir.join(DATABASE_NAME);
    let live_images = data_dir.join(IMAGES_DIR);
    let staged_db = staging.join(DATABASE_NAME);
    let staged_images = staging.join(IMAGES_DIR);
    let kept_db = data_dir.join(format!("{DATABASE_NAME}.replaced-{stamp}"));
    let kept_images = data_dir.join(format!("{IMAGES_DIR}.replaced-{stamp}"));

    fn move_one(from: &Path, to: &Path, moved: &mut Vec<(PathBuf, PathBuf)>) -> Result<(), String> {
        fs::rename(from, to)
            .map_err(|error| format!("Could not move {}: {error}", from.display()))?;
        moved.push((from.to_path_buf(), to.to_path_buf()));
        Ok(())
    }

    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let result = (|| {
        if live_db.exists() {
            move_one(&live_db, &kept_db, &mut moved)?;
        }
        if live_images.is_dir() {
            move_one(&live_images, &kept_images, &mut moved)?;
        }
        move_one(&staged_db, &live_db, &mut moved)?;
        if staged_images.is_dir() {
            move_one(&staged_images, &live_images, &mut moved)?;
        }
        Ok::<(), String>(())
    })();
    match result {
        Ok(()) => {
            // The images folder must exist even when the backup carried none.
            let _ = fs::create_dir_all(&live_images);
            Ok(kept_db.to_string_lossy().to_string())
        }
        Err(error) => {
            while let Some((from, to)) = moved.pop() {
                let _ = fs::rename(&to, &from);
            }
            Err(error)
        }
    }
}

/// A consistent copy of the live database. `VACUUM INTO` reads through the
/// write-ahead log, so a capture made moments before the backup is included
/// and the copy does not depend on the main file's pages alone.
fn snapshot_database(source: &Path, target: &Path) -> Result<(), String> {
    let connection = rusqlite::Connection::open(source)
        .map_err(|error| format!("Could not open the history file: {error}"))?;
    let _ = connection.busy_timeout(std::time::Duration::from_secs(5));
    connection
        .execute(
            "VACUUM INTO ?1",
            rusqlite::params![target.to_string_lossy().to_string()],
        )
        .map(|_| ())
        .map_err(|error| format!("Could not copy the history file: {error}"))
}

/// Copy every file under `from` into `to`, returning how many files were
/// copied. A missing source is zero files, not an error: a Bard that never
/// kept an image is an ordinary Bard.
fn copy_tree(from: &Path, to: &Path) -> Result<usize, String> {
    if !from.is_dir() {
        return Ok(0);
    }
    fs::create_dir_all(to)
        .map_err(|error| format!("Could not create {}: {error}", to.display()))?;
    let mut copied = 0;
    let entries = fs::read_dir(from)
        .map_err(|error| format!("Could not read {}: {error}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("Could not read {}: {error}", from.display()))?;
        let target = to.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Could not read {}: {error}", from.display()))?;
        if file_type.is_dir() {
            copied += copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("Could not copy {}: {error}", entry.path().display()))?;
            copied += 1;
        }
    }
    Ok(copied)
}

fn write_manifest(folder: &Path, manifest: &BackupManifest) -> Result<(), String> {
    let text = serde_json::to_string_pretty(manifest).map_err(|error| error.to_string())?;
    fs::write(folder.join(MANIFEST_NAME), text)
        .map_err(|error| format!("Could not write the backup manifest: {error}"))
}

/// What this Bard currently holds. Loading the lists is deliberate: a backup
/// is a user-waited action, and these are the numbers the manifest and the
/// fresh-database check both use.
fn live_counts(app: &AppHandle) -> BackupCounts {
    BackupCounts {
        items: db::list_items(app).len() as i64,
        sections: db::list_sections(app).len() as i64,
        packs: db::list_packs(app).len() as i64,
        attachments: db::list_all_attachments(app).len() as i64,
        images: 0,
    }
}

fn unique_backup_folder(destination: &Path, stamp: i64) -> PathBuf {
    let label = timestamp_label(stamp);
    let base = destination.join(format!("Bard-backup-{label}"));
    if !base.exists() {
        return base;
    }
    for index in 2..1000 {
        let candidate = destination.join(format!("Bard-backup-{label}-{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    destination.join(format!("Bard-backup-{label}-{}", std::process::id()))
}

/// `YYYYMMDD-HHMMSS` in UTC from milliseconds since the epoch, so backup
/// folders sort by name the way they were taken. Dependency-free on purpose:
/// one date format is not worth a crate.
fn timestamp_label(unix_ms: i64) -> String {
    let seconds = unix_ms.div_euclid(1000).max(0);
    let days = seconds.div_euclid(86_400);
    let time_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60
    )
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

/// True when two paths name the same folder. Canonicalised where possible so
/// a picked path that differs only in case or separators still matches.
fn same_folder(left: &Path, right: &Path) -> bool {
    let canonical = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical(left) == canonical(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tests_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bard-backup-test-{label}-{}", db::now_ms()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn seed_database(path: &Path, content: &str) -> rusqlite::Connection {
        let connection = db::init(path).unwrap();
        connection
            .execute(
                "INSERT INTO items(content, created_at, updated_at) VALUES(?1, 1, 1)",
                rusqlite::params![content],
            )
            .unwrap();
        connection
    }

    #[test]
    fn timestamp_labels_are_utc_and_sortable() {
        assert_eq!(timestamp_label(0), "19700101-000000");
        assert_eq!(timestamp_label(1_000_000_000_000), "20010909-014640");
    }

    #[test]
    fn a_backup_carries_the_database_images_and_a_manifest() {
        let root = tests_dir("create");
        let data = root.join("data");
        fs::create_dir_all(data.join(IMAGES_DIR)).unwrap();
        fs::write(data.join(IMAGES_DIR).join("shot.png"), b"png").unwrap();
        let connection = seed_database(&data.join(DATABASE_NAME), "kept");
        drop(connection);
        let destination = root.join("backups");
        fs::create_dir_all(&destination).unwrap();

        let live = BackupCounts {
            items: 1,
            ..BackupCounts::default()
        };
        let summary = create_backup(&data, &destination, live).unwrap();
        let folder = PathBuf::from(&summary.path);
        assert!(folder.is_dir(), "the backup folder is announced as written");
        assert_eq!(summary.counts.items, 1);
        assert_eq!(summary.counts.images, 1);

        let manifest = read_manifest(&folder).unwrap();
        assert_eq!(manifest.format, MANIFEST_FORMAT);
        assert_eq!(manifest.format_version, MANIFEST_VERSION);
        assert_eq!(manifest.counts.images, 1);
        assert!(folder.join(IMAGES_DIR).join("shot.png").is_file());

        let snapshot = rusqlite::Connection::open(folder.join(DATABASE_NAME)).unwrap();
        let content: String = snapshot
            .query_row("SELECT content FROM items", [], |row| row.get(0))
            .unwrap();
        assert_eq!(content, "kept");

        // A folder without a manifest is refused before anything is read.
        assert!(read_manifest(&destination).is_err());
        drop(snapshot);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_database_that_is_not_bards_is_refused() {
        let root = tests_dir("validation");
        let path = root.join(DATABASE_NAME);
        fs::write(&path, b"not a database").unwrap();
        assert!(validate_database(&path).is_err());
        // A missing file is refused rather than created empty by the open.
        assert!(validate_database(&root.join("absent.db")).is_err());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn restoring_swaps_the_database_and_keeps_the_previous_one() {
        let root = tests_dir("swap");
        let data = root.join("data");
        let staging = root.join("staging");
        fs::create_dir_all(data.join(IMAGES_DIR)).unwrap();
        fs::write(data.join(IMAGES_DIR).join("old.png"), b"old").unwrap();
        fs::create_dir_all(staging.join(IMAGES_DIR)).unwrap();
        fs::write(staging.join(IMAGES_DIR).join("new.png"), b"new").unwrap();
        let live = seed_database(&data.join(DATABASE_NAME), "live");
        drop(live);
        let backup = seed_database(&staging.join(DATABASE_NAME), "backup");
        drop(backup);

        let previous = swap_in(&data, &staging, 42).unwrap();
        assert!(previous.contains("replaced-42"));

        let restored = rusqlite::Connection::open(data.join(DATABASE_NAME)).unwrap();
        let content: String = restored
            .query_row("SELECT content FROM items", [], |row| row.get(0))
            .unwrap();
        assert_eq!(content, "backup");
        assert!(data.join(IMAGES_DIR).join("new.png").is_file());
        assert!(!data.join(IMAGES_DIR).join("old.png").exists());
        // The replaced library is kept beside the live one, not deleted.
        let kept = rusqlite::Connection::open(data.join("bard.db.replaced-42")).unwrap();
        let kept_content: String = kept
            .query_row("SELECT content FROM items", [], |row| row.get(0))
            .unwrap();
        assert_eq!(kept_content, "live");
        assert!(data.join("images.replaced-42").join("old.png").is_file());
        // Close both handles before the second swap: Windows will not rename a
        // file another handle holds open, which is exactly what the restore
        // path's lock exists to prevent.
        drop(restored);
        drop(kept);

        // A backup with no images leaves an images folder behind: the app
        // writes screenshots into it on the next capture.
        let empty = root.join("empty-staging");
        fs::create_dir_all(&empty).unwrap();
        drop(seed_database(&empty.join(DATABASE_NAME), "second"));
        swap_in(&data, &empty, 43).unwrap();
        assert!(data.join(IMAGES_DIR).is_dir());
        assert!(!data.join(IMAGES_DIR).join("new.png").exists());

        fs::remove_dir_all(root).ok();
    }
}
