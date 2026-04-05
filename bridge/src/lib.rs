//! Kaspa Stratum bridge library.
//!
//! Implementation files are grouped under `util/`, `jsonrpc/`, `mining/`, `stratum/`, `config/`,
//! `kaspa/`, `host/`, and `cpu_miner/`. The crate root re-exports the same module names as before
//! so `kaspa_stratum_bridge::stratum_context`, `::mining_state`, etc. stay stable.

mod util {
    pub mod errors;
    pub mod log_colors;
    pub mod net_utils;
}

mod jsonrpc {
    pub mod jsonrpc_event;
}

mod mining {
    pub mod hasher;
    pub mod mining_state;
    pub mod pow_diagnostic;
}

mod stratum {
    pub mod client_handler;
    pub mod default_client;
    pub mod stratum_context;
    pub mod stratum_line_codec;
    pub mod stratum_listener;
    pub mod stratum_server;
}

mod config {
    pub mod app_config;
}

mod kaspa {
    pub mod kaspaapi;
}

mod host {
    pub mod host_metrics;
}

#[cfg(feature = "rkstratum_cpu_miner")]
mod cpu_miner {
    pub mod rkstratum_cpu_miner;
}

// Public module paths unchanged for downstream / tests.
pub use config::app_config;
pub use host::host_metrics;
pub use jsonrpc::jsonrpc_event;
pub use kaspa::kaspaapi;
pub use mining::hasher;
pub use mining::mining_state;
pub use mining::pow_diagnostic;
pub use stratum::client_handler;
pub use stratum::default_client;
pub use stratum::stratum_context;
pub use stratum::stratum_line_codec;
pub use stratum::stratum_listener;
pub use stratum::stratum_server;
pub use util::errors;
pub use util::log_colors;
pub use util::net_utils;

pub mod prom;
pub mod share_handler;

#[cfg(feature = "rkstratum_cpu_miner")]
pub use cpu_miner::rkstratum_cpu_miner;

pub use app_config::{BridgeConfig, InstanceConfig};
pub use client_handler::*;
pub use default_client::*;
pub use errors::*;
pub use hasher::*;
pub use jsonrpc_event::*;
pub use kaspaapi::*;
pub use mining_state::*;
pub use prom::{WorkerContext, *};
#[cfg(feature = "rkstratum_cpu_miner")]
pub use rkstratum_cpu_miner::*;
pub use share_handler::*;
pub use stratum_context::*;
pub use stratum_listener::*;
pub use stratum_server::BridgeConfig as StratumServerBridgeConfig;
pub use stratum_server::*;
