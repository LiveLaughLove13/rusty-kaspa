use crate::kaspaapi::KaspaApi;
#[cfg(feature = "rkstratum_cpu_miner")]
use crate::share_handler::increment_internal_cpu_blocks_all_time;
use kaspa_consensus_core::block::Block;
use kaspa_rpc_core::{RpcRawBlock, SubmitBlockRejectReason, SubmitBlockReport};
use parking_lot::{Condvar, Mutex};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, watch};

// Mirror share_handler's block confirmation behavior so "Blocks" means confirmed BLUE.
const INTERNAL_BLOCK_CONFIRM_RETRY_DELAY: Duration = Duration::from_secs(2);
const INTERNAL_BLOCK_CONFIRM_MAX_ATTEMPTS: usize = 30;

// Performance: batch hash metrics and poll work periodically (high BPS networks).
const BATCH_SIZE: u64 = 1000;
const CHECK_WORK_INTERVAL: u64 = 200;

#[cfg(feature = "rkstratum_cpu_miner")]
pub struct InternalMinerMetrics {
    pub hashes_tried: AtomicU64,
    pub blocks_submitted: AtomicU64,
    pub blocks_accepted: AtomicU64,
}

#[cfg(feature = "rkstratum_cpu_miner")]
impl Default for InternalMinerMetrics {
    fn default() -> Self {
        Self { hashes_tried: AtomicU64::new(0), blocks_submitted: AtomicU64::new(0), blocks_accepted: AtomicU64::new(0) }
    }
}

#[derive(Clone)]
pub struct InternalCpuMinerConfig {
    pub enabled: bool,
    pub mining_address: String,
    pub threads: usize,
    pub throttle: Option<Duration>,
    pub template_poll_interval: Duration,
}

struct Work {
    id: u64,
    block: Block,
    rpc_block: RpcRawBlock,
    pow_state: Arc<kaspa_pow::State>,
}

struct WorkSlot {
    work: Option<Work>,
    version: u64,
}

struct SharedWork {
    slot: Mutex<WorkSlot>,
    cv: Condvar,
}

impl SharedWork {
    fn new() -> Self {
        Self { slot: Mutex::new(WorkSlot { work: None, version: 0 }), cv: Condvar::new() }
    }

    fn publish(&self, work: Work) {
        let mut slot = self.slot.lock();
        slot.version = slot.version.wrapping_add(1);
        slot.work = Some(work);
        self.cv.notify_all();
    }

    fn wait_for_update(&self, last_seen: u64, shutdown_flag: &AtomicBool) -> (u64, Option<Work>) {
        let mut slot = self.slot.lock();
        while slot.version == last_seen && !shutdown_flag.load(Ordering::Acquire) {
            self.cv.wait(&mut slot);
        }
        if shutdown_flag.load(Ordering::Acquire) && slot.version == last_seen {
            return (last_seen, None);
        }
        (
            slot.version,
            slot.work.as_ref().map(|w| Work {
                id: w.id,
                block: w.block.clone(),
                rpc_block: w.rpc_block.clone(),
                pow_state: Arc::clone(&w.pow_state),
            }),
        )
    }

    fn notify_all(&self) {
        self.cv.notify_all();
    }

    /// Drop current template so hash threads idle until a mining-ready template is published.
    fn pause_work(&self) {
        let mut slot = self.slot.lock();
        slot.version = slot.version.wrapping_add(1);
        slot.work = None;
        self.cv.notify_all();
    }
}

