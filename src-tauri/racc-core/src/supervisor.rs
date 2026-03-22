use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{info, warn, error};
use tokio::sync::broadcast;

use crate::agent::{AgentType, AgentSignal, HealthPatterns, analyze_output};
use crate::error::CoreError;
use crate::events::RaccEvent;
use crate::AppContext;

// ---------------------------------------------------------------------------
// Status mapping
// ---------------------------------------------------------------------------

fn supervisor_to_base_status(supervisor_status: &str) -> &'static str {
    match supervisor_status {
        "Pending" => "open",
        "Assigned" | "Running" => "working",
        "Completed" | "Failed" => "closed",
        "NeedsInput" => "working",
        _ => "open",
    }
}

/// Atomically update both `supervisor_status` and the base `status` for a task.
pub fn set_task_supervisor_status(
    ctx: &AppContext,
    task_id: i64,
    supervisor_status: &str,
    session_id: Option<i64>,
) -> Result<(), CoreError> {
    let base_status = supervisor_to_base_status(supervisor_status);
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;

    if let Some(sid) = session_id {
        conn.execute(
            "UPDATE tasks SET status = ?1, supervisor_status = ?2, session_id = ?3, updated_at = datetime('now') WHERE id = ?4",
            rusqlite::params![base_status, supervisor_status, sid, task_id],
        )?;
    } else {
        conn.execute(
            "UPDATE tasks SET status = ?1, supervisor_status = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![base_status, supervisor_status, task_id],
        )?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Retry tracking
// ---------------------------------------------------------------------------

/// Increment retry_count and return (retry_count, max_retries).
pub fn increment_retry(ctx: &AppContext, task_id: i64) -> Result<(i64, i64), CoreError> {
    let conn = ctx.db.lock().map_err(|e| CoreError::Other(e.to_string()))?;
    conn.execute(
        "UPDATE tasks SET retry_count = retry_count + 1, last_retry_at = datetime('now'), updated_at = datetime('now') WHERE id = ?1",
        [task_id],
    )?;
    let (retry_count, max_retries): (i64, i64) = conn.query_row(
        "SELECT retry_count, max_retries FROM tasks WHERE id = ?1",
        [task_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok((retry_count, max_retries))
}

// ---------------------------------------------------------------------------
// SessionHealth
// ---------------------------------------------------------------------------

const MAX_OUTPUT_BUFFER: usize = 4096;

pub struct SessionHealth {
    pub last_output_at: Instant,
    pub output_buffer: Vec<u8>,
    pub agent_type: AgentType,
    pub task_id: Option<i64>,
}

impl SessionHealth {
    pub fn new(agent_type: AgentType, task_id: Option<i64>) -> Self {
        Self {
            last_output_at: Instant::now(),
            output_buffer: Vec::new(),
            agent_type,
            task_id,
        }
    }

    pub fn push_output(&mut self, data: &[u8]) {
        self.output_buffer.extend_from_slice(data);
        if self.output_buffer.len() > MAX_OUTPUT_BUFFER {
            let drain_to = self.output_buffer.len() - MAX_OUTPUT_BUFFER;
            self.output_buffer.drain(..drain_to);
        }
        self.last_output_at = Instant::now();
    }

    pub fn is_stuck(&self) -> bool {
        let patterns = HealthPatterns::for_agent(&self.agent_type);
        self.last_output_at.elapsed() > Duration::from_secs(patterns.stuck_timeout_secs)
    }

    pub fn analyze(&self) -> AgentSignal {
        analyze_output(&self.output_buffer, &self.agent_type, MAX_OUTPUT_BUFFER)
    }
}

// ---------------------------------------------------------------------------
// Supervisor
// ---------------------------------------------------------------------------

pub struct Supervisor {
    ctx: Arc<AppContext>,
    interval: Duration,
    health: HashMap<i64, SessionHealth>,
}

impl Supervisor {
    pub fn new(ctx: Arc<AppContext>, interval_ms: u64) -> Self {
        Self {
            ctx,
            interval: Duration::from_millis(interval_ms),
            health: HashMap::new(),
        }
    }

    /// Spawn the supervisor loop. Returns a JoinHandle for the background task.
    pub fn start(mut self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut terminal_rx = self.ctx.terminal_tx.subscribe();
            let mut tick_interval = tokio::time::interval(self.interval);

            info!("Supervisor started (interval={}ms)", self.interval.as_millis());

            loop {
                tokio::select! {
                    result = terminal_rx.recv() => {
                        match result {
                            Ok(td) => {
                                if let Some(health) = self.health.get_mut(&td.session_id) {
                                    health.push_output(&td.data);
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {
                                // Output clearly happened — reset all idle timers
                                for health in self.health.values_mut() {
                                    health.last_output_at = Instant::now();
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                info!("Supervisor: terminal_tx closed, shutting down");
                                break;
                            }
                        }
                    }
                    _ = tick_interval.tick() => {
                        self.reconcile().await;
                    }
                }
            }
        })
    }

    // -----------------------------------------------------------------------
    // Reconciliation
    // -----------------------------------------------------------------------

    async fn reconcile(&mut self) {
        self.reconcile_session_liveness().await;
        self.check_health().await;
        self.assign_pending_tasks().await;
    }

    /// Probe running sessions — mark dead ones as Completed or Disconnected.
    async fn reconcile_session_liveness(&mut self) {
        let running_sessions: Vec<(i64, Option<String>)> = {
            let conn = match self.ctx.db.lock() {
                Ok(c) => c,
                Err(e) => {
                    error!("Supervisor: failed to lock db: {}", e);
                    return;
                }
            };
            let mut stmt = match conn.prepare(
                "SELECT id, server_id FROM sessions WHERE status = 'Running'",
            ) {
                Ok(s) => s,
                Err(e) => {
                    error!("Supervisor: failed to query sessions: {}", e);
                    return;
                }
            };
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap_or_else(|_| {
                    // Return an empty Rows iterator on error
                    // We handle this by returning an empty vec below
                    unreachable!()
                })
                .filter_map(|r| r.ok())
                .collect()
        };

        for (session_id, server_id) in running_sessions {
            let alive = if let Some(ref sid) = server_id {
                // Remote session: check via SSH/tmux
                if self.ctx.ssh_manager.is_connected(sid).await {
                    let tmux_name = format!("racc-{}", session_id);
                    match self
                        .ctx
                        .ssh_manager
                        .exec(sid, &format!("tmux has-session -t {}", tmux_name))
                        .await
                    {
                        Ok(output) if output.exit_code == 0 => true,
                        _ => false,
                    }
                } else {
                    false
                }
            } else {
                // Local session: check transport manager
                self.ctx.transport_manager.is_alive(session_id).await
            };

            if !alive {
                let new_status = if server_id.is_some() { "Disconnected" } else { "Completed" };
                let conn = match self.ctx.db.lock() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let _ = conn.execute(
                    "UPDATE sessions SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
                    rusqlite::params![new_status, session_id],
                );
                info!(
                    "Supervisor: session {} marked as {} (not alive)",
                    session_id, new_status
                );
            }
        }
    }

    /// For each tracked session, check health signals and act accordingly.
    async fn check_health(&mut self) {
        let session_ids: Vec<i64> = self.health.keys().copied().collect();

        for session_id in session_ids {
            let alive = self.ctx.transport_manager.is_alive(session_id).await;

            // We need to borrow self.health mutably potentially, so get the analysis first
            let (signal, is_stuck, task_id) = {
                let health = match self.health.get(&session_id) {
                    Some(h) => h,
                    None => continue,
                };
                (health.analyze(), health.is_stuck(), health.task_id)
            };

            let Some(task_id) = task_id else {
                continue;
            };

            if !alive {
                // Process exited
                let _ = set_task_supervisor_status(&self.ctx, task_id, "Completed", None);
                self.ctx.event_bus.emit(RaccEvent::SupervisorAction {
                    action: "completed".to_string(),
                    task_id,
                    session_id: Some(session_id),
                }).await;
                self.health.remove(&session_id);
                info!("Supervisor: session {} exited, task {} marked Completed", session_id, task_id);
                continue;
            }

            match signal {
                AgentSignal::Error(ref msg) => {
                    warn!("Supervisor: error signal in session {}: {}", session_id, msg);
                    match increment_retry(&self.ctx, task_id) {
                        Ok((retry_count, max_retries)) => {
                            if retry_count < max_retries {
                                let _ = set_task_supervisor_status(&self.ctx, task_id, "Pending", None);
                                let _ = self.ctx.transport_manager.remove(session_id).await;
                                self.health.remove(&session_id);
                                self.ctx.event_bus.emit(RaccEvent::SupervisorAction {
                                    action: "restarted".to_string(),
                                    task_id,
                                    session_id: Some(session_id),
                                }).await;
                                info!("Supervisor: task {} retry {}/{}, re-queued as Pending", task_id, retry_count, max_retries);
                            } else {
                                let _ = set_task_supervisor_status(&self.ctx, task_id, "Failed", None);
                                self.ctx.event_bus.emit(RaccEvent::SupervisorAlert {
                                    level: "failure".to_string(),
                                    message: format!("Task {} failed after {} retries: {}", task_id, retry_count, msg),
                                    task_id: Some(task_id),
                                }).await;
                                self.health.remove(&session_id);
                                error!("Supervisor: task {} failed permanently after {} retries", task_id, retry_count);
                            }
                        }
                        Err(e) => {
                            error!("Supervisor: failed to increment retry for task {}: {}", task_id, e);
                        }
                    }
                }
                AgentSignal::Completion => {
                    // Prompt reappearance in last 200 chars — task is done
                    let _ = set_task_supervisor_status(&self.ctx, task_id, "Completed", None);
                    self.ctx.event_bus.emit(RaccEvent::SupervisorAction {
                        action: "completed".to_string(),
                        task_id,
                        session_id: Some(session_id),
                    }).await;
                    self.health.remove(&session_id);
                    info!("Supervisor: task {} completed (prompt detected)", task_id);
                }
                AgentSignal::Idle => {
                    if is_stuck {
                        let _ = set_task_supervisor_status(&self.ctx, task_id, "NeedsInput", Some(session_id));
                        self.ctx.event_bus.emit(RaccEvent::SupervisorAlert {
                            level: "needs_input".to_string(),
                            message: format!("Task {} appears stuck — no output for extended period", task_id),
                            task_id: Some(task_id),
                        }).await;
                        warn!("Supervisor: task {} appears stuck in session {}", task_id, session_id);
                    }
                    // Otherwise: do nothing
                }
            }
        }
    }

    /// Find repos with pending tasks and assign them to new sessions.
    async fn assign_pending_tasks(&mut self) {
        let max_agents: usize = std::env::var("RACC_MAX_AGENTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        // Count currently running sessions
        let running_count: usize = {
            let conn = match self.ctx.db.lock() {
                Ok(c) => c,
                Err(_) => return,
            };
            conn.query_row(
                "SELECT COUNT(*) FROM sessions WHERE status = 'Running'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as usize
        };

        if running_count >= max_agents {
            return;
        }

        let available_slots = max_agents - running_count;

        // Get all repos that have pending tasks
        let repos_with_pending: Vec<i64> = {
            let conn = match self.ctx.db.lock() {
                Ok(c) => c,
                Err(_) => return,
            };
            let mut stmt = match conn.prepare(
                "SELECT DISTINCT repo_id FROM tasks WHERE supervisor_status = 'Pending' OR (supervisor_status IS NULL AND status = 'open')",
            ) {
                Ok(s) => s,
                Err(_) => return,
            };
            stmt.query_map([], |row| row.get(0))
                .unwrap_or_else(|_| unreachable!())
                .filter_map(|r| r.ok())
                .collect()
        };

        let mut assigned = 0;
        for repo_id in repos_with_pending {
            if assigned >= available_slots {
                break;
            }

            let pending_tasks = match crate::commands::task::get_pending_tasks(&self.ctx, repo_id) {
                Ok(tasks) => tasks,
                Err(e) => {
                    error!("Supervisor: failed to get pending tasks for repo {}: {}", repo_id, e);
                    continue;
                }
            };

            for task in pending_tasks {
                if assigned >= available_slots {
                    break;
                }

                // Check exponential backoff: 2^retry_count * 5 seconds since last_retry_at
                if task.retry_count > 0 {
                    if let Some(ref last_retry_str) = task.last_retry_at {
                        if let Ok(last_retry) = chrono::NaiveDateTime::parse_from_str(last_retry_str, "%Y-%m-%d %H:%M:%S") {
                            let backoff_secs = (2_i64.pow(task.retry_count as u32)) * 5;
                            let now = chrono::Utc::now().naive_utc();
                            let elapsed = (now - last_retry).num_seconds();
                            if elapsed < backoff_secs {
                                continue; // Still in backoff window
                            }
                        }
                    }
                }

                let branch = format!("racc/task-{}", task.id);

                // Mark as Assigned
                if let Err(e) = set_task_supervisor_status(&self.ctx, task.id, "Assigned", None) {
                    error!("Supervisor: failed to mark task {} as Assigned: {}", task.id, e);
                    continue;
                }

                // Create a session for this task
                match crate::commands::session::create_session(
                    &self.ctx,
                    repo_id,
                    true,
                    Some(branch),
                    None,
                    Some(task.description.clone()),
                    None,
                    Some(true),
                )
                .await
                {
                    Ok(session) => {
                        let _ = set_task_supervisor_status(
                            &self.ctx,
                            task.id,
                            "Running",
                            Some(session.id),
                        );
                        self.health.insert(
                            session.id,
                            SessionHealth::new(AgentType::ClaudeCode, Some(task.id)),
                        );
                        self.ctx.event_bus.emit(RaccEvent::SupervisorAction {
                            action: "assigned".to_string(),
                            task_id: task.id,
                            session_id: Some(session.id),
                        }).await;
                        assigned += 1;
                        info!(
                            "Supervisor: assigned task {} to session {}",
                            task.id, session.id
                        );
                    }
                    Err(e) => {
                        let _ = set_task_supervisor_status(&self.ctx, task.id, "Failed", None);
                        self.ctx.event_bus.emit(RaccEvent::SupervisorAlert {
                            level: "failure".to_string(),
                            message: format!("Failed to create session for task {}: {}", task.id, e),
                            task_id: Some(task.id),
                        }).await;
                        error!("Supervisor: failed to create session for task {}: {}", task.id, e);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supervisor_to_base_status() {
        assert_eq!(supervisor_to_base_status("Pending"), "open");
        assert_eq!(supervisor_to_base_status("Assigned"), "working");
        assert_eq!(supervisor_to_base_status("Running"), "working");
        assert_eq!(supervisor_to_base_status("Completed"), "closed");
        assert_eq!(supervisor_to_base_status("Failed"), "closed");
        assert_eq!(supervisor_to_base_status("NeedsInput"), "working");
        assert_eq!(supervisor_to_base_status("unknown"), "open");
    }

    #[test]
    fn test_session_health_sliding_window() {
        let mut health = SessionHealth::new(AgentType::ClaudeCode, Some(1));
        let chunk = vec![b'a'; 3000];
        health.push_output(&chunk);
        health.push_output(&chunk); // 6000 total
        assert_eq!(health.output_buffer.len(), 4096);
    }

    #[test]
    fn test_session_health_stuck_detection() {
        let mut health = SessionHealth::new(AgentType::ClaudeCode, Some(1));
        assert!(!health.is_stuck());
        health.last_output_at = Instant::now() - Duration::from_secs(200);
        assert!(health.is_stuck()); // 200 > 180 threshold
    }

    #[test]
    fn test_session_health_not_stuck_within_threshold() {
        let mut health = SessionHealth::new(AgentType::ClaudeCode, Some(1));
        health.last_output_at = Instant::now() - Duration::from_secs(100);
        assert!(!health.is_stuck()); // 100 < 180
    }
}
