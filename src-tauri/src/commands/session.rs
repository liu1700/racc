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
    repo_path: String,
    project: String,
    branch: String,
    agent: String,
) -> Result<Session, String> {
    let session_name = format!("otte::{}::{}", project, branch);

    // Check if tmux session already exists
    let check = Command::new("tmux")
        .args(["has-session", "-t", &session_name])
        .output()
        .map_err(|e| format!("Failed to check tmux session: {}", e))?;

    if check.status.success() {
        return Err(format!("Session '{}' already exists", session_name));
    }

    // Resolve absolute worktree path
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let worktree_path = format!("{}/otte-worktrees/{}/{}", home, project, branch);

    // Create parent directory (but not the worktree dir itself — git needs it absent)
    let parent = std::path::Path::new(&worktree_path)
        .parent()
        .ok_or("Invalid worktree path")?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("Failed to create worktree parent dir: {}", e))?;

    // Try to create worktree with new branch, fall back to existing branch
    let output = Command::new("git")
        .current_dir(&repo_path)
        .args(["worktree", "add", &worktree_path, "-b", &branch])
        .output()
        .map_err(|e| format!("Failed to create worktree: {}", e))?;

    if !output.status.success() {
        // Branch might already exist, try without -b
        let output2 = Command::new("git")
            .current_dir(&repo_path)
            .args(["worktree", "add", &worktree_path, &branch])
            .output()
            .map_err(|e| format!("Failed to create worktree: {}", e))?;

        if !output2.status.success() {
            let stderr = String::from_utf8_lossy(&output2.stderr);
            return Err(format!("Failed to create worktree: {}", stderr));
        }
    }

    // Create tmux session with working directory
    let tmux_output = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            &session_name,
            "-c",
            &worktree_path,
            "-x",
            "200",
            "-y",
            "50",
        ])
        .output()
        .map_err(|e| format!("Failed to create tmux session: {}", e))?;

    if !tmux_output.status.success() {
        let stderr = String::from_utf8_lossy(&tmux_output.stderr);
        return Err(format!("Failed to create tmux session: {}", stderr));
    }

    // Start agent in tmux session
    let agent_cmd = match agent.as_str() {
        "claude-code" => "claude",
        "aider" => "aider",
        "codex" => "codex",
        "shell" => "bash",
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
        .filter(|line| line.starts_with("otte::"))
        .filter_map(|name| {
            // Format: otte::project::branch
            let rest = name.strip_prefix("otte::")?;
            let (project, branch) = rest.split_once("::")?;
            Some(Session {
                id: name.to_string(),
                name: name.to_string(),
                project: project.to_string(),
                branch: branch.to_string(),
                agent: "claude-code".to_string(),
                status: SessionStatus::Running,
                worktree_path: String::new(),
            })
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
