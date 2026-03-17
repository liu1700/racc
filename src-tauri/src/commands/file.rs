use tauri::State;
pub use racc_core::commands::file::{FileContent, FileMatch};

/// Compatibility wrapper for assistant.rs: maps CoreError to String
pub fn read_file_core(
    conn: &rusqlite::Connection,
    session_id: Option<i64>,
    repo_id: Option<i64>,
    file_path: &str,
    max_lines: Option<usize>,
) -> Result<FileContent, String> {
    racc_core::commands::file::read_file_core(conn, session_id, repo_id, file_path, max_lines)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn read_file(
    ctx: State<'_, racc_core::AppContext>,
    session_id: Option<i64>,
    repo_id: Option<i64>,
    file_path: String,
    max_lines: Option<usize>,
) -> Result<FileContent, String> {
    racc_core::commands::file::read_file(&ctx, session_id, repo_id, file_path, max_lines)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_files(
    ctx: State<'_, racc_core::AppContext>,
    session_id: Option<i64>,
    repo_id: Option<i64>,
    query: String,
) -> Result<Vec<FileMatch>, String> {
    racc_core::commands::file::search_files(&ctx, session_id, repo_id, query)
        .await
        .map_err(|e| e.to_string())
}
