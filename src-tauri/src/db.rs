use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, Manager};

pub struct Db(pub Mutex<Option<Connection>>);

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
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
    pub image_path: Option<String>,
    pub favorite: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: i64,
    pub item_id: i64,
    pub path: String,
    pub position: i64,
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
    pub keybinds: String,
    pub onboarding_completed: bool,
    pub autostart_enabled: bool,
    pub capture_double_shift: bool,
    pub model_id: String,
    pub model_ready: bool,
    pub mic_ready: bool,
}

pub fn init(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS items(
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           content TEXT NOT NULL,
           kind TEXT NOT NULL DEFAULT 'text',
           image_path TEXT,
           favorite INTEGER NOT NULL DEFAULT 0,
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
         CREATE INDEX IF NOT EXISTS idx_attachments_item ON attachments(item_id);",
    )?;
    let _ = conn.execute(
        "ALTER TABLE items ADD COLUMN section_id INTEGER REFERENCES sections(id) ON DELETE SET NULL",
        [],
    );
    Ok(conn)
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
    with_conn(app, |c| {
        c.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
            .ok()
    })
}

pub fn set_setting(app: &AppHandle, key: &str, value: &str) {
    with_conn_mut(app, |c| {
        let _ = c.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        );
    });
}

pub fn insert_item(app: &AppHandle, content: &str, kind: &str, image_path: Option<&str>) -> i64 {
    let now = now_ms();
    let section = get_active_section(app);
    with_conn_mut(app, |c| {
        c.execute(
            "INSERT INTO items(section_id, content, kind, image_path, favorite, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, 0, ?5, ?5)",
            params![section, content, kind, image_path, now],
        )
        .unwrap();
        c.last_insert_rowid()
    })
}

pub fn get_active_section(app: &AppHandle) -> Option<i64> {
    get_setting(app, "active_section")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("id").and_then(|i| i.as_i64()))
        .filter(|id| section_exists(app, *id))
}

pub fn set_active_section(app: &AppHandle, id: Option<i64>) {
    match id {
        Some(i) => set_setting(app, "active_section", &serde_json::json!({"id": i}).to_string()),
        None => set_setting(app, "active_section", ""),
    }
}

fn section_exists(app: &AppHandle, id: i64) -> bool {
    with_conn(app, |c| {
        c.query_row("SELECT 1 FROM sections WHERE id = ?1", params![id], |_| Ok(()))
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
    with_conn_mut(app, |c| {
        c.execute(
            "INSERT INTO sections(name, created_at) VALUES(?1, ?2)",
            params![trimmed, now_ms()],
        )
        .unwrap();
        c.last_insert_rowid()
    })
}

pub fn list_sections(app: &AppHandle) -> Vec<Section> {
    with_conn(app, |c| {
        let mut stmt = c
            .prepare("SELECT id, name FROM sections ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| {
            Ok(Section { id: r.get(0)?, name: r.get(1)? })
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
        c.execute(
            "INSERT INTO attachments(item_id, path, position, created_at) VALUES(?1, ?2, ?3, ?4)",
            params![item_id, path, pos, now_ms()],
        )
        .unwrap();
        c.last_insert_rowid()
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

pub fn delete_attachment(app: &AppHandle, id: i64) -> Option<String> {
    let path = with_conn(app, |c| {
        c.query_row("SELECT path FROM attachments WHERE id = ?1", params![id], |r| r.get(0))
            .ok()
    });
    with_conn_mut(app, |c| {
        let _ = c.execute("DELETE FROM attachments WHERE id = ?1", params![id]);
    });
    path
}

pub fn delete_item(app: &AppHandle, id: i64) {
    with_conn_mut(app, |c| {
        let _ = c.execute("DELETE FROM items WHERE id = ?1", params![id]);
    });
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

pub fn add_note_to_image(app: &AppHandle, id: i64) -> Option<String> {
    let path = with_conn(app, |c| {
        c.query_row("SELECT image_path FROM items WHERE id = ?1", params![id], |r| r.get::<_, Option<String>>(0)).ok().flatten()
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
    files.extend(
        list_attachments_for(app, id)
            .into_iter()
            .map(|a| a.path),
    );
    files
}

pub fn list_items(app: &AppHandle) -> Vec<Item> {
    with_conn(app, |c| {
        let mut stmt = c
            .prepare(
                "SELECT id, section_id, content, kind, image_path, favorite, created_at, updated_at
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
                favorite: r.get(5)?,
                created_at: r.get(6)?,
                updated_at: r.get(7)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default()
    })
}

pub fn prune_history(app: &AppHandle, limit: usize) {
    with_conn_mut(app, |c| {
        let _ = c.execute(
            "DELETE FROM items WHERE id NOT IN (
               SELECT id FROM items ORDER BY id DESC LIMIT ?1
             )",
            params![limit as i64],
        );
    });
}

pub fn clear_history(app: &AppHandle) {
    with_conn_mut(app, |c| {
        let _ = c.execute("DELETE FROM items", []);
    });
}

pub fn get_state(app: &AppHandle) -> AppState {
    AppState {
        sections: list_sections(app),
        items: list_items(app),
        attachments: list_all_attachments(app),
        active_section_id: get_active_section(app),
        theme: get_setting(app, "theme").unwrap_or_else(|| "system".into()),
        app_version: env!("CARGO_PKG_VERSION").into(),
        keybinds: get_setting(app, "keybinds").unwrap_or_else(|| "{}".into()),
        onboarding_completed: get_setting(app, "onboarding_completed") == Some("1".into()),
        autostart_enabled: get_setting(app, "autostart_enabled") == Some("1".into()),
        capture_double_shift: get_setting(app, "capture_double_shift") != Some("0".into()),
        model_id: get_setting(app, "model_id").unwrap_or_default(),
        model_ready: crate::dictation::model_ready(app),
        mic_ready: crate::dictation::mic_ready(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_init_ok() {
        let dir = std::env::temp_dir().join(format!("bard-test-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = init(&dir.join("test.db")).unwrap();
        conn.execute("SELECT 1 FROM items", []).unwrap();
        conn.execute("SELECT 1 FROM settings", []).unwrap();
        conn.execute("SELECT 1 FROM sections", []).unwrap();
        conn.execute("SELECT 1 FROM attachments", []).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }
}