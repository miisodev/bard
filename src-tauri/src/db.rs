use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, Manager};

/// The open database. Always open: [`open_or_recover`] never returns nothing,
/// and the restore path holds this lock across its file swap, so no reader can
/// observe the state between the old file closing and the new one opening.
///
/// It used to be `Mutex<Option<Connection>>`, and every read unwrapped it. A
/// failed open then became a panic under `panic = "abort"` — a silent process
/// death with no window, on a launch that showed nothing.
pub struct Db(pub Mutex<Connection>);

/// What opening the database has to report to a person — a damaged file set
/// aside, a file that could not be opened, roaming data that could not be
/// moved. Both `get_state` and the diagnostic report read it; nothing invents
/// a value in it.
pub struct DbNotice(pub Mutex<Option<String>>);

/// Milliseconds since the Unix epoch, or 0 if the system clock is before it.
///
/// Never unwraps. `overlay::holding_clicks` calls this from the mouse hook
/// thread, so under `panic = "abort"` a clock set earlier than 1970 would kill
/// the whole process and take the click guard with it. A wrong timestamp is
/// recoverable; a dead process is not.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: i64,
    pub name: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: i64,
    pub section_id: Option<i64>,
    pub content: String,
    pub kind: String,
    #[serde(skip_serializing)]
    pub image_path: Option<String>,
    pub origin: String,
    pub favorite: i64,
    pub done: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// When the item was pinned, or `None`. Pinned items lead the timeline in
    /// the order they were pinned, so they are slots 1–3 of Right Shift × 2.
    pub pinned_at: Option<i64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: i64,
    pub item_id: i64,
    #[serde(skip_serializing)]
    pub path: String,
    pub position: i64,
}

/// A pack: kept items grouped into one unit for the shelf's Packs zone.
/// `preview_item_ids` is the leading few members in pack order, enough to
/// paint a card without loading the whole pack.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackSummary {
    pub id: i64,
    pub name: String,
    pub item_count: i64,
    pub preview_item_ids: Vec<i64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackDetail {
    pub pack: PackSummary,
    pub items: Vec<Item>,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModelOwnership {
    /// A file Bard downloaded into its own app-data models directory.
    App,
    /// A file Bard may use but does not own (for example, the shared HF cache).
    External,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CustomModel {
    pub id: String,
    pub repo: String,
    pub file: String,
    pub size_mb: u64,
    pub size_bytes: u64,
    pub sha256: String,
    pub local_path: String,
    pub ownership: ModelOwnership,
    pub compatibility: crate::hf::ModelCompatibility,
}

impl CustomModel {
    pub fn owned(&self) -> bool {
        self.ownership == ModelOwnership::App
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub sections: Vec<Section>,
    pub items: Vec<Item>,
    pub attachments: Vec<Attachment>,
    pub active_section_id: Option<i64>,
    pub theme: String,
    pub app_version: String,
    pub onboarding_completed: bool,
    pub autostart_enabled: bool,
    pub capture_double_shift: bool,
    pub model_id: String,
    pub model_ready: bool,
    pub mic_ready: bool,
    pub active_item_id: Option<i64>,
    pub license: crate::license::LicenseStatus,
    /// "direct" for the miiso.dev download, "store" for the Microsoft Store
    /// copy, "flatpak" inside a Flatpak sandbox.
    pub channel: String,
    /// "theme", "light", "dark" or "custom"; the custom colour is `background_color`.
    pub background: String,
    pub background_color: String,
    /// The timeline's order: one of `SORT_ORDERS`.
    pub sort: String,
    /// The screen edge the shelf is docked to: one of `geometry::Edge`'s strings.
    pub shelf_edge: String,
    /// A storage problem the person should know about — a damaged history file
    /// set aside, a file that could not be opened. `None` when opening was
    /// quiet; never invented.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_notice: Option<String>,
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|name| name == column))
}

/// The schema version this build writes. Migration snapshots are named with
/// the version they were taken from, and `user_version` is written to match.
const SCHEMA_VERSION: i64 = 4;

/// How many pre-migration snapshots are kept beside the database.
const MIGRATION_BACKUPS_KEPT: usize = 2;

/// How many times opening an existing file is attempted before it is called
/// unavailable. A file can be locked for a moment — a backup tool reading it,
/// antivirus scanning it — and a retry turns that into an ordinary launch
/// instead of a temporary database.
const OPEN_ATTEMPTS: usize = 3;

/// Every column added after the original schema. Each entry is checked before
/// it runs, so applying the list to an older database adds only what is
/// missing and to a current one adds nothing.
const MIGRATIONS: &[(&str, &str, &str)] = &[
    (
        "items",
        "section_id",
        "ALTER TABLE items ADD COLUMN section_id INTEGER REFERENCES sections(id) ON DELETE SET NULL",
    ),
    ("items", "done", "ALTER TABLE items ADD COLUMN done INTEGER NOT NULL DEFAULT 0"),
    ("items", "pinned_at", "ALTER TABLE items ADD COLUMN pinned_at INTEGER"),
    (
        "items",
        "origin",
        "ALTER TABLE items ADD COLUMN origin TEXT NOT NULL DEFAULT 'manual'",
    ),
    (
        "custom_models",
        "size_bytes",
        "ALTER TABLE custom_models ADD COLUMN size_bytes INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "custom_models",
        "sha256",
        "ALTER TABLE custom_models ADD COLUMN sha256 TEXT NOT NULL DEFAULT ''",
    ),
    (
        "custom_models",
        "ownership",
        "ALTER TABLE custom_models ADD COLUMN ownership TEXT NOT NULL DEFAULT 'external'",
    ),
    (
        "custom_models",
        "compatibility",
        "ALTER TABLE custom_models ADD COLUMN compatibility TEXT NOT NULL DEFAULT 'unverified'",
    ),
];

pub fn init(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn, Some(path))?;
    Ok(conn)
}

/// Set when [`open_or_recover`] had to replace or bypass the database file.
///
/// The startup orphan-image sweep reads it: a fresh database references none
/// of the images beside it, so sweeping after a recovery would delete exactly
/// the files the quarantined database was set aside to keep recoverable.
static RECOVERED_THIS_LAUNCH: AtomicBool = AtomicBool::new(false);

/// Whether this launch lost the use of the database file on disk — a damaged
/// file quarantined, or a file that could not be opened at all.
pub fn recovered_this_launch() -> bool {
    RECOVERED_THIS_LAUNCH.load(Ordering::Relaxed)
}

/// Open the database at `path`, recovering from a corrupt file rather than
/// dying on it.
///
/// Returns a connection that always works — the app must never hold state with
/// no database under it — and a notice for a person when recovery changed the
/// file or could not use it. Only `bard.db` and its `-wal`/`-shm` sidecars are
/// touched: the images folder is never moved, so a quarantined database keeps
/// every screenshot and drawing waiting for the file to be recovered.
pub fn open_or_recover(path: &Path) -> (Connection, Option<String>) {
    if path.exists() {
        match open_existing(path) {
            Existing::Ready(connection) => return (connection, None),
            Existing::Unavailable(error) => {
                crate::crash::record_event("db:open-failed");
                log::warn!("could not open {}: {error}", path.display());
                return (
                    in_memory(),
                    Some(
                        "Bard could not open its history file, so it is running with a \
                         temporary one. Another program may be using the file; close Bard \
                         and open it again. Nothing on disk was changed."
                            .into(),
                    ),
                );
            }
            Existing::Corrupt => match quarantine(path) {
                Ok(kept) => match init(path) {
                    Ok(connection) => {
                        RECOVERED_THIS_LAUNCH.store(true, Ordering::Relaxed);
                        crate::crash::record_event("db:quarantined");
                        log::warn!("history file was damaged; kept as {}", kept.display());
                        return (
                            connection,
                            Some(format!(
                                "Bard found its history file damaged and set it aside as {}. \
                                 It started a fresh history; your images were left untouched \
                                 and the damaged file was kept.",
                                kept.display()
                            )),
                        );
                    }
                    Err(error) => {
                        crate::crash::record_event("db:recreate-failed");
                        log::warn!("could not start a fresh history file: {error}");
                        return (
                            in_memory(),
                            Some(
                                "Bard found its history file damaged and could not start a \
                                 fresh one, so it is running with a temporary history that \
                                 will not be kept. The damaged file was left in place."
                                    .into(),
                            ),
                        );
                    }
                },
                Err(error) => {
                    crate::crash::record_event("db:quarantine-failed");
                    log::warn!("could not set the damaged history file aside: {error}");
                    return (
                        in_memory(),
                        Some(
                            "Bard found its history file damaged but could not set it aside, \
                             so it is running with a temporary history. Nothing on disk was \
                             changed."
                                .into(),
                        ),
                    );
                }
            },
        }
    }
    match init(path) {
        Ok(connection) => (connection, None),
        Err(error) => {
            RECOVERED_THIS_LAUNCH.store(true, Ordering::Relaxed);
            crate::crash::record_event("db:create-failed");
            log::warn!("could not create {}: {error}", path.display());
            (
                in_memory(),
                Some(
                    "Bard could not create its history file, so it is running with a \
                     temporary history that will not be kept."
                        .into(),
                ),
            )
        }
    }
}

/// A working database with no file behind it, used when no file is usable.
///
/// The app keeps running — the alternative was a process that died on the
/// first read — and everything written lands somewhere temporary, which the
/// notice always says. Allocation failure here is the one case that cannot be
/// handled: without memory a database is not the problem.
pub fn in_memory() -> Connection {
    RECOVERED_THIS_LAUNCH.store(true, Ordering::Relaxed);
    let connection =
        Connection::open_in_memory().expect("Bard could not allocate an in-memory database.");
    if let Err(error) = configure(&connection, None) {
        log::warn!("could not prepare the temporary database: {error}");
    }
    connection
}

enum Existing {
    Ready(Connection),
    Corrupt,
    Unavailable(String),
}

/// The verdict of `PRAGMA quick_check`. Busy is deliberately its own case:
/// a file someone else is using right now is not a damaged file, and moving
/// it aside would turn a lock into data loss. Only `Corrupt` quarantines.
enum Integrity {
    Ok,
    Corrupt(String),
    Busy(String),
}

/// Open and prepare an existing file, retrying a transient lock.
///
/// Corruption is decided by `PRAGMA quick_check`, never by an open error: a
/// file that merely cannot be opened right now is left exactly where it is,
/// because moving it aside would turn a lock into data loss. `quick_check`
/// failing is different — it means SQLite read the file and found it damaged.
fn open_existing(path: &Path) -> Existing {
    let mut last = String::new();
    for attempt in 0..OPEN_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        let connection = match Connection::open(path) {
            Ok(connection) => connection,
            Err(error) => {
                last = error.to_string();
                continue;
            }
        };
        // A short wait so a checkpoint or another process's brief lock does
        // not read as a failure; the retry loop above covers the rest.
        let _ = connection.busy_timeout(std::time::Duration::from_millis(500));
        match quick_check(&connection) {
            Integrity::Ok => {}
            Integrity::Corrupt(verdict) => {
                log::warn!("{} failed its integrity check: {verdict}", path.display());
                return Existing::Corrupt;
            }
            Integrity::Busy(error) => {
                last = error;
                continue;
            }
        }
        match configure(&connection, Some(path)) {
            Ok(()) => return Existing::Ready(connection),
            Err(error) => {
                last = error.to_string();
                continue;
            }
        }
    }
    Existing::Unavailable(last)
}

/// The first verdict of `PRAGMA quick_check`: `ok`, or the damage SQLite
/// reports. This reads every page and index in the file and is the check the
/// open path runs before a person can trust an empty timeline either way.
fn quick_check(conn: &Connection) -> Integrity {
    match conn.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0)) {
        Ok(verdict) if verdict == "ok" => Integrity::Ok,
        Ok(verdict) => Integrity::Corrupt(verdict),
        Err(error) if is_busy(&error) => Integrity::Busy(error.to_string()),
        Err(error) => Integrity::Corrupt(error.to_string()),
    }
}

/// SQLite's two "someone else is using it" answers, as opposed to damage.
fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(inner, _)
            if matches!(
                inner.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}

/// Move a damaged database and its sidecars aside, timestamped, and return the
/// new name. The images are deliberately not part of this: they are the bulk
/// of the user's data and nothing is wrong with them.
fn quarantine(path: &Path) -> Result<PathBuf, String> {
    let kept = path.with_file_name(format!("bard.db.corrupt-{}", now_ms()));
    std::fs::rename(path, &kept).map_err(|error| error.to_string())?;
    let name = kept
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "bard.db.corrupt".into());
    for suffix in ["-wal", "-shm"] {
        let sidecar = path.with_file_name(format!("bard.db{suffix}"));
        if sidecar.exists() {
            let _ = std::fs::rename(&sidecar, kept.with_file_name(format!("{name}{suffix}")));
        }
    }
    Ok(kept)
}

