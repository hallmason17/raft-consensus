# Raft Examples

This directory contains examples demonstrating how to use the Raft consensus implementation.

## Key-Value Store (`kv_store.rs`)

A simple in-memory key-value store implementation of the `StateMachine` trait. This shows how to:

- Define commands (Set, Get, Delete) that can be replicated via Raft
- Serialize/deserialize commands and responses using bincode
- Implement snapshot and restore for state machine recovery

### Commands

```rust
pub enum KvCommand {
    Set { key: String, value: String },
    Get { key: String },
    Delete { key: String },
}
```

### Running Tests

```bash
cargo test --example kv_store
```

## Single Node Example (`single_node.rs`)

Demonstrates setting up a single Raft node with the KV store.

```bash
# Run with default logging
cargo run --example single_node

# Run with debug logging
RUST_LOG=debug cargo run --example single_node
```

## Three Node Cluster (`three_node_cluster.rs`)

Demonstrates a 3-node Raft cluster that can elect a leader and replicate state.

```bash
# Run with info-level logging
RUST_LOG=info cargo run --example three_node_cluster

# Run with debug logging to see detailed Raft protocol messages
RUST_LOG=debug cargo run --example three_node_cluster
```

### What to Watch For

When you run the three-node cluster, you'll see:

1. **Initial State**: All nodes start as Followers
2. **Election Timeout**: After ~150-300ms, a node will timeout and become a Candidate
3. **Vote Requests**: The Candidate sends RequestVote RPCs to peers
4. **Leader Election**: If it gets a majority (2 out of 3), it becomes Leader
5. **Heartbeats**: The Leader sends periodic heartbeats every 100ms to maintain leadership

Example log output:
```
INFO raft_consensus::raft_node: Starting election for term 1
INFO raft_consensus::raft_node: Granted vote to node2 for term 1
INFO raft_consensus::raft_node: Election succeeded, becoming leader
INFO raft_consensus::raft_node: Becoming leader
INFO raft_consensus::raft_node: Sending heartbeats
```

## Implementing Your Own State Machine

To create your own state machine:

1. Define your command types with `#[derive(Encode, Decode)]`
2. Implement the `StateMachine` trait:
   - `apply(&mut self, command: &[u8])` - Execute a command
   - `snapshot(&self)` - (Optional) Create a snapshot
   - `restore(&mut self, snapshot: &[u8])` - (Optional) Restore from snapshot

Example:
```rust
use bincode::{Encode, Decode};
use raft_consensus::state_machine::StateMachine;

#[derive(Debug, Clone, Encode, Decode)]
enum MyCommand {
    // Your commands here
}

#[derive(Debug)]
struct MyStateMachine {
    // Your state here
}

impl StateMachine for MyStateMachine {
    fn apply(&mut self, command: &[u8]) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        let (cmd, _): (MyCommand, _) =
            bincode::decode_from_slice(command, bincode::config::standard())?;

        // Execute command and return result
        // ...
    }
}
```
