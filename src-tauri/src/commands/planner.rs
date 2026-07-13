use tauri::State;

pub use racc_core::commands::planner::TaskPlanRun;
use racc_core::commands::task::Task;

#[tauri::command]
pub fn get_latest_task_plan(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
) -> Result<Option<TaskPlanRun>, String> {
    racc_core::commands::planner::get_latest_task_plan(&ctx, repo_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn start_task_plan(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
    source_input: String,
    agent: String,
) -> Result<TaskPlanRun, String> {
    racc_core::commands::planner::start_task_plan(&ctx, repo_id, source_input, agent)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn confirm_task_plan(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    run_id: i64,
    selected_keys: Vec<String>,
) -> Result<Vec<Task>, String> {
    racc_core::commands::planner::confirm_task_plan(&ctx, run_id, selected_keys)
        .await
        .map_err(|error| error.to_string())
}
