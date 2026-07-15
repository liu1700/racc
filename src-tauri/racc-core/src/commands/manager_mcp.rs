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
use serde_json::{json, Map, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

use super::merge::{apply_ship_result_db, emit_merge_changed, validate_ship_result, ShipResult};
use super::session::SessionLaunchOptions;
use super::test_manager::{
    apply_test_result_db, emit_test_changed, validate_test_result, TestResult,
};
use crate::{AppContext, CoreError};

pub(super) const MERGE_MCP_SERVER_NAME: &str = "racc_merge_manager";
pub(super) const MERGE_MCP_TOOL_NAME: &str = "submit_merge_result";
const MERGE_MCP_TOKEN_ENV: &str = "RACC_MERGE_MANAGER_TOKEN";
pub(super) const TEST_MCP_SERVER_NAME: &str = "racc_test_manager";
pub(super) const TEST_MCP_TOOL_NAME: &str = "submit_test_result";
const TEST_MCP_TOKEN_ENV: &str = "RACC_TEST_MANAGER_TOKEN";

#[derive(Clone)]
enum ManagerKind {
    Merge { allowed_urls: Vec<String> },
    Test,
}

impl ManagerKind {
    fn server_name(&self) -> &'static str {
        match self {
            Self::Merge { .. } => MERGE_MCP_SERVER_NAME,
            Self::Test => TEST_MCP_SERVER_NAME,
        }
    }

    fn tool_name(&self) -> &'static str {
        match self {
            Self::Merge { .. } => MERGE_MCP_TOOL_NAME,
            Self::Test => TEST_MCP_TOOL_NAME,
        }
    }

    fn token_env(&self) -> &'static str {
        match self {
            Self::Merge { .. } => MERGE_MCP_TOKEN_ENV,
            Self::Test => TEST_MCP_TOKEN_ENV,
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            Self::Merge { .. } => "Merge Manager",
            Self::Test => "Test Manager",
        }
    }
}

struct ManagerMcpState {
    ctx: AppContext,
    run_id: i64,
    repo_id: i64,
    bearer_token: String,
    kind: ManagerKind,
    submitted_tx: Mutex<Option<oneshot::Sender<()>>>,
}

/// A capability-scoped loopback MCP endpoint for exactly one manager run.
/// The endpoint owns the only supported result-submission path; terminal text
/// is never parsed as structured manager state.
pub(super) struct ManagerMcpRuntime {
    pub url: String,
    pub bearer_token: String,
    kind: ManagerKind,
    submitted_rx: oneshot::Receiver<()>,
    _shutdown_tx: oneshot::Sender<()>,
}

impl ManagerMcpRuntime {
    pub async fn start_merge(
        ctx: AppContext,
        run_id: i64,
        repo_id: i64,
        allowed_urls: Vec<String>,
    ) -> Result<Self, CoreError> {
        Self::start(ctx, run_id, repo_id, ManagerKind::Merge { allowed_urls }).await
    }

    pub async fn start_test(ctx: AppContext, run_id: i64, repo_id: i64) -> Result<Self, CoreError> {
        Self::start(ctx, run_id, repo_id, ManagerKind::Test).await
    }

