//! Single node Raft cluster example with a key-value store.
//!
//! This example demonstrates how to set up a single Raft node with
//! the key-value store state machine.

use raft_consensus::{RaftNode, RaftNodeConfig};
use std::collections::HashMap;
use std::net::SocketAddr;

mod kv_store;
use kv_store::KvStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // Configure the node
    let addr: SocketAddr = "127.0.0.1:5001".parse()?;
    let config = RaftNodeConfig::new(
        "node1".to_string(),
        addr,
        HashMap::from([("node1".to_string(), addr)]),
    );

    // Create the state machine
    let state_machine = Box::new(KvStore::new());

    // Create and run the Raft node
    let mut node = RaftNode::new(config, state_machine);
    println!("Starting Raft node at {}", addr);
    println!("Press Ctrl+C to stop");

    node.run().await?;

    Ok(())
}
