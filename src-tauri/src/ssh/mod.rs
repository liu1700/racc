pub mod config_parser;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting { attempt: u32 },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub struct SshManager {
    connections: Arc<Mutex<HashMap<String, SshConnection>>>,
}

struct SshConnection {
    #[allow(dead_code)]
    client: russh::client::Handle<SshClientHandler>,
    status: ConnectionStatus,
}

struct SshClientHandler;

#[async_trait::async_trait]
impl russh::client::Handler for SshClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // For MVP, accept all host keys.
        // Production should verify against known_hosts.
        Ok(true)
    }
}

impl SshManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn connect(
        &self,
        server_id: &str,
        host: &str,
        port: u16,
        username: &str,
        _auth_method: &str,
        _key_path: Option<&str>,
    ) -> Result<(), String> {
        // 1. Build russh config
        // 2. Connect via russh::client::connect
        // 3. Load key and authenticate
        // 4. Store connection
        let _ = (server_id, host, port, username);
        todo!("Implement SSH connection — will be fleshed out in Task 17")
    }

    pub async fn exec(&self, server_id: &str, command: &str) -> Result<CommandOutput, String> {
        // 1. Get connection
        // 2. Open channel
        // 3. Execute command
        // 4. Collect output
        let _ = (server_id, command);
        todo!("Implement SSH exec — will be fleshed out in Task 17")
    }

    pub async fn open_shell(
        &self,
        server_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<russh::Channel<russh::client::Msg>, String> {
        // For tmux attach — returns interactive channel
        let _ = (server_id, cols, rows);
        todo!("Implement shell — will be fleshed out in Task 17")
    }

    pub async fn disconnect(&self, server_id: &str) -> Result<(), String> {
        let mut conns = self.connections.lock().await;
        if let Some(_conn) = conns.remove(server_id) {
            // client.disconnect(...)
        }
        Ok(())
    }

    pub async fn is_connected(&self, server_id: &str) -> bool {
        let conns = self.connections.lock().await;
        conns
            .get(server_id)
            .map_or(false, |c| c.status == ConnectionStatus::Connected)
    }
}
