use crate::events::{EventSender, RaccEvent};
use futures_util::{SinkExt, StreamExt};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message;

// --- Types ---

type ConnId = u64;
type ConnPool = Arc<RwLock<HashMap<ConnId, mpsc::UnboundedSender<Message>>>>;
type Db = Arc<Mutex<Connection>>;

#[derive(Debug, Deserialize)]
struct Request {
    id: String,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct Response {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

// --- Helper: run a blocking closure that accesses the DB ---

async fn with_db<F, T>(db: &Db, f: F) -> Result<T, String>
where
    F: FnOnce(&Connection) -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    let db = db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| e.to_string())?;
        f(&conn)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

// --- Helper: emit a RaccEvent to broadcast channel + Tauri event system ---

fn emit_event(app_handle: &AppHandle, event: RaccEvent) {
    let tx = app_handle.state::<EventSender>().inner().clone();
    let _ = tx.send(event.clone());
    let _ = app_handle.emit("racc://event", &event);
}

// --- Broadcast: fan out RaccEvent to all WS clients ---

async fn broadcast_events(
    app_handle: AppHandle,
    pool: ConnPool,
) {
    let tx = app_handle.state::<EventSender>().inner().clone();
    let mut rx = tx.subscribe();

    loop {
        match rx.recv().await {
            Ok(event) => {
                // RaccEvent uses #[serde(tag = "event", content = "data")],
                // so serializing directly produces {"event":"...", "data":{...}}
                let msg_text = match serde_json::to_string(&event) {
                    Ok(j) => j,
                    Err(e) => {
                        log::error!("Failed to serialize event: {}", e);
                        continue;
                    }
                };
                let clients = pool.read().await;
                for (id, sender) in clients.iter() {
                    if let Err(e) = sender.send(Message::text(msg_text.clone())) {
                        log::warn!("Failed to send to client {}: {}", id, e);
                    }
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                log::warn!("Event broadcast lagged by {} messages", n);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                log::info!("Event broadcast channel closed");
                break;
            }
        }
    }
}

// --- Main server entry point ---

pub async fn start(app_handle: AppHandle, db: Db, mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
    let addr = "127.0.0.1:9399";
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => {
            log::info!("WebSocket server listening on ws://{}", addr);
            l
        }
        Err(e) => {
            log::error!("Failed to bind WebSocket server on {}: {}", addr, e);
            return;
        }
    };

    let pool: ConnPool = Arc::new(RwLock::new(HashMap::new()));

    // Spawn event broadcaster
    {
        let pool_clone = pool.clone();
        let handle_clone = app_handle.clone();
        tauri::async_runtime::spawn(broadcast_events(handle_clone, pool_clone));
    }

    let mut next_id: ConnId = 0;

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        log::info!("New WebSocket connection from {}", addr);
                        let conn_id = next_id;
                        next_id += 1;

                        let pool_clone = pool.clone();
                        let handle_clone = app_handle.clone();
                        let db_clone = db.clone();

                        tauri::async_runtime::spawn(async move {
                            handle_connection(conn_id, stream, pool_clone, handle_clone, db_clone).await;
                        });
                    }
                    Err(e) => {
                        log::error!("WebSocket accept error: {}", e);
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                if !*shutdown_rx.borrow() { continue; }
                log::info!("WebSocket server shutting down");
                // Send close frame to all connected clients
                let pool_read = pool.read().await;
                for (_, sender) in pool_read.iter() {
                    let _ = sender.send(Message::Close(None));
                }
                break;
            }
        }
    }
}

// --- Per-connection handler ---

