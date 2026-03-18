use crate::AppContext;
use crate::error::CoreError;

pub async fn transport_write(
    ctx: &AppContext,
    session_id: i64,
    data: Vec<u8>,
) -> Result<(), CoreError> {
    ctx.transport_manager
        .write(session_id, &data)
        .await
        .map_err(|e| CoreError::Transport(e.to_string()))
}

pub async fn transport_resize(
    ctx: &AppContext,
    session_id: i64,
    cols: u16,
    rows: u16,
) -> Result<(), CoreError> {
    ctx.transport_manager
        .resize(session_id, cols, rows)
        .await
        .map_err(|e| CoreError::Transport(e.to_string()))
}

pub async fn transport_get_buffer(
    ctx: &AppContext,
    session_id: i64,
) -> Result<Vec<u8>, CoreError> {
    ctx.transport_manager
        .get_buffer(session_id)
        .await
        .ok_or_else(|| CoreError::NotFound(format!("No buffer for session {}", session_id)))
}

pub async fn transport_is_alive(
    ctx: &AppContext,
    session_id: i64,
) -> Result<bool, CoreError> {
    Ok(ctx.transport_manager.is_alive(session_id).await)
}
