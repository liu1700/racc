use super::{Transport, TransportError, RingBuffer};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

const MAX_BUFFER_SIZE: usize = 1_048_576; // 1MB per session

pub struct TransportManager {
    transports: Arc<Mutex<HashMap<i64, Box<dyn Transport>>>>,
    buffers: Arc<Mutex<HashMap<i64, RingBuffer>>>,
    buffer_tx: tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)>,
    buffer_rx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<(i64, Vec<u8>)>>>>,
}

impl TransportManager {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            transports: Arc::new(Mutex::new(HashMap::new())),
            buffers: Arc::new(Mutex::new(HashMap::new())),
            buffer_tx: tx,
            buffer_rx: Arc::new(Mutex::new(Some(rx))),
        }
    }

    /// Start the buffer aggregation task. Call once during app setup.
    pub fn start_buffer_task(&self) {
        let buffers = self.buffers.clone();
        let rx = self.buffer_rx.clone();
        tauri::async_runtime::spawn(async move {
            let mut rx = rx.lock().await.take().expect("buffer task already started");
            while let Some((session_id, data)) = rx.recv().await {
                let mut bufs = buffers.lock().await;
                if let Some(buf) = bufs.get_mut(&session_id) {
                    buf.push(data);
                }
            }
        });
    }

    pub fn buffer_sender(&self) -> tokio::sync::mpsc::UnboundedSender<(i64, Vec<u8>)> {
        self.buffer_tx.clone()
    }

    pub async fn insert(&self, session_id: i64, transport: Box<dyn Transport>) {
        self.buffers.lock().await.insert(session_id, RingBuffer::new(MAX_BUFFER_SIZE));
        self.transports.lock().await.insert(session_id, transport);
    }

    pub async fn write(&self, session_id: i64, data: &[u8]) -> Result<(), TransportError> {
        let transports = self.transports.lock().await;
        let transport = transports.get(&session_id)
            .ok_or_else(|| TransportError::NotFound(format!("session {}", session_id)))?;
        transport.write(data).await
    }

    pub async fn resize(&self, session_id: i64, cols: u16, rows: u16) -> Result<(), TransportError> {
        let transports = self.transports.lock().await;
        let transport = transports.get(&session_id)
            .ok_or_else(|| TransportError::NotFound(format!("session {}", session_id)))?;
        transport.resize(cols, rows).await
    }

    pub async fn get_buffer(&self, session_id: i64) -> Option<Vec<u8>> {
        let buffers = self.buffers.lock().await;
        buffers.get(&session_id).map(|b| b.get_all())
    }

    pub async fn remove(&self, session_id: i64) -> Result<(), TransportError> {
        if let Some(transport) = self.transports.lock().await.remove(&session_id) {
            transport.close().await?;
        }
        self.buffers.lock().await.remove(&session_id);
        Ok(())
    }

    pub async fn is_alive(&self, session_id: i64) -> bool {
        let transports = self.transports.lock().await;
        transports.get(&session_id).map_or(false, |t| t.is_alive())
    }
}
