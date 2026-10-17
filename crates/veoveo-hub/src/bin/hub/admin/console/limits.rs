//! Connection limits shared by the Console audit stream and export.
use std::{collections::BTreeMap, sync::Arc};

use parking_lot::Mutex;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const MAX_CONCURRENT_STREAMS: usize = 16;
const MAX_STREAMS_PER_PRINCIPAL: usize = 3;

#[derive(Clone)]
pub(crate) struct ConsoleStreamLimits {
    global: Arc<Semaphore>,
    per_principal: Arc<Mutex<BTreeMap<String, usize>>>,
}

pub(crate) struct StreamSlot {
    _global: OwnedSemaphorePermit,
    per_principal: Arc<Mutex<BTreeMap<String, usize>>>,
    principal: String,
}

impl Drop for StreamSlot {
    fn drop(&mut self) {
        let mut per_principal = self.per_principal.lock();
        if let Some(count) = per_principal.get_mut(&self.principal) {
            *count -= 1;
            if *count == 0 {
                per_principal.remove(&self.principal);
            }
        }
    }
}

impl ConsoleStreamLimits {
    pub(crate) fn new() -> Self {
        Self {
            global: Arc::new(Semaphore::new(MAX_CONCURRENT_STREAMS)),
            per_principal: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub(crate) fn acquire(&self, principal: &str) -> Option<StreamSlot> {
        let global = self.global.clone().try_acquire_owned().ok()?;
        let mut per_principal = self.per_principal.lock();
        let count = per_principal.entry(principal.to_owned()).or_default();
        if *count >= MAX_STREAMS_PER_PRINCIPAL {
            return None;
        }
        *count += 1;
        Some(StreamSlot {
            _global: global,
            per_principal: self.per_principal.clone(),
            principal: principal.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principals_are_limited_independently_and_slots_release_on_drop() {
        let limits = ConsoleStreamLimits::new();
        let held = (0..MAX_STREAMS_PER_PRINCIPAL)
            .map(|_| limits.acquire("alice").unwrap())
            .collect::<Vec<_>>();
        assert!(limits.acquire("alice").is_none());
        assert!(limits.acquire("bob").is_some());
        drop(held);
        assert!(limits.acquire("alice").is_some());
    }
}