/// Copy the database aside before a migration rewrites its schema.
///
/// `VACUUM INTO` writes a consistent snapshot — including everything still in
/// the write-ahead log — to a new file, so the snapshot is what the database
/// actually contained rather than what the main file happened to hold. The
/// copy is named with the schema version it came from and the time it was
/// taken; the newest [`MIGRATION_BACKUPS_KEPT`] are kept.
fn snapshot_before_migration(
    conn: &Connection,
    db_path: &Path,
    from_version: i64,
) -> Result<PathBuf, String> {
    let directory = db_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let stamp = now_ms();
    let mut target = directory.join(format!("bard.db.bak-{from_version}-{stamp}"));
    for attempt in 1..100 {
        if !target.exists() {
            break;
        }
        target = directory.join(format!("bard.db.bak-{from_version}-{stamp}-{attempt}"));
    }
    conn.execute(
        "VACUUM INTO ?1",
        params![target.to_string_lossy().to_string()],
    )
    .map_err(|error| format!("could not copy the database aside: {error}"))?;
    prune_migration_backups(directory, MIGRATION_BACKUPS_KEPT);
    Ok(target)
}

/// Keep the newest `keep` `bard.db.bak-*` snapshots. Best effort: a file that
/// cannot be removed is left rather than failing the migration over it.
fn prune_migration_backups(directory: &Path, keep: usize) -> usize {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return 0;
    };
    let mut backups: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("bard.db.bak-")
        })
        .map(|entry| entry.path())
        .collect();
    backups.sort_by(|left, right| {
        let left_modified = left
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok();
        let right_modified = right
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok();
        left_modified
            .cmp(&right_modified)
            .then_with(|| left.cmp(right))
    });
    if backups.len() <= keep {
        return 0;
    }
    let mut removed = 0;
    for path in &backups[..backups.len() - keep] {
        if std::fs::remove_file(path).is_ok() {
            removed += 1;
        }
    }
    removed
}

fn configure(conn: &Connection, db_path: Option<&Path>) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // NORMAL is the documented durability companion to WAL, and the right
    // trade here. SQLite documents NORMAL in WAL mode as application-crash
    // durable — a trust case for a single-user desktop history: a process
    // that dies mid-write loses no committed transaction — while a power loss
    // can lose the last transaction but never corrupt the database. FULL
    // fsyncs every commit instead, which for a clipboard history that writes
    // continuously is a disk sync per capture for durability against the one
    // event this product cannot distinguish from a crash anyway. Neither
    // `secure_delete` below nor the delete path's `wal_checkpoint(TRUNCATE)`
    // is affected: NORMAL changes when the WAL is synced, not what a
    // checkpoint does with it.
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Zero freed pages instead of leaving them readable in the main database file.
    // The delete path truncates the write-ahead log, which covers the log; this
    // covers the pages the log had already written into the database itself.
    // Without it "Clear captures" removes the text from queries while leaving it
    // recoverable from `bard.db` with a hex editor — which is not what someone
    // clearing a password out of their history was told would happen.
    //
    // The cost is a write-path memset on every delete. That is the right trade for
    // a local-first product whose pitch is that your data is yours, and the delete
    // path is not a hot loop.
    conn.pragma_update(None, "secure_delete", "ON")?;

    // Detect a migration before the base schema is created: `CREATE TABLE IF
    // NOT EXISTS` makes every later check pass, so "did this file already hold
    // a database" has to be answered first. A file that already holds a
    // person's history is snapshotted before any schema change touches it.
    let preexisting_items = table_has_column(conn, "items", "id")?;
    if preexisting_items {
        if let Some(db_path) = db_path {
            let version: i64 = conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .unwrap_or(0);
            let missing_column = MIGRATIONS
                .iter()
                .any(|(table, column, _)| !table_has_column(conn, table, column).unwrap_or(true));
            let missing_packs = !table_has_column(conn, "packs", "id")?;
            if version < SCHEMA_VERSION || missing_column || missing_packs {
                match snapshot_before_migration(conn, db_path, version) {
                    Ok(path) => {
                        crate::crash::record_event("db:migration-snapshot");
                        log::info!("migration snapshot written to {}", path.display());
                    }
                    // A snapshot that could not be taken does not block the
                    // migration: refusing to open the database is the worse
                    // failure. It is recorded so the gap is visible.
                    Err(error) => {
                        crate::crash::record_event("db:migration-snapshot-failed");
                        log::warn!("could not snapshot before a schema migration: {error}");
                    }
                }
            }
        }
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS items(
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           section_id INTEGER REFERENCES sections(id) ON DELETE SET NULL,
           content TEXT NOT NULL,
           kind TEXT NOT NULL DEFAULT 'text',
           image_path TEXT,
           origin TEXT NOT NULL DEFAULT 'manual'
             CHECK(origin IN ('manual', 'clipboard', 'dictation')),
           favorite INTEGER NOT NULL DEFAULT 0,
           done INTEGER NOT NULL DEFAULT 0,
           created_at INTEGER NOT NULL,
           updated_at INTEGER NOT NULL,
           pinned_at INTEGER
         );
         CREATE INDEX IF NOT EXISTS idx_items_created ON items(created_at DESC);
         CREATE TABLE IF NOT EXISTS settings(
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS sections(
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           name TEXT NOT NULL,
           created_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS attachments(
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
           path TEXT NOT NULL,
           position INTEGER NOT NULL DEFAULT 0,
           created_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_attachments_item ON attachments(item_id);
         CREATE TABLE IF NOT EXISTS custom_models(
           id TEXT PRIMARY KEY,
           repo TEXT NOT NULL,
           file TEXT NOT NULL,
           size_mb INTEGER NOT NULL DEFAULT 0,
           size_bytes INTEGER NOT NULL DEFAULT 0,
           sha256 TEXT NOT NULL DEFAULT '',
           local_path TEXT NOT NULL,
           ownership TEXT NOT NULL DEFAULT 'external',
           compatibility TEXT NOT NULL DEFAULT 'unverified',
           created_at INTEGER NOT NULL
         );",
    )?;

    // All schema changes are one transaction. The old implementation ignored
    // every ALTER error, which could leave a half-migrated database that only
    // failed later through an unrelated unwrap.
    let compatibility_existed = table_has_column(conn, "custom_models", "compatibility")?;
    let transaction = conn.unchecked_transaction()?;
    for (table, column, statement) in MIGRATIONS {
        if !table_has_column(&transaction, table, column)? {
            transaction.execute(statement, [])?;
        }
    }
    if !compatibility_existed {
        transaction.execute(
            "UPDATE custom_models SET compatibility = 'verified' WHERE compatibility = 'unverified'",
            [],
        )?;
    }
    // v4 adds packs: named groups of kept items for the shelf's Packs zone.
    transaction.execute_batch(&format!(
        "CREATE INDEX IF NOT EXISTS idx_items_origin_created
           ON items(origin, created_at);
         CREATE TABLE IF NOT EXISTS packs(
           id INTEGER PRIMARY KEY,
           name TEXT NOT NULL,
           created_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS pack_items(
           pack_id INTEGER NOT NULL REFERENCES packs(id) ON DELETE CASCADE,
           item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
           position INTEGER NOT NULL,
           PRIMARY KEY (pack_id, item_id)
         );
         CREATE INDEX IF NOT EXISTS idx_pack_items_item ON pack_items(item_id);
         PRAGMA user_version = {SCHEMA_VERSION};",
    ))?;
    transaction.commit()?;
    Ok(())
}

/// Whether the database is open.
///
/// Window events fire before the builder's managed state exists, so anything
/// reading settings from that path has to ask first. The connection itself is
/// always present once [`Db`] is managed: [`open_or_recover`] never fails to
/// yield one.
pub fn is_ready(app: &AppHandle) -> bool {
    app.try_state::<Db>().is_some()
}

fn with_conn<T>(app: &AppHandle, f: impl FnOnce(&Connection) -> T) -> T {
    let db = app.state::<Db>();
    // A poisoned lock means a reader panicked while holding it. Recovering the
    // guard keeps the panic from taking a second thread down on the way out —
    // under `panic = "abort"` there is no unwinding for a second chance.
    let guard = db.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&guard)
}

fn with_conn_mut<T>(app: &AppHandle, f: impl FnOnce(&mut Connection) -> T) -> T {
    let db = app.state::<Db>();
    let mut guard = db.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard)
}

pub fn get_setting(app: &AppHandle, key: &str) -> Option<String> {
    // Before the database is open there are no settings, and saying so is
    // correct. Panicking instead took the whole app down at launch — windows
    // are created before setup() runs, so anything reacting to a window event
    // can land here first, and under `panic = "abort"` it dies silently.
    if !is_ready(app) {
        return None;
    }
    with_conn(app, |c| {
        c.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .ok()
    })
}

pub fn set_setting(app: &AppHandle, key: &str, value: &str) {
    // Dropping a write is better than dying; nothing writes a setting before
    // the database exists except paths that fire during window creation.
    if !is_ready(app) {
        return;
    }
    with_conn_mut(app, |c| {
        let _ = c.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        );
    });
}

/// Fallible counterpart for load-bearing writes such as model activation. A
/// verified model must not be reported as active when the setting did not save.
pub fn set_setting_checked(app: &AppHandle, key: &str, value: &str) -> Result<(), String> {
    set_settings_checked(app, &[(key, value)])
}

/// Fallible multi-setting write used by the post-processing form so its
/// enabled/model pair cannot be persisted half-way.
pub fn set_settings_checked(app: &AppHandle, settings: &[(&str, &str)]) -> Result<(), String> {
    if !is_ready(app) {
        return Err("Database is not ready.".into());
    }
    with_conn_mut(app, |connection| {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("Could not begin setting transaction: {error}"))?;
        for (key, value) in settings {
            transaction
                .execute(
                    "INSERT INTO settings(key, value) VALUES(?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, value],
                )
                .map_err(|error| format!("Could not save setting {key}: {error}"))?;
        }
        transaction
            .commit()
            .map_err(|error| format!("Could not commit settings: {error}"))
    })
}

/// The rowid returned when an insert did not happen. SQLite rowids start at 1,
/// so 0 is unambiguous, and it is the only value a caller must check.
pub const INSERT_FAILED: i64 = 0;

pub fn insert_item(app: &AppHandle, content: &str, kind: &str, image_path: Option<&str>) -> i64 {
    insert_item_with_origin(app, content, kind, image_path, "manual")
}

/// Insert an item with an explicit lifecycle origin. `manual` is the default
/// so composer notes, drawings, and attached files remain user-owned; the
/// clipboard and dictation paths pass their own origin for retention cleanup.
///
/// Returns [`INSERT_FAILED`] rather than panicking when the write cannot happen.
/// The release profile is `panic = "abort"`, so an `unwrap` on `execute` turned a
/// full disk or a locked database into the whole process dying with no window,
/// no message, and a crash log the user cannot reach without an app that still
/// runs. Bard writes continuously in the background, so a full disk is an
/// ordinary event that has to produce a message, not an abort.
pub fn insert_item_with_origin(
    app: &AppHandle,
    content: &str,
    kind: &str,
    image_path: Option<&str>,
    origin: &str,
) -> i64 {
    let now = now_ms();
    let section = get_active_section(app);
    let origin = if matches!(origin, "clipboard" | "dictation") {
        origin
    } else {
        "manual"
    };
    let written = with_conn_mut(app, |c| {
        c.execute(
            "INSERT INTO items(section_id, content, kind, image_path, origin, favorite, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)",
            params![section, content, kind, image_path, origin, now],
        )
        .map(|_| c.last_insert_rowid())
    });
    match written {
        Ok(id) => id,
        Err(error) => {
            crate::crash::record_event("db:insert-failed");
            log::warn!("could not store an item: {error}");
            // A background agent that cannot write must say so rather than
            // appearing to have captured something. A full disk is the ordinary
            // cause and the message is written for a person, not a log reader.
            let _ = tauri::Emitter::emit(
                app,
                "db-write-failed",
                "Bard could not save this. There may be no free space left on this drive.",
            );
            INSERT_FAILED
        }
    }
}

pub fn get_active_section(app: &AppHandle) -> Option<i64> {
    get_setting(app, "active_section")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("id").and_then(|i| i.as_i64()))
        .filter(|id| section_exists(app, *id))
}

pub fn set_active_section(app: &AppHandle, id: Option<i64>) {
    match id {
        Some(i) => set_setting(
            app,
            "active_section",
            &serde_json::json!({"id": i}).to_string(),
        ),
        None => set_setting(app, "active_section", ""),
    }
}

pub fn set_active_item(app: &AppHandle, id: Option<i64>) {
    match id.filter(|id| item_exists(app, *id)) {
        Some(id) => set_setting(app, "active_item_id", &id.to_string()),
        None => set_setting(app, "active_item_id", ""),
    }
}

pub fn active_item(app: &AppHandle) -> Option<i64> {
    get_setting(app, "active_item_id")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|id| item_exists(app, *id))
        // The fallback is the newest item, so one row is the whole answer —
        // this used to read the entire timeline to take its first element.
        .or_else(|| list_items_windowed(app, 1).first().map(|item| item.id))
}