    async fn start(
        ctx: AppContext,
        run_id: i64,
        repo_id: i64,
        kind: ManagerKind,
    ) -> Result<Self, CoreError> {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|error| {
                CoreError::Other(format!(
                    "Could not start {} MCP server: {error}",
                    kind.display_name()
                ))
            })?;
        let address = listener.local_addr().map_err(|error| {
            CoreError::Other(format!(
                "Could not read {} MCP address: {error}",
                kind.display_name()
            ))
        })?;
        let bearer_token = uuid::Uuid::new_v4().to_string();
        let (submitted_tx, submitted_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let state = Arc::new(ManagerMcpState {
            ctx,
            run_id,
            repo_id,
            bearer_token: bearer_token.clone(),
            kind: kind.clone(),
            submitted_tx: Mutex::new(Some(submitted_tx)),
        });
        let app = Router::new()
            .route("/mcp", post(handle_mcp_post))
            .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
            .with_state(state);

        let display_name = kind.display_name();
        tokio::spawn(async move {
            let server = axum::serve(listener, app).with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            });
            if let Err(error) = server.await {
                log::error!("{display_name} MCP server stopped unexpectedly: {error}");
            }
        });

        Ok(Self {
            url: format!("http://{address}/mcp"),
            bearer_token,
            kind,
            submitted_rx,
            _shutdown_tx: shutdown_tx,
        })
    }

    pub fn launch_options(&self, agent_name: &str) -> Result<SessionLaunchOptions, CoreError> {
        let server_name = self.kind.server_name();
        let token_env = self.kind.token_env();
        let mut env = std::collections::HashMap::new();
        env.insert(token_env.to_string(), self.bearer_token.clone());

        let command = match agent_name {
            "codex" => format!(
                "codex --dangerously-bypass-approvals-and-sandbox \
-c 'mcp_servers.{server_name}.url=\"{}\"' \
-c 'mcp_servers.{server_name}.bearer_token_env_var=\"{token_env}\"' \
-c 'mcp_servers.{server_name}.required=true'\n",
                self.url
            ),
            "claude-code" => {
                let mut servers = Map::new();
                servers.insert(
                    server_name.to_string(),
                    json!({
                        "type": "http",
                        "url": self.url,
                        "headers": {
                            "Authorization": format!("Bearer ${{{token_env}}}")
                        }
                    }),
                );
                let config = json!({"mcpServers": servers});
                format!(
                    "PATH=$HOME/.local/bin:$PATH claude --dangerously-skip-permissions \
--strict-mcp-config --mcp-config '{}'\n",
                    config
                )
            }
            _ => {
                return Err(CoreError::Other(format!(
                    "Unsupported {} agent: {agent_name}",
                    self.kind.display_name()
                )))
            }
        };

        Ok(SessionLaunchOptions { command, env })
    }

    pub(super) async fn wait_for_submission(self) -> Result<(), oneshot::error::RecvError> {
        let Self {
            mut submitted_rx,
            _shutdown_tx,
            ..
        } = self;
        let result = (&mut submitted_rx).await;
        drop(_shutdown_tx);
        result
    }
}

async fn handle_mcp_post(
    State(state): State<Arc<ManagerMcpState>>,
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
                Json(json!({"error": "Manager MCP only accepts local origins"})),
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
            Json(json!({"error": "Invalid manager capability token"})),
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

async fn dispatch_request(state: &Arc<ManagerMcpState>, request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str)?;
    let Some(id) = id else {
        return None;
    };

    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {
                "name": state.kind.server_name(),
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
        "ping" => json!({}),
        "tools/list" => json!({"tools": [manager_tool_definition(&state.kind)]}),
        "tools/call" => {
            let tool_name = request
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if tool_name != state.kind.tool_name() {
                return Some(tool_error_response(
                    id,
                    format!("Unknown manager tool: {tool_name}"),
                ));
            }
            let arguments = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));

            let structured = match &state.kind {
                ManagerKind::Merge { allowed_urls } => {
                    let result: ShipResult = match serde_json::from_value(arguments) {
                        Ok(result) => result,
                        Err(error) => {
                            return Some(tool_error_response(
                                id,
                                format!("Invalid {MERGE_MCP_TOOL_NAME} arguments: {error}"),
                            ));
                        }
                    };
                    if let Err(error) = validate_ship_result(state.run_id, allowed_urls, &result) {
                        return Some(tool_error_response(id, error));
                    }
                    if let Err(error) = apply_ship_result_db(&state.ctx.db, &result) {
                        return Some(tool_error_response(id, error.to_string()));
                    }
                    emit_merge_changed(&state.ctx.event_bus, state.repo_id, Some(state.run_id))
                        .await;
                    json!({
                        "accepted": true,
                        "run_id": state.run_id,
                        "merged_count": result.merged_prs.len(),
                        "failed_count": result.failed_prs.len()
                    })
                }
                ManagerKind::Test => {
                    let result: TestResult = match serde_json::from_value(arguments) {
                        Ok(result) => result,
                        Err(error) => {
                            return Some(tool_error_response(
                                id,
                                format!("Invalid {TEST_MCP_TOOL_NAME} arguments: {error}"),
                            ));
                        }
                    };
                    if let Err(error) = validate_test_result(state.run_id, &result) {
                        return Some(tool_error_response(id, error));
                    }
                    if let Err(error) = apply_test_result_db(&state.ctx.db, &result) {
                        return Some(tool_error_response(id, error.to_string()));
                    }
                    emit_test_changed(&state.ctx.event_bus, state.repo_id, Some(state.run_id))
                        .await;
                    json!({
                        "accepted": true,
                        "run_id": state.run_id,
                        "test_count": result.tests.len()
                    })
                }
            };

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
                        "text": format!(
                            "{} result accepted. Racc UI has been updated.",
                            state.kind.display_name()
                        )
                    }],
                    "structuredContent": structured
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

