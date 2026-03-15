use crate::transport::manager::TransportManager;
use tauri::State;

#[tauri::command]
pub async fn transport_write(
    session_id: i64,
    data: Vec<u8>,
    transport_manager: State<'_, TransportManager>,
) -> Result<(), String> {
    transport_manager.write(session_id, &data).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transport_resize(
    session_id: i64,
    cols: u16,
    rows: u16,
    transport_manager: State<'_, TransportManager>,
) -> Result<(), String> {
    transport_manager.resize(session_id, cols, rows).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transport_get_buffer(
    session_id: i64,
    transport_manager: State<'_, TransportManager>,
) -> Result<Vec<u8>, String> {
    transport_manager.get_buffer(session_id).await
        .ok_or_else(|| format!("No buffer for session {}", session_id))
}
