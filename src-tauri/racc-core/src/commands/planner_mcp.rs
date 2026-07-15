use axum::{
    extract::{DefaultBodyLimit, State},
    http::{
        header::{AUTHORIZATION, ORIGIN},
        HeaderMap, StatusCode,
    },
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

use super::planner::{
    emit_task_plan_changed, store_plan_result_db, validate_task_plan_result, TaskPlanResult,
};
use crate::{AppContext, CoreError};

pub(super) const MCP_SERVER_NAME: &str = "racc_task_plan";
pub(super) const MCP_TOOL_NAME: &str = "submit_task_plan";
pub(super) const MCP_TOKEN_ENV: &str = "RACC_TASK_PLAN_TOKEN";

struct PlannerMcpState {
    ctx: AppContext,
    run_id: i64,
    repo_id: i64,
    bearer_token: String,
    submitted_tx: Mutex<Option<oneshot::Sender<()>>>,
}

/// A run-scoped, loopback-only MCP endpoint. Dropping the runtime shuts the
/// HTTP server down; successful submission is delivered separately so the
/// planner can stop its agent session without inspecting terminal output.
pub(super) struct PlannerMcpRuntime {
    pub url: String,
    pub bearer_token: String,
    pub submitted_rx: oneshot::Receiver<()>,
    _shutdown_tx: oneshot::Sender<()>,
}

impl PlannerMcpRuntime {
    pub async fn start(ctx: AppContext, run_id: i64, repo_id: i64) -> Result<Self, CoreError> {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|error| {
                CoreError::Other(format!("Could not start task planner MCP server: {error}"))
            })?;
        let address = listener.local_addr().map_err(|error| {
            CoreError::Other(format!("Could not read task planner MCP address: {error}"))
        })?;
        let bearer_token = uuid::Uuid::new_v4().to_string();
        let (submitted_tx, submitted_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let state = Arc::new(PlannerMcpState {
            ctx,
            run_id,
            repo_id,
            bearer_token: bearer_token.clone(),
            submitted_tx: Mutex::new(Some(submitted_tx)),
        });
        let app = Router::new()
            .route("/mcp", post(handle_mcp_post))
            .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
            .with_state(state);

        tokio::spawn(async move {
            let server = axum::serve(listener, app).with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            });
            if let Err(error) = server.await {
                log::error!("Task planner MCP server stopped unexpectedly: {error}");
            }
        });

        Ok(Self {
            url: format!("http://{address}/mcp"),
            bearer_token,
            submitted_rx,
            _shutdown_tx: shutdown_tx,
        })
    }
}

async fn handle_mcp_post(
    State(state): State<Arc<PlannerMcpState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    if let Some(origin) = headers.get(ORIGIN).and_then(|value| value.to_str().ok()) {
        let local_origin = origin.starts_with("http://127.0.0.1:")
            || origin.starts_with("http://localhost:")
            || origin.starts_with("https://127.0.0.1:")
            || origin.starts_with("https://localhost:");
        if !local_origin {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "Task planner MCP only accepts local origins"})),
            )
                .into_response();
        }
    }

    let expected = format!("Bearer {}", state.bearer_token);
    let authorized = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected);
    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid task planner capability token"})),
        )
            .into_response();
    }

    if let Some(requests) = payload.as_array() {
        let mut responses = Vec::new();
        for request in requests {
            if let Some(response) = dispatch_request(&state, request).await {
                responses.push(response);
            }
        }
        if responses.is_empty() {
            StatusCode::ACCEPTED.into_response()
        } else {
            Json(Value::Array(responses)).into_response()
        }
    } else if let Some(response) = dispatch_request(&state, &payload).await {
        Json(response).into_response()
    } else {
        StatusCode::ACCEPTED.into_response()
    }
}

