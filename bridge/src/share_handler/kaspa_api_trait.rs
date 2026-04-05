use kaspa_consensus_core::block::Block;

// Trait for kaspa API operations
#[async_trait::async_trait]
pub trait KaspaApiTrait: Send + Sync {
    async fn get_block_template(
        &self,
        wallet_addr: &str,
        remote_app: &str,
        canxium_addr: &str,
    ) -> Result<Block, Box<dyn std::error::Error + Send + Sync>>;

    async fn submit_block(
        &self,
        block: Block,
    ) -> Result<kaspa_rpc_core::SubmitBlockResponse, Box<dyn std::error::Error + Send + Sync>>;

    /// Get balances by addresses (for Prometheus metrics)
    /// Get balances for addresses
    async fn get_balances_by_addresses(
        &self,
        addresses: &[String],
    ) -> Result<Vec<(String, u64)>, Box<dyn std::error::Error + Send + Sync>>;

    async fn get_current_block_color(&self, block_hash: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>;

    /// `true` only when the node reports fully synced for mining (`getSyncStatus`: sink recent + not in transitional IBD).
    async fn is_node_synced_for_mining(&self) -> bool;
}
