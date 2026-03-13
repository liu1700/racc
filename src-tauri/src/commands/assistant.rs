use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::Stdio;
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, BufReader as TokioBufReader};
use tokio::process::{Child, ChildStdout, Command as TokioCommand};
use tauri::Manager;

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

// --- Sidecar State ---

pub struct SidecarState {
    pub child: Option<Child>,
    pub stdin: Option<std::process::ChildStdin>,
    pub reader: Option<TokioBufReader<ChildStdout>>,
}

impl SidecarState {
    pub fn new() -> Self {
        Self { child: None, stdin: None, reader: None }
    }
}

fn resolve_sidecar_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    // Determine platform triple suffix
    let suffix = if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        "x86_64-apple-darwin"
    } else if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else {
        return Err("Unsupported platform".to_string());
    };

    let binary_name = format!("racc-assistant-{suffix}");

    // Production: check Tauri resource dir
    if let Ok(resource_dir) = app.path().resource_dir() {
        let path = resource_dir.join("binaries").join(&binary_name);
        if path.exists() {
            return Ok(path);
        }
    }

    // Development: check src-tauri/binaries (Tauri sets CWD to src-tauri during dev)
    let dev_path = std::path::PathBuf::from("binaries").join(&binary_name);
    if dev_path.exists() {
        return Ok(dev_path);
    }

    // Development fallback: check from project root
    let project_path = std::path::PathBuf::from("src-tauri/binaries").join(&binary_name);
    if project_path.exists() {
        return Ok(project_path);
    }

    Err(format!(
        "Sidecar binary '{binary_name}' not found. Run sidecar/build.sh first."
    ))
}

fn spawn_sidecar(app: &tauri::AppHandle) -> Result<(Child, std::process::ChildStdin, TokioBufReader<ChildStdout>), String> {
    let path = resolve_sidecar_path(app)?;

    let mut child = TokioCommand::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn sidecar: {e}"))?;

    // Take ownership of stdin and stdout separately to avoid borrow conflicts
    let stdout = child.stdout.take()
        .ok_or("Failed to capture sidecar stdout")?;
    let stdin = child.stdin.take()
        .ok_or("Failed to capture sidecar stdin")?;

    // Convert tokio ChildStdin to std ChildStdin for synchronous writes
    let std_stdin = stdin.into_std().map_err(|e| format!("Failed to convert stdin: {e}"))?;
    let reader = TokioBufReader::new(stdout);

    Ok((child, std_stdin, reader))
}

