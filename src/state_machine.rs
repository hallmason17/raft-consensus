use std::fmt::Debug;

use bincode::{Decode, Encode};

pub trait StateMachine: Send + Sync + Debug {
    type Error: std::error::Error + Send + Sync + 'static;
    /// # Errors
    fn apply(&mut self, command: &[u8]) -> Result<Vec<u8>, Self::Error>;
    /// # Errors
    fn snapshot(&self) -> Result<Vec<u8>, Self::Error> {
        Ok(Vec::new())
    }
    /// # Errors
    fn restore(&mut self, _snapshot: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum NodeState {
    Follower,
    Candidate,
    Leader,
}
#[derive(Debug, Clone, Decode, Encode)]
pub struct LogEntry {
    pub term: u64,
    pub idx: u64,
    pub command: Vec<u8>,
}
