use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
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
/// Never unwraps. It is called from background threads, so under
/// `panic = "abort"` a clock set earlier than 1970 would kill the whole
/// process. A wrong timestamp is
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

impl CustomModel {}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub sections: Vec<Section>,
    pub items: Vec<Item>,
    pub attachments: Vec<Attachment>,
    pub active_section_id: Option<i64>,
    pub theme: String,
    pub app_version: String,
    pub platform: String,
    pub desktop: crate::desktop::DesktopStatus,
    pub onboarding_completed: bool,
    pub autostart_enabled: bool,
    /// The person's pause switch: nothing copied is kept while it is on.
    pub capture_paused: bool,
    pub model_id: String,
    pub model_ready: bool,
    pub mic_ready: bool,
    pub active_item_id: Option<i64>,
    /// "direct" for the miiso.dev download, "store" for the Microsoft Store
    /// copy, "flatpak" inside a Flatpak sandbox.
    pub channel: String,
    /// "theme", "light", "dark" or "custom"; the custom colour is `background_color`.
    pub background: String,
    pub background_color: String,
    /// The timeline's order: one of `SORT_ORDERS`.
    pub sort: String,
    /// The three items the slot shortcuts copy, in slot order.
    ///
    /// The panel used to work this out for itself by taking the first three
    /// rows it had been given, which was only right while the timeline's
    /// order and the slots' order were the same. They are not: the slots are
    /// always the pins and then the most recent, whatever order the timeline
    /// is shown in, so a person who chose "Oldest first" saw the number 1 on
    /// the oldest item while Alt+Shift+1 copied the newest. The assignment is
    /// published here instead, from the same read the shortcuts use, so the
    /// badge and the shortcut can never disagree.
    pub slot_ids: Vec<i64>,
    /// Where the main window lives: "float", "left" or "right".
    pub dock: String,
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
    get_setting_checked(app, key).ok().flatten()
}

fn read_setting(connection: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
}

#[cfg(test)]
#[test]
fn checked_setting_reads_distinguish_absence_from_a_failed_query() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
        .unwrap();
    assert_eq!(read_setting(&connection, "trial-start").unwrap(), None);
    connection
        .execute(
            "INSERT INTO settings VALUES (?1, ?2)",
            params!["trial-start", "1000"],
        )
        .unwrap();
    assert_eq!(
        read_setting(&connection, "trial-start").unwrap().as_deref(),
        Some("1000")
    );
    connection.execute_batch("DROP TABLE settings;").unwrap();
    assert!(read_setting(&connection, "trial-start").is_err());
}

