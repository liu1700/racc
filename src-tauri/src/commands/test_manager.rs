use tauri::State;

pub use racc_core::commands::test_manager::{TestManagerState, TestRun, TestSettings};

#[tauri::command]
pub fn get_test_manager(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
) -> Result<TestManagerState, String> {
    racc_core::commands::test_manager::get_test_manager(&ctx, repo_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_test_settings(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
    target_branch: String,
    agent: String,
    instructions: String,
) -> Result<TestSettings, String> {
    racc_core::commands::test_manager::update_test_settings(
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
pub async fn start_test_run(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
) -> Result<TestRun, String> {
    racc_core::commands::test_manager::start_test_run(&ctx, repo_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn resolve_test_run(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    run_id: i64,
    status: String,
) -> Result<TestRun, String> {
    racc_core::commands::test_manager::resolve_test_run(&ctx, run_id, &status)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn retry_test_run(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    run_id: i64,
) -> Result<TestRun, String> {
    racc_core::commands::test_manager::retry_test_run(&ctx, run_id)
        .await
        .map_err(|error| error.to_string())
}