const ITEM_COLUMNS: &str = "id, section_id, content, kind, image_path, origin, favorite, done, \
                            created_at, updated_at, pinned_at";

fn item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    Ok(Item {
        id: row.get(0)?,
        section_id: row.get(1)?,
        content: row.get(2)?,
        kind: row.get(3)?,
        image_path: row.get(4)?,
        origin: row.get(5)?,
        favorite: row.get(6)?,
        done: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        pinned_at: row.get(10)?,
    })
}

pub fn item_by_id(app: &AppHandle, id: i64) -> Option<Item> {
    if !is_ready(app) {
        return None;
    }
    with_conn(app, |connection| {
        connection
            .query_row(
                &format!("SELECT {ITEM_COLUMNS} FROM items WHERE id = ?1"),
                params![id],
                item_from_row,
            )
            .ok()
    })
}

fn item_exists(app: &AppHandle, id: i64) -> bool {
    with_conn(app, |connection| {
        connection
            .query_row("SELECT 1 FROM items WHERE id = ?1", params![id], |_| Ok(()))
            .is_ok()
    })
}

fn section_exists(app: &AppHandle, id: i64) -> bool {
    with_conn(app, |c| {
        c.query_row("SELECT 1 FROM sections WHERE id = ?1", params![id], |_| {
            Ok(())
        })
        .is_ok()
    })
}

pub fn find_or_create_section(app: &AppHandle, name: &str) -> i64 {
    let trimmed = name.trim();
    let existing = with_conn(app, |c| {
        c.query_row(
            "SELECT id FROM sections WHERE name = ?1 COLLATE NOCASE",
            params![trimmed],
            |r| r.get(0),
        )
        .ok()
    });
    if let Some(id) = existing {
        return id;
    }
    // Same reasoning as `insert_item_with_origin`: a full disk must not take the
    // process down. Falls back to the default section, which always exists.
    let created = with_conn_mut(app, |c| {
        c.execute(
            "INSERT INTO sections(name, created_at) VALUES(?1, ?2)",
            params![trimmed, now_ms()],
        )
        .map(|_| c.last_insert_rowid())
    });
    match created {
        Ok(id) => id,
        Err(error) => {
            crate::crash::record_event("db:section-insert-failed");
            log::warn!("could not create a section: {error}");
            default_section(app)
        }
    }
}

/// The section every item belongs to when none has been chosen.
fn default_section(app: &AppHandle) -> i64 {
    with_conn(app, |c| {
        c.query_row("SELECT id FROM sections ORDER BY id LIMIT 1", [], |row| {
            row.get(0)
        })
        .unwrap_or(INSERT_FAILED)
    })
}

