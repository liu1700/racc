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

    if version < 3 {
        // Issue #70: record which claude conversation belongs to a session so
        // reattach can `claude --resume <uuid>` the exact conversation instead
        // of betting on `--continue` (cwd + recency). NULL for non-claude
        // agents and for legacy rows, which keep the --continue fallback.
        conn.execute_batch(
            "
            ALTER TABLE sessions ADD COLUMN agent_session_id TEXT;
            PRAGMA user_version = 3;
            ",
        )?;
    }

    if version < 4 {
        conn.execute_batch(
            "
            CREATE TABLE merge_settings (
                repo_id INTEGER PRIMARY KEY,
                target_branch TEXT NOT NULL,
                agent TEXT NOT NULL DEFAULT 'claude-code',
                instructions TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE merge_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                repo_id INTEGER NOT NULL,
                session_id INTEGER,
                target_branch TEXT NOT NULL,
                agent TEXT NOT NULL,
                integration_branch TEXT,
                prompt TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'starting',
                result_json TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX idx_merge_runs_repo_status
                ON merge_runs(repo_id, status);

            CREATE TABLE merge_queue_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                repo_id INTEGER NOT NULL,
                task_id INTEGER NOT NULL,
                source_session_id INTEGER NOT NULL,
                pr_url TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'queued',
                run_id INTEGER,
                result_message TEXT,
                added_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(repo_id, pr_url),
                UNIQUE(repo_id, task_id)
            );
            CREATE INDEX idx_merge_queue_repo_status
                ON merge_queue_items(repo_id, status);

            PRAGMA user_version = 4;
            ",
        )?;
    }

    if version < 5 {
        conn.execute_batch(
            "
            -- NULL identifies legacy sessions whose original launch choice was
            -- not persisted. New sessions always store 0 or 1.
            ALTER TABLE sessions ADD COLUMN skip_permissions INTEGER;

            CREATE TABLE task_plan_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                repo_id INTEGER NOT NULL,
                session_id INTEGER,
                agent TEXT NOT NULL,
                source_input TEXT NOT NULL,
                prompt TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'starting',
                result_json TEXT,
                error TEXT,
                created_task_ids TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX idx_task_plan_runs_repo_status
                ON task_plan_runs(repo_id, status);

            PRAGMA user_version = 5;
            ",
        )?;
    }

    Ok(conn)
}

pub fn reset_db(conn: &Connection) -> Result<(), CoreError> {
    conn.execute_batch(
        "
        DROP TABLE IF EXISTS task_plan_runs;
        DROP TABLE IF EXISTS merge_queue_items;
        DROP TABLE IF EXISTS merge_runs;
        DROP TABLE IF EXISTS merge_settings;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_v4_creates_merge_manager_tables() {
        let path =
            std::env::temp_dir().join(format!("racc-merge-migration-{}.db", uuid::Uuid::new_v4()));
        let conn = init_db(path.clone()).expect("database should initialize");

        let version: i32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("user_version should be readable");
        assert_eq!(version, 5);

        for table in ["merge_settings", "merge_runs", "merge_queue_items"] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .expect("table lookup should succeed");
            assert_eq!(exists, 1, "missing table {table}");
        }

        drop(conn);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migration_v5_creates_task_planner_and_session_permission_column() {
        let path = std::env::temp_dir()
            .join(format!("racc-planner-migration-{}.db", uuid::Uuid::new_v4()));
        let conn = init_db(path.clone()).expect("database should initialize");

        let planner_table: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'task_plan_runs'",
                [],
                |row| row.get(0),
            )
            .expect("planner table query");
        assert_eq!(planner_table, 1);

        let permission_column: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'skip_permissions'",
                [],
                |row| row.get(0),
            )
            .expect("session column query");
        assert_eq!(permission_column, 1);

        drop(conn);
        let _ = std::fs::remove_file(path);
    }
}