async fn dispatch_request(state: &Arc<PlannerMcpState>, request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str)?;

    // MCP notifications deliberately receive an empty 202 response.
    let Some(id) = id else {
        return None;
    };

    let result = match method {
        "initialize" => {
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {
                    "name": "racc-task-planner",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({"tools": [task_plan_tool_definition()]}),
        "tools/call" => {
            let tool_name = request
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if tool_name != MCP_TOOL_NAME {
                return Some(tool_error_response(
                    id,
                    format!("Unknown planner tool: {tool_name}"),
                ));
            }

            let arguments = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let plan: TaskPlanResult = match serde_json::from_value(arguments) {
                Ok(plan) => plan,
                Err(error) => {
                    return Some(tool_error_response(
                        id,
                        format!("Invalid submit_task_plan arguments: {error}"),
                    ));
                }
            };
            if let Err(error) = validate_task_plan_result(state.run_id, &plan) {
                return Some(tool_error_response(id, error));
            }
            if let Err(error) = store_plan_result_db(&state.ctx.db, &plan) {
                return Some(tool_error_response(id, error.to_string()));
            }

            emit_task_plan_changed(&state.ctx.event_bus, state.repo_id, state.run_id).await;
            if let Ok(mut sender) = state.submitted_tx.lock() {
                if let Some(sender) = sender.take() {
                    let _ = sender.send(());
                }
            }

            return Some(json_rpc_result(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Task plan accepted. It is now available in Racc for user review."
                    }],
                    "structuredContent": {
                        "accepted": true,
                        "run_id": state.run_id,
                        "task_count": plan.tasks.len()
                    }
                }),
            ));
        }
        _ => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("Method not found: {method}")}
            }));
        }
    };

    Some(json_rpc_result(id, result))
}

fn json_rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn tool_error_response(id: Value, message: String) -> Value {
    json_rpc_result(
        id,
        json!({
            "content": [{"type": "text", "text": message}],
            "isError": true
        }),
    )
}

