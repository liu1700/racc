use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionEvent {
    pub session_id: i64,
    pub event_type: String,
    pub payload: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Insight {
    pub id: i64,
    pub insight_type: String,
    pub severity: String,
    pub title: String,
    pub summary: String,
    pub detail_json: String,
    pub fingerprint: String,
    pub status: String,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

#[tauri::command]
pub async fn record_session_events(
    db: State<'_, Mutex<Connection>>,
    events: Vec<SessionEvent>,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
    for event in &events {
        conn.execute(
            "INSERT INTO session_events (session_id, event_type, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![event.session_id, event.event_type, event.payload, event.created_at],
        )
        .map_err(|e| format!("Failed to insert event: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_insights(
    db: State<'_, Mutex<Connection>>,
    status: Option<String>,
) -> Result<Vec<Insight>, String> {
    let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
    let status_filter = status.as_deref().unwrap_or("active");
    let mut stmt = conn
        .prepare(
            "SELECT id, insight_type, severity, title, summary, detail_json, fingerprint, status, created_at, resolved_at
             FROM insights WHERE status = ?1 ORDER BY created_at DESC",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![status_filter], |row| {
            Ok(Insight {
                id: row.get(0)?,
                insight_type: row.get(1)?,
                severity: row.get(2)?,
                title: row.get(3)?,
                summary: row.get(4)?,
                detail_json: row.get(5)?,
                fingerprint: row.get(6)?,
                status: row.get(7)?,
                created_at: row.get(8)?,
                resolved_at: row.get(9)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    let mut insights = Vec::new();
    for row in rows {
        insights.push(row.map_err(|e| format!("Row error: {e}"))?);
    }
    Ok(insights)
}

#[tauri::command]
pub async fn update_insight_status(
    db: State<'_, Mutex<Connection>>,
    id: i64,
    status: String,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
    let resolved_at: Option<i64> = if status == "applied" || status == "dismissed" || status == "expired" {
        Some(chrono::Utc::now().timestamp_millis())
    } else {
        None
    };
    conn.execute(
        "UPDATE insights SET status = ?1, resolved_at = ?2 WHERE id = ?3",
        rusqlite::params![status, resolved_at, id],
    )
    .map_err(|e| format!("Update error: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn save_insight(
    db: State<'_, Mutex<Connection>>,
    insight_type: String,
    severity: String,
    title: String,
    summary: String,
    detail_json: String,
    fingerprint: String,
) -> Result<Option<i64>, String> {
    let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;

    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM insights WHERE fingerprint = ?1 AND status = 'active'",
            rusqlite::params![fingerprint],
            |row| row.get(0),
        )
        .ok();

    if existing.is_some() {
        return Ok(None);
    }

    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "INSERT INTO insights (insight_type, severity, title, summary, detail_json, fingerprint, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7)",
        rusqlite::params![insight_type, severity, title, summary, detail_json, fingerprint, now],
    )
    .map_err(|e| format!("Insert error: {e}"))?;

    let id = conn.last_insert_rowid();
    Ok(Some(id))
}

#[tauri::command]
pub async fn get_session_events(
    db: State<'_, Mutex<Connection>>,
    event_type: Option<String>,
    since: Option<i64>,
) -> Result<Vec<SessionEvent>, String> {
    let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;

    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match (&event_type, &since) {
        (Some(et), Some(s)) => (
            "SELECT session_id, event_type, payload, created_at FROM session_events WHERE event_type = ?1 AND created_at >= ?2 ORDER BY created_at DESC LIMIT 500".into(),
            vec![Box::new(et.clone()), Box::new(*s)],
        ),
        (Some(et), None) => (
            "SELECT session_id, event_type, payload, created_at FROM session_events WHERE event_type = ?1 ORDER BY created_at DESC LIMIT 500".into(),
            vec![Box::new(et.clone())],
        ),
        (None, Some(s)) => (
            "SELECT session_id, event_type, payload, created_at FROM session_events WHERE created_at >= ?1 ORDER BY created_at DESC LIMIT 500".into(),
            vec![Box::new(*s)],
        ),
        (None, None) => (
            "SELECT session_id, event_type, payload, created_at FROM session_events ORDER BY created_at DESC LIMIT 500".into(),
            vec![],
        ),
    };

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("Query error: {e}"))?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(SessionEvent {
                session_id: row.get(0)?,
                event_type: row.get(1)?,
                payload: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row.map_err(|e| format!("Row error: {e}"))?);
    }
    Ok(events)
}

#[tauri::command]
pub async fn append_to_file(path: String, content: String) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Failed to open {path}: {e}"))?;

    let metadata = std::fs::metadata(&path).map_err(|e| format!("Failed to stat {path}: {e}"))?;
    if metadata.len() > 0 {
        writeln!(file).map_err(|e| format!("Write error: {e}"))?;
    }
    write!(file, "{content}").map_err(|e| format!("Write error: {e}"))?;

    Ok(())
}