pub fn get_setting_checked(app: &AppHandle, key: &str) -> Result<Option<String>, String> {
    if !is_ready(app) {
        return Err("Bard's settings are not available yet.".into());
    }
    with_conn(app, |connection| read_setting(connection, key))
        .map_err(|_| "Bard could not read its saved settings.".to_string())
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
/// Remove a setting so its default applies again.
pub fn delete_setting(app: &AppHandle, key: &str) {
    let _ = with_conn(app, |c| {
        c.execute("DELETE FROM settings WHERE key = ?1", params![key])
    });
}

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

/// Resolve a fresh copy of something Bard already holds (text or files).
///
/// A kept copy (pinned, starred, dictated or added by hand) is never
/// duplicated: its id comes back and nothing is inserted. Plain clipboard
/// copies of the same text, with nothing attached to them, are removed so the
/// new capture takes their place at the top. `None` means insert as new.
pub fn take_duplicate(app: &AppHandle, content: &str, kind: &str) -> Option<i64> {
    let rows: Vec<(i64, String, bool, bool, bool)> = with_conn(app, |c| {
        let mut statement = c
            .prepare(
                "SELECT i.id, i.origin, i.favorite != 0, i.pinned_at IS NOT NULL,
                        EXISTS(SELECT 1 FROM attachments a WHERE a.item_id = i.id)
                          OR EXISTS(SELECT 1 FROM pack_items p WHERE p.item_id = i.id)
                 FROM items i WHERE i.kind = ?2 AND i.content = ?1",
            )
            .ok()?;
        let rows = statement
            .query_map(params![content, kind], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .ok()?
            .flatten()
            .collect::<Vec<_>>();
        Some(rows)
    })
    .unwrap_or_default();
    if let Some((id, ..)) = rows.iter().find(|(_, origin, favorite, pinned, linked)| {
        *favorite || *pinned || *linked || origin != "clipboard"
    }) {
        return Some(*id);
    }
    let plain: Vec<i64> = rows.iter().map(|(id, ..)| *id).collect();
    if !plain.is_empty() && delete_items_with_files(app, &plain).is_err() {
        // Could not remove the older copy; keep it rather than adding another.
        return plain.first().copied();
    }
    None
}

pub fn get_active_section(app: &AppHandle) -> Option<i64> {
    get_setting(app, "active_section")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("id").and_then(|i| i.as_i64()))
        .filter(|id| section_exists(app, *id))
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

pub fn update_item(app: &AppHandle, id: i64, content: &str) -> Result<(), String> {
    with_conn(app, |connection| {
        update_item_checked(connection, id, content)
    })
}

fn update_item_checked(connection: &Connection, id: i64, content: &str) -> Result<(), String> {
    let changed = connection.execute(
        "UPDATE items SET content = ?1, updated_at = ?2 WHERE id = ?3",
        params![content, now_ms(), id],
    ).map_err(|_| "Bard could not save the note. Your changes are still here; check that the drive has free space and try again.".to_string())?;
    if changed != 1 {
        return Err("That note is no longer available. Copy your changes before closing.".into());
    }
    Ok(())
}

pub fn set_favorite(app: &AppHandle, id: i64, favorite: bool) {
    with_conn_mut(app, |c| {
        let _ = c.execute(
            "UPDATE items SET favorite = ?1, updated_at = ?2 WHERE id = ?3",
            params![favorite as i64, now_ms(), id],
        );
    });
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
        // The images directory is removed below, so one file that cannot be
        // unlinked individually is not worth failing the whole erase for.
        let _ = remove_owned_image_file(app, &file);
    }
    if let Some(root) = crate::paths::data_dir() {
        let _ = std::fs::remove_dir_all(root.join("images"));
    }
    let _ = std::fs::remove_dir_all(crate::hf::local_models_dir(app));
    let _ = std::fs::remove_dir_all(crate::paths::logs_dir());
    Ok(())
}

/// Where captured images live. Resolved from the environment rather than from
/// managed state: a window can ask for an image before setup() has run, and a
/// read that raced it used to come back empty for good.
pub fn images_root() -> Option<std::path::PathBuf> {
    crate::paths::data_dir().map(|root| root.join("images"))
}

/// Delete one file Bard owns.
///
/// A missing file is success: the row that made it Bard's is what is being
/// removed, and a file already gone has nothing left to unlink. A path outside
/// the images folder is not Bard's to delete and is left alone, also success.
/// Anything else is an error, because a deletion that leaves the file on disk
/// has not finished.
pub fn remove_owned_image_file(app: &AppHandle, path: &str) -> Result<(), String> {
    let _ = app;
    let Some(images) = images_root() else {
        return Ok(());
    };
    remove_file_within(&images, path)
}

/// The policy above, over an explicit root so a test can drive it without an
/// `AppHandle`.
fn remove_file_within(images: &Path, path: &str) -> Result<(), String> {
    if !crate::hf::path_is_within(images, Path::new(path)) {
        return Ok(());
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("could not remove {path}: {error}")),
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
///
/// Every failure here is answered rather than unwrapped. The release profile is
/// `panic = "abort"`, so a prepare or query error used to end the process with
/// no window and no message, and a single unconvertible row used to empty the
/// whole shelf — "nothing here" when the truth was "one row is bad".
fn read_items_windowed(connection: &Connection, limit: i64) -> Vec<Item> {
    let mut statement = match connection.prepare(&format!(
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
    )) {
        Ok(statement) => statement,
        Err(error) => {
            crate::crash::record_event("db:items-read-failed");
            log::warn!("could not read the timeline: {error}");
            return Vec::new();
        }
    };
    let rows = match statement.query_map(params![limit], item_from_row) {
        Ok(rows) => rows,
        Err(error) => {
            crate::crash::record_event("db:items-read-failed");
            log::warn!("could not read the timeline: {error}");
            return Vec::new();
        }
    };
    let mut items = Vec::new();
    for row in rows {
        match row {
            Ok(item) => items.push(item),
            Err(error) => {
                // Keep the rows that did convert. A shelf that says "nothing
                // here" when the truth is "one row is bad" is the harmful kind
                // of empty, so the bad row is recorded and the rest survive.
                crate::crash::record_event("db:item-row-read-failed");
                log::warn!("could not read one timeline row: {error}");
            }
        }
    }
    items
}

pub(crate) fn list_items_windowed(app: &AppHandle, limit: i64) -> Vec<Item> {
    with_conn(app, |connection| read_items_windowed(connection, limit))
}

/// Every item, newest first, or an error when the table cannot be read.
///
/// The timeline is deliberately windowed, but the export and "clear history"
/// both promise every row a person kept. This read has no ceiling and no
/// forgiveness: a failed read returns an error rather than a shorter list,
/// because either caller would otherwise act on a lie — an export that quietly
/// omitted the oldest history, or a clear that reported success having skipped
/// the rows it could not read.
pub fn all_items(app: &AppHandle) -> Result<Vec<Item>, String> {
    if !is_ready(app) {
        return Err("Bard's history is not available yet.".into());
    }
    with_conn(app, read_all_items)
}

fn read_all_items(connection: &Connection) -> Result<Vec<Item>, String> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {ITEM_COLUMNS} FROM items ORDER BY id DESC"
        ))
        .map_err(|error| format!("Could not read the history: {error}"))?;
    let rows = statement
        .query_map([], item_from_row)
        .map_err(|error| format!("Could not read the history: {error}"))?;
    let mut items = Vec::new();
    for row in rows {
        // Unlike the shelf read, a bad row is not skipped: an export that
        // silently leaves one item out is exactly the data loss it promises
        // not to have.
        items.push(row.map_err(|error| format!("Could not read an item: {error}"))?);
    }
    Ok(items)
}

