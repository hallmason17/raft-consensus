//! Key-Value Store implementation of the StateMachine trait.
//!
//! This module provides a simple in-memory key-value store that can be used
//! with the Raft consensus algorithm. Commands are serialized using bincode.

use bincode::{Decode, Encode};
use raft_consensus::state_machine::StateMachine;
use std::collections::HashMap;

/// Commands that can be executed on the key-value store.
#[derive(Debug, Clone, Encode, Decode)]
pub enum KvCommand {
    /// Set a key to a value
    Set { key: String, value: String },
    /// Get the value for a key
    Get { key: String },
    /// Delete a key
    Delete { key: String },
}

/// Response from executing a command.
#[derive(Debug, Clone, Encode, Decode)]
pub enum KvResponse {
    /// Successful set or delete operation
    Ok,
    /// Value retrieved from a get operation
    Value(Option<String>),
    /// Error message
    Error(String),
}

/// A simple in-memory key-value store.
#[derive(Debug, Clone)]
pub struct KvStore {
    data: HashMap<String, String>,
}

impl KvStore {
    /// Creates a new empty key-value store.
    pub fn new() -> Self {
        KvStore {
            data: HashMap::new(),
        }
    }
}

impl Default for KvStore {
    fn default() -> Self {
        Self::new()
    }
}

// Custom error type.
#[derive(Debug, thiserror::Error)]
pub enum KvError {
    #[error("decode error: {0}")]
    Decode(#[from] bincode::error::DecodeError),
    #[error("encode error: {0}")]
    Encode(#[from] bincode::error::EncodeError),
}

impl StateMachine for KvStore {
    type Error = KvError;
    fn apply(&mut self, command: &[u8]) -> Result<Vec<u8>, Self::Error> {
        // Deserialize the command
        let (cmd, _): (KvCommand, _) =
            bincode::decode_from_slice(command, bincode::config::standard())?;

        // Execute the command
        let response = match cmd {
            KvCommand::Set { key, value } => {
                self.data.insert(key, value);
                KvResponse::Ok
            }
            KvCommand::Get { key } => {
                let value = self.data.get(&key).cloned();
                KvResponse::Value(value)
            }
            KvCommand::Delete { key } => {
                self.data.remove(&key);
                KvResponse::Ok
            }
        };

        // Serialize the response
        let response_bytes = bincode::encode_to_vec(&response, bincode::config::standard())?;
        Ok(response_bytes)
    }

    fn snapshot(&self) -> Result<Vec<u8>, Self::Error> {
        // Serialize the entire HashMap as a snapshot
        let snapshot = bincode::encode_to_vec(&self.data, bincode::config::standard())?;
        Ok(snapshot)
    }

    fn restore(&mut self, snapshot: &[u8]) -> Result<(), Self::Error> {
        // Deserialize the snapshot and restore the HashMap
        let (data, _): (HashMap<String, String>, _) =
            bincode::decode_from_slice(snapshot, bincode::config::standard())?;
        self.data = data;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serialize_command(cmd: &KvCommand) -> Vec<u8> {
        bincode::encode_to_vec(cmd, bincode::config::standard()).unwrap()
    }

    fn deserialize_response(bytes: &[u8]) -> KvResponse {
        let (resp, _) = bincode::decode_from_slice(bytes, bincode::config::standard()).unwrap();
        resp
    }

    #[test]
    fn test_set_and_get() {
        let mut store = KvStore::new();

        // Set a value
        let set_cmd = KvCommand::Set {
            key: "foo".to_string(),
            value: "bar".to_string(),
        };
        let result = store.apply(&serialize_command(&set_cmd)).unwrap();
        let response = deserialize_response(&result);
        assert!(matches!(response, KvResponse::Ok));

        // Get the value
        let get_cmd = KvCommand::Get {
            key: "foo".to_string(),
        };
        let result = store.apply(&serialize_command(&get_cmd)).unwrap();
        let response = deserialize_response(&result);
        assert!(matches!(response, KvResponse::Value(Some(ref v)) if v == "bar"));
    }

    #[test]
    fn test_get_nonexistent() {
        let mut store = KvStore::new();

        let get_cmd = KvCommand::Get {
            key: "missing".to_string(),
        };
        let result = store.apply(&serialize_command(&get_cmd)).unwrap();
        let response = deserialize_response(&result);
        assert!(matches!(response, KvResponse::Value(None)));
    }

    #[test]
    fn test_delete() {
        let mut store = KvStore::new();

        // Set a value
        let set_cmd = KvCommand::Set {
            key: "temp".to_string(),
            value: "data".to_string(),
        };
        store.apply(&serialize_command(&set_cmd)).unwrap();

        // Delete it
        let delete_cmd = KvCommand::Delete {
            key: "temp".to_string(),
        };
        let result = store.apply(&serialize_command(&delete_cmd)).unwrap();
        let response = deserialize_response(&result);
        assert!(matches!(response, KvResponse::Ok));

        // Verify it's gone
        let get_cmd = KvCommand::Get {
            key: "temp".to_string(),
        };
        let result = store.apply(&serialize_command(&get_cmd)).unwrap();
        let response = deserialize_response(&result);
        assert!(matches!(response, KvResponse::Value(None)));
    }

    #[test]
    fn test_snapshot_and_restore() {
        let mut store = KvStore::new();

        // Add some data
        let set_cmd1 = KvCommand::Set {
            key: "key1".to_string(),
            value: "value1".to_string(),
        };
        let set_cmd2 = KvCommand::Set {
            key: "key2".to_string(),
            value: "value2".to_string(),
        };
        store.apply(&serialize_command(&set_cmd1)).unwrap();
        store.apply(&serialize_command(&set_cmd2)).unwrap();

        // Take a snapshot
        let snapshot = store.snapshot().unwrap();

        // Create a new store and restore from snapshot
        let mut new_store = KvStore::new();
        new_store.restore(&snapshot).unwrap();

        // Verify data was restored
        let get_cmd = KvCommand::Get {
            key: "key1".to_string(),
        };
        let result = new_store.apply(&serialize_command(&get_cmd)).unwrap();
        let response = deserialize_response(&result);
        assert!(matches!(response, KvResponse::Value(Some(ref v)) if v == "value1"));
    }
}

fn main() {}
