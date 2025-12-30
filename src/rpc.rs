use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;

use crate::Rpc;

#[allow(unused)]
pub struct RaftClient {
    leader_addr: SocketAddr,
    transport: Arc<dyn Rpc>,
}
impl RaftClient {
    pub async fn submit_command(&self, _command: &[u8]) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
}
