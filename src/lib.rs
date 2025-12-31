#![deny(clippy::pedantic)]
pub mod error;
pub mod raft_node;
pub mod state_machine;
use std::{collections::HashMap, net::SocketAddr, time::Duration};

use async_trait::async_trait;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub use raft_node::{RaftMessage, RaftNode, RaftNodeConfig};
pub use state_machine::{LogEntry, NodeState};

use bincode::{Decode, Encode, config};

use crate::error::RaftResult;

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

#[async_trait]
pub trait Transport: Send + Sync {
    async fn call_vote(&self, peer: String, request: VoteRequest) -> RaftResult<VoteResponse>;
    async fn call_append(&self, peer: String, request: AppendEntries)
    -> RaftResult<AppendResponse>;
}
#[derive(Debug, Clone)]
pub struct TcpTransport {
    peers: HashMap<String, SocketAddr>,
    timeout: Duration,
}
impl TcpTransport {
    #[must_use]
    pub fn new(peers: HashMap<String, SocketAddr>, timeout: Duration) -> Self {
        TcpTransport { peers, timeout }
    }
}
#[async_trait]
impl Transport for TcpTransport {
    async fn call_vote(&self, peer: String, request: VoteRequest) -> RaftResult<VoteResponse> {
        let addr = self.peers.get(&peer).unwrap();
        let resp = call_peer(*addr, &RaftRequest::Vote(request), self.timeout)
            .await
            .unwrap();
        match resp {
            RaftResponse::Vote(v) => Ok(v),
            RaftResponse::AppendEntries(_) => {
                Err(error::RaftError::RpcError("unexpected response".into()))
            }
        }
    }
    async fn call_append(
        &self,
        peer: String,
        request: AppendEntries,
    ) -> RaftResult<AppendResponse> {
        let addr = self.peers.get(&peer).unwrap();
        let resp = call_peer(*addr, &RaftRequest::AppendEntries(request), self.timeout)
            .await
            .unwrap();
        match resp {
            RaftResponse::AppendEntries(a) => Ok(a),
            RaftResponse::Vote(_) => Err(error::RaftError::RpcError("unexpected response".into())),
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
pub(crate) async fn send_msg<T: Encode>(stream: &mut TcpStream, msg: &T) -> RaftResult<()> {
    let bytes = bincode::encode_to_vec(msg, config::standard())?;
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(&bytes).await?;
    Ok(())
}

pub(crate) async fn rcv_msg<T: Decode<()>>(stream: &mut TcpStream) -> RaftResult<T> {
    let len = stream.read_u32().await?;
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await?;
    let (msg, _) = bincode::decode_from_slice(&buf, config::standard())?;
    Ok(msg)
}

pub(crate) async fn call_peer(
    addr: std::net::SocketAddr,
    request: &RaftRequest,
    timeout: Duration,
) -> RaftResult<RaftResponse> {
    tokio::time::timeout(timeout, async {
        let mut stream = TcpStream::connect(addr).await?;
        send_msg(&mut stream, request).await?;
        let response: RaftResponse = rcv_msg(&mut stream).await?;
        Ok(response)
    })
    .await?
}
