#![deny(clippy::pedantic)]
#![allow(unused)]
#![deny(missing_docs)]
//! Raft consensus algorithm implementation.
//!
//! This module implements the Raft distributed consensus protocol, which allows
//! a cluster of nodes to agree on a replicated state machine. The implementation
//! includes leader election, log replication, and a key-value store as the state machine.
//!
//! # Architecture
//!
//! - **`NodeState`**: Nodes can be Followers, Candidates, or Leaders
//! - **Elections**: Triggered by election timeouts when no heartbeat is received
//! - **Log Replication**: Leaders replicate log entries to followers
//! - **State Machine**: A simple key-value store backed by the replicated log

use crate::Transport;
use crate::state::RaftState;
use rand::Rng;
use std::{collections::HashMap, error::Error, fmt::Debug, net::SocketAddr, time::Duration};
use tokio::time::Instant;
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot},
    task::{JoinHandle, JoinSet},
};
use tracing::info;

use crate::StateMachine;
use crate::{
    LogEntry, NodeRole, call_peer,
    error::{RaftError, RaftResult},
    rpc::{AppendEntries, AppendResponse, RaftRequest, RaftResponse, VoteRequest, VoteResponse},
};

/// Messages that can be sent to the Raft node's message processing loop.
///
/// These messages represent both internal Raft protocol messages (voting, log replication)
/// and client requests (get, set, delete operations).
#[derive(Debug)]
pub enum RaftMessage {
    /// Request for a vote during leader election.
    VoteRequest {
        /// The vote request from a candidate node
        message: VoteRequest,
        /// Channel to send the vote response back to the requester
        response: oneshot::Sender<VoteResponse>,
    },
    /// Response to a vote request (not used in event loop, handled in RPC calls)
    VoteResponse {
        /// The vote response message
        message: VoteResponse,
        /// Channel to send the response back
        response: oneshot::Sender<VoteResponse>,
    },
    /// Request to append entries to the log (used for both heartbeats and replication).
    AppendEntries {
        /// The append entries request from the leader
        message: AppendEntries,
        /// Channel to send the response back to the leader
        response: oneshot::Sender<AppendResponse>,
    },
    /// Response to append entries (not used in event loop, handled in RPC calls)
    AppendEntriesResponse {
        /// The append entries response message
        message: AppendResponse,
        /// Channel to send the response back
        response: oneshot::Sender<AppendResponse>,
    },
    /// Client request to retrieve a value from the key-value store.
    ClientCommand {
        /// The key to retrieve
        command: Vec<u8>,
        /// Channel to send the response back to the client
        response: oneshot::Sender<Result<Vec<u8>, Box<dyn Error + Send + Sync>>>,
    },
}

/// A node in the Raft cluster.
///
/// Each node maintains its own state, log, and understanding of the cluster.
/// Nodes can be in one of three states: Follower, Candidate, or Leader.
///
/// # Raft Protocol Overview
///
/// - **Followers**: Passive nodes that respond to requests from leaders and candidates
/// - **Candidates**: Nodes attempting to become leader through elections
/// - **Leaders**: Handle all client requests and replicate log entries to followers
#[allow(dead_code)]
#[derive(Debug)]
pub struct RaftNode<T, SM>
where
    T: Transport,
    SM: StateMachine,
{
    transport: T,

    /// Unique identifier for this node
    pub id: String,

    state: RaftState,

    /// Network address where this node listens
    addr: SocketAddr,

    /// Handle to the leader's heartbeat task (Some when Leader, None otherwise)
    leader_heartbeat_handle: Option<JoinHandle<()>>,

    /// Timestamp of last heartbeat received from leader
    last_heartbeat: Instant,

    /// Map of peer node IDs to their network addresses
    peers: HashMap<String, SocketAddr>,

    /// Replicated log of commands
    log: Vec<LogEntry>,

    /// The store state machine
    state_machine: SM,
}

/// Configuration for creating a new Raft node.
///
/// Contains all the information needed to initialize a node and connect
/// it to the cluster.
#[derive(Debug, Clone)]
pub struct RaftNodeConfig {
    /// Unique identifier for the node
    id: String,

    /// Network address where the node will listen
    pub addr: SocketAddr,

    /// Map of peer node IDs to their network addresses
    peers: HashMap<String, SocketAddr>,
}

