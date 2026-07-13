use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::error::CoreError;
use crate::events::{EventBus, RaccEvent};
use crate::AppContext;

pub const DEFAULT_SHIP_INSTRUCTIONS: &str = "Merge every queued pull request into the integration branch in the listed order, then run the repository's full relevant test suite as one batch.";

const SHIP_RESULT_PREFIX: &str = "RACC_SHIP_RESULT:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailedPullRequest {
    pub url: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShipTestResult {
    pub command: String,
    pub status: String,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShipResult {
    pub run_id: i64,
    pub status: String,
    #[serde(default)]
    pub merged_prs: Vec<String>,
    #[serde(default)]
    pub failed_prs: Vec<FailedPullRequest>,
    #[serde(default)]
    pub tests: Vec<ShipTestResult>,
    pub summary: String,
}

pub struct ShipResultParser {
    run_id: i64,
    allowed_urls: HashSet<String>,
    buffer: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeQueueItem {
    pub id: i64,
    pub repo_id: i64,
    pub task_id: i64,
    pub source_session_id: i64,
    pub pr_url: String,
    pub status: String,
    pub run_id: Option<i64>,
    pub result_message: Option<String>,
    pub added_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeSettings {
    pub repo_id: i64,
    pub target_branch: String,
    pub agent: String,
    pub instructions: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeRun {
    pub id: i64,
    pub repo_id: i64,
    pub session_id: Option<i64>,
    pub target_branch: String,
    pub agent: String,
    pub integration_branch: Option<String>,
    pub prompt: String,
    pub status: String,
    pub result_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeManagerState {
    pub settings: MergeSettings,
    pub items: Vec<MergeQueueItem>,
    pub active_run: Option<MergeRun>,
    pub last_run: Option<MergeRun>,
}

#[derive(Debug)]
struct MergeRunReservation {
    run: MergeRun,
    item_ids: Vec<i64>,
    pr_urls: Vec<String>,
    integration_branch: String,
    repo_path: String,
}

const SELECT_QUEUE_ITEM: &str = "SELECT id, repo_id, task_id, source_session_id, pr_url, status, run_id, result_message, added_at, updated_at FROM merge_queue_items";
const SELECT_MERGE_RUN: &str = "SELECT id, repo_id, session_id, target_branch, agent, integration_branch, prompt, status, result_json, created_at, updated_at FROM merge_runs";

fn row_to_queue_item(row: &rusqlite::Row) -> rusqlite::Result<MergeQueueItem> {
    Ok(MergeQueueItem {
        id: row.get(0)?,
        repo_id: row.get(1)?,
        task_id: row.get(2)?,
        source_session_id: row.get(3)?,
        pr_url: row.get(4)?,
        status: row.get(5)?,
        run_id: row.get(6)?,
        result_message: row.get(7)?,
        added_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn row_to_merge_run(row: &rusqlite::Row) -> rusqlite::Result<MergeRun> {
    Ok(MergeRun {
        id: row.get(0)?,
        repo_id: row.get(1)?,
        session_id: row.get(2)?,
        target_branch: row.get(3)?,
        agent: row.get(4)?,
        integration_branch: row.get(5)?,
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

fn load_merge_settings(
    conn: &rusqlite::Connection,
    repo_id: i64,
) -> Result<MergeSettings, CoreError> {
    let saved = conn
        .query_row(
            "SELECT repo_id, target_branch, agent, instructions, updated_at
             FROM merge_settings WHERE repo_id = ?1",
            [repo_id],
            |row| {
                Ok(MergeSettings {
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
    Ok(MergeSettings {
        repo_id,
        target_branch: detect_default_branch(&repo_path),
        agent: "claude-code".to_string(),
        instructions: DEFAULT_SHIP_INSTRUCTIONS.to_string(),
        updated_at: None,
    })
}

pub async fn update_merge_settings(
    ctx: &AppContext,
    repo_id: i64,
    target_branch: &str,
    agent: &str,
    instructions: &str,
) -> Result<MergeSettings, CoreError> {
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
            "Unsupported Merge Master agent: {agent}"
        )));
    }
    if instructions.is_empty() {
        return Err(CoreError::Other(
            "Ship instructions are required".to_string(),
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
            "INSERT INTO merge_settings (repo_id, target_branch, agent, instructions)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(repo_id) DO UPDATE SET
                target_branch = excluded.target_branch,
                agent = excluded.agent,
                instructions = excluded.instructions,
                updated_at = datetime('now')",
            rusqlite::params![repo_id, target_branch, agent, instructions],
        )?;
        load_merge_settings(&conn, repo_id)?
    };
    emit_merge_changed(&ctx.event_bus, repo_id, None).await;
    Ok(settings)
}

pub fn get_merge_manager(ctx: &AppContext, repo_id: i64) -> Result<MergeManagerState, CoreError> {
    let mut conn = ctx
        .db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;
    reconcile_orphaned_runs(&mut conn, repo_id)?;
    let settings = load_merge_settings(&conn, repo_id)?;

    let mut item_stmt = conn.prepare(&format!(
        "{SELECT_QUEUE_ITEM} WHERE repo_id = ?1 ORDER BY added_at ASC, id ASC"
    ))?;
    let items = item_stmt
        .query_map([repo_id], row_to_queue_item)?
        .collect::<Result<Vec<_>, _>>()?;

    let active_run = conn
        .query_row(
            &format!(
                "{SELECT_MERGE_RUN} WHERE repo_id = ?1 AND status IN ('starting', 'shipping') ORDER BY id DESC LIMIT 1"
            ),
            [repo_id],
            row_to_merge_run,
        )
        .optional()?;
    let last_run = conn
        .query_row(
            &format!("{SELECT_MERGE_RUN} WHERE repo_id = ?1 ORDER BY id DESC LIMIT 1"),
            [repo_id],
            row_to_merge_run,
        )
        .optional()?;

    Ok(MergeManagerState {
        settings,
        items,
        active_run,
        last_run,
    })
}

fn reconcile_orphaned_runs(conn: &mut rusqlite::Connection, repo_id: i64) -> Result<(), CoreError> {
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE merge_runs
         SET status = 'needs_review',
             result_json = COALESCE(result_json, '{\"summary\":\"Merge Master session is no longer running\"}'),
             updated_at = datetime('now')
         WHERE repo_id = ?1 AND status = 'shipping' AND (
             session_id IS NULL OR NOT EXISTS (
                 SELECT 1 FROM sessions
                 WHERE sessions.id = merge_runs.session_id
                   AND sessions.status = 'Running'
             )
         )",
        [repo_id],
    )?;
    tx.execute(
        "UPDATE merge_runs
         SET status = 'needs_review',
             result_json = COALESCE(result_json, '{\"summary\":\"Merge Master did not finish starting\"}'),
             updated_at = datetime('now')
         WHERE repo_id = ?1 AND status = 'starting'
           AND created_at < datetime('now', '-10 minutes')",
        [repo_id],
    )?;
    tx.execute(
        "UPDATE merge_queue_items
         SET status = 'needs_review',
             result_message = COALESCE(result_message, 'Merge Master session is no longer running'),
             updated_at = datetime('now')
         WHERE repo_id = ?1 AND status = 'shipping' AND run_id IN (
             SELECT id FROM merge_runs WHERE repo_id = ?1 AND status = 'needs_review'
         )",
        [repo_id],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn apply_ship_result(ctx: &AppContext, result: &ShipResult) -> Result<(), CoreError> {
    apply_ship_result_db(&ctx.db, result)
}

fn apply_ship_result_db(
    db: &Arc<Mutex<rusqlite::Connection>>,
    result: &ShipResult,
) -> Result<(), CoreError> {
    let mut conn = db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;
    let tx = conn.transaction()?;

    let run_exists: i64 = tx.query_row(
        "SELECT COUNT(*) FROM merge_runs WHERE id = ?1 AND status IN ('starting', 'shipping')",
        [result.run_id],
        |row| row.get(0),
    )?;
    if run_exists == 0 {
        return Err(CoreError::NotFound(format!(
            "Active merge run {} not found",
            result.run_id
        )));
    }

    let mut stmt = tx.prepare(
        "SELECT id, pr_url FROM merge_queue_items
         WHERE run_id = ?1 AND status = 'shipping' ORDER BY id",
    )?;
    let queue = stmt
        .query_map([result.run_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    let queue_urls = queue.iter().map(|(_, url)| url).collect::<HashSet<_>>();
    for url in result
        .merged_prs
        .iter()
        .chain(result.failed_prs.iter().map(|failed| &failed.url))
    {
        if !queue_urls.contains(url) {
            return Err(CoreError::Other(format!(
                "Ship result contains PR outside run {}: {url}",
                result.run_id
            )));
        }
    }

    let merged = result.merged_prs.iter().collect::<HashSet<_>>();
    let failed = result
        .failed_prs
        .iter()
        .map(|entry| (&entry.url, entry.reason.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut all_succeeded = true;
    for (item_id, url) in queue {
        let (status, message) = if merged.contains(&url) {
            ("succeeded", Some(result.summary.as_str()))
        } else if let Some(reason) = failed.get(&url) {
            all_succeeded = false;
            ("failed", Some(*reason))
        } else {
            all_succeeded = false;
            (
                "needs_review",
                Some("Agent result did not mention this pull request"),
            )
        };
        tx.execute(
            "UPDATE merge_queue_items SET status = ?1, result_message = ?2,
             updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![status, message, item_id],
        )?;
    }

    let run_status = if result.status == "succeeded" && all_succeeded {
        "succeeded"
    } else if result.status == "failed" {
        "failed"
    } else {
        "needs_review"
    };
    let result_json = serde_json::to_string(result)
        .map_err(|error| CoreError::Other(format!("Could not serialize ship result: {error}")))?;
    tx.execute(
        "UPDATE merge_runs SET status = ?1, result_json = ?2,
         updated_at = datetime('now') WHERE id = ?3",
        rusqlite::params![run_status, result_json, result.run_id],
    )?;
    tx.commit()?;
    Ok(())
}

fn reserve_merge_run(ctx: &AppContext, repo_id: i64) -> Result<MergeRunReservation, CoreError> {
    let mut conn = ctx
        .db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;
    let tx = conn.transaction()?;
    let settings = load_merge_settings(&tx, repo_id)?;
    let repo_path: String = tx
        .query_row("SELECT path FROM repos WHERE id = ?1", [repo_id], |row| {
            row.get(0)
        })
        .map_err(|error| CoreError::NotFound(format!("Repo {repo_id} not found: {error}")))?;

    let active_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM merge_runs
         WHERE repo_id = ?1 AND status IN ('starting', 'shipping')",
        [repo_id],
        |row| row.get(0),
    )?;
    if active_count > 0 {
        return Err(CoreError::Other(
            "A Merge Master run is already active for this repository".to_string(),
        ));
    }

    let mut stmt = tx.prepare(
        "SELECT id, pr_url FROM merge_queue_items
         WHERE repo_id = ?1 AND status = 'queued' ORDER BY added_at ASC, id ASC",
    )?;
    let queued = stmt
        .query_map([repo_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if queued.is_empty() {
        return Err(CoreError::Other("Merge queue is empty".to_string()));
    }
    let item_ids = queued.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    let pr_urls = queued.into_iter().map(|(_, url)| url).collect::<Vec<_>>();

    tx.execute(
        "INSERT INTO merge_runs (repo_id, target_branch, agent, prompt, status)
         VALUES (?1, ?2, ?3, '', 'starting')",
        rusqlite::params![repo_id, settings.target_branch, settings.agent],
    )?;
    let run_id = tx.last_insert_rowid();
    let integration_branch = format!("racc/ship-{run_id}");
    let prompt = build_merge_prompt(
        run_id,
        &settings.target_branch,
        &settings.instructions,
        &pr_urls,
    );
    tx.execute(
        "UPDATE merge_runs SET integration_branch = ?1, prompt = ?2 WHERE id = ?3",
        rusqlite::params![integration_branch, prompt, run_id],
    )?;
    let run = tx.query_row(
        &format!("{SELECT_MERGE_RUN} WHERE id = ?1"),
        [run_id],
        row_to_merge_run,
    )?;
    tx.commit()?;

    Ok(MergeRunReservation {
        run,
        item_ids,
        pr_urls,
        integration_branch,
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
                "Unsupported Merge Master agent: {agent}"
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
        "UPDATE merge_runs SET status = 'failed', result_json = ?1,
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
    let mut conn = db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;
    let tx = conn.transaction()?;
    let result_json = serde_json::json!({ "summary": message }).to_string();
    tx.execute(
        "UPDATE merge_runs SET status = 'needs_review', result_json = ?1,
         updated_at = datetime('now')
         WHERE id = ?2 AND status IN ('starting', 'shipping')",
        rusqlite::params![result_json, run_id],
    )?;
    tx.execute(
        "UPDATE merge_queue_items SET status = 'needs_review', result_message = ?1,
         updated_at = datetime('now')
         WHERE run_id = ?2 AND status = 'shipping'",
        rusqlite::params![message, run_id],
    )?;
    tx.commit()?;
    Ok(())
}

fn activate_merge_run(
    ctx: &AppContext,
    reservation: &MergeRunReservation,
    session_id: i64,
) -> Result<MergeRun, CoreError> {
    let mut conn = ctx
        .db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;
    let tx = conn.transaction()?;
    for item_id in &reservation.item_ids {
        let changed = tx.execute(
            "UPDATE merge_queue_items SET status = 'shipping', run_id = ?1,
             result_message = NULL, updated_at = datetime('now')
             WHERE id = ?2 AND status = 'queued'",
            rusqlite::params![reservation.run.id, item_id],
        )?;
        if changed != 1 {
            return Err(CoreError::Other(
                "Merge queue changed while the run was starting".to_string(),
            ));
        }
    }
    tx.execute(
        "UPDATE merge_runs SET session_id = ?1, status = 'shipping',
         updated_at = datetime('now') WHERE id = ?2 AND status = 'starting'",
        rusqlite::params![session_id, reservation.run.id],
    )?;
    let run = tx.query_row(
        &format!("{SELECT_MERGE_RUN} WHERE id = ?1"),
        [reservation.run.id],
        row_to_merge_run,
    )?;
    tx.commit()?;
    Ok(run)
}

async fn emit_merge_changed(event_bus: &Arc<dyn EventBus>, repo_id: i64, run_id: Option<i64>) {
    event_bus
        .emit(RaccEvent::MergeManagerChanged { repo_id, run_id })
        .await;
}

fn spawn_result_watcher(
    db: Arc<Mutex<rusqlite::Connection>>,
    event_bus: Arc<dyn EventBus>,
    mut terminal_rx: tokio::sync::broadcast::Receiver<crate::TerminalData>,
    mut event_rx: tokio::sync::broadcast::Receiver<RaccEvent>,
    run: MergeRun,
    session_id: i64,
    pr_urls: Vec<String>,
) {
    tokio::spawn(async move {
        let mut parser = ShipResultParser::new(run.id, pr_urls);
        let agent_type = crate::agent::AgentType::from_agent_str(&run.agent);
        let run_token = format!("Merge Master for Racc ship run {}", run.id);
        let mut output_buffer = Vec::new();
        let mut run_seen_at: Option<tokio::time::Instant> = None;

        loop {
            tokio::select! {
                terminal = terminal_rx.recv() => {
                    match terminal {
                        Ok(data) if data.session_id == session_id => {
                            match parser.push(&data.data) {
                                Ok(Some(result)) => {
                                    let _ = apply_ship_result_db(&db, &result);
                                    emit_merge_changed(&event_bus, run.repo_id, Some(run.id)).await;
                                    break;
                                }
                                Err(error) => {
                                    let _ = mark_run_needs_review(&db, run.id, &error);
                                    emit_merge_changed(&event_bus, run.repo_id, Some(run.id)).await;
                                    break;
                                }
                                Ok(None) => {}
                            }

                            output_buffer.extend_from_slice(&data.data);
                            if output_buffer.len() > 65_536 {
                                output_buffer.drain(..32_768);
                            }
                            let text = crate::agent::strip_ansi(&output_buffer);
                            if run_seen_at.is_none() && text.contains(&run_token) {
                                run_seen_at = Some(tokio::time::Instant::now());
                                // Discard startup output so the initial ready prompt cannot be
                                // mistaken for the prompt shown after the ship task completes.
                                output_buffer.clear();
                                continue;
                            }
                            if let Some(started) = run_seen_at {
                                if started.elapsed() >= std::time::Duration::from_secs(2)
                                    && crate::agent::is_agent_ready(&agent_type, &text)
                                {
                                    let _ = mark_run_needs_review(
                                        &db,
                                        run.id,
                                        "Merge Master returned to the input prompt without a valid result marker",
                                    );
                                    emit_merge_changed(&event_bus, run.repo_id, Some(run.id)).await;
                                    break;
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => {
                            let _ = mark_run_needs_review(&db, run.id, "Terminal output closed without a result marker");
                            emit_merge_changed(&event_bus, run.repo_id, Some(run.id)).await;
                            break;
                        }
                    }
                }
                event = event_rx.recv() => {
                    match event {
                        Ok(RaccEvent::SessionStatusChanged { session_id: changed_id, status, .. })
                            if changed_id == session_id && status != "Running" =>
                        {
                            let message = format!("Merge Master session ended with status {status} without a result marker");
                            let _ = mark_run_needs_review(&db, run.id, &message);
                            emit_merge_changed(&event_bus, run.repo_id, Some(run.id)).await;
                            break;
                        }
                        Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => break,
                    }
                }
            }
        }
    });
}

pub async fn start_merge_run(ctx: &AppContext, repo_id: i64) -> Result<MergeRun, CoreError> {
    let reservation = reserve_merge_run(ctx, repo_id)?;
    let run_id = reservation.run.id;
    if let Err(error) = ensure_agent_available(&reservation.run.agent) {
        let _ = mark_run_start_failed(&ctx.db, run_id, &error.to_string());
        emit_merge_changed(&ctx.event_bus, repo_id, Some(run_id)).await;
        return Err(error);
    }
    let base_ref = match resolve_target_base(&reservation.repo_path, &reservation.run.target_branch)
    {
        Ok(base_ref) => base_ref,
        Err(error) => {
            let _ = mark_run_start_failed(&ctx.db, run_id, &error.to_string());
            emit_merge_changed(&ctx.event_bus, repo_id, Some(run_id)).await;
            return Err(error);
        }
    };

    let terminal_rx = ctx.terminal_tx.subscribe();
    let event_rx = ctx.event_bus.subscribe();
    let session = match crate::commands::session::create_session_from_base(
        ctx,
        repo_id,
        true,
        Some(reservation.integration_branch.clone()),
        Some(reservation.run.agent.clone()),
        Some(reservation.run.prompt.clone()),
        None,
        Some(true),
        Some(base_ref),
    )
    .await
    {
        Ok(session) => session,
        Err(error) => {
            let _ = mark_run_start_failed(&ctx.db, run_id, &error.to_string());
            emit_merge_changed(&ctx.event_bus, repo_id, Some(run_id)).await;
            return Err(error);
        }
    };

    let run = match activate_merge_run(ctx, &reservation, session.id) {
        Ok(run) => run,
        Err(error) => {
            let _ = crate::commands::session::stop_session(ctx, session.id).await;
            let _ = mark_run_start_failed(&ctx.db, run_id, &error.to_string());
            emit_merge_changed(&ctx.event_bus, repo_id, Some(run_id)).await;
            return Err(error);
        }
    };
    spawn_result_watcher(
        ctx.db.clone(),
        ctx.event_bus.clone(),
        terminal_rx,
        event_rx,
        run.clone(),
        session.id,
        reservation.pr_urls,
    );
    emit_merge_changed(&ctx.event_bus, repo_id, Some(run.id)).await;
    Ok(run)
}

pub async fn resolve_merge_run(
    ctx: &AppContext,
    run_id: i64,
    status: &str,
) -> Result<MergeRun, CoreError> {
    if !matches!(status, "succeeded" | "failed") {
        return Err(CoreError::Other(
            "Manual merge resolution must be succeeded or failed".to_string(),
        ));
    }
    let run = {
        let mut conn = ctx
            .db
            .lock()
            .map_err(|error| CoreError::Other(error.to_string()))?;
        let tx = conn.transaction()?;
        let repo_id: i64 = tx
            .query_row(
                "SELECT repo_id FROM merge_runs WHERE id = ?1 AND status = 'needs_review'",
                [run_id],
                |row| row.get(0),
            )
            .map_err(|error| {
                CoreError::NotFound(format!(
                    "Merge run {run_id} is not awaiting review: {error}"
                ))
            })?;
        tx.execute(
            "UPDATE merge_queue_items SET status = ?1,
             result_message = 'Resolved manually', updated_at = datetime('now')
             WHERE run_id = ?2 AND status = 'needs_review'",
            rusqlite::params![status, run_id],
        )?;
        tx.execute(
            "UPDATE merge_runs SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![status, run_id],
        )?;
        let run = tx.query_row(
            &format!("{SELECT_MERGE_RUN} WHERE id = ?1"),
            [run_id],
            row_to_merge_run,
        )?;
        tx.commit()?;
        debug_assert_eq!(run.repo_id, repo_id);
        run
    };
    emit_merge_changed(&ctx.event_bus, run.repo_id, Some(run.id)).await;
    Ok(run)
}

fn requeue_merge_run(ctx: &AppContext, run_id: i64) -> Result<i64, CoreError> {
    let mut conn = ctx
        .db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;
    let tx = conn.transaction()?;
    let repo_id: i64 = tx
        .query_row(
            "SELECT repo_id FROM merge_runs WHERE id = ?1 AND status IN ('failed', 'needs_review')",
            [run_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            CoreError::NotFound(format!("Merge run {run_id} cannot be retried: {error}"))
        })?;
    let changed = tx.execute(
        "UPDATE merge_queue_items SET status = 'queued', run_id = NULL,
         result_message = NULL, updated_at = datetime('now')
         WHERE run_id = ?1 AND status IN ('failed', 'needs_review')",
        [run_id],
    )?;
    if changed == 0 {
        return Err(CoreError::Other(
            "Merge run has no failed or review items to retry".to_string(),
        ));
    }
    tx.commit()?;
    Ok(repo_id)
}

pub async fn retry_merge_run(ctx: &AppContext, run_id: i64) -> Result<MergeRun, CoreError> {
    let repo_id = requeue_merge_run(ctx, run_id)?;
    emit_merge_changed(&ctx.event_bus, repo_id, Some(run_id)).await;
    start_merge_run(ctx, repo_id).await
}

fn is_github_pr_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    let parts = rest.split('/').collect::<Vec<_>>();
    parts.len() == 4
        && !parts[0].is_empty()
        && !parts[1].is_empty()
        && parts[2] == "pull"
        && parts[3].parse::<u64>().is_ok()
}

pub async fn set_task_ready_to_merge(
    ctx: &AppContext,
    task_id: i64,
    ready: bool,
) -> Result<Option<MergeQueueItem>, CoreError> {
    let (item, repo_id, run_id) = set_task_ready_to_merge_db(ctx, task_id, ready)?;
    if let Some(repo_id) = repo_id {
        emit_merge_changed(&ctx.event_bus, repo_id, run_id).await;
    }
    Ok(item)
}

fn set_task_ready_to_merge_db(
    ctx: &AppContext,
    task_id: i64,
    ready: bool,
) -> Result<(Option<MergeQueueItem>, Option<i64>, Option<i64>), CoreError> {
    let conn = ctx
        .db
        .lock()
        .map_err(|error| CoreError::Other(error.to_string()))?;

    if !ready {
        let existing = conn
            .query_row(
                &format!("{SELECT_QUEUE_ITEM} WHERE task_id = ?1"),
                [task_id],
                row_to_queue_item,
            )
            .optional()?;
        let repo_id = existing.as_ref().map(|item| item.repo_id);
        if let Some(item) = existing {
            if item.status == "shipping" {
                return Err(CoreError::Other(
                    "Cannot remove a pull request while it is shipping".to_string(),
                ));
            }
            if item.status == "queued" {
                let active_count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM merge_runs
                     WHERE repo_id = ?1 AND status IN ('starting', 'shipping')",
                    [item.repo_id],
                    |row| row.get(0),
                )?;
                if active_count > 0 {
                    return Err(CoreError::Other(
                        "Cannot change the merge queue while a Merge Master run is active"
                            .to_string(),
                    ));
                }
                conn.execute("DELETE FROM merge_queue_items WHERE id = ?1", [item.id])?;
            }
        }
        return Ok((None, repo_id, None));
    }

    let (repo_id, status, session_id, pr_url): (i64, String, Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT tasks.repo_id, tasks.status, tasks.session_id, sessions.pr_url
             FROM tasks LEFT JOIN sessions ON sessions.id = tasks.session_id
             WHERE tasks.id = ?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|error| CoreError::NotFound(format!("Task {task_id} not found: {error}")))?;

    if status != "working" {
        return Err(CoreError::Other(
            "Only working tasks can be marked ready to merge".to_string(),
        ));
    }
    let session_id = session_id
        .ok_or_else(|| CoreError::Other("Working task has no linked session".to_string()))?;
    let pr_url = pr_url
        .filter(|url| is_github_pr_url(url))
        .ok_or_else(|| CoreError::Other("Task has no supported GitHub pull request".to_string()))?;

    if let Some(item) = conn
        .query_row(
            &format!("{SELECT_QUEUE_ITEM} WHERE repo_id = ?1 AND (pr_url = ?2 OR task_id = ?3)"),
            rusqlite::params![repo_id, pr_url, task_id],
            row_to_queue_item,
        )
        .optional()?
    {
        if item.task_id == task_id && item.pr_url != pr_url {
            return Err(CoreError::Other(
                "Task is already associated with a different pull request in Merge Manager"
                    .to_string(),
            ));
        }
        let repo_id = item.repo_id;
        let run_id = item.run_id;
        return Ok((Some(item), Some(repo_id), run_id));
    }

    conn.execute(
        "INSERT INTO merge_queue_items (repo_id, task_id, source_session_id, pr_url)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![repo_id, task_id, session_id, pr_url],
    )?;
    let id = conn.last_insert_rowid();
    let item = conn.query_row(
        &format!("{SELECT_QUEUE_ITEM} WHERE id = ?1"),
        [id],
        row_to_queue_item,
    )?;
    let repo_id = item.repo_id;
    let run_id = item.run_id;
    Ok((Some(item), Some(repo_id), run_id))
}

impl ShipResultParser {
    pub fn new(run_id: i64, allowed_urls: Vec<String>) -> Self {
        Self {
            run_id,
            allowed_urls: allowed_urls.into_iter().collect(),
            buffer: Vec::new(),
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Option<ShipResult>, String> {
        self.buffer.extend_from_slice(chunk);
        if self.buffer.len() > 131_072 {
            let keep_from = self.buffer.len() - 65_536;
            self.buffer.drain(..keep_from);
        }

        let text = crate::agent::strip_ansi(&self.buffer);
        let Some(prefix_at) = text.find(SHIP_RESULT_PREFIX) else {
            return Ok(None);
        };
        let json_start = prefix_at + SHIP_RESULT_PREFIX.len();
        let suffix = &text[json_start..];
        let Some(line_end) = suffix.find(['\r', '\n']) else {
            return Ok(None);
        };
        let json = suffix[..line_end].trim();
        let result: ShipResult = serde_json::from_str(json)
            .map_err(|error| format!("Invalid ship result JSON: {error}"))?;
        self.validate(&result)?;
        Ok(Some(result))
    }

    fn validate(&self, result: &ShipResult) -> Result<(), String> {
        if result.run_id != self.run_id {
            return Err(format!(
                "Ship result run_id {} does not match {}",
                result.run_id, self.run_id
            ));
        }
        if !matches!(result.status.as_str(), "succeeded" | "failed") {
            return Err(format!("Invalid ship result status: {}", result.status));
        }
        for url in result
            .merged_prs
            .iter()
            .chain(result.failed_prs.iter().map(|failed| &failed.url))
        {
            if !self.allowed_urls.contains(url) {
                return Err(format!("Ship result contains unknown PR URL: {url}"));
            }
        }
        let merged = result.merged_prs.iter().collect::<HashSet<_>>();
        if merged.len() != result.merged_prs.len() {
            return Err("Ship result contains a duplicate merged PR".to_string());
        }
        let failed = result
            .failed_prs
            .iter()
            .map(|entry| &entry.url)
            .collect::<HashSet<_>>();
        if failed.len() != result.failed_prs.len() {
            return Err("Ship result contains a duplicate failed PR".to_string());
        }
        if let Some(url) = merged.intersection(&failed).next() {
            return Err(format!(
                "Ship result reports PR as both merged and failed: {url}"
            ));
        }
        for test in &result.tests {
            if !matches!(test.status.as_str(), "passed" | "failed") {
                return Err(format!("Invalid test status: {}", test.status));
            }
        }
        Ok(())
    }
}

pub fn build_merge_prompt(
    run_id: i64,
    target_branch: &str,
    instructions: &str,
    pr_urls: &[String],
) -> String {
    let queue = pr_urls
        .iter()
        .enumerate()
        .map(|(index, url)| format!("{}. {}", index + 1, url))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "You are the Merge Master for Racc ship run {run_id}.\n\n\
Target branch: {target_branch}\n\
Pull requests, in required processing order:\n{queue}\n\n\
User ship instructions:\n{instructions}\n\n\
Required workflow:\n\
1. Work only in the current integration worktree. Fetch the pull request heads and combine them into the current integration branch in the listed order.\n\
2. If a pull request is already merged, skip it safely and report it accurately.\n\
3. If integration conflicts or batch tests fail, stop before starting any new remote merges.\n\
4. After the combined tree passes the requested tests, inspect the repository's allowed/default GitHub merge method and merge the pull requests into {target_branch} in the same order.\n\
5. Do not force push. Do not bypass branch protection. Do not weaken or skip required tests.\n\
6. Always finish by printing exactly one single-line result. Concatenate the token RACC_SHIP_RESULT, one colon character, and a valid compact JSON body with no whitespace between the token and colon. The JSON body must have this shape:\n\
{{\"run_id\":{run_id},\"status\":\"succeeded|failed\",\"merged_prs\":[\"url\"],\"failed_prs\":[{{\"url\":\"url\",\"reason\":\"reason\"}}],\"tests\":[{{\"command\":\"command\",\"status\":\"passed|failed\",\"summary\":\"optional summary\"}}],\"summary\":\"summary\"}}\n\
Use only URLs from the supplied list in merged_prs and failed_prs. Report partial remote merges exactly; never claim an atomic rollback."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::BroadcastEventBus;
    use crate::ssh::SshManager;
    use crate::transport::manager::TransportManager;
    use crate::AppContext;
    use std::sync::{Arc, Mutex};

    fn test_context() -> (AppContext, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("racc-merge-command-{}.db", uuid::Uuid::new_v4()));
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

    fn seed_working_task(ctx: &AppContext, pr_url: &str) -> i64 {
        let conn = ctx.db.lock().expect("database lock");
        conn.execute(
            "INSERT INTO repos (path, name) VALUES ('/tmp/widgets', 'widgets')",
            [],
        )
        .expect("repo insert");
        let repo_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO sessions (repo_id, branch, status, pr_url) VALUES (?1, 'feat/widget', 'Running', ?2)",
            rusqlite::params![repo_id, pr_url],
        )
        .expect("session insert");
        let session_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO tasks (repo_id, description, status, session_id) VALUES (?1, 'Build widget', 'working', ?2)",
            rusqlite::params![repo_id, session_id],
        )
        .expect("task insert");
        conn.last_insert_rowid()
    }

    #[test]
    fn merge_prompt_preserves_pr_order_and_fixed_contract() {
        let prompt = build_merge_prompt(
            42,
            "main",
            "Run the full regression suite.",
            &[
                "https://github.com/acme/widgets/pull/12".to_string(),
                "https://github.com/acme/widgets/pull/7".to_string(),
            ],
        );

        let first = prompt.find("pull/12").expect("first PR should be present");
        let second = prompt.find("pull/7").expect("second PR should be present");
        assert!(first < second, "queue order must be preserved");
        assert!(prompt.contains("Run the full regression suite."));
        assert!(prompt.contains("Target branch: main"));
        assert!(prompt.contains("RACC_SHIP_RESULT"));
        assert!(prompt.contains("\"run_id\":42"));
        assert!(prompt.contains("Do not bypass branch protection"));
    }

    #[test]
    fn merge_prompt_does_not_echo_the_literal_result_sentinel() {
        let prompt = build_merge_prompt(
            42,
            "main",
            "Run tests.",
            &["https://github.com/acme/widgets/pull/12".to_string()],
        );

        assert!(prompt.contains("RACC_SHIP_RESULT"));
        assert!(!prompt.contains(SHIP_RESULT_PREFIX));
    }

    #[test]
    fn result_parser_handles_ansi_and_split_terminal_chunks() {
        let url = "https://github.com/acme/widgets/pull/12".to_string();
        let mut parser = ShipResultParser::new(42, vec![url.clone()]);

        assert!(parser
            .push(b"\x1b[32mRACC_SHIP_RES")
            .expect("partial chunks should not fail")
            .is_none());

        let tail = format!(
            "ULT:{{\"run_id\":42,\"status\":\"succeeded\",\"merged_prs\":[\"{url}\"],\"failed_prs\":[],\"tests\":[{{\"command\":\"cargo test\",\"status\":\"passed\"}}],\"summary\":\"all good\"}}\x1b[0m\r\n"
        );
        let result = parser
            .push(tail.as_bytes())
            .expect("valid marker should parse")
            .expect("marker should be complete");

        assert_eq!(result.run_id, 42);
        assert_eq!(result.status, "succeeded");
        assert_eq!(result.merged_prs, vec![url]);
        assert!(result.failed_prs.is_empty());
        assert_eq!(result.tests[0].status, "passed");
    }

    #[test]
    fn result_parser_rejects_a_pr_reported_as_both_merged_and_failed() {
        let url = "https://github.com/acme/widgets/pull/12".to_string();
        let mut parser = ShipResultParser::new(42, vec![url.clone()]);
        let marker = format!(
            "RACC_SHIP_RESULT:{{\"run_id\":42,\"status\":\"failed\",\"merged_prs\":[\"{url}\"],\"failed_prs\":[{{\"url\":\"{url}\",\"reason\":\"blocked\"}}],\"tests\":[],\"summary\":\"ambiguous\"}}\n"
        );

        let error = parser
            .push(marker.as_bytes())
            .expect_err("overlap should be rejected");
        assert!(error.contains("both merged and failed"));
    }

    #[tokio::test]
    async fn ready_to_merge_is_idempotent_and_can_be_removed_while_queued() {
        let (ctx, path) = test_context();
        let task_id = seed_working_task(&ctx, "https://github.com/acme/widgets/pull/12");

        let first = set_task_ready_to_merge(&ctx, task_id, true)
            .await
            .expect("enqueue should succeed")
            .expect("enqueue should return an item");
        let second = set_task_ready_to_merge(&ctx, task_id, true)
            .await
            .expect("repeat enqueue should succeed")
            .expect("repeat enqueue should return the same item");

        assert_eq!(first.id, second.id);
        assert_eq!(first.status, "queued");
        let count: i64 = ctx
            .db
            .lock()
            .expect("database lock")
            .query_row("SELECT COUNT(*) FROM merge_queue_items", [], |row| {
                row.get(0)
            })
            .expect("queue count");
        assert_eq!(count, 1);

        let removed = set_task_ready_to_merge(&ctx, task_id, false)
            .await
            .expect("queued item should be removable");
        assert!(removed.is_none());

        drop(ctx);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn merge_settings_are_saved_per_repo_and_returned_with_queue() {
        let (ctx, path) = test_context();
        let task_id = seed_working_task(&ctx, "https://github.com/acme/widgets/pull/12");
        let repo_id: i64 = ctx
            .db
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT repo_id FROM tasks WHERE id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .expect("repo id");
        set_task_ready_to_merge(&ctx, task_id, true)
            .await
            .expect("enqueue should succeed");

        let saved = update_merge_settings(
            &ctx,
            repo_id,
            "release",
            "codex",
            "Run the release smoke tests.",
        )
        .await
        .expect("settings should save");
        let state = get_merge_manager(&ctx, repo_id).expect("manager state should load");

        assert_eq!(saved.target_branch, "release");
        assert_eq!(state.settings, saved);
        assert_eq!(state.items.len(), 1);
        assert!(state.active_run.is_none());

        drop(ctx);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn partial_ship_result_updates_each_queue_item_without_claiming_rollback() {
        let (ctx, path) = test_context();
        {
            let conn = ctx.db.lock().expect("database lock");
            conn.execute(
                "INSERT INTO repos (path, name) VALUES ('/tmp/widgets', 'widgets')",
                [],
            )
            .expect("repo insert");
            let repo_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO merge_runs (repo_id, target_branch, agent, prompt, status)
                 VALUES (?1, 'main', 'claude-code', 'ship', 'shipping')",
                [repo_id],
            )
            .expect("run insert");
            let run_id = conn.last_insert_rowid();
            for (task_id, url) in [
                (1, "https://github.com/acme/widgets/pull/1"),
                (2, "https://github.com/acme/widgets/pull/2"),
                (3, "https://github.com/acme/widgets/pull/3"),
            ] {
                conn.execute(
                    "INSERT INTO merge_queue_items
                     (repo_id, task_id, source_session_id, pr_url, status, run_id)
                     VALUES (?1, ?2, ?2, ?3, 'shipping', ?4)",
                    rusqlite::params![repo_id, task_id, url, run_id],
                )
                .expect("queue insert");
            }
        }

        let result = ShipResult {
            run_id: 1,
            status: "failed".to_string(),
            merged_prs: vec!["https://github.com/acme/widgets/pull/1".to_string()],
            failed_prs: vec![FailedPullRequest {
                url: "https://github.com/acme/widgets/pull/2".to_string(),
                reason: "branch protection blocked the merge".to_string(),
            }],
            tests: vec![],
            summary: "partial merge".to_string(),
        };
        apply_ship_result(&ctx, &result).expect("result should apply");

        let conn = ctx.db.lock().expect("database lock");
        let statuses = [1_i64, 2, 3]
            .into_iter()
            .map(|task_id| {
                conn.query_row(
                    "SELECT status FROM merge_queue_items WHERE task_id = ?1",
                    [task_id],
                    |row| row.get::<_, String>(0),
                )
                .expect("queue status")
            })
            .collect::<Vec<_>>();
        assert_eq!(statuses, vec!["succeeded", "failed", "needs_review"]);
        let run_status: String = conn
            .query_row("SELECT status FROM merge_runs WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("run status");
        assert_eq!(run_status, "failed");

        drop(conn);
        drop(ctx);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn reserving_a_run_builds_prompt_and_rejects_parallel_runs() {
        let (ctx, path) = test_context();
        let task_id = seed_working_task(&ctx, "https://github.com/acme/widgets/pull/12");
        let repo_id: i64 = ctx
            .db
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT repo_id FROM tasks WHERE id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .expect("repo id");
        update_merge_settings(&ctx, repo_id, "main", "claude-code", "Run tests.")
            .await
            .expect("settings save");
        set_task_ready_to_merge(&ctx, task_id, true)
            .await
            .expect("enqueue");

        let reservation = reserve_merge_run(&ctx, repo_id).expect("run should reserve");
        assert_eq!(reservation.run.status, "starting");
        assert_eq!(reservation.integration_branch, "racc/ship-1");
        assert_eq!(reservation.pr_urls.len(), 1);
        assert!(reservation.run.prompt.contains("\"run_id\":1"));

        let error = reserve_merge_run(&ctx, repo_id).expect_err("parallel run should fail");
        assert!(error.to_string().contains("already active"));

        drop(ctx);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn manual_resolution_only_changes_items_that_need_review() {
        let (ctx, path) = test_context();
        {
            let conn = ctx.db.lock().expect("database lock");
            conn.execute(
                "INSERT INTO repos (path, name) VALUES ('/tmp/widgets', 'widgets')",
                [],
            )
            .expect("repo insert");
            let repo_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO merge_runs (repo_id, target_branch, agent, prompt, status)
                 VALUES (?1, 'main', 'claude-code', 'ship', 'needs_review')",
                [repo_id],
            )
            .expect("run insert");
            let run_id = conn.last_insert_rowid();
            for (task_id, status) in [(1, "succeeded"), (2, "needs_review")] {
                conn.execute(
                    "INSERT INTO merge_queue_items
                     (repo_id, task_id, source_session_id, pr_url, status, run_id)
                     VALUES (?1, ?2, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        repo_id,
                        task_id,
                        format!("https://github.com/acme/widgets/pull/{task_id}"),
                        status,
                        run_id
                    ],
                )
                .expect("queue insert");
            }
        }

        let run = resolve_merge_run(&ctx, 1, "succeeded")
            .await
            .expect("manual resolution should succeed");
        assert_eq!(run.status, "succeeded");
        let statuses = ctx
            .db
            .lock()
            .expect("database lock")
            .prepare("SELECT status FROM merge_queue_items ORDER BY task_id")
            .expect("prepare")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        assert_eq!(statuses, vec!["succeeded", "succeeded"]);

        drop(ctx);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn loading_manager_recovers_orphaned_active_local_runs_as_needs_review() {
        let (ctx, path) = test_context();
        {
            let conn = ctx.db.lock().expect("database lock");
            conn.execute(
                "INSERT INTO repos (path, name) VALUES ('/tmp/widgets', 'widgets')",
                [],
            )
            .expect("repo insert");
            let repo_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO sessions (repo_id, status) VALUES (?1, 'Disconnected')",
                [repo_id],
            )
            .expect("session insert");
            let session_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO merge_runs
                 (repo_id, session_id, target_branch, agent, prompt, status)
                 VALUES (?1, ?2, 'main', 'claude-code', 'ship', 'shipping')",
                rusqlite::params![repo_id, session_id],
            )
            .expect("run insert");
            let run_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO merge_queue_items
                 (repo_id, task_id, source_session_id, pr_url, status, run_id)
                 VALUES (?1, 1, ?2, 'https://github.com/acme/widgets/pull/1', 'shipping', ?3)",
                rusqlite::params![repo_id, session_id, run_id],
            )
            .expect("queue insert");
        }

        let state = get_merge_manager(&ctx, 1).expect("manager should load");
        assert!(state.active_run.is_none());
        assert_eq!(state.last_run.expect("last run").status, "needs_review");
        assert_eq!(state.items[0].status, "needs_review");

        drop(ctx);
        let _ = std::fs::remove_file(path);
    }
}
