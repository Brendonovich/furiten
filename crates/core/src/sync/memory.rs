//! A hub's store in memory, counting requests, for offline tests.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::Transport;
use crate::protocol::{SyncRequest, SyncResponse};
use crate::{Result, Store};

pub struct MemoryServer {
    store: Mutex<Store>,
    requests: AtomicUsize,
}

impl Default for MemoryServer {
    fn default() -> MemoryServer {
        MemoryServer { store: Mutex::new(Store::open_hub_in_memory().unwrap()), requests: AtomicUsize::new(0) }
    }
}

impl MemoryServer {
    pub fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    /// How many entities the server stores.
    pub fn len(&self) -> usize {
        self.store.sync(&SyncRequest::default()).unwrap().changes.len()
    }
}

impl Transport for MemoryServer {
    fn sync(&self, request: &SyncRequest) -> Result<SyncResponse> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        self.store.sync(request)
    }
}
