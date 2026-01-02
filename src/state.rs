use std::{collections::HashMap, time::Duration};

use tokio::time::Instant;

use crate::{NodeId, NodeRole};

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

    pub last_heartbeat: Instant,
}

impl RaftState {
    #[must_use]
    pub fn should_grant_vote(
        &self,
        term: u64,
        log_index: u64,
        log_term: u64,
        candidate_log_index: u64,
        candidate_log_term: u64,
        candidate_id: &NodeId,
    ) -> bool {
        let voted_for = self.voted_for.clone();
        if (voted_for.is_some_and(|id| &id == candidate_id) || self.voted_for.is_none())
            && candidate_log_term >= log_term
            && candidate_log_index >= log_index
            && term >= self.current_term
        {
            return true;
        }
        false
    }

    #[must_use]
    pub fn should_start_election(&self) -> bool {
        matches!(self.role, NodeRole::Follower)
            && self.last_heartbeat.elapsed() > self.election_timeout_ms
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use tokio::time::Instant;

    use crate::{NodeRole, state::RaftState};

    #[tokio::test(start_paused = true)]
    async fn should_start_election_time_elapsed_returns_true() {
        let state = RaftState {
            role: NodeRole::Follower,
            current_term: 10,
            commit_index: 10,
            last_applied: 10,
            votes_received: 0,
            voted_for: None,
            election_timeout_ms: Duration::from_millis(150),
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            last_heartbeat: Instant::now(),
        };
        tokio::time::advance(Duration::from_millis(200)).await;
        assert!(state.should_start_election());
    }

    #[tokio::test(start_paused = true)]
    async fn should_start_election_time_not_elapsed_returns_false() {
        let state = RaftState {
            role: NodeRole::Follower,
            current_term: 10,
            commit_index: 10,
            last_applied: 10,
            votes_received: 0,
            voted_for: None,
            election_timeout_ms: Duration::from_millis(150),
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            last_heartbeat: Instant::now(),
        };
        tokio::time::advance(Duration::from_millis(100)).await;
        assert!(!state.should_start_election());
    }

    #[test]
    fn should_start_election_currently_candidate_returns_false() {
        let state = RaftState {
            role: NodeRole::Candidate,
            current_term: 10,
            commit_index: 10,
            last_applied: 10,
            votes_received: 0,
            voted_for: None,
            election_timeout_ms: Duration::from_millis(150),
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            last_heartbeat: Instant::now(),
        };
        assert!(!state.should_start_election());
    }

    #[test]
    fn should_start_election_currently_leader_returns_false() {
        let state = RaftState {
            role: NodeRole::Leader,
            current_term: 10,
            commit_index: 10,
            last_applied: 10,
            votes_received: 0,
            voted_for: None,
            election_timeout_ms: Duration::from_millis(150),
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            last_heartbeat: Instant::now(),
        };
        assert!(!state.should_start_election());
    }

    #[test]
    fn should_grant_vote_prev_term_returns_false() {
        let state = RaftState {
            role: NodeRole::Follower,
            current_term: 10,
            commit_index: 10,
            last_applied: 10,
            votes_received: 0,
            voted_for: None,
            election_timeout_ms: Duration::from_millis(150),
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            last_heartbeat: Instant::now(),
        };
        let voted = state.should_grant_vote(5, 10, 10, 10, 10, &"node1".to_string());
        assert!(!voted);
    }

    #[test]
    fn should_grant_vote_old_log_returns_false() {
        let state = RaftState {
            role: NodeRole::Follower,
            current_term: 10,
            commit_index: 10,
            last_applied: 10,
            votes_received: 0,
            voted_for: None,
            election_timeout_ms: Duration::from_millis(150),
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            last_heartbeat: Instant::now(),
        };
        let voted = state.should_grant_vote(state.current_term, 10, 10, 9, 9, &"node1".to_string());
        assert!(!voted);
    }

    #[test]
    fn should_grant_vote_already_voted_for_different_node_returns_false() {
        let state = RaftState {
            role: NodeRole::Follower,
            current_term: 10,
            commit_index: 10,
            last_applied: 10,
            votes_received: 0,
            voted_for: Some(String::from("node2")),
            election_timeout_ms: Duration::from_millis(150),
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            last_heartbeat: Instant::now(),
        };
        let voted =
            state.should_grant_vote(state.current_term, 10, 10, 10, 10, &"node1".to_string());
        assert!(!voted);
    }

    #[test]
    fn should_grant_vote_voted_for_node_returns_true() {
        let state = RaftState {
            role: NodeRole::Follower,
            current_term: 10,
            commit_index: 10,
            last_applied: 10,
            votes_received: 0,
            voted_for: Some(String::from("node1")),
            election_timeout_ms: Duration::from_millis(150),
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            last_heartbeat: Instant::now(),
        };
        let voted =
            state.should_grant_vote(state.current_term, 10, 10, 10, 10, &"node1".to_string());
        assert!(voted);
    }

    #[test]
    fn should_grant_vote_not_voted_returns_true() {
        let state = RaftState {
            role: NodeRole::Follower,
            current_term: 10,
            commit_index: 10,
            last_applied: 10,
            votes_received: 0,
            voted_for: None,
            election_timeout_ms: Duration::from_millis(150),
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            last_heartbeat: Instant::now(),
        };
        let voted =
            state.should_grant_vote(state.current_term, 10, 10, 10, 10, &"node1".to_string());
        assert!(voted);
    }
}