pub fn list_sections(app: &AppHandle) -> Vec<Section> {
    with_conn(app, |c| {
        // A statement that cannot be prepared or stepped is recorded and
        // answered with nothing. It used to unwrap, which under `panic = "abort"`
        // turned an unreadable database into a silent process death.
        let Ok(mut stmt) = c.prepare("SELECT id, name FROM sections ORDER BY id") else {
            crate::crash::record_event("db:sections-read-failed");
            return Vec::new();
        };
        let sections = match stmt.query_map([], |r| {
            Ok(Section {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        }) {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(error) => {
                crate::crash::record_event("db:sections-read-failed");
                log::warn!("could not read sections: {error}");
                Vec::new()
            }
        };
        sections
    })
}

pub fn rename_section(app: &AppHandle, id: i64, name: &str) {
    with_conn_mut(app, |c| {
        let _ = c.execute(
            "UPDATE sections SET name = ?1 WHERE id = ?2",
            params![name.trim(), id],
        );
    });
}

pub fn delete_section(app: &AppHandle, id: i64) {
    with_conn_mut(app, |c| {
        let _ = c.execute(
            "UPDATE items SET section_id = NULL WHERE section_id = ?1",
            params![id],
        );
        let _ = c.execute("DELETE FROM sections WHERE id = ?1", params![id]);
    });
    if get_active_section(app) == Some(id) {
        set_active_section(app, None);
    }
}

pub fn insert_attachment(app: &AppHandle, item_id: i64, path: &str) -> i64 {
    with_conn_mut(app, |c| {
        let pos: i64 = c
            .query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM attachments WHERE item_id = ?1",
                params![item_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        // An attachment is a convenience, never a prerequisite. If the row cannot
        // be written the file is still on disk and the item still exists, so a
        // failure is recorded and reported rather than aborting the process.
        c.execute(
            "INSERT INTO attachments(item_id, path, position, created_at) VALUES(?1, ?2, ?3, ?4)",
            params![item_id, path, pos, now_ms()],
        )
        .map(|_| c.last_insert_rowid())
    })
    .unwrap_or_else(|error| {
        crate::crash::record_event("db:attachment-insert-failed");
        log::warn!("could not record an attachment: {error}");
        let _ = tauri::Emitter::emit(app, "db-write-failed", error.to_string());
        INSERT_FAILED
    })
}

pub fn list_attachments_for(app: &AppHandle, item_id: i64) -> Vec<Attachment> {
    read_attachments(
        app,
        "SELECT id, item_id, path, position FROM attachments WHERE item_id = ?1 ORDER BY position",
        params![item_id],
    )
}

pub fn list_all_attachments(app: &AppHandle) -> Vec<Attachment> {
    read_attachments(
        app,
        "SELECT id, item_id, path, position FROM attachments ORDER BY id",
        [],
    )
}

/// The attachment rows for one statement, or nothing when it cannot be read.
/// The same shape `list_items` uses: a failure is recorded, never a panic.
fn read_attachments(
    app: &AppHandle,
    sql: &str,
    parameters: impl rusqlite::Params,
) -> Vec<Attachment> {
    with_conn(app, |c| {
        let Ok(mut stmt) = c.prepare(sql) else {
            crate::crash::record_event("db:attachments-read-failed");
            return Vec::new();
        };
        let attachments = match stmt.query_map(parameters, |r| {
            Ok(Attachment {
                id: r.get(0)?,
                item_id: r.get(1)?,
                path: r.get(2)?,
                position: r.get(3)?,
            })
        }) {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(error) => {
                crate::crash::record_event("db:attachments-read-failed");
                log::warn!("could not read attachments: {error}");
                Vec::new()
            }
        };
        attachments
    })
}

pub fn attachment_path(app: &AppHandle, id: i64) -> Option<String> {
    with_conn(app, |c| {
        c.query_row(
            "SELECT path FROM attachments WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .ok()
    })
}

pub fn delete_attachment(app: &AppHandle, id: i64) -> Result<Option<String>, String> {
    let path = with_conn(app, |c| {
        c.query_row(
            "SELECT path FROM attachments WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .ok()
    });
    with_conn_mut(app, |connection| {
        connection
            .execute("DELETE FROM attachments WHERE id = ?1", params![id])
            .map_err(|error| format!("Could not delete attachment: {error}"))
    })?;
    Ok(path)
}

pub fn update_item(app: &AppHandle, id: i64, content: &str) {
    with_conn_mut(app, |c| {
        let _ = c.execute(
            "UPDATE items SET content = ?1, updated_at = ?2 WHERE id = ?3",
            params![content, now_ms(), id],
        );
    });
}

pub fn set_favorite(app: &AppHandle, id: i64, favorite: bool) {
    with_conn_mut(app, |c| {
        let _ = c.execute(
            "UPDATE items SET favorite = ?1, updated_at = ?2 WHERE id = ?3",
            params![favorite as i64, now_ms(), id],
        );
    });
}

pub fn set_done(app: &AppHandle, id: i64, done: bool) {
    with_conn_mut(app, |c| {
        let _ = c.execute(
            "UPDATE items SET done = ?1, updated_at = ?2 WHERE id = ?3",
            params![done as i64, now_ms(), id],
        );
    });
}

/// Items cleared this way are gone, images and all — the caller unlinks the
/// files first, exactly as clear_history does.
pub fn completed_items(app: &AppHandle) -> Vec<Item> {
    list_items(app)
        .into_iter()
        .filter(|i| i.done != 0)
        .collect()
}

pub fn delete_all_data(app: &AppHandle) -> Result<(), String> {
    if !is_ready(app) {
        return Err("Database is not ready.".into());
    }
    let mut files = Vec::new();
    for item in list_items(app) {
        files.extend(item_files(app, item.id));
    }
    with_conn_mut(app, |connection| {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("Could not begin data deletion: {error}"))?;
        for table in [
            "items",
            "attachments",
            "pack_items",
            "packs",
            "sections",
            "settings",
            "custom_models",
        ] {
            transaction
                .execute(&format!("DELETE FROM {table}"), [])
                .map_err(|error| format!("Could not clear {table}: {error}"))?;
        }
        transaction
            .commit()
            .map_err(|error| format!("Could not commit data deletion: {error}"))
    })?;
    with_conn_mut(app, |connection| {
        let _ = connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
        let _ = connection.execute_batch("VACUUM");
    });
    for file in files {
        remove_owned_image_file(app, &file);
    }
    if let Some(root) = crate::paths::data_dir() {
        let _ = std::fs::remove_dir_all(root.join("images"));
    }
    let _ = std::fs::remove_dir_all(crate::hf::local_models_dir(app));
    let _ = std::fs::remove_dir_all(crate::paths::logs_dir());
    Ok(())
}

pub fn add_note_to_image(app: &AppHandle, id: i64) -> Option<String> {
    let path = with_conn(app, |c| {
        c.query_row(
            "SELECT image_path FROM items WHERE id = ?1",
            params![id],
            |r| r.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten()
    });
    with_conn_mut(app, |c| {
        let _ = c.execute(
            "UPDATE items SET kind = 'text', content = '', image_path = NULL WHERE id = ?1",
            params![id],
        );
    });
    if let Some(p) = path.clone() {
        insert_attachment(app, id, &p);
    }
    path
}

/// Where captured images live. Resolved from the environment rather than from
/// managed state: a window can ask for an image before setup() has run, and a
/// read that raced it used to come back empty for good.
pub fn images_root() -> Option<std::path::PathBuf> {
    crate::paths::data_dir().map(|root| root.join("images"))
}

pub fn remove_owned_image_file(app: &AppHandle, path: &str) {
    let _ = app;
    let Some(images) = images_root() else {
        return;
    };
    if crate::hf::path_is_within(&images, std::path::Path::new(path)) {
        let _ = std::fs::remove_file(path);
    }
}

pub fn item_files(app: &AppHandle, id: i64) -> Vec<String> {
    let mut files: Vec<String> = with_conn(app, |c| {
        c.query_row(
            "SELECT image_path FROM items WHERE id = ?1",
            params![id],
            |r| r.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten()
    })
    .into_iter()
    .collect();
    files.extend(list_attachments_for(app, id).into_iter().map(|a| a.path));
    files
}

pub fn list_items(app: &AppHandle) -> Vec<Item> {
    with_conn(app, |c| {
        let Ok(mut stmt) = c.prepare(&format!(
            "SELECT {ITEM_COLUMNS} FROM items ORDER BY id DESC"
        )) else {
            crate::crash::record_event("db:items-read-failed");
            return Vec::new();
        };
        let items = match stmt.query_map([], item_from_row) {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(error) => {
                crate::crash::record_event("db:items-read-failed");
                log::warn!("could not read items: {error}");
                Vec::new()
            }
        };
        items
    })
}

/// The timeline's own read: newest first, with the automatic items bounded by
/// [`AUTOMATIC_ITEM_CEILING`] and everything a person made by hand exempt.
///
/// This is where the ceiling belongs. It is not a deletion — nothing here
/// removes a row, and the age-based retention setting is still the only
/// automatic deletion — it is a bound on how much history one refresh has to
/// re-read and re-serialise into every webview. The panel paints eight cards
/// from that read, so an unbounded read grows forever while the screen does
/// not: it was three or four megabytes of JSON per capture at a few thousand
/// items, and the felt capture latency grew with it.
///
/// Manual items, favourites and pins are outside the bound. They are the
/// library a person built deliberately and they are small by construction,
/// while a shelf that silently hides one of them — an old pin especially,
/// which is also a Right Shift × 2 paste slot — would be worse than any
/// amount of reading.
fn list_items_windowed(app: &AppHandle, limit: i64) -> Vec<Item> {
    with_conn(app, |c| {
        let mut stmt = c
            .prepare(&format!(
                "SELECT {ITEM_COLUMNS} FROM items
                 WHERE origin NOT IN ('clipboard', 'dictation')
                    OR favorite = 1
                    OR pinned_at IS NOT NULL
                    OR id IN (
                        SELECT id FROM items
                        WHERE origin IN ('clipboard', 'dictation')
                          AND favorite = 0
                          AND pinned_at IS NULL
                        ORDER BY id DESC LIMIT ?1
                    )
                 ORDER BY id DESC"
            ))
            .unwrap();
        stmt.query_map(params![limit], item_from_row)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_default()
    })
}

/// How many leading members a pack previews.
const PACK_PREVIEW_LIMIT: i64 = 4;

/// A pack name is trimmed, non-empty and no longer than this.
const PACK_NAME_MAX: usize = 60;

fn clean_pack_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the pack a name.".into());
    }
    if name.chars().count() > PACK_NAME_MAX {
        return Err(format!(
            "Pack names can be at most {PACK_NAME_MAX} characters."
        ));
    }
    Ok(name.to_string())
}

fn pack_exists(connection: &Connection, id: i64) -> bool {
    connection
        .query_row("SELECT 1 FROM packs WHERE id = ?1", params![id], |_| Ok(()))
        .is_ok()
}

/// Every pack with its member count and the leading members a card previews.
///
/// Two statements cover every pack: one for the counts, one for the leading
/// members of all packs at once. A preview query per pack would be an N+1 as
/// the shelf repaints on every refresh.
fn read_packs(connection: &Connection) -> Vec<PackSummary> {
    let Ok(mut statement) = connection.prepare(
        "SELECT p.id, p.name, (SELECT COUNT(*) FROM pack_items WHERE pack_id = p.id)
         FROM packs p ORDER BY p.id",
    ) else {
        return Vec::new();
    };
    let mut packs: Vec<PackSummary> = statement
        .query_map([], |row| {
            Ok(PackSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                item_count: row.get(2)?,
                preview_item_ids: Vec::new(),
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default();

    let Ok(mut statement) = connection.prepare(
        "SELECT pack_id, item_id FROM (
             SELECT pack_id, item_id,
                    ROW_NUMBER() OVER (PARTITION BY pack_id ORDER BY position) AS place
             FROM pack_items
         )
         WHERE place <= ?1
         ORDER BY pack_id, place",
    ) else {
        return packs;
    };
    let mut previews: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
    if let Ok(rows) = statement.query_map(params![PACK_PREVIEW_LIMIT], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    }) {
        for (pack_id, item_id) in rows.filter_map(Result::ok) {
            previews.entry(pack_id).or_default().push(item_id);
        }
    }
    for pack in &mut packs {
        pack.preview_item_ids = previews.remove(&pack.id).unwrap_or_default();
    }
    packs
}

/// One pack with its members in pack order, or `None` when it is gone.
fn read_pack(connection: &Connection, id: i64) -> Option<PackDetail> {
    let name: String = connection
        .query_row("SELECT name FROM packs WHERE id = ?1", params![id], |row| {
            row.get(0)
        })
        .ok()?;
    let mut statement = connection
        .prepare(&format!(
            "SELECT {ITEM_COLUMNS}
             FROM items JOIN pack_items ON pack_items.item_id = items.id
             WHERE pack_items.pack_id = ?1
             ORDER BY pack_items.position"
        ))
        .ok()?;
    let items: Vec<Item> = statement
        .query_map(params![id], item_from_row)
        .ok()?
        .filter_map(Result::ok)
        .collect();
    let preview_item_ids = items
        .iter()
        .take(PACK_PREVIEW_LIMIT as usize)
        .map(|item| item.id)
        .collect();
    Some(PackDetail {
        pack: PackSummary {
            id,
            name,
            item_count: items.len() as i64,
            preview_item_ids,
        },
        items,
    })
}

/// Append members at the end of the pack and return the ids that landed, in
/// order. An id that is already a member or no longer exists is skipped rather
/// than fatal, and the position advances only for a row that actually landed.
fn append_pack_items(
    connection: &Connection,
    pack_id: i64,
    item_ids: &[i64],
) -> Result<Vec<i64>, String> {
    if !pack_exists(connection, pack_id) {
        return Err("That pack no longer exists.".into());
    }
    let mut position: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM pack_items WHERE pack_id = ?1",
            params![pack_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("Could not read the pack's items: {error}"))?;
    let mut added = Vec::new();
    for item_id in item_ids {
        // The SELECT is the whole filter: it yields no row for an id that no
        // longer exists, and INSERT OR IGNORE drops one already in the pack.
        let inserted = connection
            .execute(
                "INSERT OR IGNORE INTO pack_items(pack_id, item_id, position)
                 SELECT ?1, id, ?2 FROM items WHERE id = ?3",
                params![pack_id, position, item_id],
            )
            .map_err(|error| format!("Could not add to the pack: {error}"))?;
        if inserted == 1 {
            added.push(*item_id);
            position += 1;
        }
    }
    Ok(added)
}

fn insert_pack(
    connection: &mut Connection,
    name: &str,
    item_ids: &[i64],
    now: i64,
) -> Result<PackSummary, String> {
    let name = clean_pack_name(name)?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("Could not create the pack: {error}"))?;
    transaction
        .execute(
            "INSERT INTO packs(name, created_at) VALUES(?1, ?2)",
            params![name, now],
        )
        .map_err(|error| format!("Could not create the pack: {error}"))?;
    let id = transaction.last_insert_rowid();
    let added = append_pack_items(&transaction, id, item_ids)?;
    transaction
        .commit()
        .map_err(|error| format!("Could not create the pack: {error}"))?;
    Ok(PackSummary {
        id,
        name,
        item_count: added.len() as i64,
        preview_item_ids: added
            .into_iter()
            .take(PACK_PREVIEW_LIMIT as usize)
            .collect(),
    })
}

fn set_pack_name(connection: &Connection, id: i64, name: &str) -> Result<(), String> {
    let name = clean_pack_name(name)?;
    let changed = connection
        .execute(
            "UPDATE packs SET name = ?1 WHERE id = ?2",
            params![name, id],
        )
        .map_err(|error| format!("Could not rename the pack: {error}"))?;
    if changed == 0 {
        return Err("That pack no longer exists.".into());
    }
    Ok(())
}

fn remove_pack(connection: &Connection, id: i64) -> Result<(), String> {
    let changed = connection
        .execute("DELETE FROM packs WHERE id = ?1", params![id])
        .map_err(|error| format!("Could not delete the pack: {error}"))?;
    if changed == 0 {
        return Err("That pack no longer exists.".into());
    }
    Ok(())
}

fn remove_pack_items(
    connection: &Connection,
    pack_id: i64,
    item_ids: &[i64],
) -> Result<(), String> {
    if !pack_exists(connection, pack_id) {
        return Err("That pack no longer exists.".into());
    }
    if item_ids.is_empty() {
        return Ok(());
    }
    let placeholders = vec!["?"; item_ids.len()].join(",");
    let bound = std::iter::once(pack_id).chain(item_ids.iter().copied());
    connection
        .execute(
            &format!("DELETE FROM pack_items WHERE pack_id = ?1 AND item_id IN ({placeholders})"),
            rusqlite::params_from_iter(bound),
        )
        .map(|_| ())
        .map_err(|error| format!("Could not remove from the pack: {error}"))
}

fn reorder_pack_items(
    connection: &mut Connection,
    pack_id: i64,
    item_ids: &[i64],
) -> Result<(), String> {
    if !pack_exists(connection, pack_id) {
        return Err("That pack no longer exists.".into());
    }
    let current: Vec<i64> = connection
        .prepare("SELECT item_id FROM pack_items WHERE pack_id = ?1 ORDER BY position")
        .map_err(|error| format!("Could not read the pack's items: {error}"))?
        .query_map(params![pack_id], |row| row.get(0))
        .map_err(|error| format!("Could not read the pack's items: {error}"))?
        .filter_map(Result::ok)
        .collect();
    // The caller shows what it believes the pack holds; anything else — a
    // removal, an addition, a duplicate or an id that is gone — is refused so
    // a stale view can never quietly drop or reorder members it did not see.
    if current.len() != item_ids.len()
        || current.iter().any(|id| !item_ids.contains(id))
        || item_ids.iter().any(|id| !current.contains(id))
    {
        return Err("The pack's items changed. Reopen it and try again.".into());
    }
    let transaction = connection
        .transaction()
        .map_err(|error| format!("Could not save the new order: {error}"))?;
    for (position, item_id) in item_ids.iter().enumerate() {
        transaction
            .execute(
                "UPDATE pack_items SET position = ?1 WHERE pack_id = ?2 AND item_id = ?3",
                params![position as i64, pack_id, item_id],
            )
            .map_err(|error| format!("Could not save the new order: {error}"))?;
    }
    transaction
        .commit()
        .map_err(|error| format!("Could not save the new order: {error}"))
}

/// The shelf's Packs zone: every pack with its member count and previews.
pub fn list_packs(app: &AppHandle) -> Vec<PackSummary> {
    if !is_ready(app) {
        return Vec::new();
    }
    with_conn(app, read_packs)
}

/// One pack with its items, for the open pack view.
pub fn get_pack(app: &AppHandle, id: i64) -> Result<PackDetail, String> {
    if !is_ready(app) {
        return Err("Database is not ready.".into());
    }
    with_conn(app, |connection| read_pack(connection, id))
        .ok_or_else(|| "That pack no longer exists.".into())
}

/// Create a pack from a selection. Ids that no longer exist are skipped, and
/// the caller's order is kept.
pub fn create_pack(app: &AppHandle, name: &str, item_ids: &[i64]) -> Result<PackSummary, String> {
    if !is_ready(app) {
        return Err("Database is not ready.".into());
    }
    with_conn_mut(app, |connection| {
        insert_pack(connection, name, item_ids, now_ms())
    })
}

pub fn rename_pack(app: &AppHandle, id: i64, name: &str) -> Result<(), String> {
    if !is_ready(app) {
        return Err("Database is not ready.".into());
    }
    with_conn(app, |connection| set_pack_name(connection, id, name))
}

pub fn delete_pack(app: &AppHandle, id: i64) -> Result<(), String> {
    if !is_ready(app) {
        return Err("Database is not ready.".into());
    }
    with_conn(app, |connection| remove_pack(connection, id))
}

/// Append items to a pack; memberships already there are left alone.
pub fn pack_add_items(app: &AppHandle, id: i64, item_ids: &[i64]) -> Result<(), String> {
    if !is_ready(app) {
        return Err("Database is not ready.".into());
    }
    with_conn_mut(app, |connection| {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("Could not update the pack: {error}"))?;
        append_pack_items(&transaction, id, item_ids)?;
        transaction
            .commit()
            .map_err(|error| format!("Could not update the pack: {error}"))
    })
}

pub fn pack_remove_items(app: &AppHandle, id: i64, item_ids: &[i64]) -> Result<(), String> {
    if !is_ready(app) {
        return Err("Database is not ready.".into());
    }
    with_conn(app, |connection| {
        remove_pack_items(connection, id, item_ids)
    })
}

pub fn pack_reorder(app: &AppHandle, id: i64, item_ids: &[i64]) -> Result<(), String> {
    if !is_ready(app) {
        return Err("Database is not ready.".into());
    }
    with_conn_mut(app, |connection| {
        reorder_pack_items(connection, id, item_ids)
    })
}

/// Remove expired automatic items and every owned file attached to them.
/// `None` means the user selected Forever. Favorites and manual work are never
/// removed by age-based cleanup.
///
/// Returns how many rows went, so a caller that has just told the panel about
/// one new item can tell whether the same pass also took some away — the panel
/// splices the new item in without re-reading, and only a non-zero count here
/// makes that splice incomplete.
pub fn cleanup_origin(app: &AppHandle, origin: &str) -> Result<usize, String> {
    if !matches!(origin, "clipboard" | "dictation") {
        return Ok(0);
    }
    let Some(days) = retention_days(app, origin) else {
        // Forever means Forever. Returning early is the whole contract: the old
        // code fell through to a count cap that deleted the user's data anyway,
        // which is the opposite of what they asked for.
        return Ok(0);
    };
    let cutoff = now_ms().saturating_sub(days.saturating_mul(24 * 60 * 60 * 1000));
    // `with_conn` takes a plain-value closure, so the fallible read is
    // collected into a `Result` and unwrapped outside it. A prepare or query
    // failure is reported rather than unwrapped: it used to be `.unwrap()`, which
    // under `panic = "abort"` killed the process.
    let ids = with_conn(app, |c| -> Result<Vec<i64>, String> {
        let mut stmt = c
            .prepare(
                "SELECT id FROM items
                 WHERE origin = ?1 AND favorite = 0 AND created_at < ?2",
            )
            .map_err(|error| format!("Could not read expired items: {error}"))?;
        let rows = stmt
            .query_map(params![origin, cutoff], |row| row.get::<_, i64>(0))
            .map_err(|error| format!("Could not read expired items: {error}"))?;
        Ok(rows.filter_map(Result::ok).collect())
    })?;
    let removed = ids.len();
    delete_items_with_files(app, &ids)?;
    Ok(removed)
}

/// A file written less than this long ago is never swept: a capture writes its
/// image before the row that references it, and the sweep must not race that.
const ORPHAN_GRACE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// The files under `images\` that no item and no attachment references, and
/// that are older than the grace period. Files are matched by name, not path,
/// case-insensitively: every name is a unique timestamp, and a row written
/// before the data folder moved still names the same file.
fn orphaned_images(
    files: &[(std::path::PathBuf, std::time::SystemTime)],
    referenced: &std::collections::HashSet<String>,
    now: std::time::SystemTime,
) -> Vec<std::path::PathBuf> {
    files
        .iter()
        .filter(|(path, modified)| {
            let old_enough = now
                .duration_since(*modified)
                .map(|age| age >= ORPHAN_GRACE)
                .unwrap_or(false);
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            old_enough && !name.is_empty() && !referenced.contains(&name)
        })
        .map(|(path, _)| path.clone())
        .collect()
}

/// Delete image files nothing refers to any more — left behind by a crash
/// between writing a file and its row, or by an older build's deletion path.
/// Returns how many went. A read failure sweeps nothing: an unreadable table
/// would otherwise make every file look orphaned.
pub fn sweep_orphaned_images(app: &AppHandle) -> Result<usize, String> {
    let Some(images) = images_root() else {
        return Ok(0);
    };
    let referenced = with_conn(
        app,
        |c| -> Result<std::collections::HashSet<String>, String> {
            let mut paths = std::collections::HashSet::new();
            for sql in [
                "SELECT image_path FROM items WHERE image_path IS NOT NULL",
                "SELECT path FROM attachments",
            ] {
                let mut stmt = c
                    .prepare(sql)
                    .map_err(|error| format!("Could not read image references: {error}"))?;
                let rows = stmt
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(|error| format!("Could not read image references: {error}"))?;
                for path in rows {
                    let path =
                        path.map_err(|error| format!("Could not read image references: {error}"))?;
                    if let Some(name) = std::path::Path::new(&path).file_name() {
                        paths.insert(name.to_string_lossy().to_lowercase());
                    }
                }
            }
            Ok(paths)
        },
    )?;
    let Ok(entries) = std::fs::read_dir(&images) else {
        return Ok(0);
    };
    let files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some((entry.path(), meta.modified().ok()?))
        })
        .collect();
    let orphans = orphaned_images(&files, &referenced, std::time::SystemTime::now());
    let mut removed = 0;
    for path in orphans {
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// The age applied to a kind of automatic item when the user has expressed no
/// preference.
///
/// These are the numbers the Data pane and both blueprints show. They were
/// previously 30 and 7 in this function while the UI showed 30 and 30, so every
/// dictation was deleted 23 days earlier than the person had been told — and
/// silently, because nothing on screen mentioned a seven-day window.
pub const DEFAULT_CLIPBOARD_RETENTION_DAYS: i64 = 30;
pub const DEFAULT_DICTATION_RETENTION_DAYS: i64 = 30;

/// Remove one origin's expired items and report how many went.
///
/// The count comes from the pass itself rather than a `COUNT(*)` taken just
/// before it — two reads and a race to say the same number.
pub fn prune_expired(app: &AppHandle, origin: &str) -> Result<usize, String> {
    cleanup_origin(app, origin)
}

/// The configured age for one automatic origin. `None` is the explicit
/// Forever choice; absent settings use the product defaults.
pub fn retention_days(app: &AppHandle, origin: &str) -> Option<i64> {
    let key = match origin {
        "clipboard" => "clipboard_retention_days",
        "dictation" => "dictation_retention_days",
        _ => return None,
    };
    let value = get_setting(app, key)
        .and_then(|raw| raw.parse::<i64>().ok())
        .unwrap_or(match origin {
            "clipboard" => DEFAULT_CLIPBOARD_RETENTION_DAYS,
            "dictation" => DEFAULT_DICTATION_RETENTION_DAYS,
            _ => DEFAULT_CLIPBOARD_RETENTION_DAYS,
        });
    if value < 0 {
        None
    } else {
        Some(value.clamp(1, 3650))
    }
}

pub fn set_retention_days(app: &AppHandle, origin: &str, days: Option<i64>) -> Result<(), String> {
    let key = match origin {
        "clipboard" => "clipboard_retention_days",
        "dictation" => "dictation_retention_days",
        _ => return Err("Unknown history type.".into()),
    };
    let value = days.unwrap_or(-1);
    if value != -1 && !(1..=3650).contains(&value) {
        return Err("Retention must be between 1 and 3650 days, or Forever.".into());
    }
    set_setting_checked(app, key, &value.to_string())
}

/// Every file path belonging to `ids`: attachment paths and the items' own
/// image paths.
///
/// The id list appears in the statement twice, so it is bound twice. Binding it
/// once fails with a parameter-count mismatch, the whole query returns nothing,
/// and the consequence is invisible: no error reaches a user, the rows are
/// deleted, and every image stays on disk forever. That is exactly what happened
/// once already, so the count is asserted rather than trusted, and the query is a
/// free function over `&Connection` so a test can reach it without an
/// `AppHandle`.
fn owned_file_paths(connection: &Connection, ids: &[i64]) -> Result<Vec<String>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT path FROM attachments WHERE item_id IN ({placeholders}) \
         UNION ALL SELECT image_path FROM items \
         WHERE image_path IS NOT NULL AND id IN ({placeholders})"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("Could not read attachments: {error}"))?;
    debug_assert_eq!(
        statement.parameter_count(),
        ids.len() * 2,
        "the statement binds each id twice"
    );
    let bound = ids.iter().chain(ids.iter());
    let rows = statement
        .query_map(rusqlite::params_from_iter(bound), |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| format!("Could not read attachments: {error}"))?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub fn delete_items_with_files(app: &AppHandle, ids: &[i64]) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    // Collect the paths first, delete the rows, unlink the files. That order is
    // what makes a crash safe in the right direction: a crash in the middle leaks
    // an orphan file, but never deletes a file a surviving row still references.
    //
    // The whole thing runs in chunks. It used to be one query per id, so pruning
    // 5,000 expired items meant 10,000 statements, each taking and releasing the
    // global connection mutex. A single `IN (...)` was the obvious replacement and
    // it was wrong twice: SQLite's default variable limit is 999, and the
    // placeholder list appears twice in the statement, so each id contributes two
    // bindings. Binding the list once made every query fail, which is silent --
    // no error reaches a user, and every image in the batch is simply orphaned on
    // disk forever. Chunking keeps the statement count and the binding count
    // honest together, and deletes exactly the rows whose paths it looked up.
    //
    // The statement is built from the placeholder list only; ids are always
    // bound, never interpolated.
    const DELETE_CHUNK: usize = 400;

    let mut files: Vec<String> = Vec::new();
    for chunk in ids.chunks(DELETE_CHUNK) {
        let found = with_conn(app, |connection| owned_file_paths(connection, chunk))?;
        files.extend(found);
    }
    files.sort();
    files.dedup();

    with_conn_mut(app, |connection| {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("Could not begin deletion: {error}"))?;
        for id in ids {
            transaction
                .execute("DELETE FROM items WHERE id = ?1", params![id])
                .map_err(|error| format!("Could not delete item {id}: {error}"))?;
        }
        transaction
            .commit()
            .map_err(|error| format!("Could not commit deletion: {error}"))
    })?;
    for path in &files {
        remove_owned_image_file(app, path);
    }
    // "Clear captures" and the retention controls are the app's central privacy
    // promise, and a person using them reasonably expects the text to be gone.
    // In WAL mode it was not: the deleted text stayed in `bard.db-wal` as earlier
    // page frames and in unvacuumed freelist pages, recoverable with nothing more
    // than a hex editor — including by a backup agent or file-sync client that
    // merely reads the directory. Truncating the write-ahead log is what makes
    // the promise true. `secure_delete` (set once in `init`) covers the main
    // database's freed pages; this covers the log.
    //
    // Deliberately not a `VACUUM`: vacuuming on every clipboard capture would
    // rewrite the whole database, and the full erase path still does that.
    let _ = with_conn_mut(app, |connection| {
        connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
    });
    if get_setting(app, "active_item_id")
        .and_then(|value| value.parse::<i64>().ok())
        .is_some_and(|active| ids.contains(&active))
    {
        set_active_item(app, None);
    }
    Ok(())
}

/// A hard ceiling on how many automatic items Bard will keep, whatever the age
/// setting says.
///
/// This exists because `prune_history` used to run on *every* capture and
/// deleted past 5,000 automatic items even when the user had chosen **Forever**,
/// with nothing on screen saying so. Silently deleting data a person explicitly
/// asked you to keep is not a safety feature; in a product whose pitch is "your
/// captures stay on this machine" it is a reason to stop trusting it.
///
/// It is therefore no longer wired into the capture path. The Data pane's
/// `Prune now` is the only caller, it reports what it did, and the age-based
/// setting is the only automatic deletion. If a runaway producer ever needs a
/// backstop, it belongs behind an explicit, visible control — not behind a
/// setting the user believes is in force.
pub const AUTOMATIC_ITEM_CEILING: usize = 5_000;

/// Delete automatic items beyond `limit`, oldest first. Manual items and
/// favourites are exempt. Returns how many rows were removed.
pub fn prune_history(app: &AppHandle, limit: usize) -> Result<usize, String> {
    let ids = with_conn(app, |c| -> Result<Vec<i64>, String> {
        let mut stmt = c
            .prepare(
                "SELECT id FROM items
                 WHERE origin IN ('clipboard', 'dictation') AND favorite = 0
                   AND id NOT IN (
                     SELECT id FROM items
                     WHERE origin IN ('clipboard', 'dictation') AND favorite = 0
                     ORDER BY id DESC LIMIT ?1
                   )",
            )
            .map_err(|error| format!("Could not read history to prune: {error}"))?;
        let rows = stmt
            .query_map(params![limit as i64], |row| row.get::<_, i64>(0))
            .map_err(|error| format!("Could not read history to prune: {error}"))?;
        Ok(rows.filter_map(Result::ok).collect())
    })?;
    let removed = ids.len();
    delete_items_with_files(app, &ids)?;
    Ok(removed)
}

pub fn clear_history(app: &AppHandle) -> Result<(), String> {
    let ids: Vec<i64> = list_items(app).into_iter().map(|item| item.id).collect();
    delete_items_with_files(app, &ids)
}

pub fn clear_captures(app: &AppHandle) -> Result<(), String> {
    let ids: Vec<i64> = with_conn(app, |c| {
        let Ok(mut stmt) = c.prepare(
            "SELECT id FROM items WHERE origin IN ('clipboard', 'dictation') AND favorite = 0",
        ) else {
            crate::crash::record_event("db:clear-captures-read-failed");
            return Vec::new();
        };
        let captures = match stmt.query_map([], |row| row.get(0)) {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(error) => {
                crate::crash::record_event("db:clear-captures-read-failed");
                log::warn!("could not read captures to clear: {error}");
                Vec::new()
            }
        };
        captures
    });
    delete_items_with_files(app, &ids)
}

const CUSTOM_MODEL_COLUMNS: &str =
    "id, repo, file, size_mb, size_bytes, sha256, local_path, ownership, compatibility";

fn custom_model_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CustomModel> {
    let ownership: String = row.get(7)?;
    let compatibility: String = row.get(8)?;
    Ok(CustomModel {
        id: row.get(0)?,
        repo: row.get(1)?,
        file: row.get(2)?,
        size_mb: row.get(3)?,
        size_bytes: row.get(4)?,
        sha256: row.get(5)?,
        local_path: row.get(6)?,
        ownership: if ownership.eq_ignore_ascii_case("app") {
            ModelOwnership::App
        } else {
            ModelOwnership::External
        },
        compatibility: match compatibility.as_str() {
            "verified" => crate::hf::ModelCompatibility::Verified,
            "incompatible" => crate::hf::ModelCompatibility::Incompatible,
            "missing" => crate::hf::ModelCompatibility::Missing,
            _ => crate::hf::ModelCompatibility::Unverified,
        },
    })
}

#[allow(clippy::too_many_arguments)]
pub fn register_custom_model(
    app: &AppHandle,
    id: &str,
    repo: &str,
    file: &str,
    size: u64,
    sha256: &str,
    local_path: &str,
    ownership: ModelOwnership,
    compatibility: crate::hf::ModelCompatibility,
) -> Result<(), String> {
    if !is_ready(app) {
        return Err("Database is not ready.".into());
    }
    let ownership_value = match ownership {
        ModelOwnership::App => "app",
        ModelOwnership::External => "external",
    };
    let compatibility_value = match compatibility {
        crate::hf::ModelCompatibility::Unverified => "unverified",
        crate::hf::ModelCompatibility::Verified => "verified",
        crate::hf::ModelCompatibility::Incompatible => "incompatible",
        crate::hf::ModelCompatibility::Missing => "missing",
    };
    with_conn_mut(app, |c| {
        c.execute(
            "INSERT INTO custom_models(
                 id, repo, file, size_mb, size_bytes, sha256, local_path,
                 ownership, compatibility, created_at
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                 repo = excluded.repo,
                 file = excluded.file,
                 size_mb = excluded.size_mb,
                 size_bytes = excluded.size_bytes,
                 sha256 = excluded.sha256,
                 local_path = excluded.local_path,
                 ownership = excluded.ownership,
                 compatibility = excluded.compatibility",
            params![
                id,
                repo,
                file,
                (size / (1024 * 1024)) as i64,
                size as i64,
                sha256,
                local_path,
                ownership_value,
                compatibility_value,
                now_ms(),
            ],
        )
        .map(|_| ())
        .map_err(|error| format!("Could not register model: {error}"))
    })
}

pub fn custom_model_by_id(app: &AppHandle, id: &str) -> Option<CustomModel> {
    if !is_ready(app) {
        return None;
    }
    with_conn(app, |c| {
        c.query_row(
            &format!("SELECT {CUSTOM_MODEL_COLUMNS} FROM custom_models WHERE id = ?1"),
            params![id],
            custom_model_from_row,
        )
        .ok()
    })
}

pub fn list_custom_models(app: &AppHandle) -> Vec<CustomModel> {
    if !is_ready(app) {
        return Vec::new();
    }
    with_conn(app, |c| {
        let Ok(mut stmt) = c.prepare(&format!(
            "SELECT {CUSTOM_MODEL_COLUMNS} FROM custom_models ORDER BY created_at"
        )) else {
            crate::crash::record_event("db:models-read-failed");
            return Vec::new();
        };
        let models = match stmt.query_map([], custom_model_from_row) {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(error) => {
                crate::crash::record_event("db:models-read-failed");
                log::warn!("could not read custom models: {error}");
                Vec::new()
            }
        };
        models
    })
}

pub fn set_custom_model_compatibility(
    app: &AppHandle,
    local_path: &str,
    compatibility: crate::hf::ModelCompatibility,
) {
    let value = match compatibility {
        crate::hf::ModelCompatibility::Unverified => "unverified",
        crate::hf::ModelCompatibility::Verified => "verified",
        crate::hf::ModelCompatibility::Incompatible => "incompatible",
        crate::hf::ModelCompatibility::Missing => "missing",
    };
    if !is_ready(app) {
        return;
    }
    with_conn_mut(app, |c| {
        let _ = c.execute(
            "UPDATE custom_models SET compatibility = ?1 WHERE local_path = ?2",
            params![value, local_path],
        );
    });
}

/// Unregister a custom model. This never clears or changes the active model and
/// never touches its file; ownership-aware file removal is the model manager's
/// job before this is called.
pub fn delete_custom_model(app: &AppHandle, id: &str) -> Result<CustomModel, String> {
    if !is_ready(app) {
        return Err("Database is not ready.".into());
    }
    if get_setting(app, "model_id").as_deref() == Some(id) {
        return Err("The active voice model cannot be deleted. Choose another model first.".into());
    }
    let model = custom_model_by_id(app, id).ok_or_else(|| format!("Unknown model: {id}"))?;
    let removed = with_conn_mut(app, |c| {
        c.execute("DELETE FROM custom_models WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())
    })?;
    if removed == 0 {
        return Err(format!("Unknown model: {id}"));
    }
    Ok(model)
}

pub fn get_state(app: &AppHandle) -> AppState {
    if !is_ready(app) {
        return AppState {
            sections: Vec::new(),
            items: Vec::new(),
            attachments: Vec::new(),
            active_section_id: None,
            theme: "system".into(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            onboarding_completed: false,
            autostart_enabled: false,
            capture_double_shift: true,
            model_id: String::new(),
            model_ready: false,
            mic_ready: false,
            active_item_id: None,
            license: crate::license::LicenseStatus::unavailable(),
            channel: crate::packaging::channel().into(),
            background: "theme".into(),
            background_color: String::new(),
            sort: SORT_ORDERS[0].into(),
            shelf_edge: "bottom".into(),
            storage_notice: None,
        };
    }
    AppState {
        sections: list_sections(app),
        items: timeline(app),
        attachments: list_all_attachments(app),
        active_section_id: get_active_section(app),
        theme: get_setting(app, "theme").unwrap_or_else(|| "system".into()),
        app_version: env!("CARGO_PKG_VERSION").into(),
        onboarding_completed: get_setting(app, "onboarding_completed") == Some("1".into()),
        autostart_enabled: get_setting(app, "autostart_enabled") == Some("1".into()),
        capture_double_shift: get_setting(app, "capture_double_shift") != Some("0".into()),
        model_id: get_setting(app, "model_id").unwrap_or_default(),
        model_ready: crate::dictation::model_ready(app),
        mic_ready: crate::dictation::mic_ready(),
        active_item_id: active_item(app),
        license: crate::license::status(app),
        channel: crate::packaging::channel().into(),
        background: get_setting(app, "background").unwrap_or_else(|| "theme".into()),
        background_color: get_setting(app, "background_color").unwrap_or_default(),
        sort: sort_order(app),
        shelf_edge: crate::geometry::shelf_edge(app).as_str().into(),
        storage_notice: storage_notice(app),
    }
}

/// What opening the database reported, if anything. Read by `get_state` and by
/// the diagnostic report, so both surfaces say the same thing.
pub fn storage_notice(app: &AppHandle) -> Option<String> {
    app.try_state::<DbNotice>()
        .and_then(|notice| notice.0.lock().ok().and_then(|guard| guard.clone()))
}

/// The orders the timeline can take. Newest first is the default and the
/// tiebreak inside the others, so a favourite or an unfinished item keeps its
/// place in time among its peers.
pub const SORT_ORDERS: [&str; 4] = ["newest", "oldest", "favorites", "completed"];

pub fn sort_order(app: &AppHandle) -> String {
    get_setting(app, "sort_order")
        .filter(|order| SORT_ORDERS.contains(&order.as_str()))
        .unwrap_or_else(|| SORT_ORDERS[0].into())
}

pub fn sort_items(items: &mut [Item], order: &str) {
    match order {
        "oldest" => items.sort_by_key(|item| item.id),
        "favorites" => items.sort_by(|a, b| b.favorite.cmp(&a.favorite).then(b.id.cmp(&a.id))),
        "completed" => items.sort_by(|a, b| a.done.cmp(&b.done).then(b.id.cmp(&a.id))),
        _ => items.sort_by_key(|item| std::cmp::Reverse(item.id)),
    }
    // Pins lead whatever the order, oldest pin first, so pinning a new item
    // never changes which slot the earlier pins answer to. The sort is stable,
    // so everything unpinned keeps the order chosen above.
    items.sort_by_key(|item| match item.pinned_at {
        Some(at) => (0, at, item.id),
        None => (1, 0, 0),
    });
}

/// How many items can be pinned: one per Right Shift × 2 slot.
pub const PIN_LIMIT: i64 = 3;

#[derive(Debug, PartialEq, Eq)]
pub enum PinError {
    Full,
    Missing,
    Database,
}

impl std::fmt::Display for PinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Full => "Three items are pinned. Unpin one first.",
            Self::Missing => "That item no longer exists.",
            Self::Database => "Bard could not save the pin.",
        })
    }
}

