use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub repo_id: i64,
    pub description: String,
    pub images: String,
    pub status: String,
    pub session_id: Option<i64>,
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
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

const SELECT_TASK: &str =
    "SELECT id, repo_id, description, images, status, session_id, created_at, updated_at FROM tasks";

#[tauri::command]
pub fn create_task(
    app_handle: tauri::AppHandle,
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    repo_id: i64,
    description: String,
    images: Option<String>,
) -> Result<Task, String> {
    let images = images.unwrap_or_else(|| "[]".to_string());
    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO tasks (repo_id, description, images) VALUES (?1, ?2, ?3)",
        rusqlite::params![repo_id, description, images],
    )
    .map_err(|e| format!("Failed to create task: {e}"))?;

    let id = conn.last_insert_rowid();
    let task = conn
        .query_row(
            &format!("{SELECT_TASK} WHERE id = ?1"),
            [id],
            row_to_task,
        )
        .map_err(|e| format!("Failed to fetch created task: {e}"))?;

    if let Some(tx) = app_handle.try_state::<crate::events::EventSender>() {
        let _: Result<_, _> = tx.send(crate::events::RaccEvent::TaskStatusChanged {
            task_id: task.id,
            status: "open".to_string(),
            session_id: None,
        });
    }

    Ok(task)
}

#[tauri::command]
pub fn list_tasks(
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    repo_id: i64,
) -> Result<Vec<Task>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "{SELECT_TASK} WHERE repo_id = ?1 ORDER BY created_at DESC"
        ))
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let tasks = stmt
        .query_map([repo_id], row_to_task)
        .map_err(|e| format!("Failed to query tasks: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect tasks: {e}"))?;

    Ok(tasks)
}

#[tauri::command]
pub fn update_task_status(
    app_handle: tauri::AppHandle,
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    task_id: i64,
    status: String,
    session_id: Option<i64>,
) -> Result<Task, String> {
    let valid = ["open", "working", "closed"];
    if !valid.contains(&status.as_str()) {
        return Err(format!(
            "Invalid status '{}'. Must be one of: {}",
            status,
            valid.join(", ")
        ));
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
            &format!("{SELECT_TASK} WHERE id = ?1"),
            [task_id],
            row_to_task,
        )
        .map_err(|e| format!("Failed to fetch updated task: {e}"))?;

    if let Some(tx) = app_handle.try_state::<crate::events::EventSender>() {
        let _: Result<_, _> = tx.send(crate::events::RaccEvent::TaskStatusChanged {
            task_id: task.id,
            status: task.status.clone(),
            session_id: task.session_id,
        });
    }

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
            &format!("{SELECT_TASK} WHERE id = ?1"),
            [task_id],
            row_to_task,
        )
        .map_err(|e| format!("Failed to fetch updated task: {e}"))?;

    Ok(task)
}

#[tauri::command]
pub fn update_task_images(
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    task_id: i64,
    images: String,
) -> Result<Task, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE tasks SET images = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![images, task_id],
    )
    .map_err(|e| format!("Failed to update task images: {e}"))?;

    let task = conn
        .query_row(
            &format!("{SELECT_TASK} WHERE id = ?1"),
            [task_id],
            row_to_task,
        )
        .map_err(|e| format!("Failed to fetch updated task: {e}"))?;

    Ok(task)
}

#[tauri::command]
pub fn delete_task(
    app_handle: tauri::AppHandle,
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    task_id: i64,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    let affected = conn
        .execute("DELETE FROM tasks WHERE id = ?1", [task_id])
        .map_err(|e| format!("Failed to delete task: {e}"))?;
    if affected == 0 {
        return Err(format!("Task {} not found", task_id));
    }

    if let Some(tx) = app_handle.try_state::<crate::events::EventSender>() {
        let _: Result<_, _> = tx.send(crate::events::RaccEvent::TaskDeleted { task_id });
    }

    Ok(())
}

// --- Image file commands ---

#[tauri::command]
pub fn save_task_image(
    repo_path: String,
    filename: String,
    data: Vec<u8>,
) -> Result<String, String> {
    let dir = std::path::PathBuf::from(&repo_path)
        .join(".racc")
        .join("images");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create image dir: {e}"))?;

    let path = dir.join(&filename);
    std::fs::write(&path, &data)
        .map_err(|e| format!("Failed to write image: {e}"))?;

    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn copy_file_to_task_images(
    repo_path: String,
    source_path: String,
    filename: String,
) -> Result<String, String> {
    let dir = std::path::PathBuf::from(&repo_path)
        .join(".racc")
        .join("images");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create image dir: {e}"))?;

    let dest = dir.join(&filename);
    std::fs::copy(&source_path, &dest)
        .map_err(|e| format!("Failed to copy image: {e}"))?;

    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
pub fn delete_task_image(repo_path: String, filename: String) -> Result<(), String> {
    let path = std::path::PathBuf::from(&repo_path)
        .join(".racc")
        .join("images")
        .join(&filename);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete image: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn rename_task_image(
    repo_path: String,
    old_name: String,
    new_name: String,
) -> Result<(), String> {
    let dir = std::path::PathBuf::from(&repo_path)
        .join(".racc")
        .join("images");
    let old_path = dir.join(&old_name);
    let new_path = dir.join(&new_name);
    if old_path.exists() {
        std::fs::rename(&old_path, &new_path)
            .map_err(|e| format!("Failed to rename image: {e}"))?;
    }
    Ok(())
}
