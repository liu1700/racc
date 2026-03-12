use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub project: String,
    pub branch: String,
    pub agent: String,
    pub status: SessionStatus,
    pub worktree_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Creating,
    Running,
    Waiting,
    Paused,
    Disconnected,
    Completed,
    Error,
}

#[tauri::command]
pub async fn create_session(
    project: String,
    branch: String,
    agent: String,
) -> Result<Session, String> {
    let session_name = format!("otte-{}-{}", project, branch);

    // Create git worktree
    let worktree_path = format!("../.worktrees/{}", branch);
    Command::new("git")
        .args(["worktree", "add", &worktree_path, "-b", &branch])
        .output()
        .map_err(|e| format!("Failed to create worktree: {}", e))?;

    // Create tmux session
    Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            &session_name,
            "-c",
            &worktree_path,
        ])
        .output()
        .map_err(|e| format!("Failed to create tmux session: {}", e))?;

    // Start agent in tmux session
    let agent_cmd = match agent.as_str() {
        "claude-code" => "claude",
        "aider" => "aider",
        "codex" => "codex",
        _ => return Err(format!("Unknown agent: {}", agent)),
    };

    Command::new("tmux")
        .args(["send-keys", "-t", &session_name, agent_cmd, "Enter"])
        .output()
        .map_err(|e| format!("Failed to start agent: {}", e))?;

    Ok(Session {
        id: session_name.clone(),
        name: session_name,
        project,
        branch,
        agent,
        status: SessionStatus::Running,
        worktree_path,
    })
}

#[tauri::command]
pub async fn list_sessions() -> Result<Vec<Session>, String> {
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output()
        .map_err(|e| format!("Failed to list tmux sessions: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions: Vec<Session> = stdout
        .lines()
        .filter(|line| line.starts_with("otte-"))
        .map(|name| {
            let parts: Vec<&str> = name.splitn(3, '-').collect();
            Session {
                id: name.to_string(),
                name: name.to_string(),
                project: parts.get(1).unwrap_or(&"unknown").to_string(),
                branch: parts.get(2).unwrap_or(&"unknown").to_string(),
                agent: "claude-code".to_string(),
                status: SessionStatus::Running,
                worktree_path: String::new(),
            }
        })
        .collect();

    Ok(sessions)
}

#[tauri::command]
pub async fn stop_session(session_id: String) -> Result<(), String> {
    Command::new("tmux")
        .args(["kill-session", "-t", &session_id])
        .output()
        .map_err(|e| format!("Failed to stop session: {}", e))?;

    Ok(())
}