/// Pin or unpin one item. Pinning an item that is already pinned keeps its
/// place; the limit is checked inside the write, so two pins in quick
/// succession cannot both take the third slot.
pub fn pin_item(conn: &Connection, id: i64, pinned: bool, now: i64) -> Result<(), PinError> {
    let changed = if pinned {
        conn.execute(
            "UPDATE items SET pinned_at = COALESCE(pinned_at, ?1), updated_at = ?1
             WHERE id = ?2
               AND (pinned_at IS NOT NULL
                    OR (SELECT COUNT(*) FROM items WHERE pinned_at IS NOT NULL) < ?3)",
            params![now, id, PIN_LIMIT],
        )
    } else {
        conn.execute(
            "UPDATE items SET pinned_at = NULL, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )
    }
    .map_err(|_| PinError::Database)?;
    if changed == 1 {
        return Ok(());
    }
    let exists = conn
        .query_row("SELECT 1 FROM items WHERE id = ?1", params![id], |_| Ok(()))
        .is_ok();
    Err(if exists {
        PinError::Full
    } else {
        PinError::Missing
    })
}

pub fn set_pinned(app: &AppHandle, id: i64, pinned: bool) -> Result<(), PinError> {
    if !is_ready(app) {
        return Err(PinError::Database);
    }
    with_conn_mut(app, |c| pin_item(c, id, pinned, now_ms()))
}

/// The items in the order the panel shows them. Right Shift × 2 + a number
/// pastes by this order too, so "item 1" is always the one at the top.
///
/// Bounded by [`AUTOMATIC_ITEM_CEILING`] — see [`list_items_windowed`] — and
/// that is the only reader that is: `clear_history` and the export path still
/// walk every row, because "every row" is what they are for.
pub fn timeline(app: &AppHandle) -> Vec<Item> {
    let mut items = list_items_windowed(app, AUTOMATIC_ITEM_CEILING as i64);
    sort_items(&mut items, &sort_order(app));
    items
}

/// Rewrites stored file paths that still point into the legacy roaming data
/// folder. The startup migration moves the files; a row is rebased only once
/// its file exists at the new location, so a file the migration could not move
/// keeps the path it can still be read from.
pub fn rebase_legacy_paths(conn: &Connection, legacy: &std::path::Path, local: &std::path::Path) {
    let legacy_prefix = legacy.to_string_lossy().to_string();
    let local_prefix = local.to_string_lossy().to_string();
    for (table, column) in [
        ("items", "image_path"),
        ("attachments", "path"),
        ("custom_models", "local_path"),
    ] {
        let Ok(mut statement) = conn.prepare(&format!(
            "SELECT rowid, {column} FROM {table} WHERE {column} IS NOT NULL"
        )) else {
            continue;
        };
        let rows: Vec<(i64, String)> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map(|mapped| mapped.flatten().collect())
            .unwrap_or_default();
        for (rowid, stored) in rows {
            let Some(rest) = strip_path_prefix(&stored, &legacy_prefix) else {
                continue;
            };
            let candidate = format!("{local_prefix}{rest}");
            if std::path::Path::new(&candidate).exists() {
                let _ = conn.execute(
                    &format!("UPDATE {table} SET {column} = ?1 WHERE rowid = ?2"),
                    params![candidate, rowid],
                );
            }
        }
    }
}

/// Windows paths compare case-insensitively, and a prefix only counts when it
/// ends on a path boundary — `...\bard` must not match `...\bard-other`.
fn strip_path_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    if value.len() < prefix.len() || !value.is_char_boundary(prefix.len()) {
        return None;
    }
    let (head, rest) = value.split_at(prefix.len());
    if !head.eq_ignore_ascii_case(prefix) {
        return None;
    }
    (rest.is_empty() || rest.starts_with('\\') || rest.starts_with('/')).then_some(rest)
}

