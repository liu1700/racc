use futures_util::{SinkExt, StreamExt};
use racc_core::AppContext;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tauri::{Manager, AppHandle};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message;

// --- Types ---

type ConnId = u64;
type ConnPool = Arc<RwLock<HashMap<ConnId, mpsc::UnboundedSender<Message>>>>;

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

// --- Broadcast: fan out racc_core EventBus events to all WS clients ---

async fn broadcast_events(
    app_handle: AppHandle,
    pool: ConnPool,
) {
    let ctx = app_handle.state::<Arc<AppContext>>();
    let mut rx = ctx.event_bus.subscribe();

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

pub async fn start(app_handle: AppHandle, mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
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

                        tauri::async_runtime::spawn(async move {
                            handle_connection(conn_id, stream, pool_clone, handle_clone).await;
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
                        let response = dispatch(&app_handle, req.method.as_str(), params).await;
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

async fn dispatch(app_handle: &AppHandle, method: &str, params: Value) -> Result<Value, String> {
    let ctx = app_handle.state::<Arc<AppContext>>();

    match method {
        "create_task" => handle_create_task(&ctx, params).await,
        "list_tasks" => handle_list_tasks(&ctx, params),
        "update_task_status" => handle_update_task_status(&ctx, params).await,
        "update_task_description" => handle_update_task_description(&ctx, params),
        "delete_task" => handle_delete_task(&ctx, params).await,
        "create_session" => handle_create_session(&ctx, params).await,
        "stop_session" => handle_stop_session(&ctx, params).await,
        "reattach_session" => handle_reattach_session(&ctx, params).await,
        "list_repos" => handle_list_repos(&ctx).await,
        "get_session_diff" => handle_get_session_diff(&ctx, params).await,
        _ => Err(format!("Unknown method: {}", method)),
    }
}

// --- Task handlers ---

async fn handle_create_task(ctx: &AppContext, params: Value) -> Result<Value, String> {
    let repo_id = params["repo_id"]
        .as_i64()
        .ok_or("Missing or invalid repo_id")?;
    let description = params["description"]
        .as_str()
        .ok_or("Missing or invalid description")?
        .to_string();
    let images = params["images"].as_str().map(|s| s.to_string());

    let task = racc_core::commands::task::create_task(ctx, repo_id, description, images)
        .await
        .map_err(|e| e.to_string())?;

    Ok(serde_json::to_value(task).map_err(|e| e.to_string())?)
}

fn handle_list_tasks(ctx: &AppContext, params: Value) -> Result<Value, String> {
    let repo_id = params["repo_id"]
        .as_i64()
        .ok_or("Missing or invalid repo_id")?;

    let tasks = racc_core::commands::task::list_tasks(ctx, repo_id)
        .map_err(|e| e.to_string())?;

    Ok(json!({ "tasks": tasks }))
}

async fn handle_update_task_status(ctx: &AppContext, params: Value) -> Result<Value, String> {
    let task_id = params["task_id"]
        .as_i64()
        .ok_or("Missing or invalid task_id")?;
    let status = params["status"]
        .as_str()
        .ok_or("Missing or invalid status")?
        .to_string();
    let session_id = params["session_id"].as_i64();

    let task = racc_core::commands::task::update_task_status(ctx, task_id, status, session_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(serde_json::to_value(task).map_err(|e| e.to_string())?)
}

fn handle_update_task_description(ctx: &AppContext, params: Value) -> Result<Value, String> {
    let task_id = params["task_id"]
        .as_i64()
        .ok_or("Missing or invalid task_id")?;
    let description = params["description"]
        .as_str()
        .ok_or("Missing or invalid description")?
        .to_string();

    let task = racc_core::commands::task::update_task_description(ctx, task_id, description)
        .map_err(|e| e.to_string())?;

    Ok(serde_json::to_value(task).map_err(|e| e.to_string())?)
}

async fn handle_delete_task(ctx: &AppContext, params: Value) -> Result<Value, String> {
    let task_id = params["task_id"]
        .as_i64()
        .ok_or("Missing or invalid task_id")?;

    racc_core::commands::task::delete_task(ctx, task_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({}))
}

// --- Session handlers ---

async fn handle_create_session(ctx: &AppContext, params: Value) -> Result<Value, String> {
    let repo_id = params["repo_id"]
        .as_i64()
        .ok_or("Missing or invalid repo_id")?;
    let use_worktree = params["use_worktree"].as_bool().unwrap_or(false);
    let branch = params["branch"].as_str().map(|s| s.to_string());
    let agent = params["agent"].as_str().map(|s| s.to_string());
    let task_description = params["task_description"].as_str().map(|s| s.to_string());
    let server_id = params["server_id"].as_str().map(|s| s.to_string());
    let skip_permissions = params["skip_permissions"].as_bool();

    let session = racc_core::commands::session::create_session(
        ctx,
        repo_id,
        use_worktree,
        branch,
        agent,
        task_description,
        server_id,
        skip_permissions,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(serde_json::to_value(&session).map_err(|e| e.to_string())?)
}

async fn handle_stop_session(ctx: &AppContext, params: Value) -> Result<Value, String> {
    let session_id = params["session_id"]
        .as_i64()
        .ok_or("Missing or invalid session_id")?;

    racc_core::commands::session::stop_session(ctx, session_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({}))
}

async fn handle_reattach_session(ctx: &AppContext, params: Value) -> Result<Value, String> {
    let session_id = params["session_id"]
        .as_i64()
        .ok_or("Missing or invalid session_id")?;

    let session = racc_core::commands::session::reattach_session(ctx, session_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({ "session": session }))
}

// --- Query handlers ---

async fn handle_list_repos(ctx: &AppContext) -> Result<Value, String> {
    let repos = racc_core::commands::session::list_repos(ctx)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({ "repos": repos }))
}

async fn handle_get_session_diff(ctx: &AppContext, params: Value) -> Result<Value, String> {
    let session_id = params["session_id"]
        .as_i64()
        .ok_or("Missing or invalid session_id")?;

    let diff = racc_core::commands::session::get_session_diff(ctx, session_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({ "diff": diff }))
}
