use tauri::State;
pub use racc_core::commands::session::{Repo, RepoWithSessions, Session};

#[tauri::command]
pub async fn import_repo(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    path: String,
) -> Result<Repo, String> {
    racc_core::commands::session::import_repo(&ctx, path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_repos(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<Vec<RepoWithSessions>, String> {
    racc_core::commands::session::list_repos(&ctx)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_repo(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
) -> Result<(), String> {
    racc_core::commands::session::remove_repo(&ctx, repo_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_session(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
    use_worktree: bool,
    branch: Option<String>,
    agent: Option<String>,
    task_description: Option<String>,
    server_id: Option<String>,
    skip_permissions: Option<bool>,
) -> Result<Session, String> {
    racc_core::commands::session::create_session(
        &ctx,
        repo_id,
        use_worktree,
        branch,
        agent,
        task_description,
        server_id,
        skip_permissions,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_session(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    session_id: i64,
) -> Result<(), String> {
    racc_core::commands::session::stop_session(&ctx, session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_session(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    session_id: i64,
    delete_worktree: bool,
) -> Result<(), String> {
    racc_core::commands::session::remove_session(&ctx, session_id, delete_worktree)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reattach_session(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    session_id: i64,
) -> Result<Session, String> {
    racc_core::commands::session::reattach_session(&ctx, session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reconcile_sessions(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<Vec<RepoWithSessions>, String> {
    racc_core::commands::session::reconcile_sessions(&ctx)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_session_pr_url(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    session_id: i64,
    pr_url: String,
) -> Result<(), String> {
    racc_core::commands::session::update_session_pr_url(&ctx, session_id, pr_url)
        .await
        .map_err(|e| e.to_string())
}