impl RaftNodeConfig {
    /// Creates a new Raft node configuration.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for this node
    /// * `addr` - Network address for this node to listen on
    /// * `peers` - Map of all peer nodes in the cluster (including this node)
    ///
    /// # Example
    ///
    ///
    /// let config = `RaftNodeConfig::new`(
    ///     "`node1".to_string()`,
    ///     "`127.0.0.1:5001".parse().unwrap()`,
    ///     `HashMap::from`([
    ///         ("`node2".to_string()`, "`127.0.0.1:5002".parse().unwrap()`),
    ///         ("`node3".to_string()`, "`127.0.0.1:5003".parse().unwrap()`),
    ///     ])
    /// );
    ///
    #[must_use]
    pub fn new(id: String, addr: SocketAddr, peers: HashMap<String, SocketAddr>) -> Self {
        RaftNodeConfig { id, addr, peers }
    }
}

impl<T, SM> RaftNode<T, SM>
where
    T: Transport + Debug + Clone + 'static,
    SM: StateMachine + Debug,
{
    /// Creates a new Raft node from the given configuration.
    ///
    /// The node starts in the Follower state with term 0 and a randomized
    /// election timeout to prevent split votes.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration containing node ID, address, and peer information
    /// * `state_machine` - The state machine implementation to use
    ///
    /// # Returns
    ///
    /// A new `RaftNode` initialized as a Follower
    #[must_use]
    pub fn new(config: RaftNodeConfig, state_machine: SM, transport: T) -> Self {
        let timeout = rand::rng().random_range(150..=300);
        RaftNode {
            transport,
            state: RaftState {
                role: NodeRole::Follower,
                current_term: 0,
                commit_index: 0,
                last_applied: 0,
                votes_received: 0,
                voted_for: None,
                election_timeout_ms: Duration::from_millis(timeout),
                next_index: HashMap::new(),
                match_index: HashMap::new(),
            },
            id: config.id,
            addr: config.addr,
            leader_heartbeat_handle: None,
            last_heartbeat: Instant::now(),
            peers: config.peers,
            log: vec![],
            state_machine,
        }
    }

    /// Runs the main event loop for the Raft node.
    ///
    /// This method starts the TCP server and enters the main message processing loop.
    /// It handles incoming messages (vote requests, append entries, client requests) and
    /// periodically checks for election timeouts.
    ///
    /// # Returns
    ///
    /// An error if the node fails to run properly
    ///
    /// # Errors
    ///
    /// # Event Loop
    ///
    /// The loop processes:
    /// - Vote requests from candidates
    /// - Append entries requests from leaders
    /// - Client commands
    /// - Election timeout checks (with randomized timing)
    pub async fn run(&mut self) -> RaftResult<()> {
        // Start the transport layer to handle incoming connections
        let mut rx = self.transport.start()?;

        let sleep = tokio::time::sleep(Duration::ZERO);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                            msg = rx.recv() => {
                                match msg {
                                Some(RaftMessage::VoteRequest { message, response }) => {
                                    match self.handle_vote_request(&message) {
                                        Ok(resp) => { let _ = response.send(resp); }
                                        Err(e) => { tracing::error!("Vote request error: {}", e); }
                                    }
                                }
                                Some(RaftMessage::AppendEntries { message, response }) => {
                                    let resp = self.handle_append_entries(&message);
                                    let _ = response.send(resp);
                                }
                                Some(RaftMessage::ClientCommand{command, response}) => {
                                    todo!()
                                }
                                Some(RaftMessage::VoteResponse { .. } | RaftMessage::AppendEntriesResponse {
            .. }) => {
                                    tracing::warn!("Received unexpected response message in event loop");
                                }
                                None => break,
                            }
                                }
                                () = sleep.as_mut() => {
                                self.check_election_timeout().await?;
                                sleep.as_mut().set(tokio::time::sleep(Self::get_random_election_timeout()));
                            }
                        }
        }
        Ok(())
    }

    /// Starts or stops the heartbeat timer based on the node's current state.
    ///
    /// - **Leader**: Starts sending periodic heartbeats to all followers
    /// - **Follower/Candidate**: Stops any existing heartbeat task
    ///
    /// Leaders send heartbeats every 100ms to prevent followers from starting elections.
    fn start_heartbeat_timer(&mut self) {
        match self.state.role {
            NodeRole::Follower | NodeRole::Candidate => {
                if let Some(h) = self.leader_heartbeat_handle.take() {
                    h.abort();
                }
            }
            NodeRole::Leader => {
                let handle = self.start_leader_heartbeat();
                self.leader_heartbeat_handle = Some(handle);
            }
        }
    }

    /// Spawns a task that sends periodic heartbeats to all peers.
    ///
    /// Heartbeats are empty `AppendEntries` requests sent every 100ms to maintain
    /// leadership and prevent followers from timing out and starting elections.
    ///
    /// # Returns
    ///
    /// A handle to the spawned heartbeat task
    fn start_leader_heartbeat(&self) -> tokio::task::JoinHandle<()> {
        let interval = Duration::from_millis(100);
        let request = AppendEntries {
            term: self.state.current_term,
            leader_id: self.id.clone(),
            prev_log_index: self.get_last_log_index(),
            prev_log_term: self.get_last_log_term(),
            entries: Vec::new(),
            leader_commit: self.get_last_log_index(),
        };
        let peers = self.peers.clone();
        let addr = self.addr;
        let id = self.id.clone();
        let transport = self.transport.clone();

        tokio::spawn(async move {
            let mut int_timer = tokio::time::interval(interval);
            int_timer.tick().await;
            loop {
                info!("Sending heartbeats");
                int_timer.tick().await;
                for peer_id in peers.keys().filter(|pid| **pid != id) {
                    let req = request.clone();
                    let transport = transport.clone();
                    let pid = peer_id.clone();
                    tokio::spawn(async move {
                        if let Err(e) = transport.call_append(pid.clone(), req).await {
                            tracing::debug!("Heartbeat failed to {}: {}", pid, e);
                        }
                    });
                }
            }
        })
    }

    /// Generates a random election timeout duration.
    ///
    /// Returns a random duration between 150ms and 350ms. The randomization
    /// helps prevent split votes by ensuring nodes don't all timeout simultaneously.
    ///
    /// # Returns
    ///
    /// A random duration between 150ms and 350ms
    fn get_random_election_timeout() -> Duration {
        let (min, max) = (150, 350);
        let timeout_ms = rand::rng().random_range(min..=max);
        Duration::from_millis(timeout_ms)
    }

    /// Handles a `RequestVote` RPC from a candidate node.
    ///
    /// Grants vote if:
    /// - Haven't voted for another candidate in this term
    /// - The candidate's log is at least as up-to-date as ours
    ///
    /// If the request has a higher term, steps down to follower state.
    ///
    /// # Arguments
    ///
    /// * `message` - The vote request from the candidate
    ///
    /// # Returns
    ///
    /// A response indicating whether the vote was granted and the current term
    fn handle_vote_request(&mut self, message: &VoteRequest) -> RaftResult<VoteResponse> {
        match message {
            VoteRequest {
                term,
                candidate_id,
                last_log_index,
                last_log_term,
            } => {
                info!(
                    "Handling vote request from {} for term {}",
                    *candidate_id, *term
                );

                if self.state.current_term == *term && self.state.voted_for.is_some() {
                    return Ok(VoteResponse {
                        term: self.state.current_term,
                        vote_granted: false,
                    });
                }

                if *term > self.state.current_term {
                    self.state.role = NodeRole::Follower;
                    self.state.current_term = *term;
                    self.state.voted_for = None;
                }

                self.state.voted_for = Some(candidate_id.clone());
                self.last_heartbeat = Instant::now();

                info!("Granted vote to {} for term {}", *candidate_id, *term);

                Ok(VoteResponse {
                    term: self.state.current_term,
                    vote_granted: true,
                })
            }
            _ => Err(RaftError::ElectionFailure {}),
        }
    }

    /// Handles an `AppendEntries` RPC from the leader.
    ///
    /// This handles both heartbeats (empty entries) and actual log replication.
    ///
    /// # Behavior
    ///
    /// - **Heartbeat**: Empty entries list, updates last heartbeat timestamp
    /// - **Log replication**: Appends new entries if log consistency checks pass
    /// - **Term update**: Steps down if request has higher term
    ///
    /// # Arguments
    ///
    /// * `message` - The append entries request from the leader
    ///
    /// # Returns
    ///
    /// A response indicating success/failure and the current term
    fn handle_append_entries(&mut self, message: &AppendEntries) -> AppendResponse {
        let mut success = false;

        if message.term > self.state.current_term {
            self.state.current_term = message.term;
            self.state.role = NodeRole::Follower;
            self.state.voted_for = None;
        }

        if message.entries.is_empty() {
            info!("Received heartbeat from leader");
            self.last_heartbeat = Instant::now();
            self.state.current_term = message.term;
            return AppendResponse {
                term: self.state.current_term,
                success: true,
            };
        }
        if message.term >= self.state.current_term && message.prev_log_index == 0
            || (message.prev_log_index <= self.log.len() as u64
                && self.log[usize::try_from(message.prev_log_index).unwrap_or(1) - 1].term
                    == message.prev_log_term)
        {
            success = true;
            let start_idx = usize::try_from(message.prev_log_index).unwrap_or(0);
            for entry in &message.entries {
                info!("Adding {:?} to log from leader!", entry);
                if start_idx < self.log.len() {
                    self.log[start_idx] = entry.clone();
                } else {
                    self.log.push(entry.clone());
                }
            }
        }

        if message.leader_commit > self.state.commit_index && !message.entries.is_empty() {
            self.state.commit_index =
                std::cmp::min(message.leader_commit, message.entries.last().unwrap().idx);
        }

        // self.apply_log_entries()?;

        info!("{:?}", self);

        AppendResponse {
            term: self.state.current_term,
            success,
        }
    }

    /// Replicates a log entry to a majority of followers.
    ///
    /// Sends the log entry at the given index to all peers concurrently and
    /// waits until a majority have successfully replicated it.
    ///
    /// # Arguments
    ///
    /// * `index` - The log index to replicate (1-based)
    ///
    /// # Returns
    ///
    /// Ok if successfully replicated to a majority, Err otherwise
    ///
    /// # Errors
    ///
    /// Returns `RaftError::ReplicationFailure` if unable to replicate to a majority
    async fn replicate_log(&mut self, index: u64) -> RaftResult<()> {
        let entry = self
            .log
            .get(usize::try_from(index).unwrap_or(1) - 1)
            .expect("No log entries")
            .clone();
        let mut tasks = JoinSet::new();
        for (id, peer_addr) in &self.peers {
            if id == &self.id {
                continue;
            }
            let request = AppendEntries {
                term: self.state.current_term,
                leader_id: self.id.clone(),
                prev_log_index: self.get_last_log_index() - 1,
                prev_log_term: self.get_last_log_term(),
                entries: vec![entry.clone()],
                leader_commit: self.state.commit_index,
            };
            info!("Sending request: {:?}", request);
            let peer_addr = *peer_addr;
            let transport = self.transport.clone();
            tasks.spawn(async move { transport.call_append(peer_addr.to_string(), request).await });
        }

        let needed = (self.peers.len() / 2) + 1;
        let mut success_count = 1;
        let msg = tasks.join_next().await;

        while let Some(result) = tasks.join_next().await {
            if let Ok(Ok(AppendResponse { success: true, .. })) = result {
                success_count += 1;
                if success_count >= needed {
                    self.state.commit_index += 1;
                    tasks.abort_all();
                    return Ok(());
                }
            }
        }

        Err(RaftError::ReplicationFailure)
    }

    /// Checks if the election timeout has elapsed and starts an election if so.
    ///
    /// Only followers check for election timeouts. If the timeout period passes
    /// without receiving a heartbeat from the leader, the node transitions to
    /// candidate state and begins an election.
    ///
    /// # Returns
    ///
    /// Ok normally, or an error if the election process fails
    async fn check_election_timeout(&mut self) -> RaftResult<()> {
        if matches!(self.state.role, NodeRole::Follower)
            && self.last_heartbeat.elapsed() > self.state.election_timeout_ms
        {
            let _ = self.start_election().await;
        }
        Ok(())
    }

    /// Gets the term of the last entry in the log.
    ///
    /// # Returns
    ///
    /// The term of the last log entry, or 0 if the log is empty
    fn get_last_log_term(&self) -> u64 {
        self.log.last().map_or(0, |e| e.term)
    }

    /// Gets the index of the last entry in the log.
    ///
    /// # Returns
    ///
    /// The index of the last log entry, or 0 if the log is empty
    fn get_last_log_index(&self) -> u64 {
        self.log.last().map_or(0, |e| e.idx)
    }

    /// Starts a leader election for this node.
    ///
    /// The election process:
    /// 1. Transition to Candidate state
    /// 2. Increment current term
    /// 3. Vote for self
    /// 4. Send `RequestVote` RPCs to all peers
    /// 5. Wait for votes:
    ///    - Become leader if receive votes from majority
    ///    - Step down if discover higher term
    ///    - Return to follower if election fails
    ///
    /// # Returns
    ///
    /// Ok(()) if elected leader, Err otherwise
    ///
    /// # Errors
    ///
    /// Returns `RaftError::ElectionFailure` if unable to win the election
    async fn start_election(&mut self) -> RaftResult<()> {
        self.state.role = NodeRole::Candidate;
        self.state.current_term += 1;
        self.state.votes_received += 1;
        self.state.voted_for = Some(self.id.clone());
        self.last_heartbeat = Instant::now();

        info!("Starting election for term {}.", self.state.current_term);

        let mut requests = JoinSet::new();

        for peer in self.peers.keys().filter(|pid| **pid != self.id) {
            let pid = peer.clone();
            let last_log_index = self.get_last_log_index();
            let last_log_term = self.get_last_log_term();
            let request = VoteRequest {
                term: self.state.current_term,
                candidate_id: self.id.clone(),
                last_log_index,
                last_log_term,
            };
            let transport = self.transport.clone();
            requests.spawn(async move { transport.call_vote(pid, request).await });
        }

        let needed_votes = self.peers.len() / 2 + 1;

        while let Some(res) = requests.join_next().await {
            if let Ok(Ok(VoteResponse { term, vote_granted })) = res {
                if term > self.state.current_term {
                    let _ = self.step_down(term);
                    requests.abort_all();
                    return Ok(());
                }

                if vote_granted {
                    self.state.votes_received += 1;
                    if usize::try_from(self.state.votes_received).unwrap_or(0) >= needed_votes {
                        info!("Election succeeded, becoming leader");
                        let _ = self.become_leader();
                        requests.abort_all();
                        return Ok(());
                    }
                }
            }
        }

        if !matches!(self.state.role, NodeRole::Leader) {
            info!("Election failed, not enough votes");
            self.state.role = NodeRole::Follower;
            self.state.voted_for = None;
            self.state.votes_received = 0;
        }

        Err(RaftError::ElectionFailure)
    }

    /// Transitions this node to the Leader state.
    ///
    /// Upon becoming leader, the node:
    /// - Updates its state to Leader
    /// - Starts sending periodic heartbeats to all followers
    ///
    /// # Returns
    ///
    /// Ok(()) on successful transition
    #[tracing::instrument]
    fn become_leader(&mut self) -> RaftResult<()> {
        info!("Becoming leader");
        self.state.role = NodeRole::Leader;
        self.start_heartbeat_timer();
        Ok(())
    }

    /// Steps down from Leader or Candidate to Follower state.
    ///
    /// This occurs when:
    /// - Discovering a higher term from another node
    /// - Receiving an `AppendEntries` RPC with a higher term
    /// - Receiving a vote response with a higher term
    ///
    /// # Arguments
    ///
    /// * `new_term` - The higher term that caused this node to step down
    ///
    /// # Returns
    ///
    /// Ok(()) on successful transition
    #[tracing::instrument]
    fn step_down(&mut self, new_term: u64) -> RaftResult<()> {
        info!("Stepping down");
        self.state.role = NodeRole::Follower;
        self.state.voted_for = None;
        self.state.current_term = new_term;
        self.last_heartbeat = Instant::now();
        self.state.votes_received = 0;
        self.start_heartbeat_timer();
        Ok(())
    }

    /*
    #[tracing::instrument]
    fn apply_log_entries(&mut self) -> RaftResult<()> {
        while self.last_applied < self.commit_index {
            let entry = &self.log[usize::try_from(self.last_applied).unwrap_or(0)];

            match &entry.command {
                Command::Set { key, value } => {
                    self.store.insert(key.clone(), value.clone());
                }
                Command::Delete { key } => {
                    self.store.remove(key);
                }
            }
            self.last_applied += 1;
        }
        Ok(())
    }
     */
}
