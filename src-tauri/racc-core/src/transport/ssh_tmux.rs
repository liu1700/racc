use super::{Transport, TransportError};
use crate::ssh::SshManager;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// Transport that wraps an SSH channel connected to a tmux session on a remote server.
///
/// The background reader owns the `Channel` (for `wait()`), while a separate
/// `ChannelTx` writer (created via `make_writer()` before the channel is moved)
/// handles outgoing data. Resize is routed to the reader task (which owns the
/// channel) so it can issue the SSH window-change; kill is dispatched as a tmux
/// command over `SshManager::exec`.
pub struct SshTmuxTransport {
    session_id: i64,
    server_id: String,
    ssh_manager: Arc<SshManager>,
    alive: Arc<std::sync::atomic::AtomicBool>,
    writer: Arc<Mutex<Box<dyn tokio::io::AsyncWrite + Send + Unpin>>>,
    /// Sends (cols, rows) resize requests to the reader task. The reader owns
    /// the channel, so it performs the SSH `window_change` that resizes the PTY
    /// client — the remote tmux window then follows the client size.
    resize_tx: tokio::sync::mpsc::UnboundedSender<(u16, u16)>,
    /// Handle to the background reader task so we can abort on close.
    read_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl SshTmuxTransport {
    /// Spawn a new remote tmux session and start streaming its output.
    ///
    /// 1. Creates a tmux session on the remote via `ssh exec`.
    /// 2. Opens an interactive shell channel with a PTY.
    /// 3. Sends `tmux attach` through the shell.
    /// 4. Spawns a background tokio task that reads channel output and sends
    ///    `TerminalData` via the broadcast channel + feeds the ring-buffer sender.
    pub async fn spawn(
        session_id: i64,
        server_id: &str,
        agent_cmd: &str,
        cols: u16,
        rows: u16,
        ssh_manager: Arc<SshManager>,
        terminal_tx: tokio::sync::broadcast::Sender<crate::TerminalData>,
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

        // 5. Spawn background read task. It also owns the resize receiver so it
        //    can issue SSH window-change requests (the channel can't be shared).
        let (resize_tx, resize_rx) = tokio::sync::mpsc::unbounded_channel::<(u16, u16)>();
        let read_task = Self::spawn_reader(
            session_id,
            channel,
            resize_rx,
            alive.clone(),
            terminal_tx,
            buffer_tx,
        );

        Ok(Self {
            session_id,
            server_id: server_id.to_string(),
            ssh_manager,
            alive,
            writer: Arc::new(Mutex::new(writer)),
            resize_tx,
            read_task: Mutex::new(Some(read_task)),
        })
    }

    /// Spawn the background tokio task that reads `ChannelMsg` from the SSH
    /// channel and forwards data via the broadcast channel + ring buffer. It
    /// also services resize requests: because the channel can't be shared, the
    /// reader (which owns it) issues the SSH `window_change`. The resize is
    /// captured into a local and applied *after* the `select!` block — calling
    /// `channel.window_change()` inside the resize arm would clash with the
    /// `channel.wait()` borrow held by the other arm.
    fn spawn_reader(
        session_id: i64,
        mut channel: russh::Channel<russh::client::Msg>,
        mut resize_rx: tokio::sync::mpsc::UnboundedReceiver<(u16, u16)>,
        alive: Arc<std::sync::atomic::AtomicBool>,
        terminal_tx: tokio::sync::broadcast::Sender<crate::TerminalData>,
        buffer_tx: tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            while alive.load(std::sync::atomic::Ordering::SeqCst) {
                let mut pending_resize: Option<(u16, u16)> = None;
                tokio::select! {
                    msg = channel.wait() => {
                        match msg {
                            Some(russh::ChannelMsg::Data { ref data }) => {
                                let bytes = data.to_vec();
                                let _ = terminal_tx.send(crate::TerminalData {
                                    session_id,
                                    data: bytes.clone(),
                                });
                                let _ = buffer_tx.send((session_id, bytes));
                            }
                            Some(russh::ChannelMsg::ExtendedData { ref data, .. }) => {
                                // Forward stderr as well (tmux may emit on stderr)
                                let bytes = data.to_vec();
                                let _ = terminal_tx.send(crate::TerminalData {
                                    session_id,
                                    data: bytes.clone(),
                                });
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
                    maybe = resize_rx.recv() => {
                        match maybe {
                            // Coalesce to the latest pending size to avoid a burst
                            // of window-changes during a drag-resize.
                            Some(size) => {
                                pending_resize = Some(size);
                                while let Ok(newer) = resize_rx.try_recv() {
                                    pending_resize = Some(newer);
                                }
                            }
                            None => {} // all senders dropped; keep reading
                        }
                    }
                }
                if let Some((cols, rows)) = pending_resize {
                    let _ = channel
                        .window_change(cols as u32, rows as u32, 0, 0)
                        .await;
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

    /// Resize the remote terminal. We resize the SSH PTY client (via an
    /// `window_change` issued by the reader task, which owns the channel); the
    /// remote tmux window follows the client size. Resizing the tmux window
    /// directly via `resize-window` doesn't help because tmux clamps the window
    /// to the attached client's size — which is exactly what we resize here.
    async fn resize(&self, cols: u16, rows: u16) -> Result<(), TransportError> {
        // Best-effort: if the reader task has exited the request is simply
        // dropped (the session is gone anyway).
        let _ = self.resize_tx.send((cols, rows));
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
