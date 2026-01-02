#![deny(clippy::pedantic)]
pub mod error;
pub mod raft_node;
pub mod rpc;
pub mod state;
use std::{collections::HashMap, fmt::Debug, net::SocketAddr, time::Duration};

use async_trait::async_trait;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
};

pub use raft_node::{RaftMessage, RaftNode, RaftNodeConfig};

use bincode::{Decode, Encode, config};
use tracing::info;

use crate::{
    error::{RaftError, RaftResult},
    rpc::{AppendEntries, AppendResponse, RaftRequest, RaftResponse, VoteRequest, VoteResponse},
};

#[derive(Debug, Clone)]
pub enum NodeRole {
    Follower,
    Candidate,
    Leader,
}
impl Default for NodeRole {
    fn default() -> Self {
        NodeRole::Follower
    }
}

type NodeId = String;

#[derive(Debug, Clone, Decode, Encode, Default)]
pub struct LogEntry {
    pub term: u64,
    pub idx: u64,
    pub command: Vec<u8>,
}

pub enum ElectionOutcome {
    Won,
    StepDown { new_term: u64 },
    Continue,
}

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

#[async_trait]
pub trait Transport: Send + Sync {
    async fn call_vote(&self, peer: String, request: VoteRequest) -> RaftResult<VoteResponse>;
    async fn call_append(&self, peer: String, request: AppendEntries)
    -> RaftResult<AppendResponse>;
    /// # Errors
    fn start(&self) -> RaftResult<mpsc::Receiver<RaftMessage>>;
}
#[derive(Debug, Clone)]
pub struct TcpTransport {
    addr: SocketAddr,
    peers: HashMap<String, SocketAddr>,
    timeout: Duration,
}
impl TcpTransport {
    #[must_use]
    pub fn new(addr: SocketAddr, peers: HashMap<String, SocketAddr>, timeout: Duration) -> Self {
        TcpTransport {
            addr,
            peers,
            timeout,
        }
    }
}
#[async_trait]
impl Transport for TcpTransport {
    async fn call_vote(&self, peer: String, request: VoteRequest) -> RaftResult<VoteResponse> {
        if let Some(addr) = self.peers.get(&peer) {
            let resp = call_peer(*addr, &RaftRequest::Vote(request), self.timeout).await?;

            match resp {
                RaftResponse::Vote(v) => Ok(v),
                RaftResponse::AppendEntries(_) => {
                    Err(error::RaftError::RpcError("unexpected response".into()))
                }
            }
        } else {
            tracing::warn!("Peer address not found!");
            return Err(RaftError::ElectionFailure);
        }
    }
    async fn call_append(
        &self,
        peer: String,
        request: AppendEntries,
    ) -> RaftResult<AppendResponse> {
        let addr = self.peers.get(&peer).unwrap();
        let resp = call_peer(*addr, &RaftRequest::AppendEntries(request), self.timeout).await?;
        match resp {
            RaftResponse::AppendEntries(a) => Ok(a),
            RaftResponse::Vote(_) => Err(error::RaftError::RpcError("unexpected response".into())),
        }
    }

    fn start(&self) -> RaftResult<mpsc::Receiver<RaftMessage>> {
        let addr = self.addr;
        let (tx, rx) = mpsc::channel(100);

        info!("Cluster node starting on {}!", addr.clone());

        tokio::spawn(async move {
            let listener = TcpListener::bind(addr).await.unwrap();
            loop {
                let (conn, _paddr) = listener.accept().await.unwrap();
                let tx = tx.clone();
                tokio::spawn(async move {
                    handle_conn(conn, tx).await;
                });
            }
        });
        Ok(rx)
    }
}
async fn handle_conn(mut conn: TcpStream, tx: mpsc::Sender<RaftMessage>) {
    // Read the incoming RPC message
    let request: RaftRequest = match rcv_msg(&mut conn).await {
        Ok(msg) => msg,
        Err(e) => {
            tracing::error!("Failed to receive message: {}", e);
            return;
        }
    };

    let response = match request {
        RaftRequest::Vote(vote_request) => {
            let (response_tx, response_rx) = oneshot::channel();
            let msg = RaftMessage::VoteRequest {
                message: vote_request,
                response: response_tx,
            };
            if tx.send(msg).await.is_err() {
                tracing::error!("Failed to send vote request!");
                return;
            }
            let Ok(vote_resp) = response_rx.await else {
                tracing::error!("Error receiving vote response!");
                return;
            };
            RaftResponse::Vote(vote_resp)
        }
        RaftRequest::AppendEntries(append_request) => {
            let (response_tx, response_rx) = oneshot::channel();
            let msg = RaftMessage::AppendEntries {
                message: append_request,
                response: response_tx,
            };
            if tx.send(msg).await.is_err() {
                tracing::error!("Failed to send append request!");
                return;
            }

            let Ok(append_resp) = response_rx.await else {
                tracing::error!("Error receiving append response!");
                return;
            };
            RaftResponse::AppendEntries(append_resp)
        }
    };

    if let Err(e) = send_msg(&mut conn, &response).await {
        tracing::error!("Failed to send response: {}", e);
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
