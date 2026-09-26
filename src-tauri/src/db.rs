use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, Manager};

pub struct Db(pub Mutex<Option<Connection>>);

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
    /// "direct" for the miiso.dev download, "store" for the Microsoft Store copy.
    pub channel: String,
    /// "theme", "light", "dark" or "custom"; the custom colour is `background_color`.
    pub background: String,
    pub background_color: String,
    /// The timeline's order: one of `SORT_ORDERS`.
    pub sort: String,
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|name| name == column))
}

pub fn init(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
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
           updated_at INTEGER NOT NULL
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
    let compatibility_existed = table_has_column(&conn, "custom_models", "compatibility")?;
    let transaction = conn.unchecked_transaction()?;
    let migrations: &[(&str, &str, &str)] = &[
        (
            "items",
            "section_id",
            "ALTER TABLE items ADD COLUMN section_id INTEGER REFERENCES sections(id) ON DELETE SET NULL",
        ),
        ("items", "done", "ALTER TABLE items ADD COLUMN done INTEGER NOT NULL DEFAULT 0"),
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
    for (table, column, statement) in migrations {
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
    transaction.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_items_origin_created
           ON items(origin, created_at);
         PRAGMA user_version = 2;",
    )?;
    transaction.commit()?;
    Ok(conn)
}

/// Whether the database is open.
///
/// Window events fire before `setup()` runs, so anything reading settings from
/// that path has to ask first — `with_conn` unwraps, and a focus event arriving
/// during window creation was enough to panic the whole app on launch.
pub fn is_ready(app: &AppHandle) -> bool {
    app.try_state::<Db>()
        .map(|db| db.0.lock().map(|g| g.is_some()).unwrap_or(false))
        .unwrap_or(false)
}

fn with_conn<T>(app: &AppHandle, f: impl FnOnce(&Connection) -> T) -> T {
    let db = app.state::<Db>();
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    f(conn)
}

fn with_conn_mut<T>(app: &AppHandle, f: impl FnOnce(&mut Connection) -> T) -> T {
    let db = app.state::<Db>();
    let mut guard = db.0.lock().unwrap();
    let conn = guard.as_mut().unwrap();
    f(conn)
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
        .or_else(|| list_items(app).first().map(|item| item.id))
}

pub fn item_by_id(app: &AppHandle, id: i64) -> Option<Item> {
    if !is_ready(app) {
        return None;
    }
    with_conn(app, |connection| {
        connection
            .query_row(
                "SELECT id, section_id, content, kind, image_path, origin, favorite, done,
                        created_at, updated_at
                 FROM items WHERE id = ?1",
                params![id],
                |row| {
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
                    })
                },
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
        c.query_row(
            "SELECT id FROM sections ORDER BY id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(INSERT_FAILED)
    })
}

pub fn list_sections(app: &AppHandle) -> Vec<Section> {
    with_conn(app, |c| {
        let mut stmt = c
            .prepare("SELECT id, name FROM sections ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| {
            Ok(Section {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default()
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
    with_conn(app, |c| {
        let mut stmt = c
            .prepare(
                "SELECT id, item_id, path, position FROM attachments WHERE item_id = ?1 ORDER BY position",
            )
            .unwrap();
        stmt.query_map(params![item_id], |r| {
            Ok(Attachment {
                id: r.get(0)?,
                item_id: r.get(1)?,
                path: r.get(2)?,
                position: r.get(3)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default()
    })
}

pub fn list_all_attachments(app: &AppHandle) -> Vec<Attachment> {
    with_conn(app, |c| {
        let mut stmt = c
            .prepare("SELECT id, item_id, path, position FROM attachments ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| {
            Ok(Attachment {
                id: r.get(0)?,
                item_id: r.get(1)?,
                path: r.get(2)?,
                position: r.get(3)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default()
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
    if let Some(root) = local_data_root() {
        let _ = std::fs::remove_dir_all(root.join("images"));
    }
    let _ = std::fs::remove_dir_all(crate::hf::local_models_dir(app));
    let _ = std::fs::remove_dir_all(
        std::env::var("LOCALAPPDATA")
            .map(|base| {
                std::path::PathBuf::from(base)
                    .join("com.miisodev.bard")
                    .join("logs")
            })
            .unwrap_or_default(),
    );
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
    local_data_root().map(|root| root.join("images"))
}

fn local_data_root() -> Option<std::path::PathBuf> {
    std::env::var("LOCALAPPDATA")
        .ok()
        .map(|base| std::path::PathBuf::from(base).join("com.miisodev.bard"))
}

pub fn remove_owned_image_file(app: &AppHandle, path: &str) {
    let _ = app;
    let Some(root) = local_data_root() else {
        return;
    };
    let images = root.join("images");
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
        let mut stmt = c
            .prepare(
                "SELECT id, section_id, content, kind, image_path, origin, favorite, done,
                        created_at, updated_at
                 FROM items ORDER BY id DESC",
            )
            .unwrap();
        stmt.query_map([], |r| {
            Ok(Item {
                id: r.get(0)?,
                section_id: r.get(1)?,
                content: r.get(2)?,
                kind: r.get(3)?,
                image_path: r.get(4)?,
                origin: r.get(5)?,
                favorite: r.get(6)?,
                done: r.get(7)?,
                created_at: r.get(8)?,
                updated_at: r.get(9)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default()
    })
}

/// Remove expired automatic items and every owned file attached to them.
/// `None` means the user selected Forever. Favorites and manual work are never
/// removed by age-based cleanup.
pub fn cleanup_origin(app: &AppHandle, origin: &str) -> Result<(), String> {
    if !matches!(origin, "clipboard" | "dictation") {
        return Ok(());
    }
    let Some(days) = retention_days(app, origin) else {
        // Forever means Forever. Returning early is the whole contract: the old
        // code fell through to a count cap that deleted the user's data anyway,
        // which is the opposite of what they asked for.
        return Ok(());
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
    delete_items_with_files(app, &ids)
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
/// `cleanup_origin` is the same pass without the count, for the fire-and-forget
/// startup and capture paths where nobody is waiting to be told.
pub fn prune_expired(app: &AppHandle, origin: &str) -> Result<usize, String> {
    let removed = count_expired(app, origin);
    cleanup_origin(app, origin)?;
    Ok(removed)
}

fn count_expired(app: &AppHandle, origin: &str) -> usize {
    if !matches!(origin, "clipboard" | "dictation") {
        return 0;
    }
    let Some(days) = retention_days(app, origin) else {
        return 0;
    };
    let cutoff = now_ms().saturating_sub(days.saturating_mul(24 * 60 * 60 * 1000));
    with_conn(app, |c| {
        c.query_row(
            "SELECT COUNT(*) FROM items
             WHERE origin = ?1 AND favorite = 0 AND created_at < ?2",
            params![origin, cutoff],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count.max(0) as usize)
        .unwrap_or(0)
    })
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

pub fn delete_items_with_files(app: &AppHandle, ids: &[i64]) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    // One query for all the ids. This used to be one query per id, so pruning
    // 5,000 expired items meant 10,000 statements, each taking and releasing the
    // global connection mutex. The statement is built from the placeholder list
    // only — `ids` are bound, never interpolated — and the length is capped by
    // SQLite's own variable limit, so the id count is clamped to stay inside it.
    let files = {
        // SQLite's default `SQLITE_MAX_VARIABLE_NUMBER` is 999. The same
        // placeholder list appears twice, so each id contributes two bindings.
        let take = ids.len().min(400);
        let subset = &ids[..take];
        let placeholders = vec!["?"; subset.len()].join(",");
        let sql = format!(
            "SELECT path FROM attachments WHERE item_id IN ({placeholders}) \
             UNION ALL SELECT image_path FROM items \
             WHERE image_path IS NOT NULL AND id IN ({placeholders})"
        );
        with_conn(app, |connection| {
            let mut files: Vec<String> = match connection.prepare(&sql) {
                Err(error) => {
                    log::warn!("could not read attachments: {error}");
                    return Vec::new();
                }
                Ok(mut statement) => {
                    match statement.query_map(rusqlite::params_from_iter(subset.iter()), |row| {
                        row.get::<_, String>(0)
                    }) {
                        Err(error) => {
                            log::warn!("could not read attachments: {error}");
                            return Vec::new();
                        }
                        Ok(rows) => rows.filter_map(Result::ok).collect(),
                    }
                }
            };
            files.sort();
            files.dedup();
            files
        })
    };
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
    for path in files {
        remove_owned_image_file(app, &path);
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
        let mut stmt = c
            .prepare(
                "SELECT id FROM items WHERE origin IN ('clipboard', 'dictation') AND favorite = 0",
            )
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect()
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
        let mut stmt = c
            .prepare(&format!(
                "SELECT {CUSTOM_MODEL_COLUMNS} FROM custom_models ORDER BY created_at"
            ))
            .unwrap();
        stmt.query_map([], custom_model_from_row)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_default()
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
            license: crate::license::LicenseStatus {
                state: "unavailable".into(),
                can_use: false,
                key_configured: false,
                trial_started_at: None,
                trial_expires_at: None,
                expires_at: None,
                last_checked_at: None,
                message: Some("Bard is still starting.".into()),
            },
            channel: crate::packaging::channel().into(),
            background: "theme".into(),
            background_color: String::new(),
            sort: SORT_ORDERS[0].into(),
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
    }
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
}

/// The items in the order the panel shows them. Right Shift × 2 + a number
/// pastes by this order too, so "item 1" is always the one at the top.
pub fn timeline(app: &AppHandle) -> Vec<Item> {
    let mut items = list_items(app);
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

#[cfg(test)]
mod tests {
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
        }
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
        let moved = legacy.join("images").join("moved.png");
        let stuck = legacy.join("images").join("stuck.png");
        for path in [&moved, &stuck] {
            conn.execute(
                "INSERT INTO items (content, kind, image_path, created_at, updated_at) VALUES ('', 'image', ?1, 0, 0)",
                params![path.to_string_lossy().to_uppercase()],
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
        assert!(paths[0].ends_with("MOVED.PNG"));
        assert_eq!(paths[1], stuck.to_string_lossy().to_uppercase());
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
        assert_eq!(version, 2);
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
        assert_eq!(version, 2);
        std::fs::remove_dir_all(&dir).ok();
    }
}
