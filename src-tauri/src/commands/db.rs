use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::State;

pub fn db_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("Could not find home directory")?;
    let dir = home.join(".racc");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create ~/.racc: {e}"))?;
    Ok(dir.join("racc.db"))
}

pub fn init_db() -> Result<Connection, String> {
    let path = db_path()?;
    let conn = Connection::open(&path).map_err(|e| format!("Failed to open database: {e}"))?;

    let version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| format!("Failed to read user_version: {e}"))?;

    if version < 1 {
        conn.execute_batch(
            "
        CREATE TABLE repos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            added_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id INTEGER NOT NULL,
            agent TEXT NOT NULL DEFAULT 'claude-code',
            worktree_path TEXT,
            branch TEXT,
            status TEXT NOT NULL DEFAULT 'Running',
            pr_url TEXT,
            server_id TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE assistant_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            tool_name TEXT,
            tool_call_id TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE assistant_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id INTEGER NOT NULL,
            description TEXT NOT NULL,
            images TEXT NOT NULL DEFAULT '[]',
            status TEXT NOT NULL DEFAULT 'open',
            session_id INTEGER,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE session_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX idx_events_session ON session_events(session_id);
        CREATE INDEX idx_events_type ON session_events(event_type);

        CREATE TABLE insights (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            insight_type TEXT NOT NULL,
            severity TEXT NOT NULL,
            title TEXT NOT NULL,
            summary TEXT NOT NULL,
            detail_json TEXT NOT NULL,
            fingerprint TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active',
            created_at INTEGER NOT NULL,
            resolved_at INTEGER
        );
        CREATE UNIQUE INDEX idx_insights_fingerprint
            ON insights(fingerprint) WHERE status = 'active';

        PRAGMA user_version = 2;
        ",
        )
        .map_err(|e| format!("Migration failed: {e}"))?;
    }

    if version >= 1 && version < 2 {
        conn.execute_batch(
            "
            ALTER TABLE tasks ADD COLUMN images TEXT NOT NULL DEFAULT '[]';
            ALTER TABLE sessions ADD COLUMN server_id TEXT;
            PRAGMA user_version = 2;
            ",
        )
        .map_err(|e| format!("Migration v2 failed: {e}"))?;
    }

    // Migration v2 → v3: create servers table
    if version < 3 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER DEFAULT 22,
                username TEXT NOT NULL,
                auth_method TEXT NOT NULL,
                key_path TEXT,
                ssh_config_host TEXT,
                setup_status TEXT DEFAULT 'pending',
                setup_details TEXT,
                ai_provider TEXT,
                ai_api_key TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| format!("Migration v3 failed: {e}"))?;
        conn.pragma_update(None, "user_version", 3)
            .map_err(|e| format!("Migration v3 version update failed: {e}"))?;
    }

    Ok(conn)
}

#[tauri::command]
pub fn reset_db(db: State<'_, Arc<Mutex<Connection>>>) -> Result<(), String> {
    let path = db_path()?;

    // Close current connection by replacing it with a fresh one
    let mut conn = db.lock().map_err(|e| format!("Failed to lock db: {e}"))?;

    // Remove the database file
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to delete database: {e}"))?;
    }

    // Reinitialize
    let new_conn = init_db()?;
    *conn = new_conn;

    Ok(())
}