/// How many leading members a pack previews.
const PACK_PREVIEW_LIMIT: i64 = 4;

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

/// The shelf's Packs zone: every pack with its member count and previews.
pub fn list_packs(app: &AppHandle) -> Vec<PackSummary> {
    if !is_ready(app) {
        return Vec::new();
    }
    with_conn(app, read_packs)
}

/// Remove expired automatic items and every owned file attached to them.
/// `None` means the user selected Forever. Favorites, pins and manual work are
/// never removed by age-based cleanup.
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
    let ids = with_conn(app, |c| expired_item_ids(c, origin, cutoff))?;
    let removed = ids.len();
    delete_items_with_files(app, &ids)?;
    Ok(removed)
}

/// The ids one origin's age-based pass will remove.
///
/// Favourites are exempt because a kept copy is a decision, and pins are
/// exempt for the same reason: a pin is a decision the person made, and the
/// shelf already treats it as outside every window. An automatic pass that
/// deleted one would contradict the person and the read at once.
fn expired_item_ids(
    connection: &Connection,
    origin: &str,
    cutoff: i64,
) -> Result<Vec<i64>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id FROM items
             WHERE origin = ?1 AND favorite = 0 AND pinned_at IS NULL
               AND created_at < ?2",
        )
        .map_err(|error| format!("Could not read expired items: {error}"))?;
    let rows = statement
        .query_map(params![origin, cutoff], |row| row.get::<_, i64>(0))
        .map_err(|error| format!("Could not read expired items: {error}"))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row.map_err(|error| format!("Could not read expired items: {error}"))?);
    }
    Ok(ids)
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
    // The rows are gone now, so a file that cannot be unlinked is the only
    // thing left to report. It is reported rather than swallowed: a deletion
    // that leaves the image on disk has not finished, even though the row that
    // named it has.
    let mut failures = Vec::new();
    for path in &files {
        if let Err(error) = remove_owned_image_file(app, path) {
            crate::crash::record_event("db:image-delete-failed");
            log::warn!("{error}");
            failures.push(error);
        }
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
    if let Err(error) = with_conn_mut(app, |connection| truncate_wal(connection)) {
        crate::crash::record_event("db:wal-truncate-failed");
        log::warn!("{error}");
        failures.push(error);
    }
    if get_setting(app, "active_item_id")
        .and_then(|value| value.parse::<i64>().ok())
        .is_some_and(|active| ids.contains(&active))
    {
        set_active_item(app, None);
    }
    if failures.is_empty() {
        Ok(())
    } else {
        // The rows are deleted; saying only "could not delete" would be a lie
        // in the other direction. The caller shows this to the person.
        Err(format!(
            "Bard deleted the history, but could not finish cleaning up: {}",
            failures.join("; ")
        ))
    }
}