/// Clears a stored model choice that no longer resolves to anything Bard can
/// load.
///
/// A catalog update can retire models — the two-model catalog removes Whisper
/// Small and Parakeet v2 — and a `model_id` stored before the upgrade then
/// names neither a catalog row nor a registered custom model. Every reader
/// already treats such a value as "no model chosen": the settings surface marks
/// no row active and dictation refuses to start. This makes the stored value
/// say so, once, when the database opens, so nothing keeps reporting a choice
/// the user cannot actually make. A selection that still resolves is untouched,
/// and nothing is downloaded, verified, or deleted, which keeps this safe to
/// run on every open.
pub fn clear_stale_model_selection(conn: &Connection) {
    let Ok(id) = conn.query_row(
        "SELECT value FROM settings WHERE key = 'model_id'",
        [],
        |row| row.get::<_, String>(0),
    ) else {
        return;
    };
    if id.is_empty() || crate::catalog::find(&id).is_some() {
        return;
    }
    let registered: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM custom_models WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if registered > 0 {
        return;
    }
    let _ = conn.execute("DELETE FROM settings WHERE key = 'model_id'", []);
}

#[cfg(test)]
mod tests {

    #[test]
    fn orphan_sweep_keeps_referenced_and_fresh_files() {
        use std::collections::HashSet;
        use std::path::PathBuf;
        use std::time::{Duration, SystemTime};
        let now = SystemTime::now();
        let old = now - Duration::from_secs(2 * 60 * 60);
        let fresh = now - Duration::from_secs(60);
        // Forward slashes: a separator on both platforms, so `file_name`
        // resolves to the same name on each. The contract under test is
        // name-matching, not the host's separator.
        let files = vec![
            (PathBuf::from("C:/Data/images/Kept.png"), old),
            (PathBuf::from("C:/Data/images/orphan.png"), old),
            (PathBuf::from("C:/Data/images/just-written.png"), fresh),
        ];
        let referenced: HashSet<String> = ["kept.png".to_string()].into();
        assert_eq!(
            orphaned_images(&files, &referenced, now),
            vec![PathBuf::from("C:/Data/images/orphan.png")]
        );
    }
    use super::*;

    fn item(id: i64, favorite: i64, done: i64) -> Item {
        Item {
            id,
            section_id: None,
            content: String::new(),
            kind: "text".into(),
            image_path: None,
            origin: "manual".into(),
            favorite,
            done,
            created_at: id,
            updated_at: id,
            pinned_at: None,
        }
    }

