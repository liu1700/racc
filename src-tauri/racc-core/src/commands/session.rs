use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::agent;
use crate::AppContext;
use crate::error::CoreError;
use crate::events::RaccEvent;
use crate::transport::local_pty::LocalPtyTransport;
use crate::transport::manager::TransportManager;
use crate::rtk;

/// Type a task into an agent composer, then submit it with a distinct Enter
/// event. Keeping these as separate PTY writes is important: both Claude Code
/// and Codex can treat a single large `prompt + Enter` write as pasted text and
/// leave it sitting in the composer instead of starting the agent.
async fn inject_and_submit_task(
    transport_manager: &TransportManager,
    session_id: i64,
    agent_type: &agent::AgentType,
    task_description: &str,
) -> Result<(), crate::transport::TransportError> {
    let task_input = agent::inject_task_input(agent_type, task_description);
    transport_manager.write(session_id, &task_input).await?;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    // Agent TUIs in PTY raw mode expect carriage return for Enter.
    transport_manager.write(session_id, b"\r").await
}

// --- Types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Running,
    Completed,
    Disconnected,
    Error,
}

impl SessionStatus {
    fn from_str(s: &str) -> Self {
        match s {
            "Running" => Self::Running,
            "Completed" => Self::Completed,
            "Disconnected" => Self::Disconnected,
            _ => Self::Error,
        }
    }
}

/// Result of a silent `reconnect_session` attempt (serialized to the frontend
/// as a bare string, e.g. `"Reconnected"`).
#[derive(Debug, Clone, Serialize)]
pub enum ReconnectOutcome {
    /// The in-memory transport was already live — nothing was done.
    AlreadyLive,
    /// A dead remote transport was re-attached to its still-running tmux session.
    Reconnected,
    /// The caller should run the heavier `reattach_session` (local session that
    /// needs `claude --continue`, or a stopped session).
    FullReattach,
    /// The remote tmux session no longer exists; the session was marked Completed.
    Gone,
}