/// Truncate the write-ahead log so deleted text is not left recoverable in it.
///
/// The pragma reports a busy checkpoint as a row rather than as an error, so
/// the row is read: `execute_batch` threw the answer away and called a refused
/// checkpoint a success.
fn truncate_wal(connection: &Connection) -> Result<(), String> {
    let (busy, log, checkpointed): (i64, i64, i64) = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|error| format!("the write-ahead log could not be cleared: {error}"))?;
    if busy != 0 {
        return Err(format!(
            "the write-ahead log could not be cleared ({checkpointed} of {log} pages \
             checkpointed), so deleted text may still be recoverable in the database file"
        ));
    }
    Ok(())
}

/// How many automatic items the shelf's own read returns.
///
/// This is a bound on how much history one refresh re-reads and re-serialises
/// into every webview — not a retention rule and not a deletion. Nothing in the
/// app removes a row because of it; the age-based retention setting is still
/// the only automatic deletion. It used to be wired into `Prune now`, which
/// deleted past 5,000 automatic items even when the person had chosen
/// **Forever**, with nothing on screen saying so. Silently deleting data a
/// person explicitly asked you to keep is not a safety feature; in a product
/// whose pitch is "your captures stay on this machine" it is a reason to stop
/// trusting it.
///
/// If a runaway producer ever needs a backstop, it belongs behind an explicit,
/// visible control of its own — not behind a setting the person believes is in
/// force, and not inside an action that promises to apply their lifespans.
pub const AUTOMATIC_ITEM_CEILING: usize = 5_000;

pub fn clear_history(app: &AppHandle) -> Result<(), String> {
    if !is_ready(app) {
        return Err("Bard's history is not available yet.".into());
    }
    // The id list comes from a read that can fail. `list_items` answers a
    // failed query with an empty vector, and `delete_items_with_files` answers
    // an empty vector with `Ok(())` — together, a read error used to show
    // "History cleared." over an untouched database.
    let ids = with_conn(app, |connection| {
        read_ids(
            connection,
            "SELECT id FROM items",
            "Could not read the history",
        )
    })?;
    delete_items_with_files(app, &ids)
}

pub fn clear_captures(app: &AppHandle) -> Result<(), String> {
    if !is_ready(app) {
        return Err("Bard's history is not available yet.".into());
    }
    let ids = with_conn(app, read_capture_ids)?;
    delete_items_with_files(app, &ids)
}

