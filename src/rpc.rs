use crate::LogEntry;
use bincode::{Decode, Encode};
#[derive(Debug, Encode, Decode, Clone)]
pub struct VoteRequest {
    pub term: u64,
    pub candidate_id: String,
    pub last_log_index: u64,
    pub last_log_term: u64,
}

#[derive(Debug, Encode, Decode, Clone)]
pub struct VoteResponse {
    pub term: u64,
    pub vote_granted: bool,
}

#[derive(Debug, Encode, Decode, Clone)]
pub struct AppendEntries {
    pub term: u64,
    pub leader_id: String,
    pub prev_log_index: u64,
    pub prev_log_term: u64,
    pub entries: Vec<LogEntry>,
    pub leader_commit: u64,
}

#[derive(Debug, Encode, Decode, Clone)]
pub struct AppendResponse {
    pub term: u64,
    pub success: bool,
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum RaftRequest {
    Vote(VoteRequest),
    AppendEntries(AppendEntries),
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum RaftResponse {
    Vote(VoteResponse),
    AppendEntries(AppendResponse),
}
