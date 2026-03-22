use tauri::State;

#[tauri::command]
pub async fn transport_write(
    session_id: i64,
    data: Vec<u8>,
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<(), String> {
    racc_core::commands::transport::transport_write(&ctx, session_id, data)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transport_resize(
    session_id: i64,
    cols: u16,
    rows: u16,
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<(), String> {
    racc_core::commands::transport::transport_resize(&ctx, session_id, cols, rows)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transport_get_buffer(
    session_id: i64,
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<Vec<u8>, String> {
    racc_core::commands::transport::transport_get_buffer(&ctx, session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transport_is_alive(
    session_id: i64,
    ctx: State<'_, std::sync::Arc<racc_core::AppContext>>,
) -> Result<bool, String> {
    racc_core::commands::transport::transport_is_alive(&ctx, session_id)
        .await
        .map_err(|e| e.to_string())
}
