pub mod local_pty;
pub mod manager;
pub mod ssh_tmux;

use async_trait::async_trait;
use std::collections::VecDeque;
use std::fmt;

#[derive(Debug)]
pub enum TransportError {
    NotFound(String),
    IoError(String),
    Closed,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::NotFound(msg) => write!(f, "Transport not found: {}", msg),
            TransportError::IoError(msg) => write!(f, "I/O error: {}", msg),
            TransportError::Closed => write!(f, "Transport closed"),
        }
    }
}

impl From<TransportError> for String {
    fn from(e: TransportError) -> String {
        e.to_string()
    }
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn write(&self, data: &[u8]) -> Result<(), TransportError>;
    async fn resize(&self, cols: u16, rows: u16) -> Result<(), TransportError>;
    async fn close(&self) -> Result<(), TransportError>;
    fn is_alive(&self) -> bool;
}

/// Ring buffer for terminal output. Drops oldest chunks when exceeding max size.
/// Uses VecDeque for O(1) front removal on the hot output path.
pub struct RingBuffer {
    chunks: VecDeque<Vec<u8>>,
    total_size: usize,
    max_size: usize,
}

impl RingBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            total_size: 0,
            max_size,
        }
    }

    pub fn push(&mut self, data: Vec<u8>) {
        self.total_size += data.len();
        self.chunks.push_back(data);
        while self.total_size > self.max_size {
            if let Some(removed) = self.chunks.pop_front() {
                self.total_size -= removed.len();
            } else {
                break;
            }
        }
    }

    pub fn get_all(&self) -> Vec<u8> {
        self.chunks.iter().flat_map(|c| c.iter()).copied().collect()
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
        self.total_size = 0;
    }
}