async fn handle_connection(
    conn_id: ConnId,
    stream: tokio::net::TcpStream,
    pool: ConnPool,
    app_handle: AppHandle,
    db: Db,
) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            log::error!("WebSocket handshake failed for conn {}: {}", conn_id, e);
            return;
        }
    };

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Outgoing channel for this connection
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    pool.write().await.insert(conn_id, tx);

    // Spawn sender task
    let send_task = tauri::async_runtime::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = ws_sender.send(msg).await {
                log::warn!("WebSocket send error for conn {}: {}", conn_id, e);
                break;
            }
        }
    });

    // Spawn heartbeat task
    let pool_clone = pool.clone();
    let heartbeat_task = tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let clients = pool_clone.read().await;
            if let Some(sender) = clients.get(&conn_id) {
                if sender.send(Message::Ping(vec![].into())).is_err() {
                    break;
                }
            } else {
                break;
            }
        }
    });

    // Read loop
    while let Some(msg_result) = ws_receiver.next().await {
        match msg_result {
            Ok(Message::Text(ref text)) => {
                let text_str = text.to_string();
                match serde_json::from_str::<Request>(&text_str) {
                    Ok(req) => {
                        let req_id = req.id.clone();
                        let params = req.params.clone().unwrap_or(Value::Object(Default::default()));
                        let response = dispatch(&app_handle, &db, req.method.as_str(), params).await;
                        let reply = match response {
                            Ok(result) => Response {
                                id: req_id,
                                result: Some(result),
                                error: None,
                            },
                            Err(err_msg) => Response {
                                id: req_id,
                                result: None,
                                error: Some(err_msg),
                            },
                        };
                        if let Ok(json) = serde_json::to_string(&reply) {
                            let clients = pool.read().await;
                            if let Some(sender) = clients.get(&conn_id) {
                                let _ = sender.send(Message::text(json));
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to parse request from conn {}: {}", conn_id, e);
                    }
                }
            }
            Ok(Message::Pong(_)) => {
                // heartbeat acknowledged
            }
            Ok(Message::Close(_)) => {
                log::info!("WebSocket conn {} closed", conn_id);
                break;
            }
            Err(e) => {
                log::warn!("WebSocket read error for conn {}: {}", conn_id, e);
                break;
            }
            _ => {}
        }
    }

    // Cleanup
    pool.write().await.remove(&conn_id);
    send_task.abort();
    heartbeat_task.abort();
    log::info!("WebSocket conn {} cleaned up", conn_id);
}

// --- Method dispatcher ---

async fn dispatch(app_handle: &AppHandle, db: &Db, method: &str, params: Value) -> Result<Value, String> {
    match method {
        "create_task" => handle_create_task(app_handle, db, params).await,
        "list_tasks" => handle_list_tasks(db, params).await,
        "update_task_status" => handle_update_task_status(app_handle, db, params).await,
        "update_task_description" => handle_update_task_description(db, params).await,
        "delete_task" => handle_delete_task(app_handle, db, params).await,
        "create_session" => handle_create_session(app_handle, db, params).await,
        "stop_session" => handle_stop_session(app_handle, db, params).await,
        "reattach_session" => handle_reattach_session(app_handle, db, params).await,
        "list_repos" => handle_list_repos(db, params).await,
        "get_session_diff" => handle_get_session_diff(db, params).await,
        _ => Err(format!("Unknown method: {}", method)),
    }
}

// --- Task handlers ---

async fn handle_create_task(app_handle: &AppHandle, db: &Db, params: Value) -> Result<Value, String> {
    let repo_id = params["repo_id"]
        .as_i64()
        .ok_or("Missing or invalid repo_id")?;
    let description = params["description"]
        .as_str()
        .ok_or("Missing or invalid description")?
        .to_string();

    let task_id = with_db(db, move |conn| {
        conn.execute(
            "INSERT INTO tasks (repo_id, description) VALUES (?1, ?2)",
            rusqlite::params![repo_id, description],
        )
        .map_err(|e| format!("Failed to create task: {e}"))?;
        Ok(conn.last_insert_rowid())
    })
    .await?;

    emit_event(
        app_handle,
        RaccEvent::TaskStatusChanged {
            task_id,
            status: "open".to_string(),
            session_id: None,
        },
    );

    Ok(json!({ "task_id": task_id }))
}

async fn handle_list_tasks(db: &Db, params: Value) -> Result<Value, String> {
    let repo_id = params["repo_id"]
        .as_i64()
        .ok_or("Missing or invalid repo_id")?;

    let tasks = with_db(db, move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, repo_id, description, status, session_id, created_at, updated_at FROM tasks WHERE repo_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let tasks: Vec<Value> = stmt
            .query_map([repo_id], |row| {
                let id: i64 = row.get(0)?;
                let repo_id: i64 = row.get(1)?;
                let description: String = row.get(2)?;
                let status: String = row.get(3)?;
                let session_id: Option<i64> = row.get(4)?;
                let created_at: String = row.get(5)?;
                let updated_at: String = row.get(6)?;
                Ok(json!({
                    "id": id,
                    "repo_id": repo_id,
                    "description": description,
                    "status": status,
                    "session_id": session_id,
                    "created_at": created_at,
                    "updated_at": updated_at,
                }))
            })
            .map_err(|e| format!("Failed to query tasks: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect tasks: {e}"))?;

        Ok(tasks)
    })
    .await?;

    Ok(json!({ "tasks": tasks }))
}