fn manager_tool_definition(kind: &ManagerKind) -> Value {
    match kind {
        ManagerKind::Merge { .. } => json!({
            "name": MERGE_MCP_TOOL_NAME,
            "title": "Submit a Racc merge result",
            "description": "Submit the verified outcome of the current ordered Merge Manager run. Racc validates the run id and pull request URLs, stores the result, and immediately updates the Merge Manager UI.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["run_id", "status", "merged_prs", "failed_prs", "tests", "summary"],
                "properties": {
                    "run_id": {"type": "integer"},
                    "status": {"type": "string", "enum": ["succeeded", "failed"]},
                    "merged_prs": {"type": "array", "items": {"type": "string", "minLength": 1}},
                    "failed_prs": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["url", "reason"],
                            "properties": {
                                "url": {"type": "string", "minLength": 1},
                                "reason": {"type": "string", "minLength": 1}
                            }
                        }
                    },
                    "tests": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["command", "status"],
                            "properties": {
                                "command": {"type": "string", "minLength": 1},
                                "status": {"type": "string", "enum": ["passed", "failed"]},
                                "summary": {"type": "string"}
                            }
                        }
                    },
                    "summary": {"type": "string", "minLength": 1}
                }
            },
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": false,
                "idempotentHint": false,
                "openWorldHint": false
            }
        }),
        ManagerKind::Test => json!({
            "name": TEST_MCP_TOOL_NAME,
            "title": "Submit a Racc test result",
            "description": "Submit the verified automated-test and UAT outcome for the current Test Manager run. Racc validates and stores the result, then immediately updates the Test Manager UI.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["run_id", "status", "tests", "summary"],
                "properties": {
                    "run_id": {"type": "integer"},
                    "status": {"type": "string", "enum": ["succeeded", "failed"]},
                    "tests": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["name", "status"],
                            "properties": {
                                "name": {"type": "string", "minLength": 1},
                                "status": {"type": "string", "enum": ["passed", "failed"]},
                                "summary": {"type": "string"}
                            }
                        }
                    },
                    "summary": {"type": "string", "minLength": 1}
                }
            },
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": false,
                "idempotentHint": false,
                "openWorldHint": false
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::BroadcastEventBus;
    use crate::ssh::SshManager;
    use crate::transport::manager::TransportManager;

    fn test_context() -> (AppContext, std::path::PathBuf, i64, i64, i64) {
        let path =
            std::env::temp_dir().join(format!("racc-manager-mcp-{}.db", uuid::Uuid::new_v4()));
        let conn = crate::db::init_db(path.clone()).expect("database should initialize");
        conn.execute(
            "INSERT INTO repos (path, name) VALUES ('/tmp/widgets', 'widgets')",
            [],
        )
        .unwrap();
        let repo_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO sessions (repo_id, status) VALUES (?1, 'Running')",
            [repo_id],
        )
        .unwrap();
        let session_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO merge_runs
             (repo_id, session_id, target_branch, agent, prompt, status)
             VALUES (?1, ?2, 'main', 'codex', 'merge', 'shipping')",
            rusqlite::params![repo_id, session_id],
        )
        .unwrap();
        let merge_run_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO merge_queue_items
             (repo_id, task_id, source_session_id, pr_url, status, run_id)
             VALUES (?1, 1, ?2, 'https://github.com/acme/widgets/pull/1', 'shipping', ?3)",
            rusqlite::params![repo_id, session_id, merge_run_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO test_runs
             (repo_id, session_id, target_branch, agent, prompt, status)
             VALUES (?1, ?2, 'main', 'codex', 'test', 'testing')",
            rusqlite::params![repo_id, session_id],
        )
        .unwrap();
        let test_run_id = conn.last_insert_rowid();
        let (terminal_tx, _) = tokio::sync::broadcast::channel(64);
        let ctx = AppContext::new(
            Arc::new(Mutex::new(conn)),
            TransportManager::new(),
            Arc::new(SshManager::new()),
            Arc::new(BroadcastEventBus::new()),
            terminal_tx,
        );
        (ctx, path, repo_id, merge_run_id, test_run_id)
    }

    async fn post_rpc(
        url: &str,
        authorization: Option<String>,
        payload: Value,
    ) -> reqwest::Response {
        let client = reqwest::Client::new();
        let mut request = client
            .post(url)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(payload.to_string());
        if let Some(authorization) = authorization {
            request = request.header("authorization", authorization);
        }
        request.send().await.expect("MCP request should complete")
    }

    #[test]
    fn manager_tools_expose_closed_structured_schemas() {
        let merge = manager_tool_definition(&ManagerKind::Merge {
            allowed_urls: vec![],
        });
        let test = manager_tool_definition(&ManagerKind::Test);
        assert_eq!(merge["name"], MERGE_MCP_TOOL_NAME);
        assert_eq!(test["name"], TEST_MCP_TOOL_NAME);
        assert_eq!(merge["inputSchema"]["additionalProperties"], false);
        assert_eq!(test["inputSchema"]["additionalProperties"], false);
        assert_eq!(test["inputSchema"]["properties"]["tests"]["minItems"], 1);
    }

    #[tokio::test]
    async fn scoped_mcp_submissions_require_capabilities_and_update_manager_state() {
        let (ctx, path, repo_id, merge_run_id, test_run_id) = test_context();
        let merge_runtime = ManagerMcpRuntime::start_merge(
            ctx.clone(),
            merge_run_id,
            repo_id,
            vec!["https://github.com/acme/widgets/pull/1".to_string()],
        )
        .await
        .expect("merge MCP should start");
        let launch = merge_runtime
            .launch_options("codex")
            .expect("Codex launch should include MCP config");
        assert!(launch.command.contains(MERGE_MCP_SERVER_NAME));
        assert!(launch.command.contains(MERGE_MCP_TOKEN_ENV));
        assert!(!launch.command.contains(&merge_runtime.bearer_token));
        assert_eq!(
            launch.env.get(MERGE_MCP_TOKEN_ENV),
            Some(&merge_runtime.bearer_token)
        );
        let merge_url = merge_runtime.url.clone();
        let merge_auth = format!("Bearer {}", merge_runtime.bearer_token);
        let merge_submission = tokio::spawn(merge_runtime.wait_for_submission());

        let unauthorized = post_rpc(
            &merge_url,
            None,
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
        )
        .await;
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let rejected = post_rpc(
            &merge_url,
            Some(merge_auth.clone()),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": MERGE_MCP_TOOL_NAME,
                    "arguments": {
                        "run_id": merge_run_id + 1,
                        "status": "succeeded",
                        "merged_prs": [],
                        "failed_prs": [],
                        "tests": [],
                        "summary": "wrong run"
                    }
                }
            }),
        )
        .await;
        let rejected: Value = serde_json::from_str(&rejected.text().await.unwrap()).unwrap();
        assert_eq!(rejected["result"]["isError"], true);

        let accepted = post_rpc(
            &merge_url,
            Some(merge_auth),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": MERGE_MCP_TOOL_NAME,
                    "arguments": {
                        "run_id": merge_run_id,
                        "status": "succeeded",
                        "merged_prs": ["https://github.com/acme/widgets/pull/1"],
                        "failed_prs": [],
                        "tests": [{"command": "cargo test", "status": "passed"}],
                        "summary": "merged and verified"
                    }
                }
            }),
        )
        .await;
        let accepted: Value = serde_json::from_str(&accepted.text().await.unwrap()).unwrap();
        assert_eq!(accepted["result"]["structuredContent"]["accepted"], true);
        tokio::time::timeout(std::time::Duration::from_secs(2), merge_submission)
            .await
            .expect("merge submission should arrive")
            .expect("merge wait task should not panic")
            .expect("merge sender should stay alive");

        let test_runtime = ManagerMcpRuntime::start_test(ctx.clone(), test_run_id, repo_id)
            .await
            .expect("test MCP should start");
        let test_url = test_runtime.url.clone();
        let test_auth = format!("Bearer {}", test_runtime.bearer_token);
        let test_submission = tokio::spawn(test_runtime.wait_for_submission());
        let accepted = post_rpc(
            &test_url,
            Some(test_auth),
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": TEST_MCP_TOOL_NAME,
                    "arguments": {
                        "run_id": test_run_id,
                        "status": "succeeded",
                        "tests": [{"name": "Full UAT", "status": "passed", "summary": "clean"}],
                        "summary": "all scenarios passed"
                    }
                }
            }),
        )
        .await;
        let accepted: Value = serde_json::from_str(&accepted.text().await.unwrap()).unwrap();
        assert_eq!(accepted["result"]["structuredContent"]["accepted"], true);
        tokio::time::timeout(std::time::Duration::from_secs(2), test_submission)
            .await
            .expect("test submission should arrive")
            .expect("test wait task should not panic")
            .expect("test sender should stay alive");

        let conn = ctx.db.lock().unwrap();
        let merge_status: String = conn
            .query_row(
                "SELECT status FROM merge_runs WHERE id = ?1",
                [merge_run_id],
                |row| row.get(0),
            )
            .unwrap();
        let queue_status: String = conn
            .query_row(
                "SELECT status FROM merge_queue_items WHERE run_id = ?1",
                [merge_run_id],
                |row| row.get(0),
            )
            .unwrap();
        let test_status: String = conn
            .query_row(
                "SELECT status FROM test_runs WHERE id = ?1",
                [test_run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(merge_status, "succeeded");
        assert_eq!(queue_status, "succeeded");
        assert_eq!(test_status, "succeeded");
        drop(conn);
        drop(ctx);
        let _ = std::fs::remove_file(path);
    }
}
