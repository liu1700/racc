use super::{Transport, TransportError};
use crate::ssh::SshManager;
use async_trait::async_trait;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// Transport that wraps an SSH channel connected to a tmux session on a remote server.
///
/// The background reader owns the `Channel` (for `wait()`), while a separate
/// `ChannelTx` writer (created via `make_writer()` before the channel is moved)
/// handles outgoing data. Control operations (resize, kill) are dispatched as
/// tmux commands over `SshManager::exec`.
pub struct SshTmuxTransport {
    session_id: i64,
    server_id: String,
    ssh_manager: Arc<SshManager>,
    alive: Arc<std::sync::atomic::AtomicBool>,
    writer: Arc<Mutex<Box<dyn tokio::io::AsyncWrite + Send + Unpin>>>,
    /// Handle to the background reader task so we can abort on close.
    read_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl SshTmuxTransport {
    /// Spawn a new remote tmux session and start streaming its output.
    ///
    /// 1. Creates a tmux session on the remote via `ssh exec`.
    /// 2. Opens an interactive shell channel with a PTY.
    /// 3. Sends `tmux attach` through the shell.
    /// 4. Spawns a background tokio task that reads channel output and emits
    ///    `transport:data` events + feeds the ring-buffer sender.
    pub async fn spawn(
        session_id: i64,
        server_id: &str,
        agent_cmd: &str,
        cols: u16,
        rows: u16,
        ssh_manager: Arc<SshManager>,
        app: AppHandle,
        buffer_tx: tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)>,
    ) -> Result<Self, TransportError> {
        let tmux_session_name = format!("racc-{}", session_id);

        // 1. Create tmux session on remote (detached, running the agent command)
        let create_cmd = format!(
            "tmux new-session -d -s {} -x {} -y {} '{}'",
            tmux_session_name, cols, rows, agent_cmd
        );
        ssh_manager
            .exec(server_id, &create_cmd)
            .await
            .map_err(|e| TransportError::IoError(format!("Failed to create remote tmux session: {}", e)))?;

        // 2. Open interactive shell channel with PTY
        let channel = ssh_manager
            .open_shell(server_id, cols, rows)
            .await
            .map_err(|e| TransportError::IoError(format!("Failed to open SSH shell: {}", e)))?;

        // 3. Create writer from channel before moving it into the read task.
        //    `make_writer()` clones the internal sender so it is independent.
        let writer: Box<dyn tokio::io::AsyncWrite + Send + Unpin> =
            Box::new(channel.make_writer());

        // 4. Send `tmux attach` command through the shell
        let attach_cmd = format!("tmux attach -t {}\n", tmux_session_name);
        channel
            .data(attach_cmd.as_bytes())
            .await
            .map_err(|e| TransportError::IoError(format!("Failed to attach to tmux: {}", e)))?;

        let alive = Arc::new(std::sync::atomic::AtomicBool::new(true));

        // 5. Spawn background read task
        let read_task = Self::spawn_reader(session_id, channel, alive.clone(), app, buffer_tx);

        Ok(Self {
            session_id,
            server_id: server_id.to_string(),
            ssh_manager,
            alive,
            writer: Arc::new(Mutex::new(writer)),
            read_task: Mutex::new(Some(read_task)),
        })
    }

    /// Spawn the background tokio task that reads `ChannelMsg` from the SSH
    /// channel and forwards data to the frontend + ring buffer.
    fn spawn_reader(
        session_id: i64,
        mut channel: russh::Channel<russh::client::Msg>,
        alive: Arc<std::sync::atomic::AtomicBool>,
        app: AppHandle,
        buffer_tx: tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            while alive.load(std::sync::atomic::Ordering::SeqCst) {
                match channel.wait().await {
                    Some(russh::ChannelMsg::Data { ref data }) => {
                        let bytes = data.to_vec();
                        let _ = app.emit(
                            "transport:data",
                            serde_json::json!({
                                "session_id": session_id,
                                "data": &bytes,
                            }),
                        );
                        let _ = buffer_tx.send((session_id, bytes));
                    }
                    Some(russh::ChannelMsg::ExtendedData { ref data, .. }) => {
                        // Forward stderr as well (tmux may emit on stderr)
                        let bytes = data.to_vec();
                        let _ = app.emit(
                            "transport:data",
                            serde_json::json!({
                                "session_id": session_id,
                                "data": &bytes,
                            }),
                        );
                        let _ = buffer_tx.send((session_id, bytes));
                    }
                    Some(russh::ChannelMsg::Eof) | Some(russh::ChannelMsg::Close) => {
                        alive.store(false, std::sync::atomic::Ordering::SeqCst);
                        break;
                    }
                    Some(_) => {
                        // Ignore other message types (ExitStatus, WindowAdjusted, etc.)
                    }
                    None => {
                        // Channel closed
                        alive.store(false, std::sync::atomic::Ordering::SeqCst);
                        break;
                    }
                }
            }
        })
    }
}

#[async_trait]
impl Transport for SshTmuxTransport {
    /// Write data to the SSH channel (forwarded to the tmux session).
    async fn write(&self, data: &[u8]) -> Result<(), TransportError> {
        let mut writer = self.writer.lock().await;
        writer
            .write_all(data)
            .await
            .map_err(|e| TransportError::IoError(format!("SSH write failed: {}", e)))?;
        writer
            .flush()
            .await
            .map_err(|e| TransportError::IoError(format!("SSH flush failed: {}", e)))?;
        Ok(())
    }

    /// Resize the remote PTY by running `tmux resize-window` + resizing the
    /// SSH PTY via a separate exec channel.
    async fn resize(&self, cols: u16, rows: u16) -> Result<(), TransportError> {
        let tmux_session_name = format!("racc-{}", self.session_id);

        // Resize the tmux window dimensions
        let resize_cmd = format!(
            "tmux resize-window -t {} -x {} -y {}",
            tmux_session_name, cols, rows
        );
        self.ssh_manager
            .exec(&self.server_id, &resize_cmd)
            .await
            .map_err(|e| TransportError::IoError(format!("Failed to resize tmux window: {}", e)))?;

        Ok(())
    }

    /// Kill the remote tmux session and close the SSH channel.
    async fn close(&self) -> Result<(), TransportError> {
        self.alive
            .store(false, std::sync::atomic::Ordering::SeqCst);

        let tmux_session_name = format!("racc-{}", self.session_id);

        // Kill the remote tmux session (best-effort; may fail if already gone)
        let kill_cmd = format!("tmux kill-session -t {}", tmux_session_name);
        let _ = self.ssh_manager.exec(&self.server_id, &kill_cmd).await;

        // Abort the background reader
        {
            let mut task = self.read_task.lock().await;
            if let Some(handle) = task.take() {
                handle.abort();
            }
        }

        // Shut down the writer (sends EOF on the channel)
        {
            let mut writer = self.writer.lock().await;
            let _ = writer.shutdown().await;
        }

        Ok(())
    }

    fn is_alive(&self) -> bool {
        self.alive.load(std::sync::atomic::Ordering::SeqCst)
    }
}
