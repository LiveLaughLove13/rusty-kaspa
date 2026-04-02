//! Measure `get_block_template` latency against a live kaspad over gRPC.
//!
//! Usage:
//!   cargo run -p kaspa-stratum-bridge --release --bin bench-kaspad-live -- \
//!     --address kaspa:... --grpc 127.0.0.1:16110 --samples 100 --parallel 8
//!
//! Env (optional): `KASPA_BENCH_GRPC`, `KASPA_BENCH_MINING_ADDRESS`.

use anyhow::{Context, Result};
use clap::Parser;
use kaspa_stratum_bridge::KaspaApi;
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, watch};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(about = "Benchmark get_block_template against a live Kaspa node (same RPC path as the bridge).")]
struct Args {
    /// gRPC address (with or without `grpc://` prefix; same as bridge `kaspad_address`).
    #[arg(long, env = "KASPA_BENCH_GRPC", default_value = "127.0.0.1:16110")]
    grpc: String,

    /// Mining payout address (bech32), same as a Stratum wallet address.
    #[arg(long, env = "KASPA_BENCH_MINING_ADDRESS")]
    address: String,

    /// Total number of template requests to time (after warmup).
    #[arg(long, default_value_t = 50)]
    samples: u32,

    /// Max concurrent in-flight template requests (simulates many workers sharing one node).
    #[arg(long, default_value_t = 1)]
    parallel: u32,

    /// Template fetches before measuring (discarded).
    #[arg(long, default_value_t = 2)]
    warmup: u32,

    /// Do not wait for kaspad to report synced (unsafe if the node is still syncing).
    #[arg(long)]
    skip_sync: bool,
}

fn percentile(sorted_ns: &[u128], p: f64) -> u128 {
    if sorted_ns.is_empty() {
        return 0;
    }
    let n = sorted_ns.len();
    let idx = ((n as f64 - 1.0) * p / 100.0).round() as usize;
    sorted_ns[idx.min(n - 1)]
}

fn print_stats(label: &str, durations: &[Duration]) {
    let mut ns: Vec<u128> = durations.iter().map(|d| d.as_nanos()).collect();
    ns.sort_unstable();
    let n = ns.len();
    if n == 0 {
        println!("{label}: no samples");
        return;
    }
    let sum: u128 = ns.iter().sum();
    let mean_ns = sum / n as u128;
    println!(
        "{label}: n={}  min={:.2}ms  p50={:.2}ms  p95={:.2}ms  p99={:.2}ms  max={:.2}ms  mean={:.2}ms",
        n,
        ns[0] as f64 / 1e6,
        percentile(&ns, 50.0) as f64 / 1e6,
        percentile(&ns, 95.0) as f64 / 1e6,
        percentile(&ns, 99.0) as f64 / 1e6,
        ns[n - 1] as f64 / 1e6,
        mean_ns as f64 / 1e6,
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args = Args::parse();
    if args.samples == 0 {
        anyhow::bail!("--samples must be >= 1");
    }
    if args.parallel == 0 {
        anyhow::bail!("--parallel must be >= 1");
    }

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let api = KaspaApi::new(args.grpc.clone(), None, shutdown_rx.clone()).await.context("failed to connect KaspaApi to node")?;

    if !args.skip_sync {
        api.wait_for_sync_with_shutdown(shutdown_rx.clone()).await.context("wait for sync")?;
    }

    let remote_app = "";
    let canxium_addr = "";

    for i in 0..args.warmup {
        api.get_block_template(&args.address, remote_app, canxium_addr)
            .await
            .with_context(|| format!("warmup template fetch {}", i + 1))?;
    }

    let sem = std::sync::Arc::new(Semaphore::new(args.parallel as usize));
    let api = std::sync::Arc::clone(&api);
    let address = args.address.clone();

    let t0 = Instant::now();
    let mut handles = Vec::with_capacity(args.samples as usize);
    for _ in 0..args.samples {
        let permit = sem.clone().acquire_owned().await.expect("semaphore closed");
        let api = std::sync::Arc::clone(&api);
        let addr = address.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            let start = Instant::now();
            let res = api.get_block_template(&addr, remote_app, canxium_addr).await;
            (start.elapsed(), res)
        }));
    }

    let mut latencies: Vec<Duration> = Vec::with_capacity(args.samples as usize);
    for h in handles {
        let (elapsed, res) = h.await.context("join benchmark task")?;
        res.context("get_block_template")?;
        latencies.push(elapsed);
    }
    let wall = t0.elapsed();

    print_stats("get_block_template", &latencies);
    println!(
        "wall clock: {:.2}s  ({} samples, parallel={}, {:.1} req/s)",
        wall.as_secs_f64(),
        args.samples,
        args.parallel,
        args.samples as f64 / wall.as_secs_f64().max(1e-9),
    );

    drop(shutdown_tx);
    Ok(())
}
