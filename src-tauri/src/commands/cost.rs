pub use racc_core::commands::cost::ProjectCosts;

#[tauri::command]
pub async fn get_project_costs(worktree_path: String) -> Result<ProjectCosts, String> {
    racc_core::commands::cost::get_project_costs(worktree_path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_global_costs() -> Result<ProjectCosts, String> {
    racc_core::commands::cost::get_global_costs()
        .await
        .map_err(|e| e.to_string())
}
