# WebSocket Remote API Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a WebSocket server to Racc's Rust backend so external clients can create tasks, start sessions, and receive status events remotely.

**Architecture:** Embed `tokio-tungstenite` WebSocket server in Tauri's async runtime, sharing DB access via `AppHandle`. A `tokio::broadcast` channel serves as event bus — mutations from both Tauri commands and WS handlers emit events, which are fanned out to WS clients and the frontend via `AppHandle.emit()`.

**Tech Stack:** `tokio-tungstenite`, `futures-util`, `tokio::broadcast`, Tauri 2.x event system

**Spec:** `docs/superpowers/specs/2026-03-14-websocket-remote-api-design.md`

**Note:** No test framework is configured in this project. Steps that would normally be TDD use manual verification via `cargo check` / `cargo build` / runtime testing with a WebSocket client (e.g., `websocat`).

---

## Chunk 1: Foundation — Events Module + Dependencies

### Task 1: Add Cargo dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add tokio-tungstenite and futures-util to Cargo.toml**

Add after the existing `tokio` dependency (around line 15):

```toml
tokio-tungstenite = "0.24"
futures-util = { version = "0.3", default-features = false, features = ["sink"] }
log = "0.4"
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Successful compilation, no errors

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: add tokio-tungstenite and futures-util dependencies"
```

---

### Task 2: Create events module

**Files:**
- Create: `src-tauri/src/events.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod events`)

- [ ] **Step 1: Create `src-tauri/src/events.rs`**

```rust
use serde::Serialize;
use tokio::sync::broadcast;

/// Events emitted when session or task state changes.
/// Consumed by the WebSocket server (fan-out to clients) and
/// the frontend (via AppHandle.emit()).
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum RaccEvent {
    #[serde(rename = "session_status_changed")]
    SessionStatusChanged {
        session_id: i64,
        status: String,
        pr_url: Option<String>,
        #[serde(skip)]
        source: String, // "local" or "remote" — internal only, not sent to WS clients
    },
    #[serde(rename = "task_status_changed")]
    TaskStatusChanged {
        task_id: i64,
        status: String,
        session_id: Option<i64>,
    },
    #[serde(rename = "task_deleted")]
    TaskDeleted {
        task_id: i64,
    },
}

/// Type alias for the broadcast sender stored in Tauri managed state.
pub type EventSender = broadcast::Sender<RaccEvent>;

/// Create a new event bus with the given capacity.
pub fn create_event_bus() -> (EventSender, broadcast::Receiver<RaccEvent>) {
    broadcast::channel(64)
}
```

- [ ] **Step 2: Add `mod events` to `lib.rs`**

