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
}

/// Type alias for the broadcast sender stored in Tauri managed state.
pub type EventSender = broadcast::Sender<RaccEvent>;

/// Create a new event bus with the given capacity.
pub fn create_event_bus() -> (EventSender, broadcast::Receiver<RaccEvent>) {
    broadcast::channel(64)
}