pub fn spawn_internal_cpu_miner(
    kaspa_api: Arc<KaspaApi>,
    cfg: InternalCpuMinerConfig,
    shutdown_rx: watch::Receiver<bool>,
) -> Result<Arc<InternalMinerMetrics>, anyhow::Error> {
    if !cfg.enabled {
        return Ok(Arc::new(InternalMinerMetrics::default()));
    }

    if cfg.mining_address.trim().is_empty() {
        return Err(anyhow::anyhow!("internal mining address is required when internal cpu miner is enabled"));
    }

    let work = Arc::new(SharedWork::new());

    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_threads = Arc::clone(&shutdown_flag);
    let work_shutdown = Arc::clone(&work);
    tokio::spawn(async move {
        let mut rx = shutdown_rx;
        let _ = rx.wait_for(|v| *v).await;
        shutdown_flag_threads.store(true, Ordering::Release);
        work_shutdown.notify_all();
    });

    let metrics = Arc::new(InternalMinerMetrics::default());
    let metrics_submit = Arc::clone(&metrics);

    let (submit_tx, mut submit_rx) = mpsc::unbounded_channel::<RpcRawBlock>();
    let kaspa_api_submit = Arc::clone(&kaspa_api);
    let shutdown_flag_submit = Arc::clone(&shutdown_flag);
    let work_on_submit_reject = Arc::clone(&work);
    tokio::spawn(async move {
        while let Some(rpc_block) = submit_rx.recv().await {
            if shutdown_flag_submit.load(Ordering::Acquire) {
                break;
            }

            let meta = Block::try_from(rpc_block.clone()).ok().map(|b| {
                use kaspa_consensus_core::hashing::header;
                let hash = header::hash(&b.header).to_string();
                (hash, b.header.nonce, b.header.blue_score)
            });

            metrics_submit.blocks_submitted.fetch_add(1, Ordering::Relaxed);

            let res = kaspa_api_submit.submit_rpc_block(rpc_block).await;
            match res {
                Ok(response) => {
                    if response.report.is_success() {
                        tracing::info!("[InternalMiner] block accepted by node");

                        if let Some((hash_str, nonce, bluescore)) = meta {
                            let kaspa_api_confirm = Arc::clone(&kaspa_api_submit);
                            let metrics_confirm = Arc::clone(&metrics_submit);
                            tokio::spawn(async move {
                                for _ in 0..INTERNAL_BLOCK_CONFIRM_MAX_ATTEMPTS {
                                    match kaspa_api_confirm.get_current_block_color(&hash_str).await {
                                        Ok(true) => {
                                            metrics_confirm.blocks_accepted.fetch_add(1, Ordering::Relaxed);
                                            increment_internal_cpu_blocks_all_time();
                                            crate::prom::record_internal_cpu_recent_block(hash_str, nonce, bluescore);
                                            tracing::info!("[InternalMiner] block confirmed BLUE in DAG");
                                            return;
                                        }
                                        Ok(false) => {
                                            tokio::time::sleep(INTERNAL_BLOCK_CONFIRM_RETRY_DELAY).await;
                                        }
                                        Err(_) => {
                                            tokio::time::sleep(INTERNAL_BLOCK_CONFIRM_RETRY_DELAY).await;
                                        }
                                    }
                                }
                                tracing::info!(
                                    "[InternalMiner] block not confirmed blue after {} attempts (not counted as Blocks)",
                                    INTERNAL_BLOCK_CONFIRM_MAX_ATTEMPTS
                                );
                            });
                        } else {
                            tracing::warn!(
                                "[InternalMiner] accepted block but could not derive consensus header for BLUE confirmation"
                            );
                        }
                    } else if matches!(
                        &response.report,
                        SubmitBlockReport::Reject(SubmitBlockRejectReason::IsInIBD)
                    ) {
                        work_on_submit_reject.pause_work();
                        tracing::warn!(
                            "[InternalMiner] block rejected by node: {:?} — paused hashing until node is mining-ready",
                            response.report
                        );
                    } else {
                        tracing::warn!("[InternalMiner] block rejected by node: {:?}", response.report);
                    }
                }
                Err(e) => {
                    tracing::warn!("[InternalMiner] submit_rpc_block failed: {e}");
                }
            }
        }
    });

    let work_publisher = Arc::clone(&work);
    let kaspa_api_templates = Arc::clone(&kaspa_api);
    let mining_address = cfg.mining_address.clone();
    let poll = cfg.template_poll_interval;
    let shutdown_flag_templates = Arc::clone(&shutdown_flag);
    let next_id = Arc::new(AtomicU64::new(0));
    let next_id_templates = Arc::clone(&next_id);
    tokio::spawn(async move {
        match kaspa_api_templates.get_block_template_rpc(&mining_address, "internal", "").await {
            Ok((block, rpc_block)) => {
                let id = next_id_templates.fetch_add(1, Ordering::Relaxed);
                let header = block.header.clone();
                let pow_state = Arc::new(kaspa_pow::State::new(&header));
                work_publisher.publish(Work { id, block, rpc_block, pow_state });
            }
            Err(e) => {
                tracing::warn!("[InternalMiner] initial get_block_template_rpc failed: {e}");
            }
        }

        let mut interval = tokio::time::interval(poll);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            if shutdown_flag_templates.load(Ordering::Acquire) {
                break;
            }
            interval.tick().await;
            if shutdown_flag_templates.load(Ordering::Acquire) {
                break;
            }

            match kaspa_api_templates.get_block_template_rpc(&mining_address, "internal", "").await {
                Ok((block, rpc_block)) => {
                    let id = next_id_templates.fetch_add(1, Ordering::Relaxed);
                    let header = block.header.clone();
                    let pow_state = Arc::new(kaspa_pow::State::new(&header));
                    work_publisher.publish(Work { id, block, rpc_block, pow_state });
                }
                Err(e) => {
                    tracing::warn!("[InternalMiner] get_block_template_rpc failed: {e}");
                }
            }
        }
    });

    let threads = cfg.threads.max(1);
    let throttle = cfg.throttle;
    let found_counter = Arc::new(AtomicU64::new(0));

    for thread_idx in 0..threads {
        let work = Arc::clone(&work);
        let submit_tx = submit_tx.clone();
        let shutdown_flag = Arc::clone(&shutdown_flag);
        let found_counter = Arc::clone(&found_counter);
        let metrics_threads = Arc::clone(&metrics);

        std::thread::spawn(move || {
            let mut last_version = 0u64;
            let nonce_step = threads as u64;
            let mut nonce = thread_idx as u64;
            let mut local_hash_count = 0u64;

            loop {
                if shutdown_flag.load(Ordering::Acquire) {
                    break;
                }

                let (ver, maybe_work) = work.wait_for_update(last_version, &shutdown_flag);
                last_version = ver;

                let Some(w) = maybe_work else {
                    continue;
                };

                let mut hashes_since_work_check = 0u64;

                loop {
                    local_hash_count += 1;
                    hashes_since_work_check += 1;

                    let current_nonce = nonce;
                    nonce = nonce.wrapping_add(nonce_step);

                    let (passed, _) = w.pow_state.check_pow(current_nonce);
                    if passed {
                        if local_hash_count > 0 {
                            metrics_threads.hashes_tried.fetch_add(local_hash_count, Ordering::Relaxed);
                            local_hash_count = 0;
                        }

                        let mined_rpc_block = RpcRawBlock {
                            header: {
                                let mut h = w.rpc_block.header.clone();
                                h.nonce = current_nonce;
                                h
                            },
                            transactions: w.rpc_block.transactions.clone(),
                        };
                        let _ = submit_tx.send(mined_rpc_block);
                        found_counter.fetch_add(1, Ordering::Relaxed);

                        if let Some(slot) = work.slot.try_lock() {
                            if slot.version != last_version {
                                drop(slot);
                                break;
                            }
                        }
                        hashes_since_work_check = 0;
                    }

                    if local_hash_count >= BATCH_SIZE {
                        metrics_threads.hashes_tried.fetch_add(BATCH_SIZE, Ordering::Relaxed);
                        local_hash_count -= BATCH_SIZE;
                    }

                    if let Some(d) = throttle {
                        if (hashes_since_work_check & 127) == 0 {
                            std::thread::sleep(d);
                        }
                    }

                    if hashes_since_work_check >= CHECK_WORK_INTERVAL {
                        if shutdown_flag.load(Ordering::Acquire) {
                            if local_hash_count > 0 {
                                metrics_threads.hashes_tried.fetch_add(local_hash_count, Ordering::Relaxed);
                            }
                            return;
                        }

                        let slot = work.slot.lock();
                        if slot.version != last_version {
                            drop(slot);
                            if local_hash_count > 0 {
                                metrics_threads.hashes_tried.fetch_add(local_hash_count, Ordering::Relaxed);
                                local_hash_count = 0;
                            }
                            break;
                        }
                        drop(slot);

                        hashes_since_work_check = 0;
                    }
                }
            }

            if local_hash_count > 0 {
                metrics_threads.hashes_tried.fetch_add(local_hash_count, Ordering::Relaxed);
            }
        });
    }

    Ok(metrics)
}
