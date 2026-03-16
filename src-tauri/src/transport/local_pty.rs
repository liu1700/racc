use super::{Transport, TransportError};
use async_trait::async_trait;
use std::sync::Arc;
use std::io::{Read, Write};
use tokio::sync::Mutex;
use tauri::{AppHandle, Emitter};

pub struct LocalPtyTransport {
    _session_id: i64,
    pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pty_master: Arc<Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>>,
    alive: Arc<std::sync::atomic::AtomicBool>,
}

impl LocalPtyTransport {
    pub async fn spawn(
        session_id: i64,
        cwd: &str,
        cmd: &str,
        cols: u16,
        rows: u16,
        app: AppHandle,
        buffer_tx: tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)>,
    ) -> Result<Self, TransportError> {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| TransportError::IoError(e.to_string()))?;

        let mut cmd_builder = CommandBuilder::new(cmd);
        cmd_builder.cwd(cwd);
        cmd_builder.env("TERM", "xterm-256color");
        let _child = pair.slave.spawn_command(cmd_builder)
            .map_err(|e| TransportError::IoError(e.to_string()))?;
        drop(pair.slave);

        let writer = pair.master.take_writer()
            .map_err(|e| TransportError::IoError(e.to_string()))?;
        let mut reader = pair.master.try_clone_reader()
            .map_err(|e| TransportError::IoError(e.to_string()))?;

        let alive = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let alive_clone = alive.clone();
        let sid = session_id;

        // Background read task: PTY stdout → event emit + ring buffer
        tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 4096];
            loop {
                if !alive_clone.load(std::sync::atomic::Ordering::SeqCst) { break; }
                match reader.read(&mut buf) {
                    Ok(0) => { alive_clone.store(false, std::sync::atomic::Ordering::SeqCst); break; }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        let _ = app.emit("transport:data", serde_json::json!({ "session_id": sid, "data": &data }));
                        let _ = buffer_tx.send((sid, data));
                    }
                    Err(_) => { alive_clone.store(false, std::sync::atomic::Ordering::SeqCst); break; }
                }
            }
        });

        Ok(Self {
            _session_id: session_id,
            pty_writer: Arc::new(Mutex::new(writer)),
            pty_master: Arc::new(Mutex::new(Some(pair.master))),
            alive,
        })
    }
}

#[async_trait]
impl Transport for LocalPtyTransport {
    async fn write(&self, data: &[u8]) -> Result<(), TransportError> {
        let mut writer = self.pty_writer.lock().await;
        writer.write_all(data).map_err(|e| TransportError::IoError(e.to_string()))?;
        Ok(())
    }

    async fn resize(&self, cols: u16, rows: u16) -> Result<(), TransportError> {
        let master = self.pty_master.lock().await;
        if let Some(ref master) = *master {
            master.resize(portable_pty::PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
                .map_err(|e| TransportError::IoError(e.to_string()))?;
        }
        Ok(())
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.alive.store(false, std::sync::atomic::Ordering::SeqCst);
        let mut master = self.pty_master.lock().await;
        *master = None;
        Ok(())
    }

    fn is_alive(&self) -> bool {
        self.alive.load(std::sync::atomic::Ordering::SeqCst)
    }
}