async fn handle_update_task_status(app_handle: &AppHandle, db: &Db, params: Value) -> Result<Value, String> {
    let task_id = params["task_id"]
        .as_i64()
        .ok_or("Missing or invalid task_id")?;
    let status = params["status"]
        .as_str()
        .ok_or("Missing or invalid status")?
        .to_string();
    let session_id = params["session_id"].as_i64();

    let valid = ["open", "working", "closed"];
    if !valid.contains(&status.as_str()) {
        return Err(format!(
            "Invalid status '{}'. Must be one of: {}",
            status,
            valid.join(", ")
        ));
    }

    let status_clone = status.clone();
    with_db(db, move |conn| {
        if let Some(sid) = session_id {
            conn.execute(
                "UPDATE tasks SET status = ?1, session_id = ?2, updated_at = datetime('now') WHERE id = ?3",
                rusqlite::params![status_clone, sid, task_id],
            )
            .map_err(|e| format!("Failed to update task: {e}"))?;
        } else {
            conn.execute(
                "UPDATE tasks SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
                rusqlite::params![status_clone, task_id],
            )
            .map_err(|e| format!("Failed to update task: {e}"))?;
        }
        Ok(())
    })
    .await?;

    emit_event(
        app_handle,
        RaccEvent::TaskStatusChanged {
            task_id,
            status,
            session_id,
        },
    );

    Ok(json!({}))
}

async fn handle_update_task_description(
    db: &Db,
    params: Value,
) -> Result<Value, String> {
    let task_id = params["task_id"]
        .as_i64()
        .ok_or("Missing or invalid task_id")?;
    let description = params["description"]
        .as_str()
        .ok_or("Missing or invalid description")?
        .to_string();

    with_db(db, move |conn| {
        conn.execute(
            "UPDATE tasks SET description = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![description, task_id],
        )
        .map_err(|e| format!("Failed to update task description: {e}"))?;
        Ok(())
    })
    .await?;

    Ok(json!({}))
}

async fn handle_delete_task(app_handle: &AppHandle, db: &Db, params: Value) -> Result<Value, String> {
    let task_id = params["task_id"]
        .as_i64()
        .ok_or("Missing or invalid task_id")?;

    with_db(db, move |conn| {
        let affected = conn
            .execute("DELETE FROM tasks WHERE id = ?1", [task_id])
            .map_err(|e| format!("Failed to delete task: {e}"))?;
        if affected == 0 {
            return Err(format!("Task {} not found", task_id));
        }
        Ok(())
    })
    .await?;

    emit_event(app_handle, RaccEvent::TaskDeleted { task_id });

    Ok(json!({}))
}

// --- Session handlers ---