In `src-tauri/src/lib.rs`, add `mod events;` at the top, after the existing `mod commands;` line.

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Successful compilation (events module unused for now, that's fine)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/events.rs src-tauri/src/lib.rs
git commit -m "feat: add RaccEvent enum and broadcast event bus"
```

---

### Task 3: Wire broadcast channel into Tauri managed state

**Files:**
- Modify: `src-tauri/src/lib.rs:6-43` (the `run()` function)

- [ ] **Step 1: Create event bus and add to managed state**

In `src-tauri/src/lib.rs`, in the `run()` function:

1. After `let db = commands::db::init_db()...` (line 7), add:
```rust
let (event_tx, _event_rx) = events::create_event_bus();
```

2. After `.manage(tokio::sync::Mutex::new(commands::assistant::SidecarState::new()))` (line 14), add:
```rust
.manage(event_tx)
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Successful compilation

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: wire broadcast event bus into Tauri managed state"
```

---

## Chunk 2: WebSocket Server Core

### Task 4: Build WebSocket server — listener and connection management

**Files:**
- Create: `src-tauri/src/ws_server.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod ws_server`, spawn in setup)

This is the largest task. The WS server handles:
- TCP listener on `127.0.0.1:9399`
- WebSocket upgrade for each connection
- Connection pool with auto-incrementing IDs
- Ping/pong heartbeat (30s interval)
- Event broadcast subscriber
- Message routing to handlers

- [ ] **Step 1: Create `src-tauri/src/ws_server.rs` with server startup and connection management**

```rust
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::events::{EventSender, RaccEvent};

/// Incoming request from a WebSocket client.
#[derive(Debug, Deserialize)]
struct WsRequest {
    id: Option<String>,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

/// Outgoing response to a WebSocket client.
#[derive(Debug, Serialize)]
struct WsResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Outgoing push event to a WebSocket client.
#[derive(Debug, Serialize)]
struct WsPushEvent {
    event: String,
    data: serde_json::Value,
}

type ConnectionPool = Arc<RwLock<HashMap<u64, tokio::sync::mpsc::UnboundedSender<Message>>>>;

static NEXT_CONN_ID: AtomicU64 = AtomicU64::new(1);

/// Start the WebSocket server. Called from Tauri setup().
pub async fn start(app_handle: AppHandle) {
    let addr = "127.0.0.1:9399";
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => {
            log::info!("WebSocket server listening on ws://{}", addr);
            l
        }
        Err(e) => {
            log::error!("Failed to bind WebSocket server to {}: {}", addr, e);
            return;
        }
    };

    let pool: ConnectionPool = Arc::new(RwLock::new(HashMap::new()));

    // Spawn event broadcaster
    let event_tx: EventSender = app_handle.state::<EventSender>().inner().clone();
    let mut event_rx = event_tx.subscribe();
    let broadcast_pool = pool.clone();
    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    broadcast_event(&broadcast_pool, &event).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("Event broadcaster lagged, dropped {} events", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    log::info!("Event bus closed, stopping broadcaster");
                    break;
                }
            }
        }
    });

    // Accept connections
    while let Ok((stream, peer_addr)) = listener.accept().await {
        let conn_id = NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed);
        log::info!("New WebSocket connection {} from {}", conn_id, peer_addr);
        let app = app_handle.clone();
        let pool = pool.clone();
        tokio::spawn(handle_connection(conn_id, stream, app, pool));
    }
}

async fn handle_connection(
    conn_id: u64,
    stream: TcpStream,
    app_handle: AppHandle,
    pool: ConnectionPool,
) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            log::error!("WebSocket handshake failed for conn {}: {}", conn_id, e);
            return;
        }
    };

    let (mut ws_sink, mut ws_stream_rx) = ws_stream.split();

    // Create an mpsc channel so the event broadcaster and heartbeat can send
    // messages without holding a lock on the sink.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Message>();

    // Register in connection pool
    pool.write().await.insert(conn_id, tx.clone());

    // Sink writer task: drains mpsc channel into WebSocket sink
    let sink_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Heartbeat: ping every 30s
    let heartbeat_tx = tx.clone();
    let heartbeat_task = tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(30));
        loop {
            tick.tick().await;
            if heartbeat_tx.send(Message::Ping(vec![].into()) // Ping takes Bytes).is_err() {
                break;
            }
        }
    });

    // Read loop: process incoming messages
    while let Some(msg_result) = ws_stream_rx.next().await {
        match msg_result {
            Ok(Message::Text(ref text)) => {
                let response = handle_message(&app_handle, text).await;
                let json = serde_json::to_string(&response).unwrap_or_default();
                if tx.send(Message::text(json)).is_err() {
                    break;
                }
            }
            Ok(Message::Pong(_)) => {
                // Client responded to ping, connection is alive
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                log::warn!("WebSocket error on conn {}: {}", conn_id, e);
                break;
            }
            _ => {} // Ignore binary, ping from client, etc.
        }
    }

    // Cleanup
    heartbeat_task.abort();
    sink_task.abort();
    pool.write().await.remove(&conn_id);
    log::info!("WebSocket connection {} closed", conn_id);
}

async fn broadcast_event(pool: &ConnectionPool, event: &RaccEvent) {
    let push = match serde_json::to_value(event) {
        Ok(val) => val,
        Err(_) => return,
    };
    // RaccEvent is serialized with tag="event", content="data" by serde,
    // so `push` is already { "event": "...", "data": { ... } }
    let json = serde_json::to_string(&push).unwrap_or_default();
    let msg = Message::text(json);

    let pool_read = pool.read().await;
    for (_, sender) in pool_read.iter() {
        let _ = sender.send(msg.clone());
    }
}

