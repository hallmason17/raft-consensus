use thiserror::Error;

#[derive(Error, Debug)]
pub enum RaftError {
    #[error("Failed to achieve majority consensus for log replication")]
    ReplicationFailure,

    #[error("Election failed: not enough votes")]
    ElectionFailure,

    #[error("Node is not the leader")]
    NotLeader,

    #[error("Log entry not found at index {0}")]
    LogEntryNotFound(u64),

    #[error("Invalid log entry conversion")]
    InvalidLogEntry,

    #[error("RPC communication error: {0}")]
    RpcError(String),

    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        from: crate::NodeState,
        to: crate::NodeState,
    },

    #[error("Term mismatch: expected {expected}, got {actual}")]
    TermMismatch { expected: u64, actual: u64 },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("encode error: {0}")]
    Encode(#[from] bincode::error::EncodeError),

    #[error("decode error: {0}")]
    Decode(#[from] bincode::error::DecodeError),

    #[error("timeout error: {0}")]
    Timeout(#[from] tokio::time::error::Elapsed),
}

pub type RaftResult<T> = Result<T, RaftError>;
