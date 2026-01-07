//! Three node Raft cluster example with a key-value store.
//!
//! This example demonstrates how to set up a 3-node Raft cluster.
//! Each node runs in its own task and can elect a leader.
//!
//! Usage:
//!   cargo run --example three_node_cluster

use raft_consensus::{RaftError, RaftNode, RaftNodeConfig, TcpTransport};
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinSet;

mod kv_store;
use kv_store::{KvCommand, KvResponse, KvStore};

const NODES: &[(&str, &str, &str)] = &[
    ("node1", "127.0.0.1:5001", "127.0.0.1:6001"),
    ("node2", "127.0.0.1:5002", "127.0.0.1:6002"),
    ("node3", "127.0.0.1:5003", "127.0.0.1:6003"),
];

fn build_peer_map() -> Result<HashMap<String, SocketAddr>, Box<dyn Error + Send + Sync>> {
    NODES
        .iter()
        .map(|(id, raft_addr, _)| Ok((id.to_string(), raft_addr.parse()?)))
        .collect()
}

fn build_client_addrs() -> Result<Vec<(String, SocketAddr)>, Box<dyn Error + Send + Sync>> {
    NODES
        .iter()
        .map(|(id, _, client_addr)| Ok((id.to_string(), client_addr.parse()?)))
        .collect()
}

async fn send_command(
    addr: &SocketAddr,
    cmd: &KvCommand,
) -> Result<Option<KvResponse>, Box<dyn Error + Send + Sync>> {
    let mut stream = TcpStream::connect(addr).await?;
    let cmd_bytes = bincode::encode_to_vec(cmd, bincode::config::standard())?;

    stream.write_u32(cmd_bytes.len() as u32).await?;
    stream.write_all(&cmd_bytes).await?;

    let len = stream.read_u32().await?;
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await?;

    if buf.is_empty() {
        return Ok(None);
    }

    let (response, _): (KvResponse, _) =
        bincode::decode_from_slice(&buf, bincode::config::standard())?;
    Ok(Some(response))
}

async fn run_client_demo(client_addrs: Vec<(String, SocketAddr)>) -> Result<(), RaftError> {
    println!("\n=== Starting client commands ===\n");

    for (node_name, addr) in &client_addrs {
        println!("Trying to send command to {}...", node_name);

        let set_cmd = KvCommand::Set {
            key: "hello".to_string(),
            value: "world".to_string(),
        };

        match send_command(addr, &set_cmd).await {
            Ok(Some(response)) => {
                println!("  Set command succeeded on {}: {:?}\n", node_name, response);

                let get_cmd = KvCommand::Get {
                    key: "hello".to_string(),
                };
                match send_command(addr, &get_cmd).await {
                    Ok(Some(response)) => println!("  Get command result: {:?}\n", response),
                    Ok(None) => println!("  Get returned empty response\n"),
                    Err(e) => println!("  Failed to send Get: {}\n", e),
                }
                break;
            }
            Ok(None) => println!("  {} is not the leader\n", node_name),
            Err(e) => println!("  Failed to connect to {}: {}\n", node_name, e),
        }
    }

    println!("=== Client commands complete ===\n");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    println!("Starting 3-node Raft cluster...");
    for (id, raft, client) in NODES {
        println!("  {}: {} (raft), {} (client)", id, raft, client);
    }
    println!("\nWatch the logs to see leader election!");
    println!("Press Ctrl+C to stop\n");

    let peers = build_peer_map()?;
    let client_addrs = build_client_addrs()?;

    let mut tasks = JoinSet::new();

    for (node_id, raft_addr, client_addr) in NODES {
        let peers = peers.clone();
        let raft_addr: SocketAddr = raft_addr.parse()?;
        let client_addr: SocketAddr = client_addr.parse()?;
        let node_id = node_id.to_string();

        tasks.spawn(async move {
            let config = RaftNodeConfig::new(node_id, raft_addr, peers.clone());
            let transport =
                TcpTransport::new(raft_addr, client_addr, peers, Duration::from_millis(5000));
            let mut node = RaftNode::new(config, KvStore::new(), transport);
            node.run().await
        });
    }

    tokio::time::sleep(Duration::from_secs(3)).await;
    tasks.spawn(run_client_demo(client_addrs));

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => println!("Node exited normally"),
            Ok(Err(e)) => eprintln!("Node error: {}", e),
            Err(e) => eprintln!("Task join error: {}", e),
        }
    }

    Ok(())
}