async fn handle_message(app_handle: &AppHandle, text: &str) -> WsResponse {
    let req: WsRequest = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            return WsResponse {
                id: None,
                result: None,
                error: Some(format!("Invalid request: {}", e)),
            };
        }
    };

    let id = req.id.clone();
    match dispatch(app_handle, &req).await {
        Ok(result) => WsResponse {
            id,
            result: Some(result),
            error: None,
        },
        Err(err) => WsResponse {
            id,
            result: None,
            error: Some(err),
        },
    }
}

/// Route a WS request to the appropriate handler.
async fn dispatch(app_handle: &AppHandle, req: &WsRequest) -> Result<serde_json::Value, String> {
    match req.method.as_str() {
        "create_task" => handlers::create_task(app_handle, &req.params).await,
        "list_tasks" => handlers::list_tasks(app_handle, &req.params).await,
        "update_task_status" => handlers::update_task_status(app_handle, &req.params).await,
        "update_task_description" => handlers::update_task_description(app_handle, &req.params).await,
        "delete_task" => handlers::delete_task(app_handle, &req.params).await,
        "create_session" => handlers::create_session(app_handle, &req.params).await,
        "stop_session" => handlers::stop_session(app_handle, &req.params).await,
        "reattach_session" => handlers::reattach_session(app_handle, &req.params).await,
        "list_repos" => handlers::list_repos(app_handle).await,
        "get_session_diff" => handlers::get_session_diff(app_handle, &req.params).await,
        _ => Err(format!("Unknown method: {}", req.method)),
    }
}

mod handlers {
    use super::*;
    use std::sync::Mutex;
    use rusqlite::Connection;

    // Helper: run a blocking closure that accesses the DB
    async fn with_db<F, T>(app_handle: &AppHandle, f: F) -> Result<T, String>
    where
        F: FnOnce(&Connection) -> Result<T, String> + Send + 'static,
        T: Send + 'static,
    {
        let db = app_handle.state::<Mutex<Connection>>().inner().clone();
        tokio::task::spawn_blocking(move || {
            let conn = db.lock().map_err(|e| e.to_string())?;
            f(&conn)
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?
    }

    // Helper: emit a RaccEvent
    fn emit_event(app_handle: &AppHandle, event: RaccEvent) {
        let tx = app_handle.state::<EventSender>();
        let _ = tx.send(event.clone());
        // Also emit to frontend via Tauri event system
        let _ = app_handle.emit("racc://event", &event);
    }

    // ---- Task handlers ----

    pub async fn create_task(
        app_handle: &AppHandle,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let repo_id = params["repo_id"].as_i64().ok_or("Missing repo_id")?;
        let description = params["description"]
            .as_str()
            .ok_or("Missing description")?
            .to_string();

        let task = with_db(app_handle, move |conn| {
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            conn.execute(
                "INSERT INTO tasks (repo_id, description, status, created_at, updated_at) VALUES (?1, ?2, 'open', ?3, ?3)",
                rusqlite::params![repo_id, description, now],
            ).map_err(|e| e.to_string())?;
            let task_id = conn.last_insert_rowid();
            Ok(serde_json::json!({ "task_id": task_id }))
        }).await?;

        emit_event(app_handle, RaccEvent::TaskStatusChanged {
            task_id: task["task_id"].as_i64().unwrap(),
            status: "open".to_string(),
            session_id: None,
        });

        Ok(task)
    }

    pub async fn list_tasks(
        app_handle: &AppHandle,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let repo_id = params["repo_id"].as_i64().ok_or("Missing repo_id")?;

        with_db(app_handle, move |conn| {
            let mut stmt = conn
                .prepare("SELECT id, repo_id, description, status, session_id, created_at, updated_at FROM tasks WHERE repo_id = ?1 ORDER BY created_at DESC")
                .map_err(|e| e.to_string())?;
            let tasks: Vec<serde_json::Value> = stmt
                .query_map(rusqlite::params![repo_id], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, i64>(0)?,
                        "repo_id": row.get::<_, i64>(1)?,
                        "description": row.get::<_, String>(2)?,
                        "status": row.get::<_, String>(3)?,
                        "session_id": row.get::<_, Option<i64>>(4)?,
                        "created_at": row.get::<_, String>(5)?,
                        "updated_at": row.get::<_, String>(6)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();
            Ok(serde_json::json!({ "tasks": tasks }))
        }).await
    }

    pub async fn update_task_status(
        app_handle: &AppHandle,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let task_id = params["task_id"].as_i64().ok_or("Missing task_id")?;
        let status = params["status"]
            .as_str()
            .ok_or("Missing status")?
            .to_string();
        let session_id = params["session_id"].as_i64();

        if !["open", "working", "closed"].contains(&status.as_str()) {
            return Err(format!("Invalid status: {}. Must be open, working, or closed", status));
        }

        let status_clone = status.clone();
        with_db(app_handle, move |conn| {
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            conn.execute(
                "UPDATE tasks SET status = ?1, session_id = ?2, updated_at = ?3 WHERE id = ?4",
                rusqlite::params![status_clone, session_id, now, task_id],
            ).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({}))
        }).await?;

        emit_event(app_handle, RaccEvent::TaskStatusChanged {
            task_id,
            status,
            session_id,
        });

        Ok(serde_json::json!({}))
    }

    pub async fn update_task_description(
        app_handle: &AppHandle,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let task_id = params["task_id"].as_i64().ok_or("Missing task_id")?;
        let description = params["description"]
            .as_str()
            .ok_or("Missing description")?
            .to_string();

        with_db(app_handle, move |conn| {
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            conn.execute(
                "UPDATE tasks SET description = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![description, now, task_id],
            ).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({}))
        }).await
    }

    pub async fn delete_task(
        app_handle: &AppHandle,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let task_id = params["task_id"].as_i64().ok_or("Missing task_id")?;

        with_db(app_handle, move |conn| {
            conn.execute("DELETE FROM tasks WHERE id = ?1", rusqlite::params![task_id])
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({}))
        }).await?;

        emit_event(app_handle, RaccEvent::TaskDeleted { task_id });

        Ok(serde_json::json!({}))
    }

