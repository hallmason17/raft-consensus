//! Three node Raft cluster example with a key-value store.
//!
//! This example demonstrates how to set up a 3-node Raft cluster.
//! Each node runs in its own task and can elect a leader.
//!
//! Usage:
//!   cargo run --example three_node_cluster

use raft_consensus::{RaftNode, RaftNodeConfig, TcpTransport};
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::task::JoinSet;

mod kv_store;
use kv_store::KvStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    println!("Starting 3-node Raft cluster...");
    println!("  Node 1: 127.0.0.1:5001");
    println!("  Node 2: 127.0.0.1:5002");
    println!("  Node 3: 127.0.0.1:5003");
    println!();
    println!("Watch the logs to see leader election!");
    println!("Press Ctrl+C to stop");
    println!();

    // Define the cluster topology
    let peers = HashMap::from([
        ("node1".to_string(), "127.0.0.1:5001".parse::<SocketAddr>()?),
        ("node2".to_string(), "127.0.0.1:5002".parse::<SocketAddr>()?),
        ("node3".to_string(), "127.0.0.1:5003".parse::<SocketAddr>()?),
    ]);

    // Create configurations for each node
    let configs = vec![
        RaftNodeConfig::new(
            "node1".to_string(),
            "127.0.0.1:5001".parse()?,
            peers.clone(),
        ),
        RaftNodeConfig::new(
            "node2".to_string(),
            "127.0.0.1:5002".parse()?,
            peers.clone(),
        ),
        RaftNodeConfig::new(
            "node3".to_string(),
            "127.0.0.1:5003".parse()?,
            peers.clone(),
        ),
    ];

    // Spawn each node in its own task
    let mut tasks = JoinSet::new();

    for config in configs {
        let p = peers.clone();
        tasks.spawn(async move {
            let state_machine = KvStore::new();
            let tcp = TcpTransport::new(p.clone(), Duration::from_millis(500));
            let mut node = RaftNode::new(config, state_machine, tcp);
            node.run().await
        });
    }

    // Wait for all nodes (or until one fails)
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => println!("Node exited normally"),
            Ok(Err(e)) => eprintln!("Node error: {}", e),
            Err(e) => eprintln!("Task join error: {}", e),
        }
    }

    Ok(())
}
