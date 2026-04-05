use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DuplicateSubmitOutcome {
    InFlight,
    Accepted,
    Stale,
    LowDiff,
    Bad,
}

struct DuplicateSubmitEntry {
    ts: Instant,
    outcome: DuplicateSubmitOutcome,
}

pub(crate) struct DuplicateSubmitGuard {
    ttl: Duration,
    max_entries: usize,
    entries: HashMap<String, DuplicateSubmitEntry>,
    order: VecDeque<String>,
}

impl DuplicateSubmitGuard {
    pub(crate) fn new(ttl: Duration, max_entries: usize) -> Self {
        Self { ttl, max_entries, entries: HashMap::new(), order: VecDeque::new() }
    }

    fn prune(&mut self, now: Instant) {
        while let Some(front) = self.order.front() {
            let remove = match self.entries.get(front) {
                Some(e) => now.duration_since(e.ts) > self.ttl,
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

    pub(crate) fn get(&mut self, key: &str, now: Instant) -> Option<DuplicateSubmitOutcome> {
        self.prune(now);
        self.entries.get(key).map(|e| e.outcome)
    }

    pub(crate) fn insert_inflight(&mut self, key: String, now: Instant) {
        self.prune(now);
        if self.entries.contains_key(&key) {
            return;
        }
        self.entries.insert(key.clone(), DuplicateSubmitEntry { ts: now, outcome: DuplicateSubmitOutcome::InFlight });
        self.order.push_back(key);
    }

    pub(crate) fn set_outcome(&mut self, key: &str, now: Instant, outcome: DuplicateSubmitOutcome) {
        self.prune(now);
        if let Some(e) = self.entries.get_mut(key) {
            e.ts = now;
            e.outcome = outcome;
        }
    }
}
