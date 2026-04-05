//! Stratum `mining.submit`: parse job/nonce, duplicate guard, PoW / pool diff, block pipeline.
//!
//! Submodules: [`parse`], [`duplicate`], [`pow_loop`], [`finish`]; [`handle`] wires them in order.

mod duplicate;
mod error;
mod finish;
mod handle;
mod parse;
mod pow_loop;

use super::ShareHandler;
use super::kaspa_api_trait::KaspaApiTrait;
use crate::jsonrpc_event::JsonRpcEvent;
use crate::stratum_context::StratumContext;
use kaspa_consensus_core::block::Block;
use std::sync::Arc;

impl ShareHandler {
    pub async fn handle_submit(
        &self,
        ctx: Arc<StratumContext>,
        event: JsonRpcEvent,
        kaspa_api: Arc<dyn KaspaApiTrait + Send + Sync>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        handle::handle_submit(self, ctx, event, kaspa_api).await.map_err(Into::into)
    }

    #[allow(dead_code)]
    async fn submit_block(
        &self,
        _ctx: &StratumContext,
        _block: Block,
        _nonce: u64,
        _event_id: &serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Block submission is handled at the HandleSubmit level
        // This method is kept for compatibility but actual submission
        // happens when PoW passes network target in handle_submit
        Ok(())
    }
}