fn write_to_stdin(stdin: &mut std::process::ChildStdin, msg: &str) -> Result<(), String> {
    writeln!(stdin, "{}", msg).map_err(|e| format!("Failed to write to sidecar: {e}"))?;
    stdin.flush().map_err(|e| format!("Failed to flush sidecar stdin: {e}"))?;
    Ok(())
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

    let output = TokioCommand::new("git")
        .args(["diff", "HEAD"])
        .current_dir(&path)
        .output()
        .await
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

// --- Sidecar Tauri Commands ---

#[tauri::command]
pub async fn assistant_send_message(
    app: tauri::AppHandle,
    sidecar: tauri::State<'_, tokio::sync::Mutex<SidecarState>>,
    db: tauri::State<'_, Mutex<Connection>>,
    content: String,
) -> Result<(), String> {
    let mut sidecar_state = sidecar.lock().await;

    // Lazy spawn
    if sidecar_state.child.is_none() {
        let (child, mut stdin, reader) = spawn_sidecar(&app)?;

        // Send config if available (read DB before holding sidecar lock long)
        let config = {
            let conn = db.lock().map_err(|e| e.to_string())?;
            let get_val = |key: &str| -> Option<String> {
                conn.query_row(
                    "SELECT value FROM assistant_config WHERE key = ?1",
                    [key],
                    |row| row.get(0),
                )
                .ok()
            };
            (get_val("provider"), get_val("api_key"), get_val("model"))
        };

        if let (Some(provider), Some(api_key), Some(model)) = config {
            let config_msg = serde_json::json!({
                "type": "set_config",
                "provider": provider,
                "api_key": api_key,
                "model": model
            });
            write_to_stdin(&mut stdin, &config_msg.to_string())?;
        }

        // Send history (exclude the most recent message — it's the user message
        // that was just persisted and will be sent separately as user_message)
        let history = {
            let conn = db.lock().map_err(|e| e.to_string())?;
            let mut stmt = conn
                .prepare(
                    "SELECT role, content, tool_name, tool_call_id FROM assistant_messages ORDER BY id DESC LIMIT 51",
                )
                .map_err(|e| e.to_string())?;

            let msgs: Vec<serde_json::Value> = stmt
                .query_map([], |row| {
                    Ok(serde_json::json!({
                        "role": row.get::<_, String>(0)?,
                        "content": row.get::<_, String>(1)?,
                        "tool_name": row.get::<_, Option<String>>(2)?,
                        "tool_call_id": row.get::<_, Option<String>>(3)?
                    }))
                })
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();

            // msgs is in DESC order; the first entry is the just-saved user message — skip it
            let mut msgs: Vec<serde_json::Value> = msgs.into_iter().skip(1).collect();
            msgs.reverse();
            msgs
        };

        let history_msg = serde_json::json!({
            "type": "history",
            "messages": history
        });
        write_to_stdin(&mut stdin, &history_msg.to_string())?;

        sidecar_state.child = Some(child);
        sidecar_state.stdin = Some(stdin);
        sidecar_state.reader = Some(reader);
    }

    // Send user message
    let msg = serde_json::json!({
        "type": "user_message",
        "content": content
    });

    if let Some(stdin) = sidecar_state.stdin.as_mut() {
        write_to_stdin(stdin, &msg.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn assistant_read_response(
    sidecar: tauri::State<'_, tokio::sync::Mutex<SidecarState>>,
    db: tauri::State<'_, Mutex<Connection>>,
) -> Result<String, String> {
    let mut sidecar_state = sidecar.lock().await;

    let reader = sidecar_state.reader.as_mut()
        .ok_or("Sidecar not running")?;

    // Read one line asynchronously (does not block the tokio runtime)
    let mut line = String::new();
    reader.read_line(&mut line).await
        .map_err(|e| format!("Failed to read from sidecar: {e}"))?;

    if line.is_empty() {
        return Err("Sidecar process exited".to_string());
    }

    // Handle tool calls in a loop — LLM may issue multiple sequential tool calls
    loop {
        let parsed = match serde_json::from_str::<serde_json::Value>(line.trim()) {
            Ok(v) => v,
            Err(_) => return Ok(line.trim().to_string()),
        };

        if parsed.get("type").and_then(|t| t.as_str()) != Some("tool_call") {
            return Ok(line.trim().to_string());
        }

        // It's a tool call — resolve it
        let tool_name = parsed["name"].as_str().unwrap_or("");
        let tool_id = parsed["id"].as_str().unwrap_or("");
        let args = &parsed["args"];

        let result = match tool_name {
            "get_all_sessions" => {
                // Reuse the existing get_all_sessions_for_assistant command logic
                let conn = db.lock().map_err(|e| e.to_string())?;
                let mut stmt = conn
                    .prepare(
                        "SELECT s.id, s.status, s.agent, s.branch, r.name, r.path, s.worktree_path, s.created_at
                         FROM sessions s JOIN repos r ON s.repo_id = r.id ORDER BY s.created_at DESC",
                    )
                    .map_err(|e| e.to_string())?;

                let now = chrono::Utc::now();
                let sessions: Vec<serde_json::Value> = stmt
                    .query_map([], |row| {
                        let created_at: String = row.get(7)?;
                        let elapsed = chrono::NaiveDateTime::parse_from_str(&created_at, "%Y-%m-%d %H:%M:%S")
                            .map(|ndt| (now - ndt.and_utc()).num_minutes())
                            .unwrap_or(0);
                        Ok(serde_json::json!({
                            "id": row.get::<_, i64>(0)?,
                            "status": row.get::<_, String>(1)?,
                            "agent": row.get::<_, String>(2)?,
                            "branch": row.get::<_, Option<String>>(3)?,
                            "repo_name": row.get::<_, String>(4)?,
                            "repo_path": row.get::<_, String>(5)?,
                            "worktree_path": row.get::<_, Option<String>>(6)?,
                            "elapsed_minutes": elapsed,
                            "created_at": created_at
                        }))
                    })
                    .map_err(|e| e.to_string())?
                    .filter_map(|r| r.ok())
                    .collect();

                serde_json::to_string(&sessions).unwrap_or_default()
            }
            "get_session_diff" => {
                let session_id = args["session_id"].as_i64().unwrap_or(0);
                let path = {
                    let conn = db.lock().map_err(|e| e.to_string())?;
                    resolve_session_path(&conn, session_id)?
                };
                // Use tokio::process for non-blocking git diff
                let output = tokio::process::Command::new("git")
                    .args(["diff", "HEAD"])
                    .current_dir(&path)
                    .output()
                    .await
                    .map_err(|e| format!("Failed to get diff: {e}"))?;
                String::from_utf8_lossy(&output.stdout).to_string()
            }
            "get_session_costs" => {
                let session_id = args["session_id"].as_i64().unwrap_or(0);
                let path = {
                    let conn = db.lock().map_err(|e| e.to_string())?;
                    resolve_session_path(&conn, session_id)?
                };
                let costs = crate::commands::cost::get_project_costs(path).await
                    .unwrap_or_default();
                serde_json::to_string(&costs).unwrap_or_default()
            }
            _ => "Unknown tool".to_string(),
        };

        // Send tool result back to sidecar
        let tool_result = serde_json::json!({
            "type": "tool_result",
            "call_id": tool_id,
            "content": result
        });
        if let Some(stdin) = sidecar_state.stdin.as_mut() {
            write_to_stdin(stdin, &tool_result.to_string())?;
        }

        // Read the next line — may be another tool call or a chunk/done/error
        line.clear();
        reader.read_line(&mut line).await
            .map_err(|e| format!("Failed to read from sidecar: {e}"))?;

        if line.is_empty() {
            return Err("Sidecar process exited".to_string());
        }

        // Loop continues to check if this is another tool_call
    }
}

#[tauri::command]
pub async fn assistant_shutdown(
    sidecar: tauri::State<'_, tokio::sync::Mutex<SidecarState>>,
) -> Result<(), String> {
    let mut state = sidecar.lock().await;
    if let Some(mut stdin) = state.stdin.take() {
        let shutdown_msg = serde_json::json!({"type": "shutdown"});
        write_to_stdin(&mut stdin, &shutdown_msg.to_string()).ok();
    }
    if let Some(mut child) = state.child.take() {
        child.kill().await.ok();
    }
    state.reader = None;
    Ok(())
}
