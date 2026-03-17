use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use strsim::normalized_levenshtein;

use crate::AppContext;
use crate::error::CoreError;

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

pub async fn record_session_events(
    ctx: &AppContext,
    events: Vec<SessionEvent>,
) -> Result<(), CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    for event in &events {
        conn.execute(
            "INSERT INTO session_events (session_id, event_type, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![event.session_id, event.event_type, event.payload, event.created_at],
        )?;
    }
    Ok(())
}

pub async fn get_insights(
    ctx: &AppContext,
    status: Option<String>,
) -> Result<Vec<Insight>, CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    let status_filter = status.as_deref().unwrap_or("active");
    let mut stmt = conn.prepare(
        "SELECT id, insight_type, severity, title, summary, detail_json, fingerprint, status, created_at, resolved_at
         FROM insights WHERE status = ?1 ORDER BY created_at DESC",
    )?;

    let rows = stmt.query_map(rusqlite::params![status_filter], |row| {
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
    })?;

    let mut insights = Vec::new();
    for row in rows {
        insights.push(row?);
    }
    Ok(insights)
}

pub async fn update_insight_status(
    ctx: &AppContext,
    id: i64,
    status: String,
) -> Result<(), CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    let resolved_at: Option<i64> =
        if status == "applied" || status == "dismissed" || status == "expired" {
            Some(chrono::Utc::now().timestamp_millis())
        } else {
            None
        };
    conn.execute(
        "UPDATE insights SET status = ?1, resolved_at = ?2 WHERE id = ?3",
        rusqlite::params![status, resolved_at, id],
    )?;
    Ok(())
}

pub async fn save_insight(
    ctx: &AppContext,
    insight_type: String,
    severity: String,
    title: String,
    summary: String,
    detail_json: String,
    fingerprint: String,
) -> Result<Option<i64>, CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;

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
    )?;

    let id = conn.last_insert_rowid();
    Ok(Some(id))
}

pub async fn get_session_events(
    ctx: &AppContext,
    event_type: Option<String>,
    since: Option<i64>,
) -> Result<Vec<SessionEvent>, CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;

    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
        match (&event_type, &since) {
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

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        Ok(SessionEvent {
            session_id: row.get(0)?,
            event_type: row.get(1)?,
            payload: row.get(2)?,
            created_at: row.get(3)?,
        })
    })?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

pub async fn append_to_file(path: String, content: String) -> Result<(), CoreError> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;

    let metadata = std::fs::metadata(&path)?;
    if metadata.len() > 0 {
        writeln!(file).map_err(|e| CoreError::Other(format!("Write error: {e}")))?;
    }
    write!(file, "{content}").map_err(|e| CoreError::Other(format!("Write error: {e}")))?;

    Ok(())
}

#[derive(Debug, Serialize)]
struct DetectedInsight {
    insight_type: String,
    severity: String,
    title: String,
    summary: String,
    detail_json: String,
    fingerprint: String,
}

