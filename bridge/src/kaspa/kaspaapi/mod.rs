//! Kaspa gRPC client (`KaspaApi`), node status snapshot for dashboards, and coinbase tag helpers.
//!
//! Split across `coinbase_tag`, `block_submit_guard`, `node_status`, and `api` for reviewability.

mod api;
mod block_submit_guard;
mod coinbase_tag;
mod node_status;

pub use api::KaspaApi;
pub use node_status::{NODE_STATUS, NodeStatusApi, NodeStatusSnapshot, network_display_from_id, node_status_for_api};
