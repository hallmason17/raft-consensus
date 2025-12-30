#![deny(clippy::pedantic)]
pub mod error;
pub mod raft_node;
pub mod state_machine;
use std::time::Duration;

use async_trait::async_trait;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub use raft_node::{RaftMessage, RaftNode, RaftNodeConfig};
pub use state_machine::{LogEntry, NodeState};

use bincode::{Decode, Encode, config};

use crate::error::RaftResult;

// Request/Response structs - make fields pub so they can be accessed
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

// Enums for wire protocol
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

#[async_trait]
pub trait Rpc: Send + Sync {
    async fn send(&self, peer: String, message: RaftMessage) -> RaftResult<()>;
    async fn call_vote(&self, peer: String, request: RaftMessage) -> RaftResult<RaftMessage>;
    async fn call_append(&self, peer: String, request: RaftMessage) -> RaftResult<RaftMessage>;
}

pub(crate) async fn send_msg<T: Encode>(
    stream: &mut TcpStream,
    msg: &T,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bytes = bincode::encode_to_vec(msg, config::standard())?;
    stream.write_u32(u32::try_from(bytes.len())?).await?;
    stream.write_all(&bytes).await?;
    Ok(())
}

pub(crate) async fn rcv_msg<T: Decode<()>>(
    stream: &mut TcpStream,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    let len = stream.read_u32().await?;
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await?;
    let (msg, _) = bincode::decode_from_slice(&buf, config::standard())?;
    Ok(msg)
}

pub(crate) async fn call_peer(
    addr: std::net::SocketAddr,
    request: &RaftRequest,
) -> Result<RaftResponse, Box<dyn std::error::Error + Send + Sync>> {
    tokio::time::timeout(Duration::from_millis(500), async {
        let mut stream = TcpStream::connect(addr).await?;
        send_msg(&mut stream, request).await?;
        let response: RaftResponse = rcv_msg(&mut stream).await?;
        Ok(response)
    })
    .await?
}
