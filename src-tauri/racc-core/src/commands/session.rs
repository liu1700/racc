use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::agent;
use crate::AppContext;
use crate::error::CoreError;
use crate::events::RaccEvent;
use crate::transport::local_pty::LocalPtyTransport;
use crate::rtk;

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
    let agent = agent.unwrap_or_else(|| "claude-code".to_string());
    let task_description = task_description.unwrap_or_default();
    let skip_permissions = skip_permissions.unwrap_or(false);

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
        let wt_dir = home
            .join("racc-worktrees")
            .join(&repo_name)
            .join(&branch);

        let wt_path = wt_dir.to_string_lossy().to_string();

        if !wt_dir.exists() {
            std::fs::create_dir_all(wt_dir.parent().unwrap())?;

            let output = Command::new("git")
                .args(["worktree", "add", &wt_path, "-b", &branch])
                .current_dir(&repo_path)
                .output()
                .map_err(|e| CoreError::Git(format!("git worktree add failed: {e}")))?;

            if !output.status.success() {
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
            "INSERT INTO sessions (repo_id, agent, worktree_path, branch, status, server_id)
             VALUES (?1, ?2, ?3, ?4, 'Running', ?5)",
            rusqlite::params![repo_id, agent, worktree_path, branch_name, server_id],
        )?;

        let id = conn.last_insert_rowid();
        let (created_at, updated_at): (String, String) = conn.query_row(
            "SELECT created_at, updated_at FROM sessions WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        (id, worktree_path.clone(), created_at, updated_at)
    }; // conn lock released here

    if let Some(ref sid) = server_id {
        // Remote session: clone repo if needed, create worktree, spawn SshTmuxTransport
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

        // Spawn SshTmuxTransport
        let agent_cmd = agent::build_command(&agent, &task_description, &remote_worktree, skip_permissions, rtk_remote);
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
        let agent_cmd = agent::build_command(&agent, &task_description, cwd, skip_permissions, false);

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

        // Send agent command after short delay to let shell initialize
        if !task_description.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            ctx.transport_manager
                .write(session_id, agent_cmd.as_bytes())
                .await
                .map_err(|e| CoreError::Transport(e.to_string()))?;
        }
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
    // Close transport before updating DB
    let _ = ctx.transport_manager.remove(session_id).await;

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
    // Close transport if still running
    let _ = ctx.transport_manager.remove(session_id).await;

    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;

    let (status, worktree_path, repo_id): (String, Option<String>, i64) = conn
        .query_row(
            "SELECT status, worktree_path, repo_id FROM sessions WHERE id = ?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| CoreError::NotFound(format!("Session not found: {e}")))?;

    // If still running, mark as completed first
    if status == "Running" {
        conn.execute(
            "UPDATE sessions SET status = 'Completed', updated_at = datetime('now') WHERE id = ?1",
            [session_id],
        )?;
    }

    // Remove worktree via git if requested
    if delete_worktree {
        if let Some(wt_path) = &worktree_path {
            let repo_path: String = conn
                .query_row(
                    "SELECT path FROM repos WHERE id = ?1",
                    [repo_id],
                    |row| row.get(0),
                )
                .map_err(|e| CoreError::NotFound(format!("Repo not found: {e}")))?;

            let output = Command::new("git")
                .args(["worktree", "remove", wt_path, "--force"])
                .current_dir(&repo_path)
                .output()
                .map_err(|e| CoreError::Git(format!("Failed to remove worktree: {e}")))?;

            if !output.status.success() {
                return Err(CoreError::Git(format!(
                    "git worktree remove failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }
    }

    conn.execute("DELETE FROM sessions WHERE id = ?1", [session_id])?;

    Ok(())
}

pub async fn reattach_session(
    ctx: &AppContext,
    session_id: i64,
) -> Result<Session, CoreError> {
    let (_status, worktree_path, repo_id, agent, branch, created_at, updated_at, pr_url, server_id, repo_path) = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;

        let (status, worktree_path, repo_id): (String, Option<String>, i64) = conn
            .query_row(
                "SELECT status, worktree_path, repo_id FROM sessions WHERE id = ?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|e| CoreError::NotFound(format!("Session not found: {e}")))?;

        if status == "Running" {
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

        conn.execute(
            "UPDATE sessions SET status = 'Running', updated_at = datetime('now') WHERE id = ?1",
            [session_id],
        )?;

        let (agent, branch, created_at, updated_at, pr_url, server_id): (String, Option<String>, String, String, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT agent, branch, created_at, updated_at, pr_url, server_id FROM sessions WHERE id = ?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )?;

        let repo_path: String = conn
            .query_row("SELECT path FROM repos WHERE id = ?1", [repo_id], |row| row.get(0))
            .map_err(|e| CoreError::NotFound(format!("Repo not found: {e}")))?;

        (status, worktree_path, repo_id, agent, branch, created_at, updated_at, pr_url, server_id, repo_path)
    }; // DB lock released here -- safe for async transport work below

    if let Some(ref sid) = server_id {
        // Remote session: reattach to existing tmux session via SSH
        let existing = ctx.transport_manager.is_alive(session_id).await;
        if existing {
            let _ = ctx.transport_manager.remove(session_id).await;
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

        let agent_cmd = agent::build_command(&agent, "", &remote_worktree, false, rtk_remote);
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
        // Local session: spawn a new PTY with `claude --continue`
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

        // Send `claude --continue` to resume the previous session
        let continue_cmd = "claude --continue\n".to_string();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        ctx.transport_manager
            .write(session_id, continue_cmd.as_bytes())
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
            // Remote session: tmux sessions survive Racc restarts -- probe them
            if ctx.ssh_manager.is_connected(sid).await {
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
