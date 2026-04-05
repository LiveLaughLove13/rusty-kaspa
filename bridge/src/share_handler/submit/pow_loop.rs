//! PoW validation, network block submit, pool difficulty, job-ID workaround.

use super::super::ShareHandler;
use super::super::duplicate_submit::DuplicateSubmitOutcome;
use super::super::kaspa_api_trait::KaspaApiTrait;
use super::error::SubmitRunError;
use super::parse::PreparedSubmit;
use crate::{
    log_colors::LogColors,
    mining_state::GetMiningState,
    prom::{
        record_block_accepted_by_node, record_block_found, record_block_not_confirmed_blue, record_invalid_share, record_stale_share,
    },
    stratum_context::StratumContext,
};
use kaspa_consensus_core::block::Block;
use num_bigint::BigUint;
use num_traits::ToPrimitive;
use num_traits::Zero;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

const BLOCK_CONFIRM_RETRY_DELAY: Duration = Duration::from_secs(2);
const BLOCK_CONFIRM_MAX_ATTEMPTS: usize = 30;

pub(super) enum PowDone {
    /// `mining.submit` already answered (stale/bad block path).
    AlreadyFinished,
    /// Run weak-share vs accepted-share finishing in `finish`.
    Continue { invalid_share: bool },
}

pub(super) async fn run_pow_validation_loop(
    handler: &ShareHandler,
    ctx: Arc<StratumContext>,
    event: &crate::jsonrpc_event::JsonRpcEvent,
    kaspa_api: Arc<dyn KaspaApiTrait + Send + Sync>,
    prep: &PreparedSubmit,
) -> Result<PowDone, SubmitRunError> {
    let state = GetMiningState(ctx.as_ref());
    let nonce_val = prep.nonce_val;
    // PoW validation with job ID workaround
    // Go validates the submitted job first, then tries previous jobs if share doesn't meet pool difficulty
    // This workaround handles IceRiver/Bitmain ASICs that submit jobs with incorrect IDs
    let mut current_job_id = prep.job_id;
    let mut current_job = prep.job.clone();
    let mut invalid_share = false;
    let mut pow_passed;
    let mut pow_value;
    let max_jobs = state.max_jobs() as u64;

    debug!("[SUBMIT] Starting PoW validation for job_id: {} (max_jobs: {})", current_job_id, max_jobs);

    loop {
        // DIAGNOSTIC: Run full diagnostic on first share
        static DIAGNOSTIC_RUN: std::sync::Once = std::sync::Once::new();
        let header = &current_job.block.header;
        let mut header_clone = (**header).clone();

        DIAGNOSTIC_RUN.call_once(|| {
            debug!("{}", LogColors::block("===== RUNNING POW DIAGNOSTIC ====="));
            crate::pow_diagnostic::diagnose_pow_issue(&header_clone, nonce_val);
            debug!("{}", LogColors::block("===== DIAGNOSTIC COMPLETE ====="));
        });

        // DEBUG: Compare what we sent to ASIC vs what we're validating (moved to debug level)
        debug!("{} {}", LogColors::validation("[DEBUG]"), LogColors::label("===== VALIDATION DEBUG ====="));
        debug!(
            "{} {} {}",
            LogColors::validation("[DEBUG]"),
            LogColors::label("Job we sent to ASIC:"),
            format!("job_id={}, timestamp={}", current_job_id, current_job.block.header.timestamp)
        );
        debug!(
            "{} {} {}",
            LogColors::validation("[DEBUG]"),
            LogColors::label("ASIC submitted:"),
            format!("job_id={}, nonce=0x{:x}", current_job_id, nonce_val)
        );
        debug!(
            "{} {} {}",
            LogColors::validation("[DEBUG]"),
            LogColors::label("Header we're validating:"),
            format!("timestamp={}, nonce={}, bits=0x{:08x}", header_clone.timestamp, header_clone.nonce, header_clone.bits)
        );

        // Set the nonce in the header
        header_clone.nonce = nonce_val;

        debug!(
            "{} {} {}",
            LogColors::validation("[DEBUG]"),
            LogColors::label("After setting nonce:"),
            format!("timestamp={}, nonce=0x{:x}, bits=0x{:08x}", header_clone.timestamp, header_clone.nonce, header_clone.bits)
        );

        // Use kaspa_pow::State for PoW validation against the header's compact bits target.
        use kaspa_pow::State as PowState;
        let pow_state = PowState::new(&header_clone);
        let (check_passed, pow_value_uint256) = pow_state.check_pow(nonce_val);

        // Convert Uint256 to BigUint for comparison
        pow_value = num_bigint::BigUint::from_bytes_be(&pow_value_uint256.to_be_bytes());

        debug!(
            "{} {} {}",
            LogColors::validation("[DEBUG]"),
            LogColors::label("PowState result:"),
            format!("check_passed={}, pow_value={:x}", check_passed, pow_value)
        );

        // Calculate network target from header.bits (debug/diagnostic only).
        use crate::hasher::calculate_target;
        let network_target = calculate_target(header_clone.bits as u64);

        // Check if pow_value meets network target (lower hash is better)
        let meets_network_target = pow_value <= network_target;
        // IMPORTANT: Use kaspa_pow's own compact-target handling as the source of truth.
        // This avoids any potential mismatch in our BigUint conversion/comparison path.
        pow_passed = check_passed;

        let pow_value_bytes = pow_value.to_bytes_be();
        let network_target_bytes = network_target.to_bytes_be();

        debug!("[SUBMIT] Target comparison:");
        debug!("[SUBMIT]   - pow_value: {:x} ({} bytes)", pow_value, pow_value_bytes.len());
        debug!("[SUBMIT]   - network_target: {:x} ({} bytes)", network_target, network_target_bytes.len());
        debug!("[SUBMIT]   - meets_network_target(BigUint): {}", meets_network_target);
        debug!("[SUBMIT]   - check_passed(kaspa_pow): {}", check_passed);

        debug!(
            "[SUBMIT] PoW check result: passed={}, pow_value={:x}, network_target={:x}, header.bits={}",
            pow_passed, pow_value, network_target, header_clone.bits
        );

        // Log detailed validation information with colors (moved to debug level)
        debug!(
            "{} {} {}",
            LogColors::validation("[VALIDATION]"),
            LogColors::label("PoW Validation -"),
            format!(
                "Nonce: {:x}, Pow Value: {:x} ({} bytes), Network Target: {:x} ({} bytes)",
                nonce_val,
                pow_value,
                pow_value_bytes.len(),
                network_target,
                network_target_bytes.len()
            )
        );
        debug!(
            "{} {} {}",
            LogColors::validation("[VALIDATION]"),
            LogColors::label("Comparison:"),
            format!("pow_value <= network_target = {} (lower hash is better)", meets_network_target)
        );
        debug!(
            "{} {} {}",
            LogColors::validation("[VALIDATION]"),
            LogColors::label("PowState.check_pow() result:"),
            format!("passed={}, Header bits: {}", pow_passed, header_clone.bits)
        );

        // On devnet, network difficulty is very low, so we should see blocks being found
        // Log at debug level (detailed validation logs moved to debug)
        if pow_passed {
            debug!(
                "{} {} {}",
                LogColors::validation("[VALIDATION]"),
                LogColors::block("*** NETWORK TARGET PASSED ***"),
                format!("pow_value={:x} <= network_target={:x}", pow_value, network_target)
            );
        } else if !network_target.is_zero() {
            let ratio = if !pow_value.is_zero() {
                let target_f64 = network_target.to_f64().unwrap_or(0.0);
                let pow_f64 = pow_value.to_f64().unwrap_or(1.0);
                if pow_f64 > 0.0 { (target_f64 / pow_f64) * 100.0 } else { 0.0 }
            } else {
                0.0
            };
            debug!(
                "{} {} {}",
                LogColors::validation("[VALIDATION]"),
                LogColors::label("Network target NOT met -"),
                format!("pow_value={:x} > network_target={:x} ({}% of target)", pow_value, network_target, ratio)
            );
        } else {
            warn!("{} {}", LogColors::validation("[VALIDATION]"), LogColors::error("Network target is ZERO - cannot validate!"));
        }

        // Check network target (block)
        // Use meets_network_target (not pow_passed) for network target validation
        // Go code compares: powValue.Cmp(&powState.Target) <= 0 where Target is network target from header.bits
        // We calculate network_target directly from current job's header.bits (not from stored state)
        // This ensures we use the correct target for each job, as different jobs may have different header.bits
        if meets_network_target {
            let wallet_addr = ctx.identity.lock().wallet_addr.clone();
            let worker_name = ctx.identity.lock().worker_name.clone();
            let prefix = handler.log_prefix();

            info!(
                "{} {} {}",
                prefix,
                LogColors::block("===== BLOCK FOUND! ====="),
                format!("Worker: {}, Wallet: {}, Nonce: {:x}", worker_name, wallet_addr, nonce_val)
            );
            debug!(
                "{} {} {} {}",
                prefix,
                LogColors::block("[BLOCK]"),
                LogColors::label("ACCEPTANCE REASON:"),
                format!("pow_value ({:x}) <= network_target ({:x})", pow_value, network_target)
            );
            debug!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("Pow Value:"), format!("{:x}", pow_value));

            // Log block details before creating the block (to avoid borrow issues)
            let header_bits = header_clone.bits;
            let header_version = header_clone.version;
            let original_timestamp = header_clone.timestamp;

            // Block found - submit it
            // Only set the nonce - keep all other header fields from the real block template
            // The header comes directly from the Kaspa node via get_block_template_call()
            // We preserve: version, bits, timestamp, all hash fields, parents, scores, etc.
            header_clone.nonce = nonce_val;

            // Verify timestamp is still valid (not too old)
            // Kaspa typically accepts blocks with timestamps within a reasonable window
            // Block templates are fetched frequently, so the timestamp should be recent
            let current_time_ms =
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
            let timestamp_age_ms = current_time_ms.saturating_sub(original_timestamp);
            let timestamp_age_sec = timestamp_age_ms / 1000;

            // Log header verification to confirm we're using real headers (moved to debug level)
            debug!(
                "{} {} {}",
                LogColors::block("[BLOCK]"),
                LogColors::label("Header Verification:"),
                "Using REAL header from Kaspa node block template"
            );
            debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("  - Header Version:"), header_version);
            debug!(
                "{} {} {}",
                LogColors::block("[BLOCK]"),
                LogColors::label("  - Header Bits:"),
                format!("{} (0x{:x})", header_bits, header_bits)
            );
            debug!(
                "{} {} {}",
                LogColors::block("[BLOCK]"),
                LogColors::label("  - Timestamp:"),
                format!("{} (age: {}s, preserved from template)", original_timestamp, timestamp_age_sec)
            );
            debug!(
                "{} {} {}",
                LogColors::block("[BLOCK]"),
                LogColors::label("  - Nonce:"),
                format!("{:x} (set from ASIC submission)", nonce_val)
            );

            // Warn if timestamp is very old (more than 60 seconds)
            // This shouldn't happen with frequent template updates, but log it for debugging
            if timestamp_age_sec > 60 {
                warn!(
                    "{} {} {}",
                    LogColors::block("[BLOCK]"),
                    LogColors::error("Timestamp is old:"),
                    format!("{} seconds old - block template may be stale", timestamp_age_sec)
                );
            }

            // Create new block with updated header
            let transactions_vec = current_job.block.transactions.iter().cloned().collect();
            let block = Block::from_arcs(Arc::new(header_clone), Arc::new(transactions_vec));
            let blue_score = block.header.blue_score;

            // Calculate block hash immediately after block creation
            // Use kaspa_consensus_core::hashing::header::hash() for block hash calculation
            // In Kaspa, the block hash is the header hash (transactions are represented by hash_merkle_root in header)
            use kaspa_consensus_core::hashing::header;
            let block_hash = header::hash(&block.header).to_string();

            // Log prominent "Block Found" message with hash
            info!("{} {} {}", prefix, LogColors::block("BLOCK FOUND!"), format!("Hash: {}", block_hash));
            debug!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("Worker:"), worker_name);
            debug!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("Wallet:"), wallet_addr);
            debug!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("Nonce:"), format!("{:x}", nonce_val));

            // Log block submission details before submission (moved to debug level)
            debug!("{} {}", LogColors::block("[BLOCK]"), LogColors::block("=== SUBMITTING BLOCK TO NODE ==="));
            debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Worker:"), worker_name);
            debug!(
                "{} {} {}",
                LogColors::block("[BLOCK]"),
                LogColors::label("Nonce:"),
                format!("{:x} (0x{:016x})", nonce_val, nonce_val)
            );
            debug!(
                "{} {} {}",
                LogColors::block("[BLOCK]"),
                LogColors::label("Bits:"),
                format!("{} (0x{:08x})", header_bits, header_bits)
            );
            debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Timestamp:"), format!("{}", original_timestamp));
            debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Blue Score:"), blue_score);
            debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Pow Value:"), format!("{:x}", pow_value));
            debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Network Target:"), format!("{:x}", network_target));
            debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Job ID:"), current_job_id);
            debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Wallet:"), wallet_addr);
            debug!(
                "{} {} {}",
                LogColors::block("[BLOCK]"),
                LogColors::label("Client:"),
                format!("{}:{}", ctx.remote_addr(), ctx.remote_port())
            );
            debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Block Hash:"), block_hash);
            debug!("{} {}", LogColors::block("[BLOCK]"), "Calling kaspa_api.submit_block()...");

            // Submit block to node
            let block_submit_result = kaspa_api.submit_block(block.clone()).await;

            match block_submit_result {
                Ok(response) => {
                    if !response.report.is_success() {
                        let prefix = handler.log_prefix();
                        warn!("{} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::error("Block rejected by node"));
                        warn!(
                            "{} {} {} {}",
                            prefix,
                            LogColors::block("[BLOCK]"),
                            LogColors::label("REJECTION REASON:"),
                            format!("{:?}", response.report)
                        );
                        invalid_share = true;
                        break;
                    }

                    let prefix = handler.log_prefix();
                    // Block accepted - log after submit to get it submitted faster
                    info!(
                        "{} {} {}",
                        prefix,
                        LogColors::block("[BLOCK]"),
                        LogColors::block(&format!("Block submitted successfully! Hash: {}", block_hash))
                    );
                    info!(
                        "{} {} {}",
                        prefix,
                        LogColors::block("[BLOCK]"),
                        LogColors::block(&format!("BLOCK ACCEPTED BY NODE! Hash: {}", block_hash))
                    );
                    info!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("  - Worker:"), worker_name);
                    info!(
                        "{} {} {} {}",
                        prefix,
                        LogColors::block("[BLOCK]"),
                        LogColors::label("  - Nonce:"),
                        format!("{:x}", nonce_val)
                    );

                    let stats = handler.get_create_stats(&ctx);
                    let overall = handler.overall.clone();
                    let instance_id = handler.instance_id.clone();
                    let prom_worker = crate::prom::WorkerContext {
                        instance_id: handler.instance_id.clone(),
                        worker_name: worker_name.clone(),
                        miner: String::new(),
                        wallet: wallet_addr.clone(),
                        ip: format!("{}:{}", ctx.remote_addr(), ctx.remote_port()),
                    };

                    record_block_accepted_by_node(&prom_worker);

                    let kaspa_api = Arc::clone(&kaspa_api);
                    let block_hash_for_confirm = block_hash.clone();

                    tokio::spawn(async move {
                        for _ in 0..BLOCK_CONFIRM_MAX_ATTEMPTS {
                            match kaspa_api.get_current_block_color(&block_hash_for_confirm).await {
                                Ok(true) => {
                                    *stats.blocks_found.lock() += 1;
                                    *overall.blocks_found.lock() += 1;
                                    record_block_found(&prom_worker, nonce_val, blue_score, block_hash_for_confirm.clone());
                                    info!(
                                        "[{}] {} {}",
                                        instance_id,
                                        LogColors::block("[BLOCK]"),
                                        LogColors::block(&format!("Block confirmed BLUE in DAG! Hash: {}", block_hash_for_confirm))
                                    );
                                    return;
                                }
                                Ok(false) => {
                                    tokio::time::sleep(BLOCK_CONFIRM_RETRY_DELAY).await;
                                }
                                Err(_) => {
                                    tokio::time::sleep(BLOCK_CONFIRM_RETRY_DELAY).await;
                                }
                            }
                        }

                        record_block_not_confirmed_blue(&prom_worker);
                        info!(
                            "[{}] {} {}",
                            instance_id,
                            LogColors::block("[BLOCK]"),
                            LogColors::label(&format!(
                                "Block not confirmed blue after {} attempts (not counted as Blocks). Hash: {}",
                                BLOCK_CONFIRM_MAX_ATTEMPTS, block_hash_for_confirm
                            ))
                        );
                    });

                    // Return allows HandleSubmit to record share (blocks are shares too!)
                    // After successful block submission, continue to record the share
                    // Don't return early - let the code continue to record the share
                    invalid_share = false;
                    break;
                }
                Err(e) => {
                    let prefix = handler.log_prefix();
                    // Only check for "ErrDuplicateBlock" (not "duplicate" or "stale")
                    // Block submission failed
                    let error_str = e.to_string();
                    error!("{} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::error("Block submission FAILED"));
                    error!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("Worker:"), worker_name);
                    error!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("Blockhash:"), block_hash);
                    error!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::error("Error:"), error_str);

                    if error_str.contains("ErrDuplicateBlock") {
                        // Block rejected, stale
                        warn!("{} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::error("block rejected, stale"));
                        warn!(
                            "{} {} {} {}",
                            prefix,
                            LogColors::block("[BLOCK]"),
                            LogColors::label("REJECTION REASON:"),
                            "Block was already submitted to the network (stale/duplicate)"
                        );

                        {
                            let now = Instant::now();
                            let mut guard = handler.duplicate_submit_guard.lock();
                            guard.set_outcome(&prep.submit_key, now, DuplicateSubmitOutcome::Stale);
                        }

                        let stats = handler.get_create_stats(&ctx);
                        *stats.stale_shares.lock() += 1;
                        *handler.overall.stale_shares.lock() += 1;

                        record_stale_share(&crate::prom::WorkerContext {
                            instance_id: handler.instance_id.clone(),
                            worker_name: worker_name.clone(),
                            miner: String::new(),
                            wallet: wallet_addr.clone(),
                            ip: format!("{}:{}", ctx.remote_addr(), ctx.remote_port()),
                        });
                        ctx.reply_stale_share(event.id.clone()).await?;
                        return Ok(PowDone::AlreadyFinished);
                    } else {
                        // Block rejected, unknown issue (probably bad pow)
                        warn!(
                            "{} {} {}",
                            prefix,
                            LogColors::block("[BLOCK]"),
                            LogColors::error("block rejected, unknown issue (probably bad pow)")
                        );
                        error!(
                            "{} {} {} {}",
                            prefix,
                            LogColors::block("[BLOCK]"),
                            LogColors::label("REJECTION REASON:"),
                            "Block failed node validation (probably bad pow)"
                        );
                        error!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::error("Error:"), error_str);

                        let stats = handler.get_create_stats(&ctx);
                        *stats.invalid_shares.lock() += 1;
                        *handler.overall.invalid_shares.lock() += 1;

                        record_invalid_share(&crate::prom::WorkerContext {
                            instance_id: handler.instance_id.clone(),
                            worker_name: worker_name.clone(),
                            miner: String::new(),
                            wallet: wallet_addr.clone(),
                            ip: format!("{}:{}", ctx.remote_addr(), ctx.remote_port()),
                        });

                        {
                            let now = Instant::now();
                            let mut guard = handler.duplicate_submit_guard.lock();
                            guard.set_outcome(&prep.submit_key, now, DuplicateSubmitOutcome::Bad);
                        }
                        ctx.reply_bad_share(event.id.clone()).await?;
                        return Ok(PowDone::AlreadyFinished);
                    }
                }
            }
        }

        // Check pool difficulty
        let pool_target = state.stratum_diff().map(|d| d.target_value.clone()).unwrap_or_else(BigUint::zero);

        // Compare FULL pow_value against pool_target (not just lower bits)
        // Compare full 256-bit values
        let pow_bytes = pow_value.to_bytes_be();
        let target_bytes = pool_target.to_bytes_be();

        // Log difficulty check for debugging
        if pool_target.is_zero() {
            warn!("stratum_diff target is zero! pow_value: {:x}, pool_target: {:x}", pow_value, pool_target);
        } else {
            let pow_len = pow_bytes.len();
            let target_len = target_bytes.len();

            debug!(
                "difficulty check: nonce: {:x} ({}), pow_value (full): {:x} ({} bytes), pool_target: {:x} ({} bytes), diff_value: {:?}, pow_value <= pool_target = {}",
                nonce_val,
                nonce_val,
                pow_value,
                pow_len,
                pool_target,
                target_len,
                state.stratum_diff().map(|d| d.diff_value),
                pow_value <= pool_target
            );
            debug!(
                "Full comparison - pow_value: {:x} ({} bytes), pool_target: {:x} ({} bytes)",
                pow_value, pow_len, pool_target, target_len
            );
        }

        // Check pool difficulty (stratum target)
        // If pow_value >= pool_target, share doesn't meet pool difficulty
        // Higher hash value means worse share
        if pow_value >= pool_target {
            // Share doesn't meet pool difficulty - might be wrong job ID (moved to debug to keep terminal clean)
            let worker_name = ctx.identity.lock().worker_name.clone();
            debug!(
                "{} {} {}",
                LogColors::validation("INVALID SHARE (too high)"),
                LogColors::label("worker:"),
                format!(
                    "{}, nonce: {:x}, pow_value: {:x}, pool_target: {:x}, pow_ge_pool_target: true",
                    worker_name, nonce_val, pow_value, pool_target
                )
            );

            if current_job_id == prep.job_id {
                debug!("low diff share... checking for bad job ID ({})", current_job_id);
                invalid_share = true;
            }

            // Job ID workaround for Bitmain/IceRiver ASICs - try previous jobs
            // Validate job ID: jobId == 1 || jobId%maxJobs == submitInfo.jobId%maxJobs+1
            if current_job_id == 1 || (current_job_id % max_jobs == ((prep.job_id % max_jobs) + 1) % max_jobs) {
                // Exhausted all previous blocks (wrapped around or reached job 1)
                debug!("Job ID loop exhausted: current_job_id={}, job_id={}, max_jobs={}", current_job_id, prep.job_id, max_jobs);
                break;
            } else {
                // Try previous job ID
                let prev_job_id = current_job_id - 1;
                if let Some(prev_job) = state.get_job(prev_job_id) {
                    current_job_id = prev_job_id;
                    current_job = prev_job;
                    debug!("Trying previous job ID: {} (submitted as {})", current_job_id, prep.job_id);
                    // Continue loop to validate with previous job
                    continue;
                } else {
                    // Job doesn't exist, exit loop - bad share will be recorded
                    debug!("Previous job ID {} doesn't exist, exiting loop", prev_job_id);
                    break;
                }
            }
        } else {
            // Valid share (pow_value < pool_target) - moved to debug to keep terminal clean
            let worker_name = ctx.identity.lock().worker_name.clone();
            debug!(
                "{} {} {}",
                LogColors::validation("VALID SHARE"),
                LogColors::label("worker:"),
                format!(
                    "{}, nonce: {:x}, pow_value: {:x}, pool_target: {:x}, pow_lt_pool_target: true",
                    worker_name, nonce_val, pow_value, pool_target
                )
            );

            if invalid_share {
                debug!("found correct job ID: {} (submitted as {})", current_job_id, prep.job_id);
            }
            invalid_share = false;
            break;
        }
    }

    Ok(PowDone::Continue { invalid_share })
}
