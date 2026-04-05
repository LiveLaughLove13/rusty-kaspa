/// Error for disconnected clients
#[derive(Debug, thiserror::Error)]
#[error("disconnecting")]
pub struct ErrorDisconnected;

/// Wallet / worker / miner-app strings for a connection (single lock vs four separate mutexes).
#[derive(Clone, Default, Debug)]
pub struct ClientIdentity {
    pub wallet_addr: String,
    pub worker_name: String,
    pub canxium_addr: String,
    pub remote_app: String,
}

/// Context summary for logging
#[derive(Debug, Clone)]
pub struct ContextSummary {
    pub remote_addr: String,
    pub remote_port: u16,
    pub wallet_addr: String,
    pub worker_name: String,
    pub remote_app: String,
}

#[cfg(test)]
mod stratum_context_types_smoke {
    #[test]
    fn client_identity_default_empty() {
        let id = super::ClientIdentity::default();
        assert!(id.wallet_addr.is_empty());
    }
}
