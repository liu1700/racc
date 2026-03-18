use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use racc_core::commands::{cost, file, git, insights, server, session, task, transport};
use racc_core::AppContext;

/// HTTP handler that upgrades to WebSocket.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(ctx): State<Arc<AppContext>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, ctx))
}

/// Main WebSocket connection handler.
async fn handle_socket(socket: WebSocket, ctx: Arc<AppContext>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Channel for sending messages back to the WebSocket client
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();

    // --- Task 1: Forward outbound messages to WebSocket sender ---
    let sender_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // --- Task 2: Forward EventBus events as JSON text frames ---
    let event_tx = out_tx.clone();
    let event_bus_rx = ctx.event_bus.subscribe();
    let event_task = tokio::spawn(async move {
        let mut rx = event_bus_rx;
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Ok(json_str) = serde_json::to_string(&event) {
                        let msg = Message::Text(json_str.into());
                        if event_tx.send(msg).is_err() {
                            break;
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // --- Task 3: Forward terminal data as binary frames ---
    // Binary frame format: 8-byte session_id (i64 LE) + PTY data
    let terminal_tx = out_tx.clone();
    let mut terminal_rx = ctx.terminal_tx.subscribe();
    let terminal_task = tokio::spawn(async move {
        loop {
            match terminal_rx.recv().await {
                Ok(terminal_data) => {
                    let mut frame = Vec::with_capacity(8 + terminal_data.data.len());
                    frame.extend_from_slice(&terminal_data.session_id.to_le_bytes());
                    frame.extend_from_slice(&terminal_data.data);
                    if terminal_tx.send(Message::Binary(frame.into())).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // --- Main loop: receive and dispatch client messages ---
    while let Some(msg_result) = ws_receiver.next().await {
        let msg = match msg_result {
            Ok(m) => m,
            Err(_) => break,
        };

        match msg {
            Message::Text(text) => {
                let parsed: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        let err_msg = json!({"error": format!("Invalid JSON: {}", e)});
                        let _ = out_tx.send(Message::Text(err_msg.to_string().into()));
                        continue;
                    }
                };

                let id = parsed.get("id").cloned().unwrap_or(Value::Null);
                let method = parsed
                    .get("method")
                    .and_then(|m| m.as_str())
                    .unwrap_or("");
                let params = parsed.get("params").cloned().unwrap_or(json!({}));

                let result = dispatch(method, params, &ctx).await;

                let response = match result {
                    Ok(value) => json!({"id": id, "result": value}),
                    Err(err_str) => json!({"id": id, "error": err_str}),
                };

                let _ = out_tx.send(Message::Text(response.to_string().into()));
            }
            Message::Binary(data) => {
                // Binary frames from client: 8-byte session_id (i64 LE) + PTY input data
                if data.len() > 8 {
                    let session_id =
                        i64::from_le_bytes(data[..8].try_into().unwrap_or([0u8; 8]));
                    let payload = data[8..].to_vec();
                    let _ = transport::transport_write(&ctx, session_id, payload).await;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Cleanup
    sender_task.abort();
    event_task.abort();
    terminal_task.abort();
}

/// Dispatch a JSON-RPC method call to the appropriate racc-core command.
async fn dispatch(
    method: &str,
    params: Value,
    ctx: &AppContext,
) -> Result<Value, String> {
    match method {
        // ── Session ──────────────────────────────────────────────
        "create_session" => {
            let repo_id = param_i64(&params, "repo_id")?;
            let use_worktree = params.get("use_worktree").and_then(|v| v.as_bool()).unwrap_or(false);
            let branch = param_opt_str(&params, "branch");
            let agent = param_opt_str(&params, "agent");
            let task_description = param_opt_str(&params, "task_description");
            let server_id = param_opt_str(&params, "server_id");
            let result = session::create_session(ctx, repo_id, use_worktree, branch, agent, task_description, server_id)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "stop_session" => {
            let session_id = param_i64(&params, "session_id")?;
            session::stop_session(ctx, session_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "reattach_session" => {
            let session_id = param_i64(&params, "session_id")?;
            let result = session::reattach_session(ctx, session_id)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "list_repos" => {
            let result = session::list_repos(ctx)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "import_repo" => {
            let path = param_str(&params, "path")?;
            let result = session::import_repo(ctx, path)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "remove_repo" => {
            let repo_id = param_i64(&params, "repo_id")?;
            session::remove_repo(ctx, repo_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "remove_session" => {
            let session_id = param_i64(&params, "session_id")?;
            let delete_worktree = params.get("delete_worktree").and_then(|v| v.as_bool()).unwrap_or(false);
            session::remove_session(ctx, session_id, delete_worktree)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "update_session_pr_url" => {
            let session_id = param_i64(&params, "session_id")?;
            let pr_url = param_str(&params, "pr_url")?;
            session::update_session_pr_url(ctx, session_id, pr_url)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "reconcile_sessions" => {
            let result = session::reconcile_sessions(ctx)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "get_session_diff" => {
            let session_id = param_i64(&params, "session_id")?;
            let result = session::get_session_diff(ctx, session_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(result))
        }

        // ── Task ─────────────────────────────────────────────────
        "create_task" => {
            let repo_id = param_i64(&params, "repo_id")?;
            let description = param_str(&params, "description")?;
            let images = param_opt_str(&params, "images");
            let result = task::create_task(ctx, repo_id, description, images)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "list_tasks" => {
            let repo_id = param_i64(&params, "repo_id")?;
            let result = task::list_tasks(ctx, repo_id)
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "update_task_status" => {
            let task_id = param_i64(&params, "task_id")?;
            let status = param_str(&params, "status")?;
            let session_id = param_opt_i64(&params, "session_id");
            let result = task::update_task_status(ctx, task_id, status, session_id)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "update_task_description" => {
            let task_id = param_i64(&params, "task_id")?;
            let description = param_str(&params, "description")?;
            let result = task::update_task_description(ctx, task_id, description)
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "delete_task" => {
            let task_id = param_i64(&params, "task_id")?;
            task::delete_task(ctx, task_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "save_task_image" => {
            let repo_path = param_str(&params, "repo_path")?;
            let filename = param_str(&params, "filename")?;
            let data_b64 = param_str(&params, "data")?;
            // Expect base64-encoded data from the client
            let data = base64_decode(&data_b64)?;
            let result = task::save_task_image(repo_path, filename, data)
                .map_err(|e| e.to_string())?;
            Ok(json!(result))
        }
        "delete_task_image" => {
            let repo_path = param_str(&params, "repo_path")?;
            let filename = param_str(&params, "filename")?;
            task::delete_task_image(repo_path, filename)
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "rename_task_image" => {
            let repo_path = param_str(&params, "repo_path")?;
            let old_name = param_str(&params, "old_name")?;
            let new_name = param_str(&params, "new_name")?;
            task::rename_task_image(repo_path, old_name, new_name)
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "update_task_images" => {
            let task_id = param_i64(&params, "task_id")?;
            let images = param_str(&params, "images")?;
            let result = task::update_task_images(ctx, task_id, images)
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "copy_file_to_task_images" => {
            let repo_path = param_str(&params, "repo_path")?;
            let source_path = param_str(&params, "source_path")?;
            let filename = param_str(&params, "filename")?;
            let result = task::copy_file_to_task_images(repo_path, source_path, filename)
                .map_err(|e| e.to_string())?;
            Ok(json!(result))
        }

        // ── Transport ────────────────────────────────────────────
        "transport_write" => {
            let session_id = param_i64(&params, "session_id")?;
            let data_b64 = param_str(&params, "data")?;
            let data = base64_decode(&data_b64)?;
            transport::transport_write(ctx, session_id, data)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "transport_resize" => {
            let session_id = param_i64(&params, "session_id")?;
            let cols = param_u16(&params, "cols")?;
            let rows = param_u16(&params, "rows")?;
            transport::transport_resize(ctx, session_id, cols, rows)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "transport_get_buffer" => {
            let session_id = param_i64(&params, "session_id")?;
            let result = transport::transport_get_buffer(ctx, session_id)
                .await
                .map_err(|e| e.to_string())?;
            // Return buffer as base64-encoded string
            Ok(json!(base64_encode(&result)))
        }
        "transport_is_alive" => {
            let session_id = param_i64(&params, "session_id")?;
            let alive = transport::transport_is_alive(ctx, session_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(alive))
        }

        // ── Server (SSH) ─────────────────────────────────────────
        "list_servers" => {
            let result = server::list_servers(ctx)
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "add_server" => {
            let config: server::ServerConfig = serde_json::from_value(params)
                .map_err(|e| format!("Invalid server config: {}", e))?;
            let result = server::add_server(ctx, config)
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "update_server" => {
            let server_id = param_str(&params, "server_id")?;
            let config: server::ServerConfig = serde_json::from_value(
                params.get("config").cloned().unwrap_or(json!({}))
            ).map_err(|e| format!("Invalid server config: {}", e))?;
            let result = server::update_server(ctx, server_id, config)
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "remove_server" => {
            let server_id = param_str(&params, "server_id")?;
            server::remove_server(ctx, server_id)
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "connect_server" => {
            let server_id = param_str(&params, "server_id")?;
            server::connect_server(ctx, server_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "disconnect_server" => {
            let server_id = param_str(&params, "server_id")?;
            server::disconnect_server(ctx, server_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "test_connection" => {
            let server_id = param_str(&params, "server_id")?;
            let result = server::test_connection(ctx, server_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(result))
        }
        "list_ssh_config_hosts" => {
            let result = server::list_ssh_config_hosts()
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "execute_remote_command" => {
            let server_id = param_str(&params, "server_id")?;
            let command = param_str(&params, "command")?;
            let result = server::execute_remote_command(ctx, server_id, command)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }

        // ── Cost ─────────────────────────────────────────────────
        "get_project_costs" => {
            let worktree_path = param_str(&params, "worktree_path")?;
            let result = cost::get_project_costs(worktree_path)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "get_global_costs" => {
            let result = cost::get_global_costs()
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }

        // ── Insights ─────────────────────────────────────────────
        "record_session_events" => {
            let events: Vec<insights::SessionEvent> = serde_json::from_value(
                params.get("events").cloned().unwrap_or(json!([]))
            ).map_err(|e| format!("Invalid events: {}", e))?;
            insights::record_session_events(ctx, events)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "get_insights" => {
            let status = param_opt_str(&params, "status");
            let result = insights::get_insights(ctx, status)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "update_insight_status" => {
            let id = param_i64(&params, "id")?;
            let status = param_str(&params, "status")?;
            insights::update_insight_status(ctx, id, status)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "run_batch_analysis" => {
            let result = insights::run_batch_analysis(ctx)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "save_insight" => {
            let insight_type = param_str(&params, "insight_type")?;
            let severity = param_str(&params, "severity")?;
            let title = param_str(&params, "title")?;
            let summary = param_str(&params, "summary")?;
            let detail_json = param_str(&params, "detail_json")?;
            let fingerprint = param_str(&params, "fingerprint")?;
            let result = insights::save_insight(ctx, insight_type, severity, title, summary, detail_json, fingerprint)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(result))
        }

        // ── File ─────────────────────────────────────────────────
        "read_file" => {
            let session_id = param_opt_i64(&params, "session_id");
            let repo_id = param_opt_i64(&params, "repo_id");
            let file_path = param_str(&params, "file_path")?;
            let max_lines = params.get("max_lines").and_then(|v| v.as_u64()).map(|v| v as usize);
            let result = file::read_file(ctx, session_id, repo_id, file_path, max_lines)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "search_files" => {
            let session_id = param_opt_i64(&params, "session_id");
            let repo_id = param_opt_i64(&params, "repo_id");
            let query = param_str(&params, "query")?;
            let result = file::search_files(ctx, session_id, repo_id, query)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }

        // ── Git ──────────────────────────────────────────────────
        "create_worktree" => {
            let path = param_str(&params, "path")?;
            let branch = param_str(&params, "branch")?;
            let result = git::create_worktree(path, branch)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(result))
        }
        "delete_worktree" => {
            let path = param_str(&params, "path")?;
            git::delete_worktree(path)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "get_diff" => {
            let worktree_path = param_str(&params, "worktree_path")?;
            let result = git::get_diff(worktree_path)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(result))
        }

        // ── Other ────────────────────────────────────────────────
        "reset_db" => {
            let conn = ctx.db.lock().map_err(|e| e.to_string())?;
            racc_core::db::reset_db(&conn).map_err(|e| e.to_string())?;
            Ok(json!(null))
        }
        "sync" => {
            let result = session::list_repos(ctx)
                .await
                .map_err(|e| e.to_string())?;
            to_json(&result)
        }
        "open_url" => {
            // No-op in headless mode
            Ok(json!(null))
        }

        _ => Err(format!("Unknown method: {}", method)),
    }
}

// ── Param extraction helpers ─────────────────────────────────────

fn param_str(params: &Value, key: &str) -> Result<String, String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Missing required string parameter: {}", key))
}

fn param_opt_str(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn param_i64(params: &Value, key: &str) -> Result<i64, String> {
    params
        .get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| format!("Missing required integer parameter: {}", key))
}

fn param_opt_i64(params: &Value, key: &str) -> Option<i64> {
    params.get(key).and_then(|v| v.as_i64())
}

fn param_u16(params: &Value, key: &str) -> Result<u16, String> {
    params
        .get(key)
        .and_then(|v| v.as_u64())
        .map(|v| v as u16)
        .ok_or_else(|| format!("Missing required u16 parameter: {}", key))
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| format!("Serialization error: {}", e))
}

// Simple base64 encode/decode without pulling in another crate
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    fn decode_char(c: u8) -> Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            b'=' => Ok(0),
            _ => Err(format!("Invalid base64 character: {}", c as char)),
        }
    }

    let input = input.as_bytes();
    let mut result = Vec::with_capacity(input.len() * 3 / 4);
    for chunk in input.chunks(4) {
        if chunk.len() < 4 {
            return Err("Invalid base64 length".to_string());
        }
        let b0 = decode_char(chunk[0])?;
        let b1 = decode_char(chunk[1])?;
        let b2 = decode_char(chunk[2])?;
        let b3 = decode_char(chunk[3])?;
        let n = ((b0 as u32) << 18) | ((b1 as u32) << 12) | ((b2 as u32) << 6) | (b3 as u32);
        result.push((n >> 16) as u8);
        if chunk[2] != b'=' {
            result.push((n >> 8) as u8);
        }
        if chunk[3] != b'=' {
            result.push(n as u8);
        }
    }
    Ok(result)
}