fn task_plan_tool_definition() -> Value {
    json!({
        "name": MCP_TOOL_NAME,
        "title": "Submit a Racc task plan",
        "description": "Submit the completed plan for the current Racc planner run. Racc validates it and stages it for user review; this does not create tasks.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["run_id", "summary", "tasks"],
            "properties": {
                "run_id": {"type": "integer", "description": "The exact planner run id from the prompt."},
                "summary": {"type": "string", "minLength": 1, "maxLength": 20000, "description": "A concise planning summary or access error."},
                "tasks": {
                    "type": "array",
                    "maxItems": 50,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["key", "title", "description", "acceptance_criteria", "depends_on"],
                        "properties": {
                            "key": {"type": "string", "minLength": 1, "maxLength": 64},
                            "title": {"type": "string", "minLength": 1, "maxLength": 200},
                            "description": {"type": "string", "minLength": 1, "maxLength": 20000},
                            "acceptance_criteria": {
                                "type": "array",
                                "maxItems": 30,
                                "items": {"type": "string", "maxLength": 2000}
                            },
                            "depends_on": {
                                "type": "array",
                                "maxItems": 50,
                                "items": {"type": "string", "maxLength": 64}
                            }
                        }
                    }
                }
            }
        },
        "outputSchema": {
            "type": "object",
            "required": ["accepted", "run_id", "task_count"],
            "properties": {
                "accepted": {"type": "boolean"},
                "run_id": {"type": "integer"},
                "task_count": {"type": "integer"}
            }
        },
        "annotations": {
            "readOnlyHint": false,
            "destructiveHint": false,
            "idempotentHint": false,
            "openWorldHint": false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::BroadcastEventBus;
    use crate::ssh::SshManager;
    use crate::transport::manager::TransportManager;

    fn test_context() -> (AppContext, std::path::PathBuf, i64, i64) {
        let path =
            std::env::temp_dir().join(format!("racc-planner-mcp-{}.db", uuid::Uuid::new_v4()));
        let conn = crate::db::init_db(path.clone()).expect("database should initialize");
        conn.execute(
            "INSERT INTO repos (path, name) VALUES ('/tmp/widgets', 'widgets')",
            [],
        )
        .unwrap();
        let repo_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO task_plan_runs (repo_id, agent, source_input, prompt, status)
             VALUES (?1, 'codex', 'epic', 'prompt', 'planning')",
            [repo_id],
        )
        .unwrap();
        let run_id = conn.last_insert_rowid();
        let (terminal_tx, _) = tokio::sync::broadcast::channel(64);
        let ctx = AppContext::new(
            Arc::new(Mutex::new(conn)),
            TransportManager::new(),
            Arc::new(SshManager::new()),
            Arc::new(BroadcastEventBus::new()),
            terminal_tx,
        );
        (ctx, path, repo_id, run_id)
    }

    async fn post_rpc(
        runtime: &PlannerMcpRuntime,
        authorization: Option<String>,
        payload: Value,
    ) -> reqwest::Response {
        let client = reqwest::Client::new();
        let mut request = client
            .post(&runtime.url)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(payload.to_string());
        if let Some(authorization) = authorization {
            request = request.header("authorization", authorization);
        }
        request.send().await.expect("MCP request should complete")
    }

    #[test]
    fn planner_tool_exposes_a_closed_structured_schema() {
        let tool = task_plan_tool_definition();
        assert_eq!(tool["name"], MCP_TOOL_NAME);
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert_eq!(tool["inputSchema"]["properties"]["tasks"]["maxItems"], 50);
    }

    #[tokio::test]
    async fn mcp_submission_requires_the_capability_and_stores_the_preview() {
        let (ctx, path, repo_id, run_id) = test_context();
        let mut runtime = PlannerMcpRuntime::start(ctx.clone(), run_id, repo_id)
            .await
            .expect("MCP server should start");

        let unauthorized = post_rpc(
            &runtime,
            None,
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
        )
        .await;
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let authorization = format!("Bearer {}", runtime.bearer_token);
        let initialized = post_rpc(
            &runtime,
            Some(authorization.clone()),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "initialize",
                "params": {"protocolVersion": "2025-06-18"}
            }),
        )
        .await;
        assert_eq!(initialized.status(), StatusCode::OK);
        let initialized: Value = serde_json::from_str(&initialized.text().await.unwrap()).unwrap();
        assert_eq!(
            initialized["result"]["capabilities"]["tools"]["listChanged"],
            false
        );

        let rejected = post_rpc(
            &runtime,
            Some(authorization.clone()),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": MCP_TOOL_NAME,
                    "arguments": {"run_id": run_id + 1, "summary": "wrong run", "tasks": []}
                }
            }),
        )
        .await;
        let rejected: Value = serde_json::from_str(&rejected.text().await.unwrap()).unwrap();
        assert_eq!(rejected["result"]["isError"], true);

        let accepted = post_rpc(
            &runtime,
            Some(authorization),
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": MCP_TOOL_NAME,
                    "arguments": {
                        "run_id": run_id,
                        "summary": "Split the epic into one task",
                        "tasks": [{
                            "key": "T1",
                            "title": "Implement the feature",
                            "description": "Build the requested behavior.",
                            "acceptance_criteria": ["The behavior is observable"],
                            "depends_on": []
                        }]
                    }
                }
            }),
        )
        .await;
        let accepted: Value = serde_json::from_str(&accepted.text().await.unwrap()).unwrap();
        assert_eq!(accepted["result"]["structuredContent"]["accepted"], true);
        tokio::time::timeout(std::time::Duration::from_secs(2), &mut runtime.submitted_rx)
            .await
            .expect("submission signal should arrive")
            .expect("submission sender should remain alive");

        let (status, result_json): (String, Option<String>) = ctx
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT status, result_json FROM task_plan_runs WHERE id = ?1",
                [run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "ready");
        assert!(result_json.unwrap().contains("Implement the feature"));

        drop(runtime);
        drop(ctx);
        let _ = std::fs::remove_file(path);
    }
}
