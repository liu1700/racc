use tauri::State;

pub use racc_core::commands::merge::{MergeManagerState, MergeQueueItem, MergeRun, MergeSettings};

#[tauri::command]
pub fn get_merge_manager(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
) -> Result<MergeManagerState, String> {
    racc_core::commands::merge::get_merge_manager(&ctx, repo_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn reset_merge_manager(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
) -> Result<(), String> {
    racc_core::commands::merge::reset_merge_manager(&ctx, repo_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_task_ready_to_merge(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    task_id: i64,
    ready: bool,
) -> Result<Option<MergeQueueItem>, String> {
    racc_core::commands::merge::set_task_ready_to_merge(&ctx, task_id, ready)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_merge_settings(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
    target_branch: String,
    agent: String,
    instructions: String,
) -> Result<MergeSettings, String> {
    racc_core::commands::merge::update_merge_settings(
        &ctx,
        repo_id,
        &target_branch,
        &agent,
        &instructions,
    )
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn start_merge_run(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
) -> Result<MergeRun, String> {
    racc_core::commands::merge::start_merge_run(&ctx, repo_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn resolve_merge_run(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    run_id: i64,
    status: String,
) -> Result<MergeRun, String> {
    racc_core::commands::merge::resolve_merge_run(&ctx, run_id, &status)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn retry_merge_run(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    run_id: i64,
) -> Result<MergeRun, String> {
    racc_core::commands::merge::retry_merge_run(&ctx, run_id)
        .await
        .map_err(|error| error.to_string())
}
