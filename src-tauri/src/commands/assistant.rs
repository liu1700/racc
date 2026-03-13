use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

// --- Types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub tool_call_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantConfig {
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: i64,
    pub status: String,
    pub agent: String,
    pub branch: Option<String>,
    pub repo_name: String,
    pub repo_path: String,
    pub worktree_path: Option<String>,
    pub elapsed_minutes: i64,
    pub created_at: String,
}

// --- Helper: resolve session ID to filesystem path ---

fn resolve_session_path(conn: &Connection, session_id: i64) -> Result<String, String> {
    let (worktree_path, repo_id): (Option<String>, i64) = conn
        .query_row(
            "SELECT worktree_path, repo_id FROM sessions WHERE id = ?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("Session not found: {e}"))?;

    if let Some(wt) = worktree_path {
        return Ok(wt);
    }

    let repo_path: String = conn
        .query_row("SELECT path FROM repos WHERE id = ?1", [repo_id], |row| {
            row.get(0)
        })
        .map_err(|e| format!("Repo not found: {e}"))?;

    Ok(repo_path)
}

// --- Tauri Commands ---

#[tauri::command]
pub async fn get_assistant_config(
    db: tauri::State<'_, Mutex<Connection>>,
) -> Result<AssistantConfig, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let get_val = |key: &str| -> Option<String> {
        conn.query_row(
            "SELECT value FROM assistant_config WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .ok()
    };

    Ok(AssistantConfig {
        provider: get_val("provider"),
        api_key: get_val("api_key"),
        model: get_val("model"),
    })
}

#[tauri::command]
pub async fn set_assistant_config(
    db: tauri::State<'_, Mutex<Connection>>,
    provider: String,
    api_key: String,
    model: String,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let upsert = |key: &str, value: &str| -> Result<(), String> {
        conn.execute(
            "INSERT INTO assistant_config (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    };

    upsert("provider", &provider)?;
    upsert("api_key", &api_key)?;
    upsert("model", &model)?;

    Ok(())
}

#[tauri::command]
pub async fn save_assistant_message(
    db: tauri::State<'_, Mutex<Connection>>,
    role: String,
    content: String,
    tool_name: Option<String>,
    tool_call_id: Option<String>,
) -> Result<AssistantMessage, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO assistant_messages (role, content, tool_name, tool_call_id) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![role, content, tool_name, tool_call_id],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    let created_at: String = conn
        .query_row(
            "SELECT created_at FROM assistant_messages WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    Ok(AssistantMessage {
        id,
        role,
        content,
        tool_name,
        tool_call_id,
        created_at,
    })
}

#[tauri::command]
pub async fn get_assistant_messages(
    db: tauri::State<'_, Mutex<Connection>>,
    limit: i64,
) -> Result<Vec<AssistantMessage>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT id, role, content, tool_name, tool_call_id, created_at
             FROM assistant_messages ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;

    let messages: Vec<AssistantMessage> = stmt
        .query_map([limit], |row| {
            Ok(AssistantMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                tool_name: row.get(3)?,
                tool_call_id: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    // Reverse to get chronological order (we queried DESC for LIMIT)
    let mut messages = messages;
    messages.reverse();
    Ok(messages)
}

#[tauri::command]
pub async fn get_all_sessions_for_assistant(
    db: tauri::State<'_, Mutex<Connection>>,
) -> Result<Vec<SessionInfo>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.status, s.agent, s.branch, r.name, r.path, s.worktree_path, s.created_at
             FROM sessions s JOIN repos r ON s.repo_id = r.id
             ORDER BY s.created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now();
    let sessions: Vec<SessionInfo> = stmt
        .query_map([], |row| {
            let created_at: String = row.get(7)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                created_at,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .map(|(id, status, agent, branch, repo_name, repo_path, worktree_path, created_at)| {
            // SQLite datetime('now') produces "YYYY-MM-DD HH:MM:SS" format
            let elapsed = chrono::NaiveDateTime::parse_from_str(&created_at, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| (now - ndt.and_utc()).num_minutes())
                .unwrap_or(0);

            SessionInfo {
                id,
                status,
                agent,
                branch,
                repo_name,
                repo_path,
                worktree_path,
                elapsed_minutes: elapsed,
                created_at,
            }
        })
        .collect();

    Ok(sessions)
}

#[tauri::command]
pub async fn get_session_diff_for_assistant(
    db: tauri::State<'_, Mutex<Connection>>,
    session_id: i64,
) -> Result<String, String> {
    let path = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        resolve_session_path(&conn, session_id)?
    };

    let output = std::process::Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(&path)
        .output()
        .map_err(|e| format!("Failed to get diff: {e}"))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub async fn get_session_costs_for_assistant(
    db: tauri::State<'_, Mutex<Connection>>,
    session_id: i64,
) -> Result<String, String> {
    let path = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        resolve_session_path(&conn, session_id)?
    };

    // Reuse existing cost logic by invoking get_project_costs
    let costs = crate::commands::cost::get_project_costs(path).await?;
    serde_json::to_string(&costs).map_err(|e| e.to_string())
}