    // ---- Session handlers ----

    pub async fn create_session(
        app_handle: &AppHandle,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let repo_id = params["repo_id"].as_i64().ok_or("Missing repo_id")?;
        let use_worktree = params["use_worktree"].as_bool().unwrap_or(true);
        let branch = params["branch"].as_str().map(|s| s.to_string());
        let agent = params["agent"]
            .as_str()
            .unwrap_or("claude-code")
            .to_string();

        // Reuse existing create_session command logic via invoke
        // We call the Tauri command directly through the app handle
        let db = app_handle.state::<std::sync::Mutex<Connection>>().inner().clone();

        let session_id = tokio::task::spawn_blocking(move || {
            let conn = db.lock().map_err(|e| e.to_string())?;

            // Look up repo
            let repo_path: String = conn
                .query_row("SELECT path FROM repos WHERE id = ?1", rusqlite::params![repo_id], |row| {
                    row.get(0)
                })
                .map_err(|e| format!("Repo not found: {}", e))?;

            let repo_name: String = conn
                .query_row("SELECT name FROM repos WHERE id = ?1", rusqlite::params![repo_id], |row| {
                    row.get(0)
                })
                .map_err(|e| e.to_string())?;

            let mut worktree_path: Option<String> = None;
            let mut actual_branch = branch.clone();

            if use_worktree {
                // Determine branch name
                let branch_name = match &branch {
                    Some(b) => b.clone(),
                    None => {
                        // Generate a branch name
                        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                        format!("racc/{}", timestamp)
                    }
                };
                actual_branch = Some(branch_name.clone());

                // Create worktree
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                let safe_branch = branch_name.replace('/', "-");
                let wt_path = format!("{}/racc-worktrees/{}/{}", home, repo_name, safe_branch);

                // Create parent dir
                std::fs::create_dir_all(&format!("{}/racc-worktrees/{}", home, repo_name))
                    .map_err(|e| format!("Failed to create worktree directory: {}", e))?;

                let output = std::process::Command::new("git")
                    .args(["worktree", "add", "-b", &branch_name, &wt_path])
                    .current_dir(&repo_path)
                    .output()
                    .map_err(|e| format!("Failed to create worktree: {}", e))?;

                if !output.status.success() {
                    // Try without -b (branch might already exist)
                    let output2 = std::process::Command::new("git")
                        .args(["worktree", "add", &wt_path, &branch_name])
                        .current_dir(&repo_path)
                        .output()
                        .map_err(|e| format!("Failed to create worktree: {}", e))?;
                    if !output2.status.success() {
                        return Err(format!(
                            "Failed to create worktree: {}",
                            String::from_utf8_lossy(&output2.stderr)
                        ));
                    }
                }
                worktree_path = Some(wt_path);
            }

            // Insert session
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            conn.execute(
                "INSERT INTO sessions (repo_id, agent, worktree_path, branch, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'Running', ?5, ?5)",
                rusqlite::params![repo_id, agent, worktree_path, actual_branch, now],
            ).map_err(|e| e.to_string())?;

            let session_id = conn.last_insert_rowid();
            Ok((session_id, worktree_path, actual_branch, repo_path, agent))
        }).await.map_err(|e| format!("Task join error: {}", e))??;

        let (sid, wt_path, branch, repo_path, agent_name) = session_id;

        // Emit event so frontend spawns PTY
        let cwd = wt_path.clone().unwrap_or(repo_path);
        let event = RaccEvent::SessionStatusChanged {
            session_id: sid,
            status: "Running".to_string(),
            pr_url: None,
            source: "remote".to_string(),
        };
        emit_event(app_handle, event);

        // Emit Tauri event with extra data for frontend PTY bootstrap
        let _ = app_handle.emit("racc://session-created", serde_json::json!({
            "session_id": sid,
            "repo_id": params["repo_id"].as_i64(),
            "branch": branch,
            "worktree_path": cwd,
            "agent": agent_name,
            "source": "remote",
        }));

        Ok(serde_json::json!({ "session_id": sid }))
    }

