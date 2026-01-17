# Raft Consensus

A Rust implementation of the [Raft distributed consensus algorithm](https://raft.github.io/).

## Overview

An implementation of Raft with a pluggable transport layer. It was extracted from a database project of mine and modified to integrate into other systems.

Currently working on separating the state machine logic from the actual Raft
protocol pieces.

**Features:**
- Async Tokio runtime
- `Transport` trait for custom networking
- `TcpTransport` with configurable timeouts
- `StateMachine` trait

## Examples

Run the three-node cluster demo:

```bash
cargo run --example three_node_cluster
```

This starts three nodes locally and demonstrates leader election. Watch the logs to see nodes vote and a leader election.
It also sends each of the nodes a request to store an entry.

## What's left
- Log compaction
- Cluster membership changes
- Persistent log storage
- Log restoration (if a node crashes and comes back up, the log is not rebuilt)

## What's implemented so far
- Leader election with randomized timeouts
- Log replication with AppendEntries RPC
- `Transport` trait (includes TCP transport)
- `StateMachine` trait for custom data structures


