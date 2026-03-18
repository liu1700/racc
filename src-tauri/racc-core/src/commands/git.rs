use std::process::Command;
use crate::error::CoreError;

pub async fn create_worktree(path: String, branch: String) -> Result<String, CoreError> {
    let output = Command::new("git")
        .args(["worktree", "add", &path, "-b", &branch])
        .output()
        .map_err(|e| CoreError::Git(format!("Failed to create worktree: {}", e)))?;

    if !output.status.success() {
        return Err(CoreError::Git(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(path)
}

pub async fn delete_worktree(path: String) -> Result<(), CoreError> {
    Command::new("git")
        .args(["worktree", "remove", &path, "--force"])
        .output()
        .map_err(|e| CoreError::Git(format!("Failed to delete worktree: {}", e)))?;

    Ok(())
}

pub async fn get_diff(worktree_path: String) -> Result<String, CoreError> {
    let output = Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(&worktree_path)
        .output()
        .map_err(|e| CoreError::Git(format!("Failed to get diff: {}", e)))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