#[derive(Debug, Clone, Serialize)]
pub struct Repo {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: i64,
    pub repo_id: i64,
    pub agent: String,
    pub worktree_path: Option<String>,
    pub branch: Option<String>,
    pub status: SessionStatus,
    pub created_at: String,
    pub updated_at: String,
    pub pr_url: Option<String>,
    pub server_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoWithSessions {
    pub repo: Repo,
    pub sessions: Vec<Session>,
}

// --- Helper: query repos with sessions ---

fn query_repos_with_sessions(conn: &Connection) -> Result<Vec<RepoWithSessions>, CoreError> {
    let mut repo_stmt = conn.prepare("SELECT id, path, name, added_at FROM repos ORDER BY name")?;

    let repos: Vec<Repo> = repo_stmt
        .query_map([], |row| {
            Ok(Repo {
                id: row.get(0)?,
                path: row.get(1)?,
                name: row.get(2)?,
                added_at: row.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut session_stmt = conn.prepare(
        "SELECT id, repo_id, agent, worktree_path, branch, status, created_at, updated_at, pr_url, server_id
         FROM sessions WHERE repo_id = ? ORDER BY created_at DESC",
    )?;

    let mut result = Vec::new();
    for repo in repos {
        let sessions: Vec<Session> = session_stmt
            .query_map([repo.id], |row| {
                let status_str: String = row.get(5)?;
                Ok(Session {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    agent: row.get(2)?,
                    worktree_path: row.get(3)?,
                    branch: row.get(4)?,
                    status: SessionStatus::from_str(&status_str),
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    pr_url: row.get(8)?,
                    server_id: row.get(9)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        result.push(RepoWithSessions { repo, sessions });
    }

    Ok(result)
}

// --- Helper: get current git branch ---

fn get_current_branch(repo_path: &str) -> Result<String, CoreError> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| CoreError::Git(format!("Failed to get branch: {e}")))?;

    if !output.status.success() {
        return Err(CoreError::Git(
            "Failed to detect current branch".to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn build_worktree_add_args(
    worktree_path: &str,
    branch: &str,
    base_ref: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "worktree".to_string(),
        "add".to_string(),
        worktree_path.to_string(),
        "-b".to_string(),
        branch.to_string(),
    ];
    if let Some(base_ref) = base_ref {
        args.push(base_ref.to_string());
    }
    args
}

// --- Commands ---

pub async fn import_repo(
    ctx: &AppContext,
    path: String,
) -> Result<Repo, CoreError> {
    let git_dir = std::path::Path::new(&path).join(".git");
    if !git_dir.exists() {
        return Err(CoreError::Git("Not a git repository".to_string()));
    }

    let name = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;

    conn.execute(
        "INSERT INTO repos (path, name) VALUES (?1, ?2)",
        rusqlite::params![path, name],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            CoreError::Other("Repository already imported".to_string())
        } else {
            CoreError::Db(e)
        }
    })?;

    let id = conn.last_insert_rowid();
    let added_at: String = conn.query_row(
        "SELECT added_at FROM repos WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;

    Ok(Repo {
        id,
        path,
        name,
        added_at,
    })
}

pub async fn list_repos(
    ctx: &AppContext,
) -> Result<Vec<RepoWithSessions>, CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    query_repos_with_sessions(&conn)
}

pub async fn remove_repo(
    ctx: &AppContext,
    repo_id: i64,
) -> Result<(), CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;

    let running_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE repo_id = ?1 AND status = 'Running'",
        [repo_id],
        |row| row.get(0),
    )?;

    if running_count > 0 {
        return Err(CoreError::Other(
            "Cannot remove repo with running sessions. Stop them first.".to_string(),
        ));
    }

    conn.execute("DELETE FROM repos WHERE id = ?1", [repo_id])?;

    Ok(())
}

pub async fn create_session(
    ctx: &AppContext,
    repo_id: i64,
    use_worktree: bool,
    branch: Option<String>,
    agent: Option<String>,
    task_description: Option<String>,
    server_id: Option<String>,
    skip_permissions: Option<bool>,
) -> Result<Session, CoreError> {
    create_session_from_base(
        ctx,
        repo_id,
        use_worktree,
        branch,
        agent,
        task_description,
        server_id,
        skip_permissions,
        None,
    )
    .await
}

pub(crate) async fn create_session_from_base(
    ctx: &AppContext,
    repo_id: i64,
    use_worktree: bool,
    branch: Option<String>,
    agent: Option<String>,
    task_description: Option<String>,
    server_id: Option<String>,
    skip_permissions: Option<bool>,
    base_ref: Option<String>,
) -> Result<Session, CoreError> {
    if base_ref.is_some() && server_id.is_some() {
        return Err(CoreError::Other(
            "Explicit worktree base refs are only supported for local sessions".to_string(),
        ));
    }
    let agent = agent.unwrap_or_else(|| "claude-code".to_string());
    let task_description = task_description.unwrap_or_default();
    let skip_permissions = skip_permissions.unwrap_or(false);

    // Pin the claude conversation's session ID at spawn time (issue #70):
    // launching with `--session-id <uuid>` and persisting the uuid lets
    // reattach deterministically `--resume <uuid>` instead of betting that
    // `--continue` picks the right conversation for the cwd. NULL for agents
    // with no resume-by-id concept.
    let agent_session_id = agent::new_agent_session_id(&agent);

    let (repo_path, repo_name) = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        let row: (String, String) = conn
            .query_row(
                "SELECT path, name FROM repos WHERE id = ?1",
                [repo_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| CoreError::NotFound(format!("Repo not found: {e}")))?;
        row
    };

    let (worktree_path, branch_name) = if use_worktree {
        let branch = branch.ok_or_else(|| {
            CoreError::Other("Branch name required for worktree".to_string())
        })?;
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .ok_or_else(|| CoreError::Other("Could not find home directory".to_string()))?;
        let worktree_leaf = if base_ref.is_some() {
            branch.replace('/', "-")
        } else {
            branch.clone()
        };
        let wt_dir = home
            .join("racc-worktrees")
            .join(&repo_name)
            .join(worktree_leaf);

        let wt_path = wt_dir.to_string_lossy().to_string();

        if base_ref.is_some() && wt_dir.exists() {
            return Err(CoreError::Git(format!(
                "Merge worktree already exists at {wt_path}; remove the stale session/worktree before retrying"
            )));
        }

        if !wt_dir.exists() {
            std::fs::create_dir_all(wt_dir.parent().unwrap())?;

            let add_args = build_worktree_add_args(&wt_path, &branch, base_ref.as_deref());
            let output = Command::new("git")
                .args(&add_args)
                .current_dir(&repo_path)
                .output()
                .map_err(|e| CoreError::Git(format!("git worktree add failed: {e}")))?;

            if !output.status.success() {
                if base_ref.is_some() {
                    return Err(CoreError::Git(format!(
                        "git worktree add failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    )));
                }
                let output2 = Command::new("git")
                    .args(["worktree", "add", &wt_path, &branch])
                    .current_dir(&repo_path)
                    .output()
                    .map_err(|e| CoreError::Git(format!("git worktree add failed: {e}")))?;

                if !output2.status.success() {
                    return Err(CoreError::Git(format!(
                        "git worktree add failed: {}",
                        String::from_utf8_lossy(&output2.stderr)
                    )));
                }
            }
        }

        (Some(wt_path), branch)
    } else {
        let branch = get_current_branch(&repo_path)?;
        (None, branch)
    };

    let (session_id, worktree_path_clone, created_at, updated_at) = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        conn.execute(
            "INSERT INTO sessions (repo_id, agent, worktree_path, branch, status, server_id, agent_session_id)
             VALUES (?1, ?2, ?3, ?4, 'Running', ?5, ?6)",
            rusqlite::params![repo_id, agent, worktree_path, branch_name, server_id, agent_session_id],
        )?;

        let id = conn.last_insert_rowid();
        let (created_at, updated_at): (String, String) = conn.query_row(
            "SELECT created_at, updated_at FROM sessions WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        (id, worktree_path.clone(), created_at, updated_at)
    }; // conn lock released here

    // Subscribe to terminal output BEFORE spawning the agent so the task
    // injector never misses the agent's ready prompt (it can appear immediately
    // when there's no trust dialog). Set inside each branch right before launch;
    // the shared injector below consumes it. Applies to BOTH local and remote.
    let mut injector_rx: Option<tokio::sync::broadcast::Receiver<crate::TerminalData>> = None;

    if let Some(ref sid) = server_id {
        // Remote session: clone repo if needed, create worktree, spawn SshTmuxTransport.
        // Ensure a live SSH connection first — setup/test both disconnect when done.
        crate::commands::server::ensure_connected(&ctx, sid).await?;

        let remote_repo_path = format!("~/racc-repos/{}", repo_name);

        // Check if repo exists on remote
        let check = ctx
            .ssh_manager
            .exec(
                sid,
                &format!(
                    "test -d {} && echo exists || echo missing",
                    remote_repo_path
                ),
            )
            .await
            .map_err(|e| CoreError::Ssh(format!("Failed to check remote repo: {}", e)))?;

        if check.stdout.trim() == "missing" {
            // Get repo URL from local repo's origin remote
            let repo_path_str = repo_path.clone();
            let url_output = Command::new("git")
                .args(["-C", &repo_path_str, "remote", "get-url", "origin"])
                .output()
                .map_err(|e| CoreError::Git(format!("Failed to get repo URL: {}", e)))?;
            let repo_url = String::from_utf8_lossy(&url_output.stdout)
                .trim()
                .to_string();

            if repo_url.is_empty() {
                return Err(CoreError::Other(
                    "No origin remote URL found for this repository".to_string(),
                ));
            }

            ctx.ssh_manager
                .exec(
                    sid,
                    &format!(
                        "mkdir -p ~/racc-repos && git clone {} {}",
                        repo_url, remote_repo_path
                    ),
                )
                .await
                .map_err(|e| CoreError::Ssh(format!("Failed to clone repo on remote: {}", e)))?;
        }

        // Create worktree on remote
        let remote_worktree = format!("~/racc-worktrees/{}/{}", repo_name, branch_name);
        let _ = ctx
            .ssh_manager
            .exec(sid, &format!(
                "mkdir -p ~/racc-worktrees/{} && (git -C {} worktree add {} -b racc-{} 2>/dev/null || git -C {} worktree add {} racc-{} 2>/dev/null || true)",
                repo_name,
                remote_repo_path, remote_worktree, session_id,
                remote_repo_path, remote_worktree, session_id
            ))
            .await;

        // RTK setup for remote Claude Code sessions
        let rtk_remote = if agent == "claude-code" {
            crate::rtk::ensure_rtk_remote(&ctx.ssh_manager, sid).await
        } else {
            false
        };

        // Spawn SshTmuxTransport. build_command ignores cwd, so cd into the
        // worktree first — otherwise the agent runs in $HOME and operates on the
        // wrong files (and triggers a trust prompt for $HOME instead).
        let agent_cmd = format!(
            "cd {} && {}",
            remote_worktree,
            agent::build_command(
                &agent,
                &remote_worktree,
                skip_permissions,
                rtk_remote,
                agent_session_id.as_deref()
            )
        );
        // Subscribe right before spawn so we capture the agent's first output
        // (the tmux attach repaint already shows the ready prompt).
        if !task_description.is_empty() {
            injector_rx = Some(ctx.terminal_tx.subscribe());
        }
        let transport = crate::transport::ssh_tmux::SshTmuxTransport::spawn(
            session_id,
            sid,
            &agent_cmd,
            80,
            24,
            ctx.ssh_manager.clone(),
            ctx.terminal_tx.clone(),
            ctx.transport_manager.buffer_sender(),
        )
        .await
        .map_err(|e| CoreError::Transport(e.to_string()))?;
        ctx.transport_manager
            .insert(session_id, Box::new(transport))
            .await;
    } else {
        // Local session: spawn LocalPtyTransport
        let cwd = worktree_path_clone.as_deref().unwrap_or(&repo_path);
        let agent_cmd =
            agent::build_command(&agent, cwd, skip_permissions, false, agent_session_id.as_deref());

        // RTK setup for Claude Code sessions
        let extra_env = if agent == "claude-code" {
            let rtk_available = rtk::ensure_rtk_local().await;
            if rtk_available {
                rtk::rtk_path_env().map(|p| {
                    let mut env = std::collections::HashMap::new();
                    env.insert("PATH".to_string(), p);
                    env
                })
            } else {
                None
            }
        } else {
            None
        };

        let transport = LocalPtyTransport::spawn(
            session_id,
            cwd,
            "/bin/zsh",
            80,
            24, // default size, frontend will resize
            ctx.terminal_tx.clone(),
            ctx.transport_manager.buffer_sender(),
            extra_env,
        )
        .await
        .map_err(|e| CoreError::Transport(e.to_string()))?;
        ctx.transport_manager
            .insert(session_id, Box::new(transport))
            .await;

        // Subscribe before sending the launch command so the injector captures
        // the agent's startup output.
        if !task_description.is_empty() {
            injector_rx = Some(ctx.terminal_tx.subscribe());
        }

        // Send agent launch command after short delay to let shell initialize
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        ctx.transport_manager
            .write(session_id, agent_cmd.as_bytes())
            .await
            .map_err(|e| CoreError::Transport(e.to_string()))?;
    }

    // Send the task once the agent is truly at its input prompt. Shared by local
    // AND remote sessions. We watch the PTY output and: (1) auto-accept the
    // first-run "trust this folder" dialog, then (2) inject the task only when
    // the real prompt is shown. Matching the dialog explicitly avoids typing the
    // task into it (the dialog renders a `❯` too, which would trigger a premature
    // inject that the dialog swallows, leaving the agent idle at an empty prompt).
    if let Some(mut rx) = injector_rx {
        let transport_manager = ctx.transport_manager.clone();
        let agent_clone = agent.clone();
        let task_desc = task_description.clone();
        tokio::spawn(async move {
            let agent_type = agent::AgentType::from_agent_str(&agent_clone);
            let timeout = tokio::time::sleep(std::time::Duration::from_secs(120));
            tokio::pin!(timeout);
            let mut buffer = Vec::new();
            let mut trust_handled = false;
            loop {
                tokio::select! {
                    result = rx.recv() => {
                        match result {
                            Ok(data) if data.session_id == session_id => {
                                buffer.extend_from_slice(&data.data);
                                let text = agent::strip_ansi(&buffer);

                                // (1) Auto-accept the workspace trust dialog once.
                                if !trust_handled && agent::is_trust_dialog(&text) {
                                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                                    // Enter confirms the pre-selected "Yes, I trust".
                                    if let Err(error) = transport_manager.write(session_id, b"\r").await {
                                        log::warn!("Failed to accept trust dialog for session {session_id}: {error}");
                                    }
                                    trust_handled = true;
                                    buffer.clear();
                                    continue;
                                }

                                // (2) Inject the task at the real input prompt.
                                if agent::is_agent_ready(&agent_type, &text) {
                                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                    if let Err(error) = inject_and_submit_task(
                                        &transport_manager,
                                        session_id,
                                        &agent_type,
                                        &task_desc,
                                    ).await {
                                        log::warn!("Failed to inject task into session {session_id}: {error}");
                                    }
                                    break;
                                }

                                // Keep buffer manageable.
                                if buffer.len() > 8192 {
                                    buffer.drain(..4096);
                                }
                            }
                            Ok(_) => {} // different session
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                            Err(_) => break,
                        }
                    }
                    _ = &mut timeout => {
                        // Timeout: send anyway as fallback
                        log::warn!("Timed out waiting for agent prompt, sending task anyway");
                        if let Err(error) = inject_and_submit_task(
                            &transport_manager,
                            session_id,
                            &agent_type,
                            &task_desc,
                        ).await {
                            log::warn!("Failed to inject fallback task into session {session_id}: {error}");
                        }
                        break;
                    }
                }
            }
        });
    }

    let session = Session {
        id: session_id,
        repo_id,
        agent,
        worktree_path,
        branch: Some(branch_name),
        status: SessionStatus::Running,
        created_at,
        updated_at,
        pr_url: None,
        server_id,
    };

    ctx.event_bus
        .emit(RaccEvent::SessionStatusChanged {
            session_id: session.id,
            status: "Running".to_string(),
            pr_url: None,
            source: "local".to_string(),
        })
        .await;

    Ok(session)
}

pub async fn stop_session(
    ctx: &AppContext,
    session_id: i64,
) -> Result<(), CoreError> {
    // Close transport before updating DB. For a tracked transport this also
    // kills the remote tmux session (SshTmuxTransport::close runs kill-session).
    let _ = ctx.transport_manager.remove(session_id).await;

    // Look up the session's server (if remote).
    let server_id: Option<String> = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        conn.query_row(
            "SELECT server_id FROM sessions WHERE id = ?1",
            [session_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten()
    };

    // For remote sessions, kill the tmux session by name directly too. This
    // covers the case where the transport is no longer tracked in memory (e.g.
    // after an app restart) — without it the remote agent would be orphaned and
    // keep running. Best-effort: ignore errors (session may already be gone).
    if let Some(sid) = server_id {
        if crate::commands::server::ensure_connected(ctx, &sid).await.is_ok() {
            let kill_cmd = format!("tmux kill-session -t racc-{}", session_id);
            let _ = ctx.ssh_manager.exec(&sid, &kill_cmd).await;
        }
    }

    {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        conn.execute(
            "UPDATE sessions SET status = 'Completed', updated_at = datetime('now') WHERE id = ?1",
            [session_id],
        )?;
    }

    ctx.event_bus
        .emit(RaccEvent::SessionStatusChanged {
            session_id,
            status: "Completed".to_string(),
            pr_url: None,
            source: "local".to_string(),
        })
        .await;

    Ok(())
}

pub async fn remove_session(
    ctx: &AppContext,
    session_id: i64,
    delete_worktree: bool,
) -> Result<(), CoreError> {
    // Close transport if still running (kills the remote tmux if tracked).
    let _ = ctx.transport_manager.remove(session_id).await;

    // Read everything we need, then release the lock before any async/SSH work.
    let (worktree_path, repo_path, server_id): (Option<String>, Option<String>, Option<String>) = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        let (worktree_path, repo_id, server_id): (Option<String>, i64, Option<String>) = conn
            .query_row(
                "SELECT worktree_path, repo_id, server_id FROM sessions WHERE id = ?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|e| CoreError::NotFound(format!("Session not found: {e}")))?;
        let repo_path: Option<String> = conn
            .query_row("SELECT path FROM repos WHERE id = ?1", [repo_id], |row| {
                row.get(0)
            })
            .ok();
        (worktree_path, repo_path, server_id)
    };

    // For remote sessions, kill the tmux session by name too — the transport may
    // not be tracked (e.g. after an app restart), so removing it from Racc would
    // otherwise leave the remote agent running orphaned. Best-effort.
    if let Some(sid) = server_id {
        if crate::commands::server::ensure_connected(ctx, &sid).await.is_ok() {
            let kill_cmd = format!("tmux kill-session -t racc-{}", session_id);
            let _ = ctx.ssh_manager.exec(&sid, &kill_cmd).await;
        }
    }

    // Remove worktree via git if requested (local worktrees only). Best-effort:
    // a failure here (e.g. the folder was already deleted manually, or the
    // worktree is locked) must NOT block removing the session from Racc —
    // otherwise the session record survives and reappears in the UI.
    if delete_worktree {
        if let (Some(wt_path), Some(repo_path)) = (&worktree_path, &repo_path) {
            match Command::new("git")
                .args(["worktree", "remove", wt_path, "--force"])
                .current_dir(repo_path)
                .output()
            {
                Ok(output) if output.status.success() => {}
                Ok(output) => {
                    log::warn!(
                        "git worktree remove failed for session {session_id}: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    );
                    // Prune the stale registration so `git worktree list` and
                    // future worktree adds don't trip over the leftover entry.
                    let _ = Command::new("git")
                        .args(["worktree", "prune"])
                        .current_dir(repo_path)
                        .output();
                }
                Err(e) => {
                    log::warn!("Failed to run git worktree remove for session {session_id}: {e}");
                }
            }
        }
    }

    {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", [session_id])?;
    }

    Ok(())
}

pub async fn reattach_session(
    ctx: &AppContext,
    session_id: i64,
) -> Result<Session, CoreError> {
    let (status, worktree_path, repo_id): (String, Option<String>, i64) = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        conn.query_row(
            "SELECT status, worktree_path, repo_id FROM sessions WHERE id = ?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| CoreError::NotFound(format!("Session not found: {e}")))?
    }; // lock released before the async liveness probe

    // Refuse only when the session is genuinely live. A stale 'Running' row
    // with no transport behind it (app killed while sessions ran, before
    // reconciliation corrected the DB) must not block recovery — that stale
    // state is exactly what reattach exists to fix (issue #70).
    if status == "Running" && ctx.transport_manager.is_alive(session_id).await {
        return Err(CoreError::Other(
            "Session is already running".to_string(),
        ));
    }

    // Verify worktree still exists if this is a worktree session
    if let Some(ref wt_path) = worktree_path {
        if !std::path::Path::new(wt_path).exists() {
            return Err(CoreError::NotFound(format!(
                "Worktree directory no longer exists: {wt_path}"
            )));
        }
    }

    let (agent, branch, created_at, updated_at, pr_url, server_id, agent_session_id, repo_path) = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;

        conn.execute(
            "UPDATE sessions SET status = 'Running', updated_at = datetime('now') WHERE id = ?1",
            [session_id],
        )?;

        let (agent, branch, created_at, updated_at, pr_url, server_id, agent_session_id): (String, Option<String>, String, String, Option<String>, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT agent, branch, created_at, updated_at, pr_url, server_id, agent_session_id FROM sessions WHERE id = ?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
            )?;

        let repo_path: String = conn
            .query_row("SELECT path FROM repos WHERE id = ?1", [repo_id], |row| row.get(0))
            .map_err(|e| CoreError::NotFound(format!("Repo not found: {e}")))?;

        (agent, branch, created_at, updated_at, pr_url, server_id, agent_session_id, repo_path)
    }; // DB lock released here -- safe for async transport work below

    if let Some(ref sid) = server_id {
        // Remote session: reattach to existing tmux session via SSH.
        // Ensure a live SSH connection first — it may have dropped since launch.
        crate::commands::server::ensure_connected(&ctx, sid).await?;

        let existing = ctx.transport_manager.is_alive(session_id).await;
        if existing {
            let _ = ctx.transport_manager.remove(session_id).await;
        }

        // Probe tmux AFTER the remove above (removing a tracked transport
        // kills its tmux session), so this reflects what spawn will find.
        // If tmux is gone, spawn recreates it and actually RUNS the resume
        // command — watch its outcome like the local path so a failed resume
        // surfaces as Error, not phantom Running. If tmux is alive we only
        // re-attach: no resume runs, and the scrollback repaint could carry
        // stale failure text that would false-flag a live session, so don't
        // watch. Probe errors count as "alive" (conservative: never false-flag
        // over a flaky SSH link).
        let tmux_gone = ctx
            .ssh_manager
            .exec(sid, &format!("tmux has-session -t racc-{}", session_id))
            .await
            .map(|out| out.exit_code != 0)
            .unwrap_or(false);
        if tmux_gone && agent == "claude-code" {
            spawn_resume_watcher(ctx, session_id);
        }

        let repo_name = std::path::Path::new(&repo_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let remote_worktree = format!(
            "~/racc-worktrees/{}/{}",
            repo_name,
            branch.as_deref().unwrap_or("main")
        );
        let rtk_remote = if agent == "claude-code" {
            crate::rtk::ensure_rtk_remote(&ctx.ssh_manager, sid).await
        } else {
            false
        };

        // cd into the worktree first (the command ignores cwd); only executed
        // if tmux has to recreate the session — in that case the original
        // claude process is gone, so resume the recorded conversation instead
        // of starting a fresh one.
        let agent_cmd = format!(
            "cd {} && {}",
            remote_worktree,
            agent::build_resume_command(&agent, agent_session_id.as_deref(), rtk_remote)
        );
        let transport = crate::transport::ssh_tmux::SshTmuxTransport::spawn(
            session_id,
            sid,
            &agent_cmd,
            80,
            24,
            ctx.ssh_manager.clone(),
            ctx.terminal_tx.clone(),
            ctx.transport_manager.buffer_sender(),
        )
        .await
        .map_err(|e| CoreError::Transport(e.to_string()))?;
        ctx.transport_manager
            .insert(session_id, Box::new(transport))
            .await;
    } else {
        // Local session: spawn a new PTY and resume the recorded conversation.
        let cwd = worktree_path.as_deref().unwrap_or(&repo_path);

        // RTK setup for Claude Code sessions
        let extra_env = if agent == "claude-code" {
            let rtk_available = rtk::ensure_rtk_local().await;
            if rtk_available {
                rtk::rtk_path_env().map(|p| {
                    let mut env = std::collections::HashMap::new();
                    env.insert("PATH".to_string(), p);
                    env
                })
            } else {
                None
            }
        } else {
            None
        };

        let transport = LocalPtyTransport::spawn(
            session_id,
            cwd,
            "/bin/zsh",
            80,
            24,
            ctx.terminal_tx.clone(),
            ctx.transport_manager.buffer_sender(),
            extra_env,
        )
        .await
        .map_err(|e| CoreError::Transport(e.to_string()))?;
        ctx.transport_manager
            .insert(session_id, Box::new(transport))
            .await;

        // For claude-code, resume the exact recorded conversation
        // (`--resume <uuid>`); legacy rows without one fall back to
        // `--continue`. Other agents are simply relaunched.
        let resume_cmd = agent::build_resume_command(&agent, agent_session_id.as_deref(), false);

        // Watch the resume outcome (subscribe BEFORE typing the command so no
        // output is missed): if claude reports "No conversation found" — the
        // transcript was never persisted or was deleted — flip the session to
        // Error so the user sees a dead session instead of a phantom
        // "Running" one sitting at a bare shell prompt (issue #70).
        if agent == "claude-code" {
            spawn_resume_watcher(ctx, session_id);
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        ctx.transport_manager
            .write(session_id, resume_cmd.as_bytes())
            .await
            .map_err(|e| CoreError::Transport(e.to_string()))?;
    }

    let session = Session {
        id: session_id,
        repo_id,
        agent,
        worktree_path,
        branch,
        status: SessionStatus::Running,
        created_at,
        updated_at,
        pr_url,
        server_id,
    };

    ctx.event_bus
        .emit(RaccEvent::SessionStatusChanged {
            session_id: session.id,
            status: "Running".to_string(),
            pr_url: None,
            source: "local".to_string(),
        })
        .await;

    Ok(session)
}

/// Watch a claude-code session that was just issued a resume command (local
/// PTY reattach, or a remote reattach whose tmux session had to be recreated)
/// for the resume outcome.
///
/// `claude --resume <uuid>` / `claude --continue` print "No conversation
/// found ..." and exit when the transcript is missing (e.g. claude was killed
/// before its first persistence — issue #70). Without this check the session
/// would sit at a dead shell prompt marked Running forever. On failure the
/// session is flipped to Error and listeners are notified; once the agent
/// reaches its ready prompt (resume worked) the watcher just exits.
fn spawn_resume_watcher(ctx: &AppContext, session_id: i64) {
    let mut rx = ctx.terminal_tx.subscribe();
    let db = ctx.db.clone();
    let event_bus = ctx.event_bus.clone();
    tokio::spawn(async move {
        // Failure prints within a couple of seconds; a successful resume shows
        // the TUI prompt well before this. Past the deadline, assume healthy.
        let timeout = tokio::time::sleep(std::time::Duration::from_secs(30));
        tokio::pin!(timeout);
        let mut buffer = Vec::new();
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(data) if data.session_id == session_id => {
                            buffer.extend_from_slice(&data.data);
                            let text = agent::strip_ansi(&buffer);

                            // Ready prompt first: a successful resume repaints
                            // the old transcript, which could itself contain
                            // the failure marker text.
                            if agent::is_agent_ready(&agent::AgentType::ClaudeCode, &text) {
                                break;
                            }

                            if agent::is_resume_failure(&text) {
                                log::warn!(
                                    "Session {session_id}: claude found no conversation to resume; marking Error"
                                );
                                // Guard is consumed inside the closure, so the
                                // lock is released before the await below.
                                if db.lock().is_ok_and(|conn| {
                                    conn.execute(
                                        "UPDATE sessions SET status = 'Error', updated_at = datetime('now') WHERE id = ?1",
                                        [session_id],
                                    )
                                    .is_ok()
                                }) {
                                    event_bus
                                        .emit(RaccEvent::SessionStatusChanged {
                                            session_id,
                                            status: "Error".to_string(),
                                            pr_url: None,
                                            source: "local".to_string(),
                                        })
                                        .await;
                                }
                                break;
                            }

                            // Keep buffer manageable.
                            if buffer.len() > 16384 {
                                buffer.drain(..8192);
                            }
                        }
                        Ok(_) => {} // different session
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => break,
                    }
                }
                _ = &mut timeout => break,
            }
        }
    });
}

/// Silently bring a session's transport back to life when it was lost while the
/// app or laptop was idle (e.g. the SSH connection dropped after a long sleep).
///
/// Called on every session "open" (clicking a task/session). It is idempotent
/// and **never tears down a healthy session**:
///   - In-memory transport already alive → [`ReconnectOutcome::AlreadyLive`] (no-op).
///   - Remote session whose tmux is still running on the server → drop the dead
///     in-memory transport WITHOUT killing tmux, then re-attach a fresh SSH
///     channel to the SAME tmux session → [`ReconnectOutcome::Reconnected`].
///   - Remote session whose tmux is gone → mark `Completed` → [`ReconnectOutcome::Gone`].
///   - Local session that isn't live (e.g. after an app restart) →
///     [`ReconnectOutcome::FullReattach`] so the caller runs `reattach_session`.
///
/// Unlike `reattach_session`, this does not refuse to act on a "Running" session
/// — that status is exactly what a slept-then-woken remote session still shows
/// while its transport is dead.
pub async fn reconnect_session(
    ctx: &AppContext,
    session_id: i64,
) -> Result<ReconnectOutcome, CoreError> {
    // A genuinely live transport must never be disturbed (re-attaching would
    // be wasteful and, worse, the tmux teardown path could kill a live agent).
    if ctx.transport_manager.is_alive(session_id).await {
        return Ok(ReconnectOutcome::AlreadyLive);
    }

    let (server_id, agent, branch, repo_path, agent_session_id) = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        let (server_id, repo_id, agent, branch, agent_session_id): (Option<String>, i64, String, Option<String>, Option<String>) =
            conn.query_row(
                "SELECT server_id, repo_id, agent, branch, agent_session_id FROM sessions WHERE id = ?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .map_err(|e| CoreError::NotFound(format!("Session not found: {e}")))?;
        let repo_path: String = conn
            .query_row("SELECT path FROM repos WHERE id = ?1", [repo_id], |row| row.get(0))
            .map_err(|e| CoreError::NotFound(format!("Repo not found: {e}")))?;
        (server_id, agent, branch, repo_path, agent_session_id)
    };

    // Local sessions can't be silently re-attached — the PTY lived in-process and
    // is gone. Defer to the full reattach (`claude --continue`).
    let sid = match server_id {
        Some(sid) => sid,
        None => return Ok(ReconnectOutcome::FullReattach),
    };

    // Remote: ensure a live SSH connection (it dropped while we slept), then
    // check whether the tmux session is still there to attach to.
    crate::commands::server::ensure_connected(ctx, &sid).await?;

    let tmux_name = format!("racc-{}", session_id);
    let probe = format!("tmux has-session -t {}", tmux_name);
    let has = match ctx.ssh_manager.exec(&sid, &probe).await {
        Ok(out) => out,
        Err(_) => {
            // `is_connected` only tracks a status flag that russh does NOT clear
            // when the TCP link dies on sleep — so `ensure_connected` may have
            // handed back a stale, dead handle. Drop it, reconnect fresh, retry.
            let _ = ctx.ssh_manager.disconnect(&sid).await;
            crate::commands::server::ensure_connected(ctx, &sid).await?;
            ctx.ssh_manager
                .exec(&sid, &probe)
                .await
                .map_err(|e| CoreError::Ssh(format!("Failed to probe tmux session: {}", e)))?
        }
    };

    if has.exit_code != 0 {
        // The remote tmux session is gone (server rebooted, OOM-killed, manually
        // killed, …). Reflect that as Completed; there's nothing to attach to.
        {
            let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
            conn.execute(
                "UPDATE sessions SET status = 'Completed', updated_at = datetime('now') WHERE id = ?1",
                [session_id],
            )?;
        }
        ctx.event_bus
            .emit(RaccEvent::SessionStatusChanged {
                session_id,
                status: "Completed".to_string(),
                pr_url: None,
                source: "local".to_string(),
            })
            .await;
        return Ok(ReconnectOutcome::Gone);
    }

    // tmux is alive → drop the dead in-memory transport WITHOUT killing tmux,
    // then re-attach a fresh SSH channel to the same session.
    ctx.transport_manager.discard(session_id).await;

    let repo_name = std::path::Path::new(&repo_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let remote_worktree = format!(
        "~/racc-worktrees/{}/{}",
        repo_name,
        branch.as_deref().unwrap_or("main")
    );
    let rtk_remote = if agent == "claude-code" {
        crate::rtk::ensure_rtk_remote(&ctx.ssh_manager, &sid).await
    } else {
        false
    };
    // The command ignores cwd; cd first so behaviour matches the create/reattach
    // paths. For a plain `tmux attach` this command body is never executed — it
    // only runs if tmux died between the probe above and this spawn, in which
    // case the original claude is gone and resuming is the right move.
    let agent_cmd = format!(
        "cd {} && {}",
        remote_worktree,
        agent::build_resume_command(&agent, agent_session_id.as_deref(), rtk_remote)
    );
    let transport = crate::transport::ssh_tmux::SshTmuxTransport::spawn(
        session_id,
        &sid,
        &agent_cmd,
        80,
        24,
        ctx.ssh_manager.clone(),
        ctx.terminal_tx.clone(),
        ctx.transport_manager.buffer_sender(),
    )
    .await
    .map_err(|e| CoreError::Transport(e.to_string()))?;
    ctx.transport_manager
        .insert(session_id, Box::new(transport))
        .await;

    // Normalise status to Running (it may already be) and notify listeners.
    {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        conn.execute(
            "UPDATE sessions SET status = 'Running', updated_at = datetime('now') WHERE id = ?1",
            [session_id],
        )?;
    }
    ctx.event_bus
        .emit(RaccEvent::SessionStatusChanged {
            session_id,
            status: "Running".to_string(),
            pr_url: None,
            source: "local".to_string(),
        })
        .await;

    Ok(ReconnectOutcome::Reconnected)
}

/// Startup reconciliation — called once before the supervisor loop starts.
/// Probes both local and remote session liveness.
/// Ongoing reconciliation is handled by the supervisor's periodic loop.
pub async fn reconcile_sessions(
    ctx: &AppContext,
) -> Result<Vec<RepoWithSessions>, CoreError> {
    // Collect all Running sessions first, then release the lock before doing async SSH ops
    let running_sessions: Vec<(i64, Option<String>)> = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, server_id FROM sessions WHERE status = 'Running'",
        )?;
        let rows: Vec<(i64, Option<String>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(CoreError::from)?
            .filter_map(|r| r.ok())
            .collect();
        rows
    };

    for (session_id, server_id) in running_sessions {
        let new_status = if let Some(ref sid) = server_id {
            // Remote session: tmux sessions survive Racc restarts -- probe them.
            // (Re)connect first so a Racc restart doesn't wrongly drop a live
            // remote session to "Disconnected" just because the in-memory SSH
            // connection was lost on restart.
            let connected = ctx.ssh_manager.is_connected(sid).await
                || crate::commands::server::ensure_connected(ctx, sid)
                    .await
                    .is_ok();
            if connected {
                let tmux_name = format!("racc-{}", session_id);
                match ctx
                    .ssh_manager
                    .exec(sid, &format!("tmux has-session -t {}", tmux_name))
                    .await
                {
                    Ok(output) if output.exit_code == 0 => {
                        // tmux session alive -- keep status "Running"
                        None
                    }
                    _ => {
                        // tmux session gone -- mark "Completed"
                        Some("Completed")
                    }
                }
            } else {
                // Can't reach server -- mark "Disconnected"
                Some("Disconnected")
            }
        } else {
            // Local session: PTY state is in-memory and lost on restart
            Some("Disconnected")
        };

        if let Some(status) = new_status {
            let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
            conn.execute(
                &format!(
                    "UPDATE sessions SET status = '{}', updated_at = datetime('now') WHERE id = ?1",
                    status
                ),
                [session_id],
            )?;
        }
    }

    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    query_repos_with_sessions(&conn)
}

pub async fn update_session_pr_url(
    ctx: &AppContext,
    session_id: i64,
    pr_url: String,
) -> Result<(), CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    conn.execute(
        "UPDATE sessions SET pr_url = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![pr_url, session_id],
    )?;
    Ok(())
}

/// Get the git diff for a session's worktree (or repo path).
pub async fn get_session_diff(
    ctx: &AppContext,
    session_id: i64,
) -> Result<String, CoreError> {
    let worktree_path = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        let (wt_path, repo_id): (Option<String>, i64) = conn
            .query_row(
                "SELECT worktree_path, repo_id FROM sessions WHERE id = ?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| CoreError::NotFound(format!("Session not found: {e}")))?;

        match wt_path {
            Some(p) => p,
            None => {
                let repo_path: String = conn
                    .query_row(
                        "SELECT path FROM repos WHERE id = ?1",
                        [repo_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| CoreError::NotFound(format!("Repo not found: {e}")))?;
                repo_path
            }
        }
    };

    let output = Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(&worktree_path)
        .output()
        .map_err(|e| CoreError::Git(format!("Failed to get diff: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    struct RecordingTransport {
        writes: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    #[async_trait]
    impl crate::transport::Transport for RecordingTransport {
        async fn write(
            &self,
            data: &[u8],
        ) -> Result<(), crate::transport::TransportError> {
            self.writes.lock().await.push(data.to_vec());
            Ok(())
        }

        async fn resize(
            &self,
            _cols: u16,
            _rows: u16,
        ) -> Result<(), crate::transport::TransportError> {
            Ok(())
        }

        async fn close(&self) -> Result<(), crate::transport::TransportError> {
            Ok(())
        }

        fn is_alive(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn task_prompt_and_submit_are_separate_pty_writes() {
        let writes = Arc::new(Mutex::new(Vec::new()));
        let transport_manager = TransportManager::new();
        transport_manager
            .insert(
                42,
                Box::new(RecordingTransport {
                    writes: writes.clone(),
                }),
            )
            .await;

        inject_and_submit_task(
            &transport_manager,
            42,
            &agent::AgentType::Codex,
            "fix the bug",
        )
        .await
        .unwrap();

        assert_eq!(*writes.lock().await, vec![b"fix the bug".to_vec(), b"\r".to_vec()]);
    }

    #[test]
    fn merge_worktree_can_be_created_from_an_explicit_base_ref() {
        assert_eq!(
            build_worktree_add_args(
                "/tmp/racc-worktrees/widgets/racc-ship-9",
                "racc/ship-9",
                Some("origin/main"),
            ),
            vec![
                "worktree",
                "add",
                "/tmp/racc-worktrees/widgets/racc-ship-9",
                "-b",
                "racc/ship-9",
                "origin/main",
            ]
        );
    }
}
