use tauri::State;
pub use racc_core::commands::insights::{Insight, SessionEvent};

#[tauri::command]
pub async fn record_session_events(
    ctx: State<'_, racc_core::AppContext>,
    events: Vec<SessionEvent>,
) -> Result<(), String> {
    racc_core::commands::insights::record_session_events(&ctx, events)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_insights(
    ctx: State<'_, racc_core::AppContext>,
    status: Option<String>,
) -> Result<Vec<Insight>, String> {
    racc_core::commands::insights::get_insights(&ctx, status)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_insight_status(
    ctx: State<'_, racc_core::AppContext>,
    id: i64,
    status: String,
) -> Result<(), String> {
    racc_core::commands::insights::update_insight_status(&ctx, id, status)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_insight(
    ctx: State<'_, racc_core::AppContext>,
    insight_type: String,
    severity: String,
    title: String,
    summary: String,
    detail_json: String,
    fingerprint: String,
) -> Result<Option<i64>, String> {
    racc_core::commands::insights::save_insight(
        &ctx,
        insight_type,
        severity,
        title,
        summary,
        detail_json,
        fingerprint,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_session_events(
    ctx: State<'_, racc_core::AppContext>,
    event_type: Option<String>,
    since: Option<i64>,
) -> Result<Vec<SessionEvent>, String> {
    racc_core::commands::insights::get_session_events(&ctx, event_type, since)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn append_to_file(path: String, content: String) -> Result<(), String> {
    racc_core::commands::insights::append_to_file(path, content)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn run_batch_analysis(
    ctx: State<'_, racc_core::AppContext>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let new_insights = racc_core::commands::insights::run_batch_analysis(&ctx)
        .await
        .map_err(|e| e.to_string())?;

    // Emit insight-detected events to the frontend for each new insight
    use tauri::Emitter;
    for insight in new_insights {
        let _ = app.emit("insight-detected", &insight);
    }

    Ok(())
}
