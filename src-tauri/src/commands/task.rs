use tauri::State;
pub use racc_core::commands::task::Task;

#[tauri::command]
pub async fn create_task(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
    description: String,
    images: Option<String>,
) -> Result<Task, String> {
    racc_core::commands::task::create_task(&ctx, repo_id, description, images)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_tasks(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    repo_id: i64,
) -> Result<Vec<Task>, String> {
    racc_core::commands::task::list_tasks(&ctx, repo_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_task_status(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    task_id: i64,
    status: String,
    session_id: Option<i64>,
) -> Result<Task, String> {
    racc_core::commands::task::update_task_status(&ctx, task_id, status, session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_task_description(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    task_id: i64,
    description: String,
) -> Result<Task, String> {
    racc_core::commands::task::update_task_description(&ctx, task_id, description)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_task_images(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    task_id: i64,
    images: String,
) -> Result<Task, String> {
    racc_core::commands::task::update_task_images(&ctx, task_id, images)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_task(
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
    task_id: i64,
) -> Result<(), String> {
    racc_core::commands::task::delete_task(&ctx, task_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_task_image(
    repo_path: String,
    filename: String,
    data: Vec<u8>,
) -> Result<String, String> {
    racc_core::commands::task::save_task_image(repo_path, filename, data)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_file_to_task_images(
    repo_path: String,
    source_path: String,
    filename: String,
) -> Result<String, String> {
    racc_core::commands::task::copy_file_to_task_images(repo_path, source_path, filename)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_task_image(repo_path: String, filename: String) -> Result<(), String> {
    racc_core::commands::task::delete_task_image(repo_path, filename)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_task_image(
    repo_path: String,
    old_name: String,
    new_name: String,
) -> Result<(), String> {
    racc_core::commands::task::rename_task_image(repo_path, old_name, new_name)
        .map_err(|e| e.to_string())
}
