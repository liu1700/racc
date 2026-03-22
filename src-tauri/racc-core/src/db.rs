use std::path::PathBuf;
use rusqlite::Connection;
use crate::error::CoreError;

pub fn init_db(db_path: PathBuf) -> Result<Connection, CoreError> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&db_path)?;

    let version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))?;

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

        CREATE TABLE servers (
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
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        PRAGMA user_version = 1;
        ",
        )?;
    }

    if version < 2 {
        conn.execute_batch(
            "
            ALTER TABLE tasks ADD COLUMN supervisor_status TEXT;
            ALTER TABLE tasks ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE tasks ADD COLUMN last_retry_at TEXT;
            ALTER TABLE tasks ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 3;
            PRAGMA journal_mode=WAL;
            PRAGMA user_version = 2;
            ",
        )?;
    }

    Ok(conn)
}

pub fn reset_db(conn: &Connection) -> Result<(), CoreError> {
    conn.execute_batch(
        "
        DROP TABLE IF EXISTS servers;
        DROP TABLE IF EXISTS insights;
        DROP TABLE IF EXISTS session_events;
        DROP TABLE IF EXISTS tasks;
        DROP TABLE IF EXISTS sessions;
        DROP TABLE IF EXISTS repos;
        PRAGMA user_version = 0;
        ",
    )?;
    Ok(())
}