/// Every id a statement yields, or an error naming what could not be read.
///
/// The destructive commands share this read so none of them can mistake a
/// failed query for "there is nothing to delete". A forgiving read is fine for
/// a shelf that will repaint; it is wrong for an action that reports a privacy
/// promise kept.
fn read_ids(connection: &Connection, sql: &str, context: &str) -> Result<Vec<i64>, String> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| format!("{context}: {error}"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, i64>(0))
        .map_err(|error| format!("{context}: {error}"))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row.map_err(|error| format!("{context}: {error}"))?);
    }
    Ok(ids)
}

/// The ids "Clear captures" will delete.
fn read_capture_ids(connection: &Connection) -> Result<Vec<i64>, String> {
    read_ids(
        connection,
        "SELECT id FROM items WHERE origin IN ('clipboard', 'dictation') AND favorite = 0",
        "Could not read the captures to clear",
    )
}

const CUSTOM_MODEL_COLUMNS: &str =
    "id, repo, file, size_mb, size_bytes, sha256, local_path, ownership, compatibility";

fn custom_model_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CustomModel> {
    let ownership: String = row.get(7)?;
    let compatibility: String = row.get(8)?;
    // SQLite's integers are signed, so the byte count is read as the signed
    // type it is stored as and widened here. Reading it as `u64` was never
    // supported by the driver and rusqlite 0.41 stopped pretending otherwise.
    let size_bytes: i64 = row.get(4)?;
    let size_mb: i64 = row.get(3)?;
    Ok(CustomModel {
        id: row.get(0)?,
        repo: row.get(1)?,
        file: row.get(2)?,
        size_mb: size_mb.max(0) as u64,
        size_bytes: size_bytes.max(0) as u64,
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

pub fn get_state(app: &AppHandle) -> AppState {
    if !is_ready(app) {
        return AppState {
            sections: Vec::new(),
            items: Vec::new(),
            attachments: Vec::new(),
            active_section_id: None,
            theme: "system".into(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            platform: std::env::consts::OS.into(),
            desktop: crate::desktop::status(),
            onboarding_completed: false,
            autostart_enabled: false,
            capture_paused: false,
            model_id: String::new(),
            model_ready: false,
            mic_ready: false,
            active_item_id: None,
            channel: crate::packaging::channel().into(),
            background: "theme".into(),
            background_color: String::new(),
            sort: SORT_ORDERS[0].into(),
            slot_ids: Vec::new(),
            dock: "float".into(),
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
        platform: std::env::consts::OS.into(),
        desktop: crate::desktop::status(),
        onboarding_completed: get_setting(app, "onboarding_completed") == Some("1".into()),
        autostart_enabled: get_setting(app, "autostart_enabled") == Some("1".into()),
        capture_paused: crate::monitor::is_paused(),
        model_id: get_setting(app, "model_id").unwrap_or_default(),
        model_ready: crate::dictation::model_ready(app),
        mic_ready: crate::dictation::mic_ready(),
        active_item_id: active_item(app),
        channel: crate::packaging::channel().into(),
        background: get_setting(app, "background").unwrap_or_else(|| "theme".into()),
        background_color: get_setting(app, "background_color").unwrap_or_default(),
        sort: sort_order(app),
        slot_ids: slot_ids(app),
        dock: crate::panel::dock(app).name().into(),
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
/// The read is bounded by [`AUTOMATIC_ITEM_CEILING`] — see
/// [`list_items_windowed`]. The export deliberately does not use it:
/// [`all_items`] carries every row a person kept, because "everything they
/// kept" is what an export is for.
pub fn timeline(app: &AppHandle) -> Vec<Item> {
    let mut items = list_items_windowed(app, AUTOMATIC_ITEM_CEILING as i64);
    sort_items(&mut items, &sort_order(app));
    items
}

/// How many rows the slot shortcuts read before taking the first three.
const SLOT_WINDOW: i64 = 60;

/// The three items the slot shortcuts copy, in slot order.
///
/// Pins lead, oldest pin first, then the most recent — the order the person
/// pinned them in, so pinning a new item never changes which slot an older
/// pin answers to. This is deliberately independent of the order the timeline
/// is shown in: the slots are a fixed set of keys, and a key that copied
/// something different because the list had been sorted would be a trap.
pub fn slot_items(app: &AppHandle) -> Vec<Item> {
    let mut items = list_items_windowed(app, SLOT_WINDOW);
    sort_items(&mut items, "newest");
    items.truncate(3);
    items
}

/// The ids of the slot items, in slot order. Published in the state so the
/// panel can label a row with the slot that copies it.
pub fn slot_ids(app: &AppHandle) -> Vec<i64> {
    slot_items(app).into_iter().map(|item| item.id).collect()
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
    fn a_missing_note_is_not_reported_as_saved() {
        let connection = super::in_memory();
        assert!(super::update_item_checked(&connection, 999, "Draft").is_err());
    }

    #[test]
    fn a_refused_note_write_keeps_the_original() {
        let connection = super::in_memory();
        connection.execute("INSERT INTO items(content, kind, origin, created_at, updated_at) VALUES('Original', 'text', 'manual', 1, 1)", []).unwrap();
        let id = connection.last_insert_rowid();
        connection.execute_batch("PRAGMA query_only = ON").unwrap();
        assert!(super::update_item_checked(&connection, id, "Revised").is_err());
        let text: String = connection
            .query_row("SELECT content FROM items WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(text, "Original");
    }

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

    /// The export's read is the one that has to carry every row: the shelf read
    /// is windowed on purpose. A test that only proved `read_all_items` returned
    /// the rows it was given would pass again the day the export went back to
    /// the windowed read.
    #[test]
    fn the_export_read_carries_every_row_while_the_shelf_read_is_windowed() {
        let connection = in_memory();
        for id in 1..=4 {
            connection
                .execute(
                    "INSERT INTO items(id, content, kind, origin, created_at, updated_at)
                     VALUES(?1, ?2, 'text', 'clipboard', ?1, ?1)",
                    params![id, format!("clip {id}")],
                )
                .unwrap();
        }
        connection
            .execute(
                "INSERT INTO items(id, content, kind, origin, created_at, updated_at)
                 VALUES(9, 'manual', 'text', 'manual', 9, 9)",
                [],
            )
            .unwrap();

        let windowed = read_items_windowed(&connection, 2);
        assert_eq!(
            windowed.iter().map(|item| item.id).collect::<Vec<_>>(),
            [9, 4, 3],
            "the shelf keeps the newest automatic rows and every manual one"
        );

        let all = read_all_items(&connection).expect("every row");
        assert_eq!(
            all.iter().map(|item| item.id).collect::<Vec<_>>(),
            [9, 4, 3, 2, 1],
            "the export read must carry the rows the shelf window leaves out"
        );
    }

    #[test]
    fn one_unreadable_row_does_not_empty_the_shelf() {
        let connection = in_memory();
        connection
            .execute(
                "INSERT INTO items(content, kind, origin, created_at, updated_at)
                 VALUES('good', 'text', 'clipboard', 1, 1)",
                [],
            )
            .unwrap();
        // SQLite keeps a value its INTEGER affinity cannot convert as TEXT, so
        // this row reads in SQL and fails in `item_from_row`.
        connection
            .execute(
                "INSERT INTO items(content, kind, origin, favorite, created_at, updated_at)
                 VALUES('bad', 'text', 'clipboard', 'not-a-number', 2, 2)",
                [],
            )
            .unwrap();

        let items = read_items_windowed(&connection, 10);
        assert_eq!(
            items
                .iter()
                .map(|item| item.content.as_str())
                .collect::<Vec<_>>(),
            ["good"],
            "a bad row must not turn a shelf with a good row into an empty one"
        );
    }

    /// A failed read must be an error, not an empty list. The clear commands
    /// used to turn exactly this into "nothing to delete" and report success
    /// over an untouched database.
    #[test]
    fn a_failed_history_read_is_an_error_not_an_empty_history() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        assert!(read_all_items(&connection).is_err());
        assert!(read_capture_ids(&connection).is_err());
    }

    #[test]
    fn age_based_cleanup_keeps_favourites_and_pins() {
        let connection = in_memory();
        let cutoff = 1_000;
        let insert = |id: i64,
                      origin: &str,
                      favorite: i64,
                      pinned: Option<i64>,
                      created_at: i64| {
            connection
                .execute(
                    "INSERT INTO items(id, content, kind, origin, favorite, pinned_at, created_at, updated_at)
                     VALUES(?1, ?2, 'text', ?3, ?4, ?5, ?6, ?6)",
                    params![id, format!("item {id}"), origin, favorite, pinned, created_at],
                )
                .unwrap();
        };
        insert(1, "clipboard", 0, None, 10); // expired and plain: the only one that goes
        insert(2, "clipboard", 1, None, 10); // expired but a favourite
        insert(3, "clipboard", 0, Some(50), 10); // expired but pinned
        insert(4, "dictation", 0, None, 10); // another origin's pass
        insert(5, "clipboard", 0, None, cutoff); // exactly at the cutoff is not past it

        assert_eq!(
            expired_item_ids(&connection, "clipboard", cutoff).expect("expired ids"),
            [1],
            "an automatic pass may remove only expired, unpinned, unkept captures"
        );
    }

    #[test]
    fn a_missing_image_is_not_a_deletion_failure() {
        let root = std::env::temp_dir().join(format!("bard-images-{}", now_ms()));
        let images = root.join("images");
        std::fs::create_dir_all(&images).unwrap();
        let owned = images.join("shot.png");
        std::fs::write(&owned, b"png").unwrap();
        let outside = root.join("elsewhere.png");
        std::fs::write(&outside, b"png").unwrap();

        assert!(remove_file_within(&images, &owned.to_string_lossy()).is_ok());
        assert!(!owned.exists());
        // Already gone is success: there is nothing left to delete.
        assert!(remove_file_within(&images, &owned.to_string_lossy()).is_ok());
        // Outside the images folder is not Bard's to delete.
        assert!(remove_file_within(&images, &outside.to_string_lossy()).is_ok());
        assert!(outside.exists());
        std::fs::remove_dir_all(root).ok();
    }

    /// The checkpoint pragma reports a refused truncation as a row, not as an
    /// error, so the old `execute_batch` called it a success. This holds a read
    /// transaction open from a second connection, which is exactly what stops
    /// SQLite truncating the log.
    #[test]
    fn a_blocked_wal_truncation_is_reported_rather_than_called_a_success() {
        let dir = std::env::temp_dir().join(format!("bard-wal-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bard.db");
        let writer = init(&path).unwrap();
        writer
            .busy_timeout(std::time::Duration::from_millis(0))
            .unwrap();
        writer
            .execute(
                "INSERT INTO items(content, kind, origin, created_at, updated_at)
                 VALUES('secret', 'text', 'clipboard', 1, 1)",
                [],
            )
            .unwrap();

        let reader = Connection::open(&path).unwrap();
        reader
            .execute_batch("BEGIN; SELECT COUNT(*) FROM items;")
            .unwrap();
        assert!(
            truncate_wal(&writer).is_err(),
            "a checkpoint that could not truncate the log must not be reported as success"
        );
        reader.execute_batch("COMMIT;").unwrap();
        assert!(truncate_wal(&writer).is_ok());

        drop(reader);
        drop(writer);
        std::fs::remove_dir_all(dir).ok();
    }
}
