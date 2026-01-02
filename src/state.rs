use std::{collections::HashMap, time::Duration};

use crate::NodeRole;

#[derive(Debug)]
pub struct RaftState {
    /// Current state of this node (Follower, Candidate, or Leader)
    pub role: NodeRole,

    /// Latest term this node has seen (monotonically increasing)
    pub current_term: u64,

    /// Index of highest log entry known to be committed
    pub commit_index: u64,

    /// Index of highest log entry applied to state machine
    pub last_applied: u64,

    /// Number of votes received in current election (used when Candidate)
    pub votes_received: u64,

    /// `CandidateId` that received vote in current term (or None)
    pub voted_for: Option<String>,

    /// For each peer, index of the next log entry to send (Leader only)
    pub next_index: HashMap<String, u64>,

    /// For each peer, index of highest log entry known to be replicated (Leader only)
    pub match_index: HashMap<String, u64>,

    /// Duration to wait before starting election (randomized to avoid split votes)
    pub election_timeout_ms: Duration,
}
