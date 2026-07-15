use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

use super::manager_mcp::{ManagerMcpRuntime, TEST_MCP_SERVER_NAME, TEST_MCP_TOOL_NAME};
use crate::error::CoreError;
use crate::events::{EventBus, RaccEvent};
use crate::AppContext;

pub const DEFAULT_TEST_INSTRUCTIONS: &str = "Perform a comprehensive full-project UAT pass. Discover how to build and run the project, execute the existing automated test suites, and exercise every important user-visible workflow end to end. Report exact commands, observed behavior, failures, and reproducible evidence. Do not change product code or weaken tests.";

const RESULT_PROMPT_SETTLE_DELAY: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TestCaseResult {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TestResult {
    pub run_id: i64,
    pub status: String,
    #[serde(default)]
    pub tests: Vec<TestCaseResult>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestSettings {
    pub repo_id: i64,
    pub target_branch: String,
    pub agent: String,
    pub instructions: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestRun {
    pub id: i64,
    pub repo_id: i64,
    pub session_id: Option<i64>,
    pub target_branch: String,
    pub agent: String,
    pub worktree_branch: Option<String>,
    pub prompt: String,
    pub status: String,
    pub result_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestManagerState {
    pub settings: TestSettings,
    pub active_run: Option<TestRun>,
    pub last_run: Option<TestRun>,
}

#[derive(Debug)]
struct TestRunReservation {
    run: TestRun,
    worktree_branch: String,
    repo_path: String,
}

const SELECT_TEST_RUN: &str = "SELECT id, repo_id, session_id, target_branch, agent, worktree_branch, prompt, status, result_json, created_at, updated_at FROM test_runs";

fn row_to_test_run(row: &rusqlite::Row) -> rusqlite::Result<TestRun> {
    Ok(TestRun {
        id: row.get(0)?,
        repo_id: row.get(1)?,
        session_id: row.get(2)?,
        target_branch: row.get(3)?,
        agent: row.get(4)?,
        worktree_branch: row.get(5)?,
        prompt: row.get(6)?,
        status: row.get(7)?,
        result_json: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn detect_default_branch(repo_path: &str) -> String {
    let remote = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .current_dir(repo_path)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .and_then(|branch| branch.strip_prefix("origin/").map(str::to_string));
    if let Some(branch) = remote.filter(|branch| !branch.is_empty()) {
        return branch;
    }

    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|branch| !branch.is_empty() && branch != "HEAD")
        .unwrap_or_else(|| "main".to_string())
}

fn load_test_settings(
    conn: &rusqlite::Connection,
    repo_id: i64,
) -> Result<TestSettings, CoreError> {
    let saved = conn
        .query_row(
            "SELECT repo_id, target_branch, agent, instructions, updated_at
             FROM test_settings WHERE repo_id = ?1",
            [repo_id],
            |row| {
                Ok(TestSettings {
                    repo_id: row.get(0)?,
                    target_branch: row.get(1)?,
                    agent: row.get(2)?,
                    instructions: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .optional()?;
    if let Some(settings) = saved {
        return Ok(settings);
    }

    let repo_path: String = conn
        .query_row("SELECT path FROM repos WHERE id = ?1", [repo_id], |row| {
            row.get(0)
        })
        .map_err(|error| CoreError::NotFound(format!("Repo {repo_id} not found: {error}")))?;
    Ok(TestSettings {
        repo_id,
        target_branch: detect_default_branch(&repo_path),
        agent: "claude-code".to_string(),
        instructions: DEFAULT_TEST_INSTRUCTIONS.to_string(),
        updated_at: None,
    })
}

pub(super) async fn emit_test_changed(
    event_bus: &Arc<dyn EventBus>,
    repo_id: i64,
    run_id: Option<i64>,
) {
    event_bus
        .emit(RaccEvent::TestManagerChanged { repo_id, run_id })
        .await;
}

pub async fn update_test_settings(
    ctx: &AppContext,
    repo_id: i64,
    target_branch: &str,
    agent: &str,
    instructions: &str,
) -> Result<TestSettings, CoreError> {
    let target_branch = target_branch.trim();
    let instructions = instructions.trim();
    if target_branch.is_empty() {
        return Err(CoreError::Other("Target branch is required".to_string()));
    }
    let valid_branch = std::process::Command::new("git")
        .args(["check-ref-format", "--branch", target_branch])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !valid_branch {
        return Err(CoreError::Other(format!(
            "Invalid target branch name: {target_branch}"
        )));
    }
    if !matches!(agent, "claude-code" | "codex") {
        return Err(CoreError::Other(format!(
            "Unsupported Test Manager agent: {agent}"
        )));
    }
    if instructions.is_empty() {
        return Err(CoreError::Other(
            "Test instructions are required".to_string(),
        ));
    }

    let settings = {
        let conn = ctx
            .db
            .lock()
            .map_err(|error| CoreError::Other(error.to_string()))?;
        conn.query_row("SELECT id FROM repos WHERE id = ?1", [repo_id], |_| Ok(()))
            .map_err(|error| CoreError::NotFound(format!("Repo {repo_id} not found: {error}")))?;
        conn.execute(
            "INSERT INTO test_settings (repo_id, target_branch, agent, instructions)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(repo_id) DO UPDATE SET
                target_branch = excluded.target_branch,
                agent = excluded.agent,
                instructions = excluded.instructions,
                updated_at = datetime('now')",
            rusqlite::params![repo_id, target_branch, agent, instructions],
        )?;
        load_test_settings(&conn, repo_id)?
    };
    emit_test_changed(&ctx.event_bus, repo_id, None).await;
    Ok(settings)
}

fn reconcile_orphaned_runs(conn: &mut rusqlite::Connection, repo_id: i64) -> Result<(), CoreError> {
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE test_runs
         SET status = 'needs_review',
             result_json = COALESCE(result_json, '{\"summary\":\"Test Manager session is no longer running\"}'),
             updated_at = datetime('now')
         WHERE repo_id = ?1 AND status = 'testing' AND (
             session_id IS NULL OR NOT EXISTS (
                 SELECT 1 FROM sessions
                 WHERE sessions.id = test_runs.session_id
                   AND sessions.status = 'Running'
             )
         )",
        [repo_id],
    )?;
    tx.execute(
        "UPDATE test_runs
         SET status = 'needs_review',
             result_json = COALESCE(result_json, '{\"summary\":\"Test Manager did not finish starting\"}'),
             updated_at = datetime('now')
         WHERE repo_id = ?1 AND status = 'starting'
           AND created_at < datetime('now', '-10 minutes')",
        [repo_id],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn get_test_manager(ctx: &AppContext, repo_id: i64) -> Result<TestManagerState, CoreError> {
    let mut conn = ctx
        .db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;
    reconcile_orphaned_runs(&mut conn, repo_id)?;
    let settings = load_test_settings(&conn, repo_id)?;
    let active_run = conn
        .query_row(
            &format!(
                "{SELECT_TEST_RUN} WHERE repo_id = ?1 AND status IN ('starting', 'testing') ORDER BY id DESC LIMIT 1"
            ),
            [repo_id],
            row_to_test_run,
        )
        .optional()?;
    let last_run = conn
        .query_row(
            &format!("{SELECT_TEST_RUN} WHERE repo_id = ?1 ORDER BY id DESC LIMIT 1"),
            [repo_id],
            row_to_test_run,
        )
        .optional()?;
    Ok(TestManagerState {
        settings,
        active_run,
        last_run,
    })
}

fn reserve_test_run(ctx: &AppContext, repo_id: i64) -> Result<TestRunReservation, CoreError> {
    let mut conn = ctx
        .db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;
    let tx = conn.transaction()?;
    let settings = load_test_settings(&tx, repo_id)?;
    let repo_path: String = tx
        .query_row("SELECT path FROM repos WHERE id = ?1", [repo_id], |row| {
            row.get(0)
        })
        .map_err(|error| CoreError::NotFound(format!("Repo {repo_id} not found: {error}")))?;
    let active_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM test_runs WHERE repo_id = ?1 AND status IN ('starting', 'testing')",
        [repo_id],
        |row| row.get(0),
    )?;
    if active_count > 0 {
        return Err(CoreError::Other(
            "A Test Manager run is already active for this repository".to_string(),
        ));
    }

    tx.execute(
        "INSERT INTO test_runs (repo_id, target_branch, agent, prompt, status)
         VALUES (?1, ?2, ?3, '', 'starting')",
        rusqlite::params![repo_id, settings.target_branch, settings.agent],
    )?;
    let run_id = tx.last_insert_rowid();
    let worktree_branch = format!("racc/test-{run_id}");
    let prompt = build_test_prompt(run_id, &settings.target_branch, &settings.instructions);
    tx.execute(
        "UPDATE test_runs SET worktree_branch = ?1, prompt = ?2 WHERE id = ?3",
        rusqlite::params![worktree_branch, prompt, run_id],
    )?;
    let run = tx.query_row(
        &format!("{SELECT_TEST_RUN} WHERE id = ?1"),
        [run_id],
        row_to_test_run,
    )?;
    tx.commit()?;
    Ok(TestRunReservation {
        run,
        worktree_branch,
        repo_path,
    })
}

fn resolve_target_base(repo_path: &str, target_branch: &str) -> Result<String, CoreError> {
    let fetch = std::process::Command::new("git")
        .args(["fetch", "origin", target_branch])
        .current_dir(repo_path)
        .output()
        .map_err(|error| CoreError::Git(format!("git fetch failed: {error}")))?;
    let remote_ref = format!("origin/{target_branch}");
    let remote_exists = std::process::Command::new("git")
        .args(["rev-parse", "--verify", &remote_ref])
        .current_dir(repo_path)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if fetch.status.success() && remote_exists {
        return Ok(remote_ref);
    }
    let local_exists = std::process::Command::new("git")
        .args(["rev-parse", "--verify", target_branch])
        .current_dir(repo_path)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if local_exists {
        return Ok(target_branch.to_string());
    }
    Err(CoreError::Git(format!(
        "Could not fetch or resolve target branch {target_branch}: {}",
        String::from_utf8_lossy(&fetch.stderr).trim()
    )))
}

fn ensure_agent_available(agent: &str) -> Result<(), CoreError> {
    let binary = match agent {
        "claude-code" => "claude",
        "codex" => "codex",
        _ => {
            return Err(CoreError::Other(format!(
                "Unsupported Test Manager agent: {agent}"
            )))
        }
    };
    let check = format!("PATH=$HOME/.local/bin:$PATH command -v {binary}");
    let output = std::process::Command::new("/bin/zsh")
        .args(["-lc", &check])
        .output()
        .map_err(|error| CoreError::Other(format!("Could not check {binary}: {error}")))?;
    if !output.status.success() {
        return Err(CoreError::Other(format!(
            "{binary} is not installed or is not available on PATH"
        )));
    }
    Ok(())
}

fn mark_run_start_failed(
    db: &Arc<Mutex<rusqlite::Connection>>,
    run_id: i64,
    message: &str,
) -> Result<(), CoreError> {
    let conn = db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;
    let result_json = serde_json::json!({ "summary": message }).to_string();
    conn.execute(
        "UPDATE test_runs SET status = 'failed', result_json = ?1,
         updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![result_json, run_id],
    )?;
    Ok(())
}

fn mark_run_needs_review(
    db: &Arc<Mutex<rusqlite::Connection>>,
    run_id: i64,
    message: &str,
) -> Result<(), CoreError> {
    let conn = db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;
    let result_json = serde_json::json!({ "summary": message }).to_string();
    conn.execute(
        "UPDATE test_runs SET status = 'needs_review', result_json = ?1,
         updated_at = datetime('now')
         WHERE id = ?2 AND status IN ('starting', 'testing')",
        rusqlite::params![result_json, run_id],
    )?;
    Ok(())
}

fn activate_test_run(
    ctx: &AppContext,
    reservation: &TestRunReservation,
    session_id: i64,
) -> Result<TestRun, CoreError> {
    let conn = ctx
        .db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;
    let changed = conn.execute(
        "UPDATE test_runs SET session_id = ?1, status = 'testing',
         updated_at = datetime('now') WHERE id = ?2 AND status = 'starting'",
        rusqlite::params![session_id, reservation.run.id],
    )?;
    if changed != 1 {
        return Err(CoreError::Other(
            "Test run changed while it was starting".to_string(),
        ));
    }
    Ok(conn.query_row(
        &format!("{SELECT_TEST_RUN} WHERE id = ?1"),
        [reservation.run.id],
        row_to_test_run,
    )?)
}

pub(super) fn apply_test_result_db(
    db: &Arc<Mutex<rusqlite::Connection>>,
    result: &TestResult,
) -> Result<(), CoreError> {
    let conn = db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;
    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM test_runs WHERE id = ?1 AND status IN ('starting', 'testing')",
        [result.run_id],
        |row| row.get(0),
    )?;
    if active == 0 {
        return Err(CoreError::NotFound(format!(
            "Active test run {} not found",
            result.run_id
        )));
    }
    let result_json = serde_json::to_string(result)
        .map_err(|error| CoreError::Other(format!("Could not serialize test result: {error}")))?;
    conn.execute(
        "UPDATE test_runs SET status = ?1, result_json = ?2,
         updated_at = datetime('now') WHERE id = ?3",
        rusqlite::params![result.status, result_json, result.run_id],
    )?;
    Ok(())
}

fn spawn_mcp_watcher(
    ctx: AppContext,
    mut terminal_rx: tokio::sync::broadcast::Receiver<crate::TerminalData>,
    mut event_rx: tokio::sync::broadcast::Receiver<RaccEvent>,
    run: TestRun,
    session_id: i64,
    mcp_runtime: ManagerMcpRuntime,
) {
    let submission = mcp_runtime.wait_for_submission();
    tokio::spawn(async move {
        tokio::pin!(submission);
        let agent_type = crate::agent::AgentType::from_agent_str(&run.agent);
        let run_token = format!("Test Manager for Racc test run {}", run.id);
        let mut output_buffer = Vec::new();
        let mut run_seen = false;
        let mut prompt_tracker = crate::agent::PromptSettleTracker::new(RESULT_PROMPT_SETTLE_DELAY);
        let prompt_settle_timeout = tokio::time::sleep(RESULT_PROMPT_SETTLE_DELAY);
        tokio::pin!(prompt_settle_timeout);
        let mut prompt_pending = false;

        loop {
            tokio::select! {
                terminal = terminal_rx.recv() => {
                    match terminal {
                        Ok(data) if data.session_id == session_id => {
                            output_buffer.extend_from_slice(&data.data);
                            if output_buffer.len() > 65_536 {
                                output_buffer.drain(..32_768);
                            }
                            let text = crate::agent::strip_ansi(&output_buffer);
                            if !run_seen && text.contains(&run_token) {
                                run_seen = true;
                                output_buffer.clear();
                                prompt_tracker.clear();
                                prompt_pending = false;
                                continue;
                            }
                            if run_seen {
                                if let Some(deadline) = prompt_tracker.observe(
                                    &agent_type,
                                    &output_buffer,
                                    tokio::time::Instant::now(),
                                ) {
                                    prompt_settle_timeout.as_mut().reset(deadline);
                                    prompt_pending = true;
                                } else {
                                    prompt_pending = false;
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => {
                            let _ = mark_run_needs_review(&ctx.db, run.id, "Test Manager terminal output closed before MCP result submission");
                            emit_test_changed(&ctx.event_bus, run.repo_id, Some(run.id)).await;
                            break;
                        }
                    }
                }
                submitted = &mut submission => {
                    if submitted.is_err() {
                        let _ = mark_run_needs_review(
                            &ctx.db,
                            run.id,
                            "Test Manager MCP endpoint stopped before receiving a result",
                        );
                        emit_test_changed(&ctx.event_bus, run.repo_id, Some(run.id)).await;
                    }
                    break;
                }
                event = event_rx.recv() => {
                    match event {
                        Ok(RaccEvent::SessionStatusChanged { session_id: changed_id, status, .. })
                            if changed_id == session_id && status != "Running" =>
                        {
                            let message = format!("Test Manager session ended with status {status} before MCP result submission");
                            let _ = mark_run_needs_review(&ctx.db, run.id, &message);
                            emit_test_changed(&ctx.event_bus, run.repo_id, Some(run.id)).await;
                            break;
                        }
                        Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => break,
                    }
                }
                _ = &mut prompt_settle_timeout, if prompt_pending => {
                    let _ = mark_run_needs_review(
                        &ctx.db,
                        run.id,
                        "Test Manager returned without calling submit_test_result",
                    );
                    emit_test_changed(&ctx.event_bus, run.repo_id, Some(run.id)).await;
                    break;
                }
            }
        }
    });
}

pub async fn start_test_run(ctx: &AppContext, repo_id: i64) -> Result<TestRun, CoreError> {
    let reservation = reserve_test_run(ctx, repo_id)?;
    let run_id = reservation.run.id;
    if let Err(error) = ensure_agent_available(&reservation.run.agent) {
        let _ = mark_run_start_failed(&ctx.db, run_id, &error.to_string());
        emit_test_changed(&ctx.event_bus, repo_id, Some(run_id)).await;
        return Err(error);
    }
    let base_ref = match resolve_target_base(&reservation.repo_path, &reservation.run.target_branch)
    {
        Ok(base_ref) => base_ref,
        Err(error) => {
            let _ = mark_run_start_failed(&ctx.db, run_id, &error.to_string());
            emit_test_changed(&ctx.event_bus, repo_id, Some(run_id)).await;
            return Err(error);
        }
    };
    let mcp_runtime = match ManagerMcpRuntime::start_test(ctx.clone(), run_id, repo_id).await {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = mark_run_start_failed(&ctx.db, run_id, &error.to_string());
            emit_test_changed(&ctx.event_bus, repo_id, Some(run_id)).await;
            return Err(error);
        }
    };
    let launch_options = match mcp_runtime.launch_options(&reservation.run.agent) {
        Ok(options) => options,
        Err(error) => {
            let _ = mark_run_start_failed(&ctx.db, run_id, &error.to_string());
            emit_test_changed(&ctx.event_bus, repo_id, Some(run_id)).await;
            return Err(error);
        }
    };
    let terminal_rx = ctx.terminal_tx.subscribe();
    let event_rx = ctx.event_bus.subscribe();
    let session = match crate::commands::session::create_session_from_base_with_launch(
        ctx,
        repo_id,
        true,
        Some(reservation.worktree_branch.clone()),
        Some(reservation.run.agent.clone()),
        Some(reservation.run.prompt.clone()),
        None,
        Some(true),
        Some(base_ref),
        Some(launch_options),
    )
    .await
    {
        Ok(session) => session,
        Err(error) => {
            let _ = mark_run_start_failed(&ctx.db, run_id, &error.to_string());
            emit_test_changed(&ctx.event_bus, repo_id, Some(run_id)).await;
            return Err(error);
        }
    };
    let run = match activate_test_run(ctx, &reservation, session.id) {
        Ok(run) => run,
        Err(error) => {
            let _ = crate::commands::session::stop_session(ctx, session.id).await;
            let _ = mark_run_start_failed(&ctx.db, run_id, &error.to_string());
            emit_test_changed(&ctx.event_bus, repo_id, Some(run_id)).await;
            return Err(error);
        }
    };
    spawn_mcp_watcher(
        ctx.clone(),
        terminal_rx,
        event_rx,
        run.clone(),
        session.id,
        mcp_runtime,
    );
    emit_test_changed(&ctx.event_bus, repo_id, Some(run.id)).await;
    Ok(run)
}

pub async fn resolve_test_run(
    ctx: &AppContext,
    run_id: i64,
    status: &str,
) -> Result<TestRun, CoreError> {
    if !matches!(status, "succeeded" | "failed") {
        return Err(CoreError::Other(
            "Manual test resolution must be succeeded or failed".to_string(),
        ));
    }
    let run = {
        let conn = ctx
            .db
            .lock()
            .map_err(|error| CoreError::Other(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE test_runs SET status = ?1, updated_at = datetime('now')
             WHERE id = ?2 AND status = 'needs_review'",
            rusqlite::params![status, run_id],
        )?;
        if changed != 1 {
            return Err(CoreError::NotFound(format!(
                "Test run {run_id} is not awaiting review"
            )));
        }
        conn.query_row(
            &format!("{SELECT_TEST_RUN} WHERE id = ?1"),
            [run_id],
            row_to_test_run,
        )?
    };
    emit_test_changed(&ctx.event_bus, run.repo_id, Some(run.id)).await;
    Ok(run)
}

pub async fn retry_test_run(ctx: &AppContext, run_id: i64) -> Result<TestRun, CoreError> {
    let repo_id = {
        let conn = ctx
            .db
            .lock()
            .map_err(|error| CoreError::Other(error.to_string()))?;
        conn.query_row(
            "SELECT repo_id FROM test_runs WHERE id = ?1 AND status IN ('failed', 'needs_review')",
            [run_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            CoreError::NotFound(format!("Test run {run_id} cannot be retried: {error}"))
        })?
    };
    start_test_run(ctx, repo_id).await
}

/// A restarted session no longer has the capability-scoped MCP endpoint that
/// was injected at launch, so surface the interruption instead of pretending
/// the run can continue to report a trustworthy structured result.
pub async fn interrupt_test_run_for_session(
    ctx: &AppContext,
    session_id: i64,
) -> Result<bool, CoreError> {
    let run = {
        let conn = ctx
            .db
            .lock()
            .map_err(|error| CoreError::Other(error.to_string()))?;
        conn.query_row(
            &format!(
                "{SELECT_TEST_RUN} WHERE session_id = ?1 AND status IN ('starting', 'testing') ORDER BY id DESC LIMIT 1"
            ),
            [session_id],
            row_to_test_run,
        )
        .optional()?
    };
    let Some(run) = run else {
        return Ok(false);
    };
    mark_run_needs_review(
        &ctx.db,
        run.id,
        "Test Manager session was restarted; its run-scoped MCP endpoint expired. Retry the run to continue with structured reporting.",
    )?;
    emit_test_changed(&ctx.event_bus, run.repo_id, Some(run.id)).await;
    Ok(true)
}

pub(super) fn validate_test_result(run_id: i64, result: &TestResult) -> Result<(), String> {
    if result.run_id != run_id {
        return Err(format!(
            "Test result run_id {} does not match {}",
            result.run_id, run_id
        ));
    }
    if !matches!(result.status.as_str(), "succeeded" | "failed") {
        return Err(format!("Invalid test result status: {}", result.status));
    }
    if result.summary.trim().is_empty() {
        return Err("Test result summary is required".to_string());
    }
    if result.tests.is_empty() {
        return Err("Test result must include at least one test or UAT scenario".to_string());
    }
    for test in &result.tests {
        if test.name.trim().is_empty() {
            return Err("Test result contains an unnamed scenario".to_string());
        }
        if !matches!(test.status.as_str(), "passed" | "failed") {
            return Err(format!("Invalid test status: {}", test.status));
        }
    }
    if result.status == "succeeded" && result.tests.iter().any(|test| test.status == "failed") {
        return Err("A succeeded test run cannot contain failed scenarios".to_string());
    }
    Ok(())
}

pub fn build_test_prompt(run_id: i64, target_branch: &str, instructions: &str) -> String {
    format!(
        "You are the Test Manager for Racc test run {run_id}.\n\n\
Target branch: {target_branch}\n\n\
User test instructions (these override the default testing scope):\n{instructions}\n\n\
Required workflow:\n\
1. Work only in the current isolated test worktree based on {target_branch}. Do not merge, push, open pull requests, or modify product code.\n\
2. Inspect the repository documentation and automation to discover the supported build, test, and launch paths.\n\
3. Run the broadest relevant automated suite, then perform end-to-end UAT for important user-visible workflows. Include negative and recovery paths when practical.\n\
4. Record exact commands or manual scenarios, pass/fail status, concise evidence, and reproducible failure details. Do not hide flaky, blocked, or skipped coverage.\n\
5. Finish by successfully calling the MCP tool `{TEST_MCP_TOOL_NAME}` from server `{TEST_MCP_SERVER_NAME}` with run_id {run_id}, the overall status, every command or UAT scenario and its evidence, and the overall summary. A text response or printed JSON does not complete this run. If the tool reports a validation error, correct the arguments and call it again. After Racc accepts the result and updates the UI, stop."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::BroadcastEventBus;
    use crate::ssh::SshManager;
    use crate::transport::manager::TransportManager;

    fn test_context() -> (AppContext, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("racc-test-manager-{}.db", uuid::Uuid::new_v4()));
        let conn = crate::db::init_db(path.clone()).expect("database should initialize");
        let (terminal_tx, _) = tokio::sync::broadcast::channel(64);
        let ctx = AppContext::new(
            Arc::new(Mutex::new(conn)),
            TransportManager::new(),
            Arc::new(SshManager::new()),
            Arc::new(BroadcastEventBus::new()),
            terminal_tx,
        );
        (ctx, path)
    }

    fn seed_repo(ctx: &AppContext) -> i64 {
        let conn = ctx.db.lock().expect("database lock");
        conn.execute(
            "INSERT INTO repos (path, name) VALUES ('/tmp/test-manager-widgets', 'widgets')",
            [],
        )
        .expect("repo insert");
        conn.last_insert_rowid()
    }

    #[test]
    fn prompt_contains_uat_contract_and_requires_mcp_submission() {
        let prompt = build_test_prompt(7, "main", "Test every settings workflow.");
        assert!(prompt.contains("Test Manager for Racc test run 7"));
        assert!(prompt.contains("Test every settings workflow."));
        assert!(prompt.contains("end-to-end UAT"));
        assert!(prompt.contains(TEST_MCP_SERVER_NAME));
        assert!(prompt.contains(TEST_MCP_TOOL_NAME));
        assert!(prompt.contains("run_id 7"));
        assert!(!prompt.contains("RACC_TEST_RESULT"));
        assert!(prompt.contains("printed JSON does not complete"));
        assert!(prompt.contains("Do not merge, push, open pull requests"));
    }

    #[test]
    fn structured_result_validation_rejects_inconsistent_success() {
        let result = TestResult {
            run_id: 8,
            status: "succeeded".to_string(),
            tests: vec![TestCaseResult {
                name: "UAT".to_string(),
                status: "failed".to_string(),
                summary: Some("bad".to_string()),
            }],
            summary: "bad".to_string(),
        };
        let error =
            validate_test_result(8, &result).expect_err("failed scenario cannot report success");
        assert!(error.contains("cannot contain failed"));
    }

    #[tokio::test]
    async fn settings_and_run_reservation_are_scoped_per_repo() {
        let (ctx, path) = test_context();
        let repo_id = seed_repo(&ctx);
        let saved = update_test_settings(&ctx, repo_id, "release", "codex", "Run release UAT.")
            .await
            .expect("settings save");
        assert_eq!(saved.instructions, "Run release UAT.");
        let reservation = reserve_test_run(&ctx, repo_id).expect("reserve test run");
        assert_eq!(reservation.worktree_branch, "racc/test-1");
        assert!(reservation.run.prompt.contains("Run release UAT."));
        let error = reserve_test_run(&ctx, repo_id).expect_err("parallel run rejected");
        assert!(error.to_string().contains("already active"));

        drop(ctx);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn applying_result_completes_the_active_run() {
        let (ctx, path) = test_context();
        let repo_id = seed_repo(&ctx);
        {
            let conn = ctx.db.lock().expect("database lock");
            conn.execute(
                "INSERT INTO test_runs (repo_id, target_branch, agent, prompt, status)
                 VALUES (?1, 'main', 'codex', 'test', 'testing')",
                [repo_id],
            )
            .expect("run insert");
        }
        apply_test_result_db(
            &ctx.db,
            &TestResult {
                run_id: 1,
                status: "succeeded".to_string(),
                tests: vec![TestCaseResult {
                    name: "smoke".to_string(),
                    status: "passed".to_string(),
                    summary: None,
                }],
                summary: "all good".to_string(),
            },
        )
        .expect("apply result");
        let state = get_test_manager(&ctx, repo_id).expect("manager state");
        assert!(state.active_run.is_none());
        assert_eq!(state.last_run.expect("last run").status, "succeeded");

        drop(ctx);
        let _ = std::fs::remove_file(path);
    }
}