    /// The regression that shipped once: the id list appears twice in the
    /// statement and was bound once, so `query_map` returned a parameter-count
    /// error, the file list came back empty, and every image Bard had ever pruned
    /// stayed on disk with no error anywhere. A test that only asserted "no
    /// panic" would have passed on that code.
    #[test]
    fn owned_file_paths_finds_an_items_image_and_its_attachments() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        connection
            .execute_batch(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, image_path TEXT);
                 CREATE TABLE attachments (id INTEGER PRIMARY KEY, item_id INTEGER, path TEXT);",
            )
            .expect("schema");
        for id in 1..=3i64 {
            connection
                .execute(
                    "INSERT INTO items(id, image_path) VALUES(?1, ?2)",
                    params![id, format!("images/{id}.png")],
                )
                .expect("insert item");
        }
        connection
            .execute(
                "INSERT INTO attachments(item_id, path) VALUES(?1, ?2)",
                params![2i64, "images/2-extra.png"],
            )
            .expect("insert attachment");

        let mut paths = owned_file_paths(&connection, &[1, 2, 3]).expect("paths");
        paths.sort();
        paths.dedup();
        assert_eq!(
            paths,
            [
                "images/1.png",
                "images/2-extra.png",
                "images/2.png",
                "images/3.png"
            ],
            "every deleted item's own image and every attachment must be collected"
        );
    }

    #[test]
    fn owned_file_paths_ignores_ids_that_are_not_there() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        connection
            .execute_batch(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, image_path TEXT);
                 CREATE TABLE attachments (id INTEGER PRIMARY KEY, item_id INTEGER, path TEXT);",
            )
            .expect("schema");
        connection
            .execute(
                "INSERT INTO items(id, image_path) VALUES(1, 'images/1.png')",
                [],
            )
            .expect("insert item");
        let paths = owned_file_paths(&connection, &[1, 99]).expect("paths");
        assert_eq!(paths, ["images/1.png"]);
        assert!(owned_file_paths(&connection, &[])
            .expect("empty")
            .is_empty());
    }

    /// More ids than one chunk holds, because the chunking is the other half of
    /// the same fix: the lookup must cover every id, not a clamped prefix.
    #[test]
    fn owned_file_paths_covers_every_id_across_chunk_boundaries() {
        const CHUNK: usize = 400;
        let connection = Connection::open_in_memory().expect("in-memory database");
        connection
            .execute_batch(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, image_path TEXT);
                 CREATE TABLE attachments (id INTEGER PRIMARY KEY, item_id INTEGER, path TEXT);",
            )
            .expect("schema");
        let ids: Vec<i64> = (1..=901).collect();
        for id in &ids {
            connection
                .execute(
                    "INSERT INTO items(id, image_path) VALUES(?1, ?2)",
                    params![*id, format!("images/{id}.png")],
                )
                .expect("insert item");
        }
        let mut collected = Vec::new();
        for chunk in ids.chunks(CHUNK) {
            collected.extend(owned_file_paths(&connection, chunk).expect("paths"));
        }
        assert_eq!(
            collected.len(),
            ids.len(),
            "every id must be looked up, including the ones past the first chunk"
        );
    }

    #[test]
    fn every_sort_order_keeps_time_as_the_tiebreak() {
        let ids = |items: &[Item]| items.iter().map(|i| i.id).collect::<Vec<_>>();
        let base = vec![item(1, 1, 1), item(2, 0, 0), item(3, 1, 0), item(4, 0, 1)];
        let mut items = base.clone();
        sort_items(&mut items, "newest");
        assert_eq!(ids(&items), [4, 3, 2, 1]);
        sort_items(&mut items, "oldest");
        assert_eq!(ids(&items), [1, 2, 3, 4]);
        sort_items(&mut items, "favorites");
        assert_eq!(ids(&items), [3, 1, 4, 2]);
        sort_items(&mut items, "completed");
        assert_eq!(ids(&items), [3, 2, 4, 1]);
        sort_items(&mut items, "anything else");
        assert_eq!(ids(&items), [4, 3, 2, 1]);
    }

    #[test]
    fn legacy_roaming_paths_are_rebased_once_the_file_has_moved() {
        let root = std::env::temp_dir().join(format!("bard-rebase-test-{}", now_ms()));
        let legacy = root.join("Roaming").join("com.miisodev.bard");
        let local = root.join("Local").join("com.miisodev.bard");
        std::fs::create_dir_all(local.join("images")).unwrap();
        std::fs::write(local.join("images").join("moved.png"), b"png").unwrap();
        let conn = init(&root.join("test.db")).unwrap();
        // Only the stored prefix is uppercased: that is what proves the
        // ignore-case match, and the tail stays in its real case so the
        // existence check can resolve it on a case-sensitive filesystem too.
        let moved = format!(
            "{}/images/moved.png",
            legacy.to_string_lossy().to_uppercase()
        );
        let stuck = format!(
            "{}/images/stuck.png",
            legacy.to_string_lossy().to_uppercase()
        );
        for path in [&moved, &stuck] {
            conn.execute(
                "INSERT INTO items (content, kind, image_path, created_at, updated_at) VALUES ('', 'image', ?1, 0, 0)",
                params![path],
            )
            .unwrap();
        }
        rebase_legacy_paths(&conn, &legacy, &local);
        let paths: Vec<String> = conn
            .prepare("SELECT image_path FROM items ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .flatten()
            .collect();
        assert!(paths[0].starts_with(&local.to_string_lossy().to_string()));
        assert!(paths[0].ends_with("images/moved.png"));
        assert_eq!(paths[1], stuck);
        assert!(strip_path_prefix(r"C:\a\bard-other\x", r"C:\a\bard").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn old_custom_model_rows_keep_historical_load_verification() {
        let dir = std::env::temp_dir().join(format!("bard-migration-test-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let old = Connection::open(&path).unwrap();
        old.execute_batch(
            "CREATE TABLE custom_models(
                 id TEXT PRIMARY KEY,
                 repo TEXT NOT NULL,
                 file TEXT NOT NULL,
                 size_mb INTEGER NOT NULL DEFAULT 0,
                 local_path TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             INSERT INTO custom_models VALUES('hf|owner|model.gguf', 'owner', 'model.gguf', 1, 'C:/cache/model.gguf', 1);",
        )
        .unwrap();
        drop(old);

        let migrated = init(&path).unwrap();
        let compatibility: String = migrated
            .query_row(
                "SELECT compatibility FROM custom_models WHERE id = 'hf|owner|model.gguf'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(compatibility, "verified");
        std::fs::remove_dir_all(dir).ok();
    }

    /// The two-model catalog retires Whisper Small and Parakeet v2, so a
    /// `model_id` stored before the upgrade can name nothing that resolves.
    /// Clearing it at open must leave every live choice alone — including a
    /// registered custom model that the catalog does not know.
    #[test]
    fn a_retired_model_choice_is_cleared_at_open_but_live_ones_survive() {
        let dir = std::env::temp_dir().join(format!("bard-model-selection-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = init(&dir.join("bard.db")).unwrap();

        let stored = |conn: &Connection| {
            conn.query_row(
                "SELECT value FROM settings WHERE key = 'model_id'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
        };

        // Nothing stored: nothing to clear, and no row is inserted.
        clear_stale_model_selection(&conn);
        assert_eq!(stored(&conn), None);

        // A catalog choice that still resolves is kept.
        conn.execute(
            "INSERT INTO settings(key, value) VALUES('model_id', 'whisper-turbo')",
            [],
        )
        .unwrap();
        clear_stale_model_selection(&conn);
        assert_eq!(stored(&conn).as_deref(), Some("whisper-turbo"));

        // A registered custom model is a real choice even though the catalog
        // does not carry it.
        conn.execute(
            "INSERT INTO custom_models(id, repo, file, local_path, created_at)
             VALUES('hf|owner/repo|model.gguf', 'owner/repo', 'model.gguf', 'C:/models/model.gguf', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE settings SET value = 'hf|owner/repo|model.gguf' WHERE key = 'model_id'",
            [],
        )
        .unwrap();
        clear_stale_model_selection(&conn);
        assert_eq!(stored(&conn).as_deref(), Some("hf|owner/repo|model.gguf"));

        // The retired id resolves to nothing, so the choice is unset.
        conn.execute(
            "UPDATE settings SET value = 'whisper-small' WHERE key = 'model_id'",
            [],
        )
        .unwrap();
        clear_stale_model_selection(&conn);
        assert_eq!(stored(&conn), None);

        // Idempotent: running it again over the same database is a no-op.
        clear_stale_model_selection(&conn);
        assert_eq!(stored(&conn), None);

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn old_items_schema_is_migrated_atomically() {
        let dir = std::env::temp_dir().join(format!("bard-items-migration-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let old = Connection::open(&path).unwrap();
        old.execute_batch(
            "CREATE TABLE items(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 content TEXT NOT NULL,
                 kind TEXT NOT NULL DEFAULT 'text',
                 image_path TEXT,
                 favorite INTEGER NOT NULL DEFAULT 0,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL
             );
             INSERT INTO items(content, kind, favorite, created_at, updated_at)
               VALUES('kept', 'text', 0, 1, 1);",
        )
        .unwrap();
        drop(old);

        let migrated = init(&path).unwrap();
        let origin: String = migrated
            .query_row(
                "SELECT origin FROM items WHERE content = 'kept'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(origin, "manual");
        let version: i64 = migrated
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 4);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn schema_init_ok() {
        let dir = std::env::temp_dir().join(format!("bard-test-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = init(&dir.join("test.db")).unwrap();
        conn.execute("SELECT 1 FROM items", []).unwrap();
        conn.execute("SELECT 1 FROM settings", []).unwrap();
        conn.execute("SELECT 1 FROM sections", []).unwrap();
        conn.execute("SELECT 1 FROM attachments", []).unwrap();
        conn.execute("SELECT 1 FROM custom_models", []).unwrap();
        conn.execute("SELECT 1 FROM packs", []).unwrap();
        conn.execute("SELECT 1 FROM pack_items", []).unwrap();
        let columns = conn
            .prepare("PRAGMA table_info(custom_models)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for required in ["size_bytes", "sha256", "ownership", "compatibility"] {
            assert!(columns.iter().any(|column| column == required));
        }
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 4);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_old_schema_is_snapshotted_before_it_is_migrated() {
        let dir = std::env::temp_dir().join(format!("bard-migration-backup-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bard.db");
        let old = Connection::open(&path).unwrap();
        old.execute_batch(
            "CREATE TABLE items(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 content TEXT NOT NULL,
                 kind TEXT NOT NULL DEFAULT 'text',
                 image_path TEXT,
                 favorite INTEGER NOT NULL DEFAULT 0,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL
             );
             INSERT INTO items(content, kind, favorite, created_at, updated_at)
               VALUES('kept', 'text', 0, 1, 1);
             PRAGMA user_version = 3;",
        )
        .unwrap();
        drop(old);

        let (migrated, notice) = open_or_recover(&path);
        assert!(notice.is_none(), "a healthy file opens quietly: {notice:?}");

        // The pre-migration copy is beside the database, named with the
        // version it came from, and it holds the schema before the migration.
        let backups: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with("bard.db.bak-3-"))
            .collect();
        assert_eq!(
            backups.len(),
            1,
            "expected one v3 snapshot, got {backups:?}"
        );
        let snapshot = Connection::open(dir.join(&backups[0])).unwrap();
        let content: String = snapshot
            .query_row("SELECT content FROM items", [], |row| row.get(0))
            .unwrap();
        assert_eq!(content, "kept");
        let snapshot_version: i64 = snapshot
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(snapshot_version, 3);
        let live_version: i64 = migrated
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(live_version, SCHEMA_VERSION);
        drop(snapshot);
        drop(migrated);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn migration_backups_rotate_and_keep_the_newest_two() {
        let dir = std::env::temp_dir().join(format!("bard-backup-rotation-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        for index in 1..=4 {
            std::fs::write(dir.join(format!("bard.db.bak-3-{index}")), b"x").unwrap();
            // Distinct modification times, so "newest two" is unambiguous.
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let removed = prune_migration_backups(&dir, MIGRATION_BACKUPS_KEPT);
        assert_eq!(removed, 2);
        let mut kept: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        kept.sort();
        assert_eq!(kept, ["bard.db.bak-3-3", "bard.db.bak-3-4"]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_corrupt_database_is_quarantined_and_replaced() {
        let dir = std::env::temp_dir().join(format!("bard-quarantine-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bard.db");
        // A plausible-looking header followed by garbage is what a truncated
        // write leaves: SQLite opens it and quick_check reports the damage.
        let mut bytes = b"SQLite format 3\0".to_vec();
        bytes.extend(std::iter::repeat_n(0xAB, 4096));
        std::fs::write(&path, &bytes).unwrap();

        let (connection, notice) = open_or_recover(&path);
        let notice = notice.expect("a quarantined file has to be reported to a person");
        assert!(notice.contains("damaged"), "{notice}");
        assert!(recovered_this_launch());
        // The fresh database works, and the damaged file was kept, not deleted.
        connection.execute("SELECT 1 FROM items", []).unwrap();
        let kept: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with("bard.db.corrupt-"))
            .collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(std::fs::read(dir.join(&kept[0])).unwrap(), bytes);
        drop(connection);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_locked_database_is_reported_not_quarantined() {
        let dir = std::env::temp_dir().join(format!("bard-locked-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bard.db");
        // A rollback-journal EXCLUSIVE lock blocks readers, unlike WAL, and is
        // the shape of "the file is being used by another process".
        let holder = init(&path).unwrap();
        holder
            .execute_batch("PRAGMA journal_mode = DELETE;")
            .unwrap();
        holder.execute_batch("BEGIN EXCLUSIVE;").unwrap();

        let (connection, notice) = open_or_recover(&path);
        assert!(
            notice.is_some(),
            "a file that cannot be opened has to be said out loud"
        );
        // The locked file stays exactly where it is: a lock is not damage.
        assert!(path.exists());
        assert!(
            !std::fs::read_dir(&dir).unwrap().flatten().any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("bard.db.corrupt-")
            }),
            "a locked database must never be quarantined"
        );
        // The fallback still answers queries for the time being.
        connection.execute("SELECT 1 FROM items", []).unwrap();

        let _ = holder.execute_batch("ROLLBACK;");
        drop(holder);
        drop(connection);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn pins_lead_every_order_in_the_order_they_were_pinned() {
        let ids = |items: &[Item]| items.iter().map(|i| i.id).collect::<Vec<_>>();
        let mut items = vec![item(1, 0, 0), item(2, 1, 0), item(3, 0, 1), item(4, 0, 0)];
        items[0].pinned_at = Some(20);
        items[2].pinned_at = Some(10);
        for order in SORT_ORDERS {
            sort_items(&mut items, order);
            assert_eq!(&ids(&items)[..2], [3, 1], "pins lead under {order}");
        }
        sort_items(&mut items, "newest");
        assert_eq!(ids(&items), [3, 1, 4, 2]);
        sort_items(&mut items, "favorites");
        assert_eq!(ids(&items), [3, 1, 2, 4]);
    }

    #[test]
    fn at_most_three_items_are_pinned() {
        let dir = std::env::temp_dir().join(format!("bard-pin-test-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = init(&dir.join("test.db")).unwrap();
        for _ in 0..5 {
            conn.execute(
                "INSERT INTO items (content, created_at, updated_at) VALUES ('x', 0, 0)",
                [],
            )
            .unwrap();
        }
        let pinned_at = |id: i64| -> Option<i64> {
            conn.query_row(
                "SELECT pinned_at FROM items WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap()
        };
        for (id, now) in [(1, 10), (2, 11), (3, 12)] {
            pin_item(&conn, id, true, now).unwrap();
        }
        assert_eq!(pin_item(&conn, 4, true, 13), Err(PinError::Full));
        assert_eq!(pinned_at(4), None);
        // Pinning again keeps the original place in the slot order.
        pin_item(&conn, 1, true, 14).unwrap();
        assert_eq!(pinned_at(1), Some(10));
        pin_item(&conn, 2, false, 15).unwrap();
        assert_eq!(pinned_at(2), None);
        pin_item(&conn, 4, true, 16).unwrap();
        assert_eq!(pinned_at(4), Some(16));
        assert_eq!(pin_item(&conn, 99, true, 17), Err(PinError::Missing));
        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    fn pack_test_db(name: &str) -> (Connection, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("bard-{name}-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let connection = init(&dir.join("test.db")).unwrap();
        (connection, dir)
    }

    fn seed_items(connection: &Connection, count: i64) {
        for _ in 0..count {
            connection
                .execute(
                    "INSERT INTO items (content, created_at, updated_at) VALUES ('x', 0, 0)",
                    [],
                )
                .unwrap();
        }
    }

    fn member_ids(detail: &PackDetail) -> Vec<i64> {
        detail.items.iter().map(|item| item.id).collect()
    }

    #[test]
    fn packs_round_trip_through_create_read_rename_delete() {
        let (mut connection, dir) = pack_test_db("packs-crud");
        seed_items(&connection, 3);
        let pack = insert_pack(&mut connection, "  Handover  ", &[3, 1], 10).unwrap();
        assert_eq!(pack.name, "Handover");
        assert_eq!(pack.item_count, 2);
        assert_eq!(pack.preview_item_ids, [3, 1]);
        let list = read_packs(&connection);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, pack.id);
        let detail = read_pack(&connection, pack.id).unwrap();
        assert_eq!(detail.pack.name, "Handover");
        assert_eq!(detail.pack.item_count, 2);
        assert_eq!(member_ids(&detail), [3, 1]);
        set_pack_name(&connection, pack.id, "Shipment").unwrap();
        assert_eq!(
            read_pack(&connection, pack.id).unwrap().pack.name,
            "Shipment"
        );
        remove_pack(&connection, pack.id).unwrap();
        assert!(read_packs(&connection).is_empty());
        assert!(read_pack(&connection, pack.id).is_none());
        drop(connection);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn deleting_an_item_removes_it_from_every_pack() {
        let (mut connection, dir) = pack_test_db("packs-item-cascade");
        seed_items(&connection, 3);
        let first = insert_pack(&mut connection, "One", &[1, 2, 3], 10).unwrap();
        let second = insert_pack(&mut connection, "Two", &[2], 11).unwrap();
        connection
            .execute("DELETE FROM items WHERE id = 2", [])
            .unwrap();
        let detail = read_pack(&connection, first.id).unwrap();
        assert_eq!(member_ids(&detail), [1, 3]);
        let list = read_packs(&connection);
        assert_eq!(list[0].preview_item_ids, [1, 3]);
        assert_eq!(list[1].item_count, 0);
        assert_eq!(
            read_pack(&connection, second.id).unwrap().pack.item_count,
            0
        );
        drop(connection);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn deleting_a_pack_removes_only_its_memberships() {
        let (mut connection, dir) = pack_test_db("packs-pack-cascade");
        seed_items(&connection, 2);
        let pack = insert_pack(&mut connection, "Gone", &[1, 2], 10).unwrap();
        remove_pack(&connection, pack.id).unwrap();
        let memberships: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pack_items WHERE pack_id = ?1",
                params![pack.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(memberships, 0);
        let items: i64 = connection
            .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
            .unwrap();
        assert_eq!(items, 2, "the items themselves are not touched");
        assert!(read_packs(&connection).is_empty());
        drop(connection);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn pack_members_keep_their_order_through_add_remove_and_reorder() {
        let (mut connection, dir) = pack_test_db("packs-order");
        seed_items(&connection, 4);
        let pack = insert_pack(&mut connection, "Order", &[1, 2, 3], 10).unwrap();
        append_pack_items(&connection, pack.id, &[4]).unwrap();
        assert_eq!(
            member_ids(&read_pack(&connection, pack.id).unwrap()),
            [1, 2, 3, 4]
        );
        remove_pack_items(&connection, pack.id, &[2]).unwrap();
        assert_eq!(
            member_ids(&read_pack(&connection, pack.id).unwrap()),
            [1, 3, 4]
        );
        reorder_pack_items(&mut connection, pack.id, &[4, 1, 3]).unwrap();
        let detail = read_pack(&connection, pack.id).unwrap();
        assert_eq!(member_ids(&detail), [4, 1, 3]);
        assert_eq!(detail.pack.preview_item_ids, [4, 1, 3]);
        drop(connection);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn duplicate_item_ids_are_kept_once_in_the_given_order() {
        let (mut connection, dir) = pack_test_db("packs-dedup");
        seed_items(&connection, 3);
        let pack = insert_pack(&mut connection, "Duplicates", &[2, 1, 2, 1, 3], 10).unwrap();
        assert_eq!(pack.item_count, 3);
        assert_eq!(pack.preview_item_ids, [2, 1, 3]);
        assert_eq!(
            member_ids(&read_pack(&connection, pack.id).unwrap()),
            [2, 1, 3]
        );
        append_pack_items(&connection, pack.id, &[1, 3]).unwrap();
        assert_eq!(
            member_ids(&read_pack(&connection, pack.id).unwrap()),
            [2, 1, 3]
        );
        drop(connection);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn unknown_item_ids_are_skipped_without_failing() {
        let (mut connection, dir) = pack_test_db("packs-unknown-ids");
        seed_items(&connection, 2);
        let pack = insert_pack(&mut connection, "Partial", &[1, 999, 2, 1000], 10).unwrap();
        assert_eq!(pack.item_count, 2);
        assert_eq!(pack.preview_item_ids, [1, 2]);
        append_pack_items(&connection, pack.id, &[997, 2]).unwrap();
        assert_eq!(
            member_ids(&read_pack(&connection, pack.id).unwrap()),
            [1, 2]
        );
        drop(connection);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn reorder_is_refused_unless_it_holds_exactly_the_current_members() {
        let (mut connection, dir) = pack_test_db("packs-reorder");
        seed_items(&connection, 4);
        let pack = insert_pack(&mut connection, "Fixed", &[1, 2, 3], 10).unwrap();
        let refused = "The pack's items changed. Reopen it and try again.";
        for wrong in [
            vec![3, 2],
            vec![1, 2, 3, 4],
            vec![1, 1, 2, 3],
            vec![1, 2, 99],
            vec![],
        ] {
            assert_eq!(
                reorder_pack_items(&mut connection, pack.id, &wrong).unwrap_err(),
                refused
            );
            assert_eq!(
                member_ids(&read_pack(&connection, pack.id).unwrap()),
                [1, 2, 3],
                "the stored order must survive a refusal"
            );
        }
        reorder_pack_items(&mut connection, pack.id, &[2, 3, 1]).unwrap();
        assert_eq!(
            member_ids(&read_pack(&connection, pack.id).unwrap()),
            [2, 3, 1]
        );
        drop(connection);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn list_packs_previews_the_first_four_members_in_pack_order() {
        let (mut connection, dir) = pack_test_db("packs-preview");
        seed_items(&connection, 7);
        let big = insert_pack(&mut connection, "Big", &[7, 6, 5, 4, 3, 2, 1], 10).unwrap();
        let small = insert_pack(&mut connection, "Small", &[2, 4], 11).unwrap();
        let packs = read_packs(&connection);
        assert_eq!(packs.len(), 2);
        assert_eq!(packs[0].id, big.id);
        assert_eq!(packs[0].item_count, 7);
        assert_eq!(packs[0].preview_item_ids, [7, 6, 5, 4]);
        assert_eq!(packs[1].id, small.id);
        assert_eq!(packs[1].item_count, 2);
        assert_eq!(packs[1].preview_item_ids, [2, 4]);
        drop(connection);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn an_emptied_pack_is_kept() {
        let (mut connection, dir) = pack_test_db("packs-emptied");
        seed_items(&connection, 2);
        let pack = insert_pack(&mut connection, "Emptied", &[1, 2], 10).unwrap();
        remove_pack_items(&connection, pack.id, &[1, 2]).unwrap();
        let detail = read_pack(&connection, pack.id).unwrap();
        assert!(detail.items.is_empty());
        assert_eq!(detail.pack.item_count, 0);
        assert!(detail.pack.preview_item_ids.is_empty());
        assert_eq!(read_packs(&connection).len(), 1);
        drop(connection);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn pack_names_are_trimmed_and_validated() {
        let (mut connection, dir) = pack_test_db("packs-names");
        seed_items(&connection, 1);
        assert_eq!(
            insert_pack(&mut connection, "   ", &[], 10).err().unwrap(),
            "Give the pack a name."
        );
        assert_eq!(
            insert_pack(&mut connection, &"x".repeat(61), &[], 10)
                .err()
                .unwrap(),
            "Pack names can be at most 60 characters."
        );
        let pack = insert_pack(&mut connection, &"x".repeat(60), &[], 10).unwrap();
        assert_eq!(pack.name.chars().count(), 60);
        assert_eq!(
            set_pack_name(&connection, pack.id, " ").unwrap_err(),
            "Give the pack a name."
        );
        assert_eq!(
            set_pack_name(&connection, 99, "Name").unwrap_err(),
            "That pack no longer exists."
        );
        assert_eq!(
            remove_pack(&connection, 99).unwrap_err(),
            "That pack no longer exists."
        );
        assert_eq!(
            append_pack_items(&connection, 99, &[1]).unwrap_err(),
            "That pack no longer exists."
        );
        assert_eq!(
            remove_pack_items(&connection, 99, &[1]).unwrap_err(),
            "That pack no longer exists."
        );
        assert_eq!(
            reorder_pack_items(&mut connection, 99, &[]).unwrap_err(),
            "That pack no longer exists."
        );
        drop(connection);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_v3_database_gains_packs_in_place() {
        let dir = std::env::temp_dir().join(format!("bard-packs-migration-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let connection = init(&path).unwrap();
        connection
            .execute_batch(
                "DROP TABLE pack_items;
                 DROP TABLE packs;
                 PRAGMA user_version = 3;",
            )
            .unwrap();
        drop(connection);
        let migrated = init(&path).unwrap();
        migrated.execute("SELECT 1 FROM packs", []).unwrap();
        migrated.execute("SELECT 1 FROM pack_items", []).unwrap();
        let version: i64 = migrated
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 4);
        drop(migrated);
        std::fs::remove_dir_all(dir).ok();
    }
}
