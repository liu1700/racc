use serde::{Deserialize, Serialize};

use crate::AppContext;
use crate::error::CoreError;
use crate::events::RaccEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub repo_id: i64,
    pub description: String,
    pub images: String,
    pub status: String,
    pub session_id: Option<i64>,
    pub supervisor_status: Option<String>,
    pub retry_count: i64,
    pub last_retry_at: Option<String>,
    pub max_retries: i64,
    pub created_at: String,
    pub updated_at: String,
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        repo_id: row.get(1)?,
        description: row.get(2)?,
        images: row.get(3)?,
        status: row.get(4)?,
        session_id: row.get(5)?,
        supervisor_status: row.get(6)?,
        retry_count: row.get(7)?,
        last_retry_at: row.get(8)?,
        max_retries: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

const SELECT_TASK: &str =
    "SELECT id, repo_id, description, images, status, session_id, supervisor_status, retry_count, last_retry_at, max_retries, created_at, updated_at FROM tasks";

pub async fn create_task(
    ctx: &AppContext,
    repo_id: i64,
    description: String,
    images: Option<String>,
) -> Result<Task, CoreError> {
    let images = images.unwrap_or_else(|| "[]".to_string());
    let task = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        conn.execute(
            "INSERT INTO tasks (repo_id, description, images) VALUES (?1, ?2, ?3)",
            rusqlite::params![repo_id, description, images],
        )?;

        let id = conn.last_insert_rowid();
        conn.query_row(
            &format!("{SELECT_TASK} WHERE id = ?1"),
            [id],
            row_to_task,
        )?
    };

    ctx.event_bus
        .emit(RaccEvent::TaskStatusChanged {
            task_id: task.id,
            status: "open".to_string(),
            session_id: None,
        })
        .await;

    Ok(task)
}

pub fn list_tasks(
    ctx: &AppContext,
    repo_id: i64,
) -> Result<Vec<Task>, CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    let mut stmt = conn.prepare(&format!(
        "{SELECT_TASK} WHERE repo_id = ?1 ORDER BY created_at DESC"
    ))?;

    let tasks = stmt
        .query_map([repo_id], row_to_task)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(tasks)
}

pub async fn update_task_status(
    ctx: &AppContext,
    task_id: i64,
    status: String,
    session_id: Option<i64>,
) -> Result<Task, CoreError> {
    let valid = ["open", "working", "closed"];
    if !valid.contains(&status.as_str()) {
        return Err(CoreError::Other(format!(
            "Invalid status '{}'. Must be one of: {}",
            status,
            valid.join(", ")
        )));
    }

    let task = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;

        if let Some(sid) = session_id {
            // Also set supervisor_status to prevent supervisor from re-picking this task
            let sup_status = match status.as_str() {
                "working" => Some("Running"),
                "closed" => Some("Completed"),
                _ => None,
            };
            if let Some(ss) = sup_status {
                conn.execute(
                    "UPDATE tasks SET status = ?1, session_id = ?2, supervisor_status = ?3, updated_at = datetime('now') WHERE id = ?4",
                    rusqlite::params![status, sid, ss, task_id],
                )?;
            } else {
                conn.execute(
                    "UPDATE tasks SET status = ?1, session_id = ?2, updated_at = datetime('now') WHERE id = ?3",
                    rusqlite::params![status, sid, task_id],
                )?;
            }
        } else {
            conn.execute(
                "UPDATE tasks SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
                rusqlite::params![status, task_id],
            )?;
        }

        conn.query_row(
            &format!("{SELECT_TASK} WHERE id = ?1"),
            [task_id],
            row_to_task,
        )?
    };

    ctx.event_bus
        .emit(RaccEvent::TaskStatusChanged {
            task_id: task.id,
            status: task.status.clone(),
            session_id: task.session_id,
        })
        .await;

    Ok(task)
}

pub fn update_task_description(
    ctx: &AppContext,
    task_id: i64,
    description: String,
) -> Result<Task, CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    conn.execute(
        "UPDATE tasks SET description = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![description, task_id],
    )?;

    let task = conn.query_row(
        &format!("{SELECT_TASK} WHERE id = ?1"),
        [task_id],
        row_to_task,
    )?;

    Ok(task)
}

pub fn update_task_images(
    ctx: &AppContext,
    task_id: i64,
    images: String,
) -> Result<Task, CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    conn.execute(
        "UPDATE tasks SET images = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![images, task_id],
    )?;

    let task = conn.query_row(
        &format!("{SELECT_TASK} WHERE id = ?1"),
        [task_id],
        row_to_task,
    )?;

    Ok(task)
}

pub async fn delete_task(
    ctx: &AppContext,
    task_id: i64,
) -> Result<(), CoreError> {
    {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        let affected = conn.execute("DELETE FROM tasks WHERE id = ?1", [task_id])?;
        if affected == 0 {
            return Err(CoreError::NotFound(format!("Task {} not found", task_id)));
        }
    }

    ctx.event_bus
        .emit(RaccEvent::TaskDeleted { task_id })
        .await;

    Ok(())
}

pub fn get_pending_tasks(ctx: &AppContext, repo_id: i64) -> Result<Vec<Task>, CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    let mut stmt = conn.prepare(&format!(
        "{SELECT_TASK} WHERE repo_id = ?1 AND (supervisor_status = 'Pending' OR (supervisor_status IS NULL AND status = 'open')) ORDER BY created_at ASC"
    ))?;
    let tasks = stmt
        .query_map([repo_id], row_to_task)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

// --- Image file commands ---

pub fn save_task_image(
    repo_path: String,
    filename: String,
    data: Vec<u8>,
) -> Result<String, CoreError> {
    let dir = std::path::PathBuf::from(&repo_path)
        .join(".racc")
        .join("images");
    std::fs::create_dir_all(&dir)?;

    let path = dir.join(&filename);
    std::fs::write(&path, &data)?;

    Ok(path.to_string_lossy().to_string())
}

pub fn copy_file_to_task_images(
    repo_path: String,
    source_path: String,
    filename: String,
) -> Result<String, CoreError> {
    let dir = std::path::PathBuf::from(&repo_path)
        .join(".racc")
        .join("images");
    std::fs::create_dir_all(&dir)?;

    let dest = dir.join(&filename);
    std::fs::copy(&source_path, &dest)?;

    Ok(dest.to_string_lossy().to_string())
}

pub fn delete_task_image(repo_path: String, filename: String) -> Result<(), CoreError> {
    let path = std::path::PathBuf::from(&repo_path)
        .join(".racc")
        .join("images")
        .join(&filename);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

pub fn rename_task_image(
    repo_path: String,
    old_name: String,
    new_name: String,
) -> Result<(), CoreError> {
    let dir = std::path::PathBuf::from(&repo_path)
        .join(".racc")
        .join("images");
    let old_path = dir.join(&old_name);
    let new_path = dir.join(&new_name);
    if old_path.exists() {
        std::fs::rename(&old_path, &new_path)?;
    }
    Ok(())
}
