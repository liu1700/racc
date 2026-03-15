pub mod config_parser;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
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

/// Try to find a default SSH private key in ~/.ssh/
fn find_default_key_path() -> Option<String> {
    let ssh_dir = dirs::home_dir()?.join(".ssh");
    // Prefer ed25519, then rsa
    for name in &["id_ed25519", "id_rsa", "id_ecdsa"] {
        let path = ssh_dir.join(name);
        if path.exists() {
            return path.to_str().map(|s| s.to_string());
        }
    }
    None
}

impl SshManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Establish an SSH connection and store it under `server_id`.
    ///
    /// `auth_method` can be `"key"`, `"agent"`, or `"ssh_config"`.
    /// For `"key"`, `key_path` should point to a private key file.
    /// For `"agent"`, the SSH agent (SSH_AUTH_SOCK) is used.
    /// For `"ssh_config"`, we try key first, then agent.
    pub async fn connect(
        &self,
        server_id: &str,
        host: &str,
        port: u16,
        username: &str,
        auth_method: &str,
        key_path: Option<&str>,
    ) -> Result<(), String> {
        // 1. Build russh client config
        let config = russh::client::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(60)),
            keepalive_interval: Some(std::time::Duration::from_secs(15)),
            keepalive_max: 3,
            ..Default::default()
        };
        let config = Arc::new(config);

        // 2. Connect to the remote host
        let handler = SshClientHandler;
        let mut handle = russh::client::connect(config, (host, port), handler)
            .await
            .map_err(|e| format!("SSH connection failed: {}", e))?;

        // 3. Authenticate based on the chosen method
        let authenticated = match auth_method {
            "key" => {
                self.authenticate_with_key(&mut handle, username, key_path)
                    .await?
            }
            "agent" => self.authenticate_with_agent(&mut handle, username).await?,
            "ssh_config" | _ => {
                // Try key first (from explicit path or default), then fall back to agent
                let key_auth = self
                    .authenticate_with_key(&mut handle, username, key_path)
                    .await;
                match key_auth {
                    Ok(true) => true,
                    _ => {
                        // Fall back to agent
                        self.authenticate_with_agent(&mut handle, username)
                            .await
                            .unwrap_or(false)
                    }
                }
            }
        };

        if !authenticated {
            return Err("SSH authentication failed: no accepted auth method".to_string());
        }

        // 4. Store connection
        let conn = SshConnection {
            client: handle,
            status: ConnectionStatus::Connected,
        };
        let mut conns = self.connections.lock().await;
        conns.insert(server_id.to_string(), conn);

        Ok(())
    }

    /// Authenticate using a private key file.
    async fn authenticate_with_key(
        &self,
        handle: &mut russh::client::Handle<SshClientHandler>,
        username: &str,
        key_path: Option<&str>,
    ) -> Result<bool, String> {
        let resolved_path = match key_path {
            Some(p) => Some(p.to_string()),
            None => find_default_key_path(),
        };
        let path = resolved_path
            .ok_or_else(|| "No SSH key path provided and no default key found".to_string())?;

        if !Path::new(&path).exists() {
            return Err(format!("SSH key file not found: {}", path));
        }

        // Load the private key (None = no passphrase; passphrase-protected keys
        // require ssh-agent instead for MVP)
        let key_pair = russh_keys::load_secret_key(&path, None)
            .map_err(|e| format!("Failed to load SSH key '{}': {}", path, e))?;

        let authed = handle
            .authenticate_publickey(username, Arc::new(key_pair))
            .await
            .map_err(|e| format!("SSH public key auth failed: {}", e))?;

        Ok(authed)
    }

    /// Authenticate using the SSH agent (SSH_AUTH_SOCK).
    async fn authenticate_with_agent(
        &self,
        handle: &mut russh::client::Handle<SshClientHandler>,
        username: &str,
    ) -> Result<bool, String> {
        let mut agent = russh_keys::agent::client::AgentClient::connect_env()
            .await
            .map_err(|e| format!("Failed to connect to SSH agent: {}", e))?;

        let identities = agent
            .request_identities()
            .await
            .map_err(|e| format!("Failed to list agent identities: {}", e))?;

        if identities.is_empty() {
            return Err("SSH agent has no identities".to_string());
        }

        // Try each identity from the agent until one succeeds
        for identity in &identities {
            let (agent_returned, auth_result) = handle
                .authenticate_future(username, identity.clone(), agent)
                .await;
            agent = agent_returned;
            match auth_result {
                Ok(true) => return Ok(true),
                Ok(false) => continue,
                Err(_) => continue,
            }
        }

        Ok(false)
    }

    /// Execute a command on a remote server and return its output.
    pub async fn exec(&self, server_id: &str, command: &str) -> Result<CommandOutput, String> {
        let conns = self.connections.lock().await;
        let conn = conns
            .get(server_id)
            .ok_or_else(|| format!("No connection found for server '{}'", server_id))?;

        if conn.status != ConnectionStatus::Connected {
            return Err(format!("Server '{}' is not connected", server_id));
        }

        // Open a session channel
        let mut channel = conn
            .client
            .channel_open_session()
            .await
            .map_err(|e| format!("Failed to open SSH channel: {}", e))?;

        // Execute the command
        channel
            .exec(true, command)
            .await
            .map_err(|e| format!("Failed to exec command: {}", e))?;

        // Drop the lock before reading channel output (which may take a while)
        drop(conns);

        // Collect stdout, stderr, and exit status from channel messages
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code: i32 = -1;

        while let Some(msg) = channel.wait().await {
            match msg {
                russh::ChannelMsg::Data { ref data } => {
                    stdout.extend_from_slice(data);
                }
                russh::ChannelMsg::ExtendedData { ref data, ext } => {
                    if ext == 1 {
                        // ext == 1 is stderr per SSH spec
                        stderr.extend_from_slice(data);
                    }
                }
                russh::ChannelMsg::ExitStatus { exit_status } => {
                    exit_code = exit_status as i32;
                }
                russh::ChannelMsg::ExitSignal { .. } => {
                    // Process was killed by a signal
                    if exit_code == -1 {
                        exit_code = 128; // Convention: 128 + signal
                    }
                }
                russh::ChannelMsg::Eof | russh::ChannelMsg::Close => {
                    break;
                }
                _ => {}
            }
        }

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
            exit_code,
        })
    }

    /// Open an interactive shell channel with a PTY for terminal-based I/O
    /// (e.g., tmux attach). Returns the raw channel for bidirectional streaming.
    pub async fn open_shell(
        &self,
        server_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<russh::Channel<russh::client::Msg>, String> {
        let conns = self.connections.lock().await;
        let conn = conns
            .get(server_id)
            .ok_or_else(|| format!("No connection found for server '{}'", server_id))?;

        if conn.status != ConnectionStatus::Connected {
            return Err(format!("Server '{}' is not connected", server_id));
        }

        // Open a session channel
        let channel = conn
            .client
            .channel_open_session()
            .await
            .map_err(|e| format!("Failed to open SSH channel: {}", e))?;

        // Request a PTY
        channel
            .request_pty(
                false,
                "xterm-256color",
                cols as u32,
                rows as u32,
                0,
                0,
                &[],
            )
            .await
            .map_err(|e| format!("Failed to request PTY: {}", e))?;

        // Request a shell
        channel
            .request_shell(true)
            .await
            .map_err(|e| format!("Failed to request shell: {}", e))?;

        Ok(channel)
    }

    /// Disconnect from a remote server and remove its stored connection.
    pub async fn disconnect(&self, server_id: &str) -> Result<(), String> {
        let mut conns = self.connections.lock().await;
        if let Some(conn) = conns.remove(server_id) {
            let _ = conn
                .client
                .disconnect(russh::Disconnect::ByApplication, "user disconnect", "en")
                .await;
        }
        Ok(())
    }

    /// Check if a connection exists and is in Connected status.
    pub async fn is_connected(&self, server_id: &str) -> bool {
        let conns = self.connections.lock().await;
        conns
            .get(server_id)
            .map_or(false, |c| c.status == ConnectionStatus::Connected)
    }

    /// Attempt to reconnect with exponential backoff (up to 5 attempts).
    pub async fn reconnect(
        &self,
        server_id: &str,
        host: &str,
        port: u16,
        username: &str,
        auth_method: &str,
        key_path: Option<&str>,
    ) -> Result<(), String> {
        // Update status to reconnecting
        {
            let mut conns = self.connections.lock().await;
            if let Some(conn) = conns.get_mut(server_id) {
                conn.status = ConnectionStatus::Reconnecting { attempt: 0 };
            }
        }

        for attempt in 0..5u32 {
            // Update attempt count
            {
                let mut conns = self.connections.lock().await;
                if let Some(conn) = conns.get_mut(server_id) {
                    conn.status = ConnectionStatus::Reconnecting { attempt };
                }
            }

            // Exponential backoff: 1s, 2s, 4s, 8s, 16s
            let delay = std::time::Duration::from_secs(1 << attempt);
            tokio::time::sleep(delay).await;

            match self
                .connect(server_id, host, port, username, auth_method, key_path)
                .await
            {
                Ok(()) => return Ok(()),
                Err(_) if attempt < 4 => continue,
                Err(e) => {
                    // Mark as disconnected after all retries exhausted
                    let mut conns = self.connections.lock().await;
                    if let Some(conn) = conns.get_mut(server_id) {
                        conn.status = ConnectionStatus::Disconnected;
                    }
                    return Err(format!("Failed to reconnect after 5 attempts: {}", e));
                }
            }
        }
        unreachable!()
    }
}
