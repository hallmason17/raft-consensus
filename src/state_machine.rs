use std::error::Error;

use bincode::{Decode, Encode};

pub trait StateMachine: Send + Sync + std::fmt::Debug {
    /// # Errors
    fn apply(&mut self, command: &[u8]) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>>;
    /// # Errors
    fn snapshot(&self) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        Ok(Vec::new())
    }
    /// # Errors
    fn restore(&mut self, _snapshot: &[u8]) -> Result<(), Box<dyn Error + Send + Sync>> {
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
