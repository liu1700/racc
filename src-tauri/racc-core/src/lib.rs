pub mod error;
pub mod events;
pub mod db;
pub mod ssh;
pub mod transport;

pub use error::CoreError;

#[derive(Clone, Debug, serde::Serialize)]
pub struct TerminalData {
    pub session_id: i64,
    pub data: Vec<u8>,
}
