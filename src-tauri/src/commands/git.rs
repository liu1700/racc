use std::process::Command;

#[tauri::command]
pub async fn create_worktree(path: String, branch: String) -> Result<String, String> {
    let output = Command::new("git")
        .args(["worktree", "add", &path, "-b", &branch])
        .output()
        .map_err(|e| format!("Failed to create worktree: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(path)
}

#[tauri::command]
pub async fn delete_worktree(path: String) -> Result<(), String> {
    Command::new("git")
        .args(["worktree", "remove", &path, "--force"])
        .output()
        .map_err(|e| format!("Failed to delete worktree: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn get_diff(worktree_path: String) -> Result<String, String> {
    let output = Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(&worktree_path)
        .output()
        .map_err(|e| format!("Failed to get diff: {}", e))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
