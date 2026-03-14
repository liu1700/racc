use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub repo_id: i64,
    pub description: String,
    pub status: String,
    pub session_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[tauri::command]
pub fn create_task(
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    repo_id: i64,
    description: String,
) -> Result<Task, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO tasks (repo_id, description) VALUES (?1, ?2)",
        rusqlite::params![repo_id, description],
    )
    .map_err(|e| format!("Failed to create task: {e}"))?;

    let id = conn.last_insert_rowid();
    let task = conn
        .query_row(
            "SELECT id, repo_id, description, status, session_id, created_at, updated_at FROM tasks WHERE id = ?1",
            [id],
            |row| {
                Ok(Task {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    description: row.get(2)?,
                    status: row.get(3)?,
                    session_id: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|e| format!("Failed to fetch created task: {e}"))?;

    Ok(task)
}

#[tauri::command]
pub fn list_tasks(
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    repo_id: i64,
) -> Result<Vec<Task>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, repo_id, description, status, session_id, created_at, updated_at FROM tasks WHERE repo_id = ?1 ORDER BY created_at DESC",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let tasks = stmt
        .query_map([repo_id], |row| {
            Ok(Task {
                id: row.get(0)?,
                repo_id: row.get(1)?,
                description: row.get(2)?,
                status: row.get(3)?,
                session_id: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| format!("Failed to query tasks: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect tasks: {e}"))?;

    Ok(tasks)
}

#[tauri::command]
pub fn update_task_status(
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    task_id: i64,
    status: String,
    session_id: Option<i64>,
) -> Result<Task, String> {
    let valid = ["open", "working", "closed"];
    if !valid.contains(&status.as_str()) {
        return Err(format!("Invalid status '{}'. Must be one of: {}", status, valid.join(", ")));
    }

    let conn = db.lock().map_err(|e| e.to_string())?;

    if let Some(sid) = session_id {
        conn.execute(
            "UPDATE tasks SET status = ?1, session_id = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![status, sid, task_id],
        )
        .map_err(|e| format!("Failed to update task: {e}"))?;
    } else {
        conn.execute(
            "UPDATE tasks SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![status, task_id],
        )
        .map_err(|e| format!("Failed to update task: {e}"))?;
    }

    let task = conn
        .query_row(
            "SELECT id, repo_id, description, status, session_id, created_at, updated_at FROM tasks WHERE id = ?1",
            [task_id],
            |row| {
                Ok(Task {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    description: row.get(2)?,
                    status: row.get(3)?,
                    session_id: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|e| format!("Failed to fetch updated task: {e}"))?;

    Ok(task)
}

#[tauri::command]
pub fn update_task_description(
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    task_id: i64,
    description: String,
) -> Result<Task, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE tasks SET description = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![description, task_id],
    )
    .map_err(|e| format!("Failed to update task description: {e}"))?;

    let task = conn
        .query_row(
            "SELECT id, repo_id, description, status, session_id, created_at, updated_at FROM tasks WHERE id = ?1",
            [task_id],
            |row| {
                Ok(Task {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    description: row.get(2)?,
                    status: row.get(3)?,
                    session_id: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|e| format!("Failed to fetch updated task: {e}"))?;

    Ok(task)
}

#[tauri::command]
pub fn delete_task(db: tauri::State<'_, Arc<Mutex<Connection>>>, task_id: i64) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    let affected = conn
        .execute("DELETE FROM tasks WHERE id = ?1", [task_id])
        .map_err(|e| format!("Failed to delete task: {e}"))?;
    if affected == 0 {
        return Err(format!("Task {} not found", task_id));
    }
    Ok(())
}
