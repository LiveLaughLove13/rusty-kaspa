//! Pure PoW / pool-target helpers for the submit loop (unit-tested, no I/O).

use num_bigint::BigUint;
use num_traits::Zero;

/// `true` when the share does **not** meet pool (Stratum) difficulty.
///
/// Convention: lower hash is better. The share is valid for the pool only when
/// `pow_value < pool_target`. This matches `pow_loop`: `pow_value >= pool_target` means too weak.
#[inline]
pub(crate) fn share_too_weak_for_pool(pow_value: &BigUint, pool_target: &BigUint) -> bool {
    pow_value >= pool_target
}

/// `true` when PoW hash is at or below the network target (lower hash is better).
#[inline]
pub(crate) fn meets_network_target_biguint(pow_value: &BigUint, network_target: &BigUint) -> bool {
    pow_value <= network_target
}

/// Whether the job-ID workaround loop has exhausted previous jobs (IceRiver / Bitmain quirk).
///
/// Mirrors the condition in `pow_loop`: stop trying older jobs when we wrapped or hit job `1`.
#[inline]
pub(crate) fn job_id_workaround_exhausted(current_job_id: u64, submitted_job_id: u64, max_jobs: u64) -> bool {
    if max_jobs == 0 {
        return true;
    }
    current_job_id == 1 || (current_job_id % max_jobs == ((submitted_job_id % max_jobs) + 1) % max_jobs)
}

/// Previous job id to validate, if any (`current_job_id > 1`).
#[inline]
pub(crate) fn previous_job_id(current_job_id: u64) -> Option<u64> {
    if current_job_id > 1 { Some(current_job_id - 1) } else { None }
}

/// Pool target for comparisons; `None` or zero target is treated like “no pool target” for logging only.
#[inline]
pub(crate) fn pool_target_or_zero(pool_target: Option<BigUint>) -> BigUint {
    pool_target.unwrap_or_else(BigUint::zero)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_too_weak_matches_ge_semantics() {
        let t = BigUint::from(100u32);
        assert!(!share_too_weak_for_pool(&BigUint::from(50u32), &t));
        assert!(share_too_weak_for_pool(&BigUint::from(100u32), &t));
        assert!(share_too_weak_for_pool(&BigUint::from(150u32), &t));
    }

    #[test]
    fn meets_network_target() {
        let nt = BigUint::from(200u32);
        assert!(meets_network_target_biguint(&BigUint::from(200u32), &nt));
        assert!(meets_network_target_biguint(&BigUint::from(10u32), &nt));
        assert!(!meets_network_target_biguint(&BigUint::from(201u32), &nt));
    }

    #[test]
    fn job_id_workaround_exhausted_examples() {
        let max = 300u64;
        // `current % max == ((submitted % max) + 1) % max` → exhausted (wrapped / next slot)
        assert!(job_id_workaround_exhausted(6, 5, max));
        // Job 1 always stops
        assert!(job_id_workaround_exhausted(1, 10, max));
        assert!(!job_id_workaround_exhausted(5, 5, max));
        assert!(!job_id_workaround_exhausted(10, 5, max));
    }

    #[test]
    fn previous_job_id_underflow_safe() {
        assert_eq!(previous_job_id(0), None);
        assert_eq!(previous_job_id(1), None);
        assert_eq!(previous_job_id(2), Some(1));
    }

    #[test]
    fn zero_max_jobs_exhausted() {
        assert!(job_id_workaround_exhausted(5, 5, 0));
    }
}
