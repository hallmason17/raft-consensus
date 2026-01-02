# Raft Consensus

A Rust implementation of the [Raft distributed consensus algorithm](https://raft.github.io/).

## Overview

An implementation of Raft with a pluggable transport layer. It was extracted from a database project of mine and designed to be easy to integrate into other systems.

**Key features:**
- Async/await with Tokio runtime
- Generic `Transport` trait for custom networking
- Built-in `TcpTransport` with configurable timeouts
- Pluggable state machine via the `StateMachine` trait
- Leader election and log replication


## Quick Start

```rust
use raft_consensus::{RaftNode, RaftNodeConfig, TcpTransport};
use std::collections::HashMap;
use std::time::Duration;

// Define your cluster peers
let peers = HashMap::from([
    ("node1".to_string(), "127.0.0.1:5001".parse().unwrap()),
    ("node2".to_string(), "127.0.0.1:5002".parse().unwrap()),
    ("node3".to_string(), "127.0.0.1:5003".parse().unwrap()),
]);

// Create node configuration
let config = RaftNodeConfig::new(
    "node1".to_string(),
    "127.0.0.1:5001".parse().unwrap(),
    peers.clone(),
);

// Create transport and state machine
let transport = TcpTransport::new(peers, Duration::from_millis(500));
let state_machine = MyStateMachine::new();

// Create and run the node
let mut node = RaftNode::new(config, state_machine, transport);
node.run().await?;
```

## Implementing a State Machine

The library uses a trait-based approach for state machines, allowing you to replicate any data structure:

```rust
use raft_consensus::state_machine::StateMachine;

#[derive(Debug)]
struct KvStore {
    data: HashMap<String, String>,
}

impl StateMachine for KvStore {
    fn apply(&mut self, command: &[u8]) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        // Deserialize command, apply to your data structure, return response
    }

    fn snapshot(&self) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        // Serialize current state for snapshots
    }

    fn restore(&mut self, snapshot: &[u8]) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Restore state from a snapshot
    }
}
```

See `examples/kv_store.rs` for a complete key-value store implementation.

## Custom Transport

The default `TcpTransport` works for most cases, but you can implement the `Transport` trait for custom networking (QUIC, Unix sockets, in-memory for testing, etc.):

```rust
use raft_consensus::{Transport, VoteRequest, VoteResponse, AppendEntries, AppendResponse};
use raft_consensus::error::RaftResult;
use async_trait::async_trait;

#[async_trait]
impl Transport for MyTransport {
    async fn call_vote(&self, peer: String, request: VoteRequest) -> RaftResult<VoteResponse> {
        // Your implementation
    }

    async fn call_append(&self, peer: String, request: AppendEntries) -> RaftResult<AppendResponse> {
        // Your implementation
    }
}
```

## Examples

Run the three-node cluster demo:

```bash
cargo run --example three_node_cluster
```

This starts three nodes locally and demonstrates leader election. Watch the logs to see nodes vote and a leader emerge.


## Dependencies

- `tokio` - Async runtime
- `bincode` - Serialization
- `async-trait` - Async trait support
- `tracing` - Structured logging
- `rand` - Election timeout randomization

## License

MIT
