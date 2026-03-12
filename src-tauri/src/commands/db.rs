use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

fn db_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("Could not find home directory")?;
    let dir = home.join(".otte");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create ~/.otte: {e}"))?;
    Ok(dir.join("otte.db"))
}

pub fn init_db() -> Result<Connection, String> {
    let path = db_path()?;
    let conn = Connection::open(&path).map_err(|e| format!("Failed to open database: {e}"))?;

    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| format!("Failed to enable foreign keys: {e}"))?;

    let version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| format!("Failed to read user_version: {e}"))?;

    if version < 1 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS repos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                added_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
                tmux_session_name TEXT NOT NULL UNIQUE,
                agent TEXT NOT NULL DEFAULT 'claude-code',
                worktree_path TEXT,
                branch TEXT,
                status TEXT NOT NULL DEFAULT 'Running',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            PRAGMA user_version = 1;
            ",
        )
        .map_err(|e| format!("Migration v1 failed: {e}"))?;
    }

    Ok(conn)
}
