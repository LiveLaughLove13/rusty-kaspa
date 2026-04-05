//! Prometheus metrics, worker counters, and HTTP dashboard (`/metrics`, `/api/*`, static files).
//! Split into [`metrics`] (registries + record helpers) and [`http`] (TCP HTTP server).

mod http;
mod metrics;

pub use http::{set_web_config_path, set_web_status_config, start_prom_server, start_web_server_all};
pub use metrics::*;
