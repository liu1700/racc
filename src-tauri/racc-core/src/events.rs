use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::broadcast;

/// Events emitted when session or task state changes.
/// Consumed by the WebSocket server (fan-out to clients) and
/// the frontend (via AppHandle.emit()).
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum RaccEvent {
    #[serde(rename = "session_status_changed")]
    SessionStatusChanged {
        session_id: i64,
        status: String,
        pr_url: Option<String>,
        #[serde(skip)]
        source: String, // "local" or "remote" — internal only, not sent to WS clients
    },
    #[serde(rename = "task_status_changed")]
    TaskStatusChanged {
        task_id: i64,
        status: String,
        session_id: Option<i64>,
    },
    #[serde(rename = "task_deleted")]
    TaskDeleted {
        task_id: i64,
    },
    #[serde(rename = "supervisor_action")]
    SupervisorAction {
        action: String,   // "assigned", "restarted", "stopped", "escalated", "completed"
        task_id: i64,
        session_id: Option<i64>,
    },
    #[serde(rename = "supervisor_alert")]
    SupervisorAlert {
        level: String,    // "info", "warning", "needs_input", "failure"
        message: String,
        task_id: Option<i64>,
    },
}

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn emit(&self, event: RaccEvent);
    fn subscribe(&self) -> broadcast::Receiver<RaccEvent>;
}

pub struct BroadcastEventBus {
    tx: broadcast::Sender<RaccEvent>,
}

impl BroadcastEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self { tx }
    }
}

#[async_trait]
impl EventBus for BroadcastEventBus {
    async fn emit(&self, event: RaccEvent) {
        let _ = self.tx.send(event);
    }
    fn subscribe(&self) -> broadcast::Receiver<RaccEvent> {
        self.tx.subscribe()
    }
}
