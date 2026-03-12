use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::Mutex;

// --- Types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Running,
    Completed,
    Disconnected,
    Error,
}

impl SessionStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Completed => "Completed",
            Self::Disconnected => "Disconnected",
            Self::Error => "Error",
        }
    }

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
    pub tmux_session_name: String,
    pub agent: String,
    pub worktree_path: Option<String>,
    pub branch: Option<String>,
    pub status: SessionStatus,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoWithSessions {
    pub repo: Repo,
    pub sessions: Vec<Session>,
}

// --- Helper: query repos with sessions ---

fn query_repos_with_sessions(conn: &Connection) -> Result<Vec<RepoWithSessions>, String> {
    let mut repo_stmt = conn
        .prepare("SELECT id, path, name, added_at FROM repos ORDER BY name")
        .map_err(|e| e.to_string())?;

    let repos: Vec<Repo> = repo_stmt
        .query_map([], |row| {
            Ok(Repo {
                id: row.get(0)?,
                path: row.get(1)?,
                name: row.get(2)?,
                added_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let mut session_stmt = conn
        .prepare(
            "SELECT id, repo_id, tmux_session_name, agent, worktree_path, branch, status, created_at, updated_at
             FROM sessions WHERE repo_id = ? ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for repo in repos {
        let sessions: Vec<Session> = session_stmt
            .query_map([repo.id], |row| {
                let status_str: String = row.get(6)?;
                Ok(Session {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    tmux_session_name: row.get(2)?,
                    agent: row.get(3)?,
                    worktree_path: row.get(4)?,
                    branch: row.get(5)?,
                    status: SessionStatus::from_str(&status_str),
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        result.push(RepoWithSessions { repo, sessions });
    }

    Ok(result)
}

// --- Helper: check tmux session exists ---

fn tmux_session_exists(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// --- Helper: get current git branch ---

fn get_current_branch(repo_path: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Failed to get branch: {e}"))?;

    if !output.status.success() {
        return Err("Failed to detect current branch".to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// --- Tauri Commands ---

#[tauri::command]
pub async fn import_repo(
    db: tauri::State<'_, Mutex<Connection>>,
    path: String,
) -> Result<Repo, String> {
    let git_dir = std::path::Path::new(&path).join(".git");
    if !git_dir.exists() {
        return Err("Not a git repository".to_string());
    }

    let name = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let conn = db.lock().map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO repos (path, name) VALUES (?1, ?2)",
        rusqlite::params![path, name],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            "Repository already imported".to_string()
        } else {
            e.to_string()
        }
    })?;

    let id = conn.last_insert_rowid();
    let added_at: String = conn
        .query_row("SELECT added_at FROM repos WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;

    Ok(Repo {
        id,
        path,
        name,
        added_at,
    })
}

#[tauri::command]
pub async fn list_repos(
    db: tauri::State<'_, Mutex<Connection>>,
) -> Result<Vec<RepoWithSessions>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    query_repos_with_sessions(&conn)
}

#[tauri::command]
pub async fn remove_repo(
    db: tauri::State<'_, Mutex<Connection>>,
    repo_id: i64,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let running_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE repo_id = ?1 AND status = 'Running'",
            [repo_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    if running_count > 0 {
        return Err("Cannot remove repo with running sessions. Stop them first.".to_string());
    }

    conn.execute("DELETE FROM repos WHERE id = ?1", [repo_id])
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn create_session(
    db: tauri::State<'_, Mutex<Connection>>,
    repo_id: i64,
    use_worktree: bool,
    branch: Option<String>,
) -> Result<Session, String> {
    let (repo_path, repo_name) = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let row: (String, String) = conn
            .query_row(
                "SELECT path, name FROM repos WHERE id = ?1",
                [repo_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| format!("Repo not found: {e}"))?;
        row
    };

    let (working_dir, worktree_path, branch_name) = if use_worktree {
        let branch = branch.ok_or("Branch name required for worktree")?;
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .ok_or("Could not find home directory")?;
        let wt_dir = home
            .join("otte-worktrees")
            .join(&repo_name)
            .join(&branch);

        let wt_path = wt_dir.to_string_lossy().to_string();

        if !wt_dir.exists() {
            std::fs::create_dir_all(wt_dir.parent().unwrap())
                .map_err(|e| format!("Failed to create worktree dir: {e}"))?;

            let output = Command::new("git")
                .args(["worktree", "add", &wt_path, "-b", &branch])
                .current_dir(&repo_path)
                .output()
                .map_err(|e| format!("git worktree add failed: {e}"))?;

            if !output.status.success() {
                let output2 = Command::new("git")
                    .args(["worktree", "add", &wt_path, &branch])
                    .current_dir(&repo_path)
                    .output()
                    .map_err(|e| format!("git worktree add failed: {e}"))?;

                if !output2.status.success() {
                    return Err(format!(
                        "git worktree add failed: {}",
                        String::from_utf8_lossy(&output2.stderr)
                    ));
                }
            }
        }

        (wt_path.clone(), Some(wt_path), branch)
    } else {
        let branch = get_current_branch(&repo_path)?;
        (repo_path.clone(), None, branch)
    };

    let tmux_name = format!("otte::{}::{}", repo_name, branch_name);

    if tmux_session_exists(&tmux_name) {
        return Err(format!("Session '{}' already exists", tmux_name));
    }

    let output = Command::new("tmux")
        .args([
            "new-session", "-d", "-s", &tmux_name,
            "-x", "200", "-y", "50", "-c", &working_dir,
        ])
        .output()
        .map_err(|e| format!("Failed to create tmux session: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "tmux new-session failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let _ = Command::new("tmux")
        .args(["send-keys", "-t", &tmux_name, "claude", "Enter"])
        .output();

    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO sessions (repo_id, tmux_session_name, agent, worktree_path, branch, status)
         VALUES (?1, ?2, 'claude-code', ?3, ?4, 'Running')",
        rusqlite::params![repo_id, tmux_name, worktree_path, branch_name],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    let (created_at, updated_at): (String, String) = conn
        .query_row(
            "SELECT created_at, updated_at FROM sessions WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;

    Ok(Session {
        id,
        repo_id,
        tmux_session_name: tmux_name,
        agent: "claude-code".to_string(),
        worktree_path,
        branch: Some(branch_name),
        status: SessionStatus::Running,
        created_at,
        updated_at,
    })
}

#[tauri::command]
pub async fn stop_session(
    db: tauri::State<'_, Mutex<Connection>>,
    session_id: i64,
) -> Result<(), String> {
    let tmux_name = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let name: String = conn
            .query_row(
                "SELECT tmux_session_name FROM sessions WHERE id = ?1",
                [session_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("Session not found: {e}"))?;
        name
    };

    let _ = Command::new("tmux")
        .args(["kill-session", "-t", &tmux_name])
        .output();

    let conn = db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET status = 'Completed', updated_at = datetime('now') WHERE id = ?1",
        [session_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn remove_session(
    db: tauri::State<'_, Mutex<Connection>>,
    session_id: i64,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let status: String = conn
        .query_row(
            "SELECT status FROM sessions WHERE id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Session not found: {e}"))?;

    if status == "Running" {
        return Err("Cannot remove a running session. Stop it first.".to_string());
    }

    conn.execute("DELETE FROM sessions WHERE id = ?1", [session_id])
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn reconcile_sessions(
    db: tauri::State<'_, Mutex<Connection>>,
) -> Result<Vec<RepoWithSessions>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT id, tmux_session_name FROM sessions WHERE status = 'Running'")
        .map_err(|e| e.to_string())?;

    let running: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    for (id, name) in &running {
        if !tmux_session_exists(name) {
            conn.execute(
                "UPDATE sessions SET status = 'Disconnected', updated_at = datetime('now') WHERE id = ?1",
                [id],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    query_repos_with_sessions(&conn)
}
