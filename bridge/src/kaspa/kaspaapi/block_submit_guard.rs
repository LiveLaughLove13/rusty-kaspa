use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

pub(super) struct BlockSubmitGuard {
    ttl: Duration,
    max_entries: usize,
    entries: HashMap<String, Instant>,
    order: VecDeque<String>,
}

impl BlockSubmitGuard {
    fn new(ttl: Duration, max_entries: usize) -> Self {
        Self { ttl, max_entries, entries: HashMap::new(), order: VecDeque::new() }
    }

    fn prune(&mut self, now: Instant) {
        while let Some(front) = self.order.front() {
            let remove = match self.entries.get(front) {
                Some(ts) => now.duration_since(*ts) > self.ttl,
                None => true,
            };
            if remove {
                if let Some(key) = self.order.pop_front() {
                    self.entries.remove(&key);
                }
            } else {
                break;
            }
        }

        while self.entries.len() > self.max_entries {
            if let Some(key) = self.order.pop_front() {
                self.entries.remove(&key);
            } else {
                break;
            }
        }
    }

    pub(super) fn try_mark(&mut self, hash: &str, now: Instant) -> bool {
        self.prune(now);
        if self.entries.contains_key(hash) {
            return false;
        }
        self.entries.insert(hash.to_string(), now);
        self.order.push_back(hash.to_string());
        true
    }

    pub(super) fn remove(&mut self, hash: &str, now: Instant) {
        self.prune(now);
        self.entries.remove(hash);
    }
}

pub(super) static BLOCK_SUBMIT_GUARD: Lazy<Mutex<BlockSubmitGuard>> =
    Lazy::new(|| Mutex::new(BlockSubmitGuard::new(Duration::from_secs(600), 50_000)));