async fn handle_create_session(app_handle: &AppHandle, db: &Db, params: Value) -> Result<Value, String> {
    let repo_id = params["repo_id"]
        .as_i64()
        .ok_or("Missing or invalid repo_id")?;
    let use_worktree = params["use_worktree"].as_bool().unwrap_or(false);
    let branch_param = params["branch"].as_str().map(|s| s.to_string());
    let agent = params["agent"]
        .as_str()
        .unwrap_or("claude-code")
        .to_string();

    // Look up repo
    let (repo_path, repo_name) = with_db(db, move |conn| {
        conn.query_row(
            "SELECT path, name FROM repos WHERE id = ?1",
            [repo_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|e| format!("Repo not found: {e}"))
    })
    .await?;

    // Determine worktree path and branch
    let (worktree_path, branch_name) = if use_worktree {
        let branch = branch_param.unwrap_or_else(|| {
            let now = chrono::Local::now();
            format!("racc/{}", now.format("%Y%m%d-%H%M%S"))
        });
        let safe_branch = branch.replace('/', "-");

        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .ok_or("Could not find home directory")?;
        let wt_dir = home
            .join("racc-worktrees")
            .join(&repo_name)
            .join(&safe_branch);

        let wt_path = wt_dir.to_string_lossy().to_string();

        std::fs::create_dir_all(wt_dir.parent().unwrap())
            .map_err(|e| format!("Failed to create worktree parent dir: {e}"))?;

        // Try git worktree add -b {branch} {path}
        let output = std::process::Command::new("git")
            .args(["worktree", "add", "-b", &branch, &wt_path])
            .current_dir(&repo_path)
            .output()
            .map_err(|e| format!("git worktree add failed: {e}"))?;

        if !output.status.success() {
            // Branch may already exist — try without -b
            let output2 = std::process::Command::new("git")
                .args(["worktree", "add", &wt_path, &branch])
                .current_dir(&repo_path)
                .output()
                .map_err(|e| format!("git worktree add failed: {e}"))?;

            if !output2.status.success() {
                return Err(format!(
                    "git worktree add failed: {}",
                    String::from_utf8_lossy(&output2.stderr)
                ));
            }
        }

        (Some(wt_path), branch)
    } else {
        // Use repo path directly, detect current branch
        let branch = {
            let output = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(&repo_path)
                .output()
                .map_err(|e| format!("Failed to get branch: {e}"))?;
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };
        (None, branch)
    };

    let agent_clone = agent.clone();
    let worktree_path_clone = worktree_path.clone();
    let branch_name_clone = branch_name.clone();

    let session_id = with_db(db, move |conn| {
        conn.execute(
            "INSERT INTO sessions (repo_id, agent, worktree_path, branch, status) VALUES (?1, ?2, ?3, ?4, 'Running')",
            rusqlite::params![repo_id, agent_clone, worktree_path_clone, branch_name_clone],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    })
    .await?;

    // Emit RaccEvent
    emit_event(
        app_handle,
        RaccEvent::SessionStatusChanged {
            session_id,
            status: "Running".to_string(),
            pr_url: None,
            source: "remote".to_string(),
        },
    );

    // Emit Tauri event for frontend PTY bootstrap
    let _ = app_handle.emit(
        "racc://session-created",
        json!({
            "session_id": session_id,
            "repo_id": repo_id,
            "branch": branch_name,
            "worktree_path": worktree_path,
            "agent": agent,
            "source": "remote",
        }),
    );

    Ok(json!({ "session_id": session_id }))
}

async fn handle_stop_session(app_handle: &AppHandle, db: &Db, params: Value) -> Result<Value, String> {
    let session_id = params["session_id"]
        .as_i64()
        .ok_or("Missing or invalid session_id")?;

    with_db(db, move |conn| {
        conn.execute(
            "UPDATE sessions SET status = 'Completed', updated_at = datetime('now') WHERE id = ?1",
            [session_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await?;

    emit_event(
        app_handle,
        RaccEvent::SessionStatusChanged {
            session_id,
            status: "Completed".to_string(),
            pr_url: None,
            source: "remote".to_string(),
        },
    );

    let _ = app_handle.emit(
        "racc://session-stopped",
        json!({
            "session_id": session_id,
            "source": "remote",
        }),
    );

    Ok(json!({}))
}

async fn handle_reattach_session(app_handle: &AppHandle, db: &Db, params: Value) -> Result<Value, String> {
    let session_id = params["session_id"]
        .as_i64()
        .ok_or("Missing or invalid session_id")?;

    let (repo_id, agent, worktree_path, branch, pr_url, created_at, updated_at) =
        with_db(db, move |conn| {
            let status: String = conn
                .query_row(
                    "SELECT status FROM sessions WHERE id = ?1",
                    [session_id],
                    |row| row.get(0),
                )
                .map_err(|e| format!("Session not found: {e}"))?;

            if status == "Running" {
                return Err("Session is already running".to_string());
            }

            conn.execute(
                "UPDATE sessions SET status = 'Running', updated_at = datetime('now') WHERE id = ?1",
                [session_id],
            )
            .map_err(|e| e.to_string())?;

            let row = conn
                .query_row(
                    "SELECT repo_id, agent, worktree_path, branch, pr_url, created_at, updated_at FROM sessions WHERE id = ?1",
                    [session_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                        ))
                    },
                )
                .map_err(|e| e.to_string())?;

            Ok(row)
        })
        .await?;

    emit_event(
        app_handle,
        RaccEvent::SessionStatusChanged {
            session_id,
            status: "Running".to_string(),
            pr_url: pr_url.clone(),
            source: "remote".to_string(),
        },
    );

    let _ = app_handle.emit(
        "racc://session-created",
        json!({
            "session_id": session_id,
            "repo_id": repo_id,
            "branch": branch,
            "worktree_path": worktree_path,
            "agent": agent,
            "source": "remote",
            "reattach": true,
        }),
    );

    Ok(json!({
        "session": {
            "id": session_id,
            "repo_id": repo_id,
            "agent": agent,
            "worktree_path": worktree_path,
            "branch": branch,
            "status": "Running",
            "created_at": created_at,
            "updated_at": updated_at,
            "pr_url": pr_url,
        }
    }))
}

// --- Query handlers ---

async fn handle_list_repos(db: &Db, _params: Value) -> Result<Value, String> {
    let repos = with_db(db, move |conn| {
        let mut stmt = conn
            .prepare("SELECT id, path, name, added_at FROM repos ORDER BY name")
            .map_err(|e| e.to_string())?;

        let repos: Vec<Value> = stmt
            .query_map([], |row| {
                Ok(json!({
                    "id": row.get::<_, i64>(0)?,
                    "path": row.get::<_, String>(1)?,
                    "name": row.get::<_, String>(2)?,
                    "added_at": row.get::<_, String>(3)?,
                }))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(repos)
    })
    .await?;

    Ok(json!({ "repos": repos }))
}

async fn handle_get_session_diff(db: &Db, params: Value) -> Result<Value, String> {
    let session_id = params["session_id"]
        .as_i64()
        .ok_or("Missing or invalid session_id")?;

    let (worktree_path, repo_path) = with_db(db, move |conn| {
        let (worktree_path, repo_id): (Option<String>, i64) = conn
            .query_row(
                "SELECT worktree_path, repo_id FROM sessions WHERE id = ?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| format!("Session not found: {e}"))?;

        let repo_path: String = conn
            .query_row(
                "SELECT path FROM repos WHERE id = ?1",
                [repo_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("Repo not found: {e}"))?;

        Ok((worktree_path, repo_path))
    })
    .await?;

    // Use worktree_path if present, fall back to repo path
    let diff_dir = worktree_path.unwrap_or(repo_path);

    let output = std::process::Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(&diff_dir)
        .output()
        .map_err(|e| format!("Failed to run git diff: {e}"))?;

    let diff = String::from_utf8_lossy(&output.stdout).to_string();

    Ok(json!({ "diff": diff }))
}