    pub async fn stop_session(
        app_handle: &AppHandle,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let session_id = params["session_id"].as_i64().ok_or("Missing session_id")?;

        with_db(app_handle, move |conn| {
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            conn.execute(
                "UPDATE sessions SET status = 'Completed', updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, session_id],
            ).map_err(|e| e.to_string())?;
            Ok(())
        }).await?;

        emit_event(app_handle, RaccEvent::SessionStatusChanged {
            session_id,
            status: "Completed".to_string(),
            pr_url: None,
            source: "remote".to_string(),
        });

        // Tell frontend to kill the PTY
        let _ = app_handle.emit("racc://session-stopped", serde_json::json!({
            "session_id": session_id,
            "source": "remote",
        }));

        Ok(serde_json::json!({}))
    }

    pub async fn reattach_session(
        app_handle: &AppHandle,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let session_id = params["session_id"].as_i64().ok_or("Missing session_id")?;

        let session = with_db(app_handle, move |conn| {
            // Verify session exists and is disconnected
            let status: String = conn
                .query_row(
                    "SELECT status FROM sessions WHERE id = ?1",
                    rusqlite::params![session_id],
                    |row| row.get(0),
                )
                .map_err(|e| format!("Session not found: {}", e))?;

            if status == "Running" {
                return Err("Session is already running".to_string());
            }

            // Update to Running
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            conn.execute(
                "UPDATE sessions SET status = 'Running', updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, session_id],
            ).map_err(|e| e.to_string())?;

            // Return session data
            let row = conn.query_row(
                "SELECT id, repo_id, agent, worktree_path, branch, status, created_at, updated_at, pr_url FROM sessions WHERE id = ?1",
                rusqlite::params![session_id],
                |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, i64>(0)?,
                        "repo_id": row.get::<_, i64>(1)?,
                        "agent": row.get::<_, String>(2)?,
                        "worktree_path": row.get::<_, Option<String>>(3)?,
                        "branch": row.get::<_, Option<String>>(4)?,
                        "status": row.get::<_, String>(5)?,
                        "created_at": row.get::<_, String>(6)?,
                        "updated_at": row.get::<_, String>(7)?,
                        "pr_url": row.get::<_, Option<String>>(8)?,
                    }))
                },
            ).map_err(|e| e.to_string())?;

            Ok(row)
        }).await?;

        emit_event(app_handle, RaccEvent::SessionStatusChanged {
            session_id,
            status: "Running".to_string(),
            pr_url: None,
            source: "remote".to_string(),
        });

        // Get worktree path for frontend
        let cwd = session["worktree_path"]
            .as_str()
            .map(|s| s.to_string());

        let _ = app_handle.emit("racc://session-created", serde_json::json!({
            "session_id": session_id,
            "repo_id": session["repo_id"],
            "branch": session["branch"],
            "worktree_path": cwd,
            "agent": session["agent"],
            "source": "remote",
            "reattach": true,
        }));

        Ok(serde_json::json!({ "session": session }))
    }

    // ---- Query handlers ----

    pub async fn list_repos(app_handle: &AppHandle) -> Result<serde_json::Value, String> {
        with_db(app_handle, |conn| {
            let mut stmt = conn
                .prepare("SELECT id, path, name, added_at FROM repos ORDER BY added_at DESC")
                .map_err(|e| e.to_string())?;
            let repos: Vec<serde_json::Value> = stmt
                .query_map([], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, i64>(0)?,
                        "path": row.get::<_, String>(1)?,
                        "name": row.get::<_, String>(2)?,
                        "added_at": row.get::<_, String>(3)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();
            Ok(serde_json::json!({ "repos": repos }))
        }).await
    }

    pub async fn get_session_diff(
        app_handle: &AppHandle,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let session_id = params["session_id"].as_i64().ok_or("Missing session_id")?;

        // Look up session's worktree path, fall back to repo path
        let diff_path = with_db(app_handle, move |conn| {
            let wt_path: Option<String> = conn
                .query_row(
                    "SELECT worktree_path FROM sessions WHERE id = ?1",
                    rusqlite::params![session_id],
                    |row| row.get(0),
                )
                .map_err(|e| format!("Session not found: {}", e))?;

            match wt_path {
                Some(p) => Ok(p),
                None => {
                    // Fall back to repo path
                    let repo_path: String = conn
                        .query_row(
                            "SELECT r.path FROM repos r JOIN sessions s ON s.repo_id = r.id WHERE s.id = ?1",
                            rusqlite::params![session_id],
                            |row| row.get(0),
                        )
                        .map_err(|e| format!("Repo not found: {}", e))?;
                    Ok(repo_path)
                }
            }
        }).await?;

        // Run git diff (blocking I/O)
        let diff = tokio::task::spawn_blocking(move || {
            let output = std::process::Command::new("git")
                .args(["diff", "HEAD"])
                .current_dir(&diff_path)
                .output()
                .map_err(|e| format!("Failed to get diff: {}", e))?;
            Ok::<String, String>(String::from_utf8_lossy(&output.stdout).to_string())
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))??;

        Ok(serde_json::json!({ "diff": diff }))
    }
}
```

- [ ] **Step 2: Add `mod ws_server` to `lib.rs` and spawn server in setup**

In `src-tauri/src/lib.rs`:

1. Add `mod ws_server;` at the top with the other mod declarations.

2. In the `.setup(|app| { ... })` closure, add at the end (before `Ok(())`):

```rust
let app_handle = app.handle().clone();
tauri::async_runtime::spawn(async move {
    ws_server::start(app_handle).await;
});
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Successful compilation

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ws_server.rs src-tauri/src/lib.rs
git commit -m "feat: add WebSocket server with task/session handlers and event broadcasting"
```

---

## Chunk 3: Emit Events from Existing Tauri Commands

### Task 5: Add event emission to session commands

**Files:**
- Modify: `src-tauri/src/commands/session.rs`

The existing Tauri commands (`create_session`, `stop_session`, `reattach_session`) need to emit `RaccEvent` so that WebSocket clients are notified when the UI triggers state changes.

- [ ] **Step 1: Add event emission to `create_session` (around line 290, after DB insert)**

At the end of the `create_session` function, before the `Ok(session)` return, add:

```rust
// Emit event for WS clients
if let Ok(tx) = app_handle.try_state::<crate::events::EventSender>() {
    let _ = tx.send(crate::events::RaccEvent::SessionStatusChanged {
        session_id: session.id,
        status: "Running".to_string(),
        pr_url: None,
        source: "local".to_string(),
    });
}
```

This requires adding `app_handle: tauri::AppHandle` as a parameter to `create_session`. Update the function signature:

```rust
#[tauri::command]
pub async fn create_session(
    db: tauri::State<'_, Mutex<Connection>>,
    app_handle: tauri::AppHandle,
    repo_id: i64,
    use_worktree: bool,
    branch: Option<String>,
) -> Result<Session, String> {
```

Tauri automatically injects `AppHandle` when listed as a parameter — no frontend changes needed.

- [ ] **Step 2: Add event emission to `stop_session` (around line 308)**

Same pattern — add `app_handle: tauri::AppHandle` parameter and emit after DB update:

```rust
#[tauri::command]
pub async fn stop_session(
    db: tauri::State<'_, Mutex<Connection>>,
    app_handle: tauri::AppHandle,
    session_id: i64,
) -> Result<(), String> {
    // ... existing DB update code ...

    if let Ok(tx) = app_handle.try_state::<crate::events::EventSender>() {
        let _ = tx.send(crate::events::RaccEvent::SessionStatusChanged {
            session_id,
            status: "Completed".to_string(),
            pr_url: None,
            source: "local".to_string(),
        });
    }
    Ok(())
}
```

- [ ] **Step 3: Add event emission to `reattach_session` (around line 415)**

Same pattern — add `app_handle: tauri::AppHandle` and emit after DB update:

```rust
if let Ok(tx) = app_handle.try_state::<crate::events::EventSender>() {
    let _ = tx.send(crate::events::RaccEvent::SessionStatusChanged {
        session_id: session.id,
        status: "Running".to_string(),
        pr_url: None,
        source: "local".to_string(),
    });
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Successful compilation

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/session.rs
git commit -m "feat: emit RaccEvent from session Tauri commands"
```

---

### Task 6: Add event emission to task commands

**Files:**
- Modify: `src-tauri/src/commands/task.rs`

- [ ] **Step 1: Add `app_handle: tauri::AppHandle` parameter and event emission to task commands**

For each of the following functions, add `app_handle: tauri::AppHandle` parameter and emit after DB write:

**`create_task` (line 17):** After the DB insert, emit:
```rust
if let Ok(tx) = app_handle.try_state::<crate::events::EventSender>() {
    let _ = tx.send(crate::events::RaccEvent::TaskStatusChanged {
        task_id: task.id,
        status: "open".to_string(),
        session_id: None,
    });
}
```

**`update_task_status` (line 83):** After the DB update, emit:
```rust
if let Ok(tx) = app_handle.try_state::<crate::events::EventSender>() {
    let _ = tx.send(crate::events::RaccEvent::TaskStatusChanged {
        task_id: updated_task.id,
        status: updated_task.status.clone(),
        session_id: updated_task.session_id,
    });
}
```

**`delete_task` (line 166):** After the DB delete, emit:
```rust
if let Ok(tx) = app_handle.try_state::<crate::events::EventSender>() {
    let _ = tx.send(crate::events::RaccEvent::TaskDeleted { task_id });
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Successful compilation

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/task.rs
git commit -m "feat: emit RaccEvent from task Tauri commands"
```

---

## Chunk 4: Frontend Event Listeners

### Task 7: Add Tauri event listeners for remote session bootstrap

**Files:**
- Modify: `src/stores/sessionStore.ts:59-90` (the `initialize()` function)

- [ ] **Step 1: Add import for `listen` from Tauri event API**

At the top of `sessionStore.ts`, add:

```typescript
import { listen } from '@tauri-apps/api/event';
```

- [ ] **Step 2: Add event listeners in `initialize()`**

At the end of the `initialize()` function (before setting `loading: false`), add:

```typescript
// Listen for remotely-created sessions (from WebSocket API)
listen<{
  session_id: number;
  repo_id: number;
  branch: string | null;
  worktree_path: string;
  agent: string;
  source: string;
  reattach?: boolean;
}>('racc://session-created', async (event) => {
  const { session_id, worktree_path, source, reattach } = event.payload;
  if (source !== 'remote') return;

  // Refresh session list from DB
  const repos = await invoke<RepoWithSessions[]>("list_repos");
  set({ repos });

  // Spawn PTY for the remotely-created session
  const agentCmd = reattach
    ? 'claude --continue --dangerously-skip-permissions'
    : 'claude --dangerously-skip-permissions';
  spawnPty(session_id, worktree_path, 80, 24, agentCmd);
  startTracking(session_id);
});

// Listen for remotely-stopped sessions
listen<{
  session_id: number;
  source: string;
}>('racc://session-stopped', async (event) => {
  const { session_id, source } = event.payload;
  if (source !== 'remote') return;

  stopTracking(session_id);
  killPty(session_id);
  const repos = await invoke<RepoWithSessions[]>("list_repos");
  set({ repos });
});
```

- [ ] **Step 3: Verify the frontend builds**

Run: `bun run build`
Expected: Successful TypeScript check and Vite build

- [ ] **Step 4: Commit**

```bash
git add src/stores/sessionStore.ts
git commit -m "feat: add frontend listeners for remote session PTY bootstrap"
```

---

## Chunk 5: Graceful Shutdown

### Task 8: Add shutdown signal to WebSocket server

**Files:**
- Modify: `src-tauri/src/ws_server.rs` (accept shutdown signal)
- Modify: `src-tauri/src/lib.rs` (send shutdown on app exit)

- [ ] **Step 1: Add a `tokio::sync::watch` channel for shutdown in `ws_server::start()`**

Change the `start` function signature to accept a shutdown receiver:

```rust
pub async fn start(app_handle: AppHandle, mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
```

Replace the accept loop with a `tokio::select!`:

```rust
loop {
    tokio::select! {
        Ok((stream, peer_addr)) = listener.accept() => {
            let conn_id = NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed);
            log::info!("New WebSocket connection {} from {}", conn_id, peer_addr);
            let app = app_handle.clone();
            let pool = pool.clone();
            tokio::spawn(handle_connection(conn_id, stream, app, pool));
        }
        _ = shutdown_rx.changed() => {
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
```

- [ ] **Step 2: Create shutdown channel in `lib.rs` and wire to app exit**

In the `run()` function, before `.setup()`:

```rust
let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
```

Update the spawn call in `setup()`:

```rust
let app_handle = app.handle().clone();
tauri::async_runtime::spawn(async move {
    ws_server::start(app_handle, shutdown_rx).await;
});
```

Add a `on_window_event` handler to send shutdown signal:

```rust
.on_window_event(move |_window, event| {
    if let tauri::WindowEvent::Destroyed = event {
        let _ = shutdown_tx.send(true);
    }
})
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Successful compilation

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ws_server.rs src-tauri/src/lib.rs
git commit -m "feat: add graceful shutdown for WebSocket server"
```

---

## Chunk 6: Integration Testing

### Task 9: Manual integration test

No test framework is configured. Test manually using `websocat` or similar WebSocket client.

- [ ] **Step 1: Start the app**

Run: `bun tauri dev`
Expected: App launches, terminal shows "WebSocket server listening on ws://127.0.0.1:9399"

- [ ] **Step 2: Test WebSocket connection with websocat**

Install if needed: `brew install websocat`

```bash
websocat ws://127.0.0.1:9399
```

Expected: Connection established (no output yet, waiting for input)

- [ ] **Step 3: Test `list_repos`**

Send:
```json
{"id":"1","method":"list_repos","params":{}}
```

Expected: JSON response with `id: "1"` and `result.repos` array

- [ ] **Step 4: Test `create_task`**

Send (use a valid repo_id from the list_repos response):
```json
{"id":"2","method":"create_task","params":{"repo_id":1,"description":"Test task from WS"}}
```

Expected: JSON response with `result.task_id` + a push event `task_status_changed`

- [ ] **Step 5: Test `create_session`**

Send:
```json
{"id":"3","method":"create_session","params":{"repo_id":1,"use_worktree":true,"branch":"test/ws-api"}}
```

Expected:
1. JSON response with `result.session_id`
2. Push event `session_status_changed` with status "Running"
3. In the Racc UI: a new session appears and the agent starts running

- [ ] **Step 6: Test `stop_session`**

Send (use session_id from previous response):
```json
{"id":"4","method":"stop_session","params":{"session_id":1}}
```

Expected:
1. JSON response with empty result
2. Push event `session_status_changed` with status "Completed"
3. In the Racc UI: agent terminal is killed, session shows as completed

- [ ] **Step 7: Test error handling**

Send:
```json
{"id":"5","method":"unknown_method","params":{}}
```

Expected: JSON response with `error: "Unknown method: unknown_method"`

- [ ] **Step 8: Clean up test worktree**

```bash
git worktree remove ~/racc-worktrees/<repo>/test-ws-api --force
```