fn detect_repeated_prompts(conn: &rusqlite::Connection) -> Vec<DetectedInsight> {
    let seven_days_ago = chrono::Utc::now().timestamp_millis() - 7 * 24 * 60 * 60 * 1000;
    let mut stmt = match conn.prepare(
        "SELECT session_id, payload, created_at FROM session_events
         WHERE event_type = 'user_input' AND created_at >= ?1
         ORDER BY created_at DESC LIMIT 500",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let events: Vec<(i64, String, i64)> = stmt
        .query_map(rusqlite::params![seven_days_ago], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    let inputs: Vec<(i64, String, i64)> = events
        .into_iter()
        .filter_map(|(sid, payload, ts)| {
            let parsed: serde_json::Value = serde_json::from_str(&payload).ok()?;
            let text = parsed
                .get("text")?
                .as_str()?
                .to_lowercase()
                .trim()
                .to_string();
            if text.len() < 10 {
                return None;
            }
            Some((sid, text, ts))
        })
        .collect();

    let mut clusters: Vec<Vec<usize>> = vec![];
    let mut assigned = HashSet::new();

    for i in 0..inputs.len() {
        if assigned.contains(&i) {
            continue;
        }
        let mut cluster = vec![i];
        assigned.insert(i);

        for j in (i + 1)..inputs.len() {
            if assigned.contains(&j) {
                continue;
            }
            let sim = normalized_levenshtein(&inputs[i].1, &inputs[j].1);
            if sim >= 0.7 {
                cluster.push(j);
                assigned.insert(j);
            }
        }
        clusters.push(cluster);
    }

    let mut results = vec![];
    for cluster in clusters {
        let session_ids: HashSet<i64> = cluster.iter().map(|&idx| inputs[idx].0).collect();
        if session_ids.len() < 3 {
            continue;
        }

        let mut sorted_sids: Vec<i64> = session_ids.into_iter().collect();
        sorted_sids.sort();
        let representative_text = &inputs[cluster[0]].1;
        let fingerprint = format!(
            "repeated_prompt:{}:{}",
            sorted_sids
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(","),
            &representative_text[..representative_text.len().min(50)]
        );

        let existing: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM insights WHERE fingerprint = ?1 AND status = 'active'",
                rusqlite::params![fingerprint],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if existing {
            continue;
        }

        let matches: Vec<serde_json::Value> = cluster
            .iter()
            .map(|&idx| {
                serde_json::json!({
                    "sessionId": inputs[idx].0,
                    "text": inputs[idx].1,
                    "timestamp": inputs[idx].2,
                    "branch": null,
                })
            })
            .collect();

        let detail = serde_json::json!({ "matches": matches });

        results.push(DetectedInsight {
            insight_type: "repeated_prompt".into(),
            severity: "warning".into(),
            title: "Repeated instruction detected".into(),
            summary: format!("Similar prompt found in {} sessions", sorted_sids.len()),
            detail_json: serde_json::to_string(&detail).unwrap_or_default(),
            fingerprint,
        });
    }

    results
}

fn detect_startup_patterns(conn: &rusqlite::Connection) -> Vec<DetectedInsight> {
    let mut stmt = match conn.prepare(
        "SELECT session_id, payload FROM session_events
         WHERE event_type = 'user_input'
         AND json_extract(payload, '$.position') <= 5
         ORDER BY session_id, created_at ASC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    let mut session_cmds: HashMap<i64, Vec<String>> = HashMap::new();
    for (sid, payload) in rows {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&payload) {
            if let Some(text) = parsed.get("text").and_then(|t| t.as_str()) {
                session_cmds
                    .entry(sid)
                    .or_default()
                    .push(text.to_lowercase().trim().to_string());
            }
        }
    }

    if session_cmds.len() < 3 {
        return vec![];
    }

    let sessions: Vec<(i64, Vec<String>)> = session_cmds.into_iter().collect();
    let mut common_prefix_groups: HashMap<String, Vec<i64>> = HashMap::new();

    for (sid, cmds) in &sessions {
        if cmds.is_empty() {
            continue;
        }
        let key = cmds[0..cmds.len().min(3)].join("|");
        common_prefix_groups.entry(key).or_default().push(*sid);
    }

    let mut results = vec![];
    for (prefix_key, sids) in common_prefix_groups {
        if sids.len() < 3 {
            continue;
        }

        let mut sorted_sids = sids.clone();
        sorted_sids.sort();
        let fingerprint = format!(
            "startup_pattern:{}",
            sorted_sids
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        let existing: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM insights WHERE fingerprint = ?1 AND status = 'active'",
                rusqlite::params![fingerprint],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if existing {
            continue;
        }

        let commands: Vec<&str> = prefix_key.split('|').collect();
        let detail = serde_json::json!({
            "commands": commands,
            "sessions": sorted_sids.iter().map(|s| serde_json::json!({"sessionId": s, "branch": null})).collect::<Vec<_>>(),
        });

        results.push(DetectedInsight {
            insight_type: "startup_pattern".into(),
            severity: "warning".into(),
            title: "Startup routine pattern".into(),
            summary: format!(
                "{} sessions start with similar commands",
                sorted_sids.len()
            ),
            detail_json: serde_json::to_string(&detail).unwrap_or_default(),
            fingerprint,
        });
    }

    results
}

fn detect_similar_sessions(conn: &rusqlite::Connection) -> Vec<DetectedInsight> {
    let mut stmt = match conn.prepare(
        "SELECT se.session_id, json_extract(se.payload, '$.filePath')
         FROM session_events se
         JOIN sessions s ON s.id = se.session_id
         WHERE se.event_type = 'file_operation' AND s.status = 'Running'",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows: Vec<(i64, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    let mut session_files: HashMap<i64, HashSet<String>> = HashMap::new();
    for (sid, file) in rows {
        if let Some(f) = file {
            session_files.entry(sid).or_default().insert(f);
        }
    }

    let session_ids: Vec<i64> = session_files.keys().cloned().collect();
    let mut results = vec![];

    for i in 0..session_ids.len() {
        for j in (i + 1)..session_ids.len() {
            let a = &session_files[&session_ids[i]];
            let b = &session_files[&session_ids[j]];
            if a.is_empty() || b.is_empty() {
                continue;
            }

            let intersection = a.intersection(b).count();
            let union = a.union(b).count();
            let jaccard = intersection as f64 / union as f64;

            if jaccard >= 0.4 {
                let mut pair = [session_ids[i], session_ids[j]];
                pair.sort();
                let fingerprint = format!("similar_sessions:{}:{}", pair[0], pair[1]);

                let existing: bool = conn
                    .query_row(
                        "SELECT COUNT(*) FROM insights WHERE fingerprint = ?1 AND status = 'active'",
                        rusqlite::params![fingerprint],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap_or(0)
                    > 0;
                if existing {
                    continue;
                }

                let shared: Vec<&String> = a.intersection(b).collect();
                let detail = serde_json::json!({
                    "sessionA": {"id": pair[0], "branch": null},
                    "sessionB": {"id": pair[1], "branch": null},
                    "similarity": jaccard,
                    "sharedFiles": shared,
                });

                results.push(DetectedInsight {
                    insight_type: "similar_sessions".into(),
                    severity: "suggestion".into(),
                    title: "Similar sessions detected".into(),
                    summary: format!(
                        "Sessions {} and {} share {} files",
                        pair[0], pair[1], intersection
                    ),
                    detail_json: serde_json::to_string(&detail).unwrap_or_default(),
                    fingerprint,
                });
            }
        }
    }

    results
}

/// Run batch analysis, detecting insights and storing them.
/// Returns the list of newly created insights.
pub async fn run_batch_analysis(ctx: &AppContext) -> Result<Vec<Insight>, CoreError> {
    // Run detection under lock, then release before insert loop
    let all_detected = {
        let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
        let mut results = vec![];
        results.extend(detect_repeated_prompts(&conn));
        results.extend(detect_startup_patterns(&conn));
        results.extend(detect_similar_sessions(&conn));
        results
    };

    // Re-acquire lock for writes
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    let mut new_insights = Vec::new();
    for detected in all_detected {
        let now = chrono::Utc::now().timestamp_millis();
        let insert_result = conn.execute(
            "INSERT OR IGNORE INTO insights (insight_type, severity, title, summary, detail_json, fingerprint, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7)",
            rusqlite::params![
                detected.insight_type,
                detected.severity,
                detected.title,
                detected.summary,
                detected.detail_json,
                detected.fingerprint,
                now,
            ],
        );

        if let Ok(changes) = insert_result {
            if changes > 0 {
                let id = conn.last_insert_rowid();
                let insight = Insight {
                    id,
                    insight_type: detected.insight_type,
                    severity: detected.severity,
                    title: detected.title,
                    summary: detected.summary,
                    detail_json: detected.detail_json,
                    fingerprint: detected.fingerprint,
                    status: "active".into(),
                    created_at: now,
                    resolved_at: None,
                };
                new_insights.push(insight);
            }
        }
    }

    Ok(new_insights)
}
