#[tauri::command]
pub async fn create_worktree(path: String, branch: String) -> Result<String, String> {
    racc_core::commands::git::create_worktree(path, branch)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_worktree(path: String) -> Result<(), String> {
    racc_core::commands::git::delete_worktree(path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_diff(worktree_path: String) -> Result<String, String> {
    racc_core::commands::git::get_diff(worktree_path)
        .await
        .map_err(|e| e.to_string())
}
