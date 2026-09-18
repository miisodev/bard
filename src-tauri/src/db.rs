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
pub struct Item {
    pub id: i64,
    pub content: String,
    pub kind: String,
    pub image_path: Option<String>,
    pub favorite: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub items: Vec<Item>,
    pub theme: String,
    pub app_version: String,
    pub keybinds: String,
    pub onboarding_completed: bool,
    pub autostart_enabled: bool,
    pub capture_double_shift: bool,
    pub model_id: String,
    pub model_ready: bool,
    pub provider_configured: bool,
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
         );",
    )?;
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
    with_conn_mut(app, |c| {
        c.execute(
            "INSERT INTO items(content, kind, image_path, favorite, created_at, updated_at)
             VALUES(?1, ?2, ?3, 0, ?4, ?4)",
            params![content, kind, image_path, now],
        )
        .unwrap();
        c.last_insert_rowid()
    })
}

pub fn delete_item(app: &AppHandle, id: i64) {
    with_conn_mut(app, |c| {
        let _ = c.execute("DELETE FROM items WHERE id = ?1", params![id]);
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

pub fn list_items(app: &AppHandle) -> Vec<Item> {
    with_conn(app, |c| {
        let mut stmt = c
            .prepare("SELECT id, content, kind, image_path, favorite, created_at, updated_at FROM items ORDER BY id DESC")
            .unwrap();
        stmt.query_map([], |r| {
            Ok(Item {
                id: r.get(0)?,
                content: r.get(1)?,
                kind: r.get(2)?,
                image_path: r.get(3)?,
                favorite: r.get(4)?,
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
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
        items: list_items(app),
        theme: get_setting(app, "theme").unwrap_or_else(|| "system".into()),
        app_version: env!("CARGO_PKG_VERSION").into(),
        keybinds: get_setting(app, "keybinds").unwrap_or_else(|| "{}".into()),
        onboarding_completed: get_setting(app, "onboarding_completed") == Some("1".into()),
        autostart_enabled: get_setting(app, "autostart_enabled") == Some("1".into()),
        capture_double_shift: get_setting(app, "capture_double_shift") != Some("0".into()),
        model_id: get_setting(app, "model_id").unwrap_or_default(),
        model_ready: crate::dictation::model_ready(app),
        provider_configured: get_setting(app, "provider_api_key").is_some()
            && get_setting(app, "provider_model").is_some(),
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
        std::fs::remove_dir_all(&dir).ok();
    }
}