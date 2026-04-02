//! Mining hot-path microbenchmarks (no live kaspad).
//!
//! Worker scaling: set a comma-separated list of synthetic “worker” multipliers, e.g.:
//!   RKSTRATUM_BENCH_WORKERS=1,8,64,256 cargo bench -p kaspa-stratum-bridge --bench mining_hot_path
//!
//! Interpreting results:
//! - `pow_new_state_and_check` — one `kaspa_pow::State::new` + `check_pow` (core share-validation work).
//! - `pow_job_walk_10` — ten state builds + checks (rough model of wrong job-id walk, upper bound).
//! - `mining_state_add_jobs_W` — storing W jobs per client (template fan-out is separate; this is local state).
//! - `tokio_join_fanout_W` — scheduling W async tasks that each yield once (bridge concurrency shape, not RPC).

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use kaspa_consensus_core::BlueWorkType;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::header::Header;
use kaspa_hashes::Hash;
use kaspa_pow::State as PowState;
use kaspa_stratum_bridge::hasher::serialize_block_header;
use kaspa_stratum_bridge::mining_state::{Job, MiningState};
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::task::JoinSet;

fn worker_param_list() -> Vec<u64> {
    std::env::var("RKSTRATUM_BENCH_WORKERS")
        .ok()
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect::<Vec<_>>())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| vec![1, 4, 8, 16, 32, 64, 128, 256])
}

fn synthetic_header() -> Header {
    Header::new_finalized(
        1,
        vec![vec![1u64.into()]].try_into().unwrap(),
        Hash::from(2u64),
        Hash::from(3u64),
        Hash::from(4u64),
        1_700_000_000_000u64,
        0x1f0fffffu32,
        0x1234567890abcdefu64,
        100u64,
        BlueWorkType::from(1u64),
        0u64,
        Hash::from(5u64),
    )
}

fn synthetic_block() -> Block {
    Block::new(synthetic_header(), vec![])
}

fn bench_pow_check_reuse_state(c: &mut Criterion) {
    let header = synthetic_header();
    let state = PowState::new(&header);
    c.bench_function("pow_check_pow_only_reused_state", |b| {
        b.iter(|| black_box(state.check_pow(black_box(0xdeadbeefcafeu64))));
    });
}

fn bench_pow_new_state_and_check(c: &mut Criterion) {
    let header = synthetic_header();
    c.bench_function("pow_new_state_and_check", |b| {
        b.iter(|| {
            let st = PowState::new(black_box(&header));
            black_box(st.check_pow(black_box(0xdeadbeefcafeu64)));
        });
    });
}

fn bench_pow_job_walk_10(c: &mut Criterion) {
    let header = synthetic_header();
    c.bench_function("pow_job_walk_10_same_header", |b| {
        b.iter(|| {
            for i in 0u64..10 {
                let mut h = header.clone();
                h.nonce = i;
                let st = PowState::new(black_box(&h));
                black_box(st.check_pow(black_box(0x1000 + i)));
            }
        });
    });
}

fn bench_mining_state_add_jobs(c: &mut Criterion) {
    let template = synthetic_block();
    let pre_pow = serialize_block_header(&template).expect("bench header serializes");

    let mut group = c.benchmark_group("mining_state_add_jobs");
    group.sample_size(40);
    for w in worker_param_list() {
        group.throughput(Throughput::Elements(w));
        group.bench_with_input(BenchmarkId::from_parameter(w), &w, |b, w_in: &u64| {
            let n = *w_in;
            b.iter_batched(
                MiningState::new,
                |state| {
                    for _ in 0..n {
                        let job = Job { block: template.clone(), pre_pow_hash: pre_pow };
                        black_box(state.add_job(job));
                    }
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_sequential_pow_fanout(c: &mut Criterion) {
    let header = synthetic_header();
    let mut group = c.benchmark_group("sequential_pow_like_per_worker");
    group.sample_size(30);
    for w in worker_param_list() {
        group.throughput(Throughput::Elements(w));
        group.bench_with_input(BenchmarkId::from_parameter(w), &w, |b, w_in: &u64| {
            let n = *w_in;
            b.iter(|| {
                for i in 0..n {
                    let st = PowState::new(black_box(&header));
                    black_box(st.check_pow(black_box(0x9000 + i)));
                }
            });
        });
    }
    group.finish();
}

fn bench_tokio_join_fanout(c: &mut Criterion) {
    let rt = Runtime::new().expect("runtime");
    let mut group = c.benchmark_group("tokio_join_fanout_yield");
    group.sample_size(20);
    for w in worker_param_list() {
        group.throughput(Throughput::Elements(w));
        group.bench_with_input(BenchmarkId::from_parameter(w), &w, |b, w_in: &u64| {
            let n = *w_in;
            b.iter(|| {
                rt.block_on(async {
                    let mut set = JoinSet::new();
                    for i in 0..n {
                        set.spawn(async move {
                            tokio::task::yield_now().await;
                            black_box(i);
                        });
                    }
                    while let Some(res) = set.join_next().await {
                        res.unwrap();
                    }
                });
            });
        });
    }
    group.finish();
}

/// Optional: concurrent PoW checks (read-only header) — models parallel share validation if jobs shared one header.
fn bench_parallel_pow_rayon(c: &mut Criterion) {
    let header = Arc::new(synthetic_header());
    let mut group = c.benchmark_group("rayon_parallel_pow_check");
    group.sample_size(20);
    for w in worker_param_list() {
        if w > 512 {
            continue;
        }
        group.throughput(Throughput::Elements(w));
        group.bench_with_input(BenchmarkId::from_parameter(w), &w, |b, w_in: &u64| {
            let n = *w_in;
            let h = Arc::clone(&header);
            b.iter(|| {
                use rayon::prelude::*;
                (0..n).into_par_iter().for_each(|i| {
                    let st = PowState::new(h.as_ref());
                    black_box(st.check_pow(black_box(0xa000 + i)));
                });
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_pow_check_reuse_state,
    bench_pow_new_state_and_check,
    bench_pow_job_walk_10,
    bench_mining_state_add_jobs,
    bench_sequential_pow_fanout,
    bench_tokio_join_fanout,
    bench_parallel_pow_rayon,
);
criterion_main!(benches);
