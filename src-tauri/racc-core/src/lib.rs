pub mod agent;
pub mod error;
pub mod events;
pub mod db;
pub mod ssh;
pub mod transport;
pub mod commands;
pub mod rtk;

pub use error::CoreError;

use std::sync::Arc;
use rusqlite::Connection;
use tokio::sync::broadcast;

use crate::events::EventBus;
use crate::ssh::SshManager;
use crate::transport::manager::TransportManager;

#[derive(Clone, Debug, serde::Serialize)]
pub struct TerminalData {
    pub session_id: i64,
    pub data: Vec<u8>,
}

pub struct AppContext {
    pub db: Arc<std::sync::Mutex<Connection>>,
    pub transport_manager: TransportManager,
    pub ssh_manager: Arc<SshManager>,
    pub event_bus: Arc<dyn EventBus>,
    pub terminal_tx: broadcast::Sender<TerminalData>,
}

impl AppContext {
    pub fn new(
        db: Arc<std::sync::Mutex<Connection>>,
        transport_manager: TransportManager,
        ssh_manager: Arc<SshManager>,
        event_bus: Arc<dyn EventBus>,
        terminal_tx: broadcast::Sender<TerminalData>,
    ) -> Self {
        Self {
            db,
            transport_manager,
            ssh_manager,
            event_bus,
            terminal_tx,
        }
    }
}
