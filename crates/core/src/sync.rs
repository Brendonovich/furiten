//! Moving changes between a device's store and its hub. Merging lives in the store (a device's
//! [`Store::finish_sync`], a hub's [`Store::serve_sync`]); this module runs the client loop over any [`Transport`]:
//! `crate::hub::HubLink` for a self-hosted hub over Iroh, or a hub's store in the same process.

use std::sync::Mutex;

use crate::protocol::{SyncRequest, SyncResponse, SyncResult};
use crate::{Result, Store};

/// Something that answers sync requests the way the contract's Sync section says.
pub trait Transport {
    fn sync(&self, request: &SyncRequest) -> Result<SyncResponse>;
}

/// A hub's store in the same process.
impl Transport for Mutex<Store> {
    fn sync(&self, request: &SyncRequest) -> Result<SyncResponse> {
        lock(self).serve_sync(request).map(|(response, _)| response)
    }
}

/// One sync round. The store is locked only while reading the outbox and while applying the response, never
/// during the request, so local writes carry on meanwhile.
pub fn sync_once(store: &Mutex<Store>, transport: &dyn Transport) -> Result<(SyncResult, u64)> {
    let (outbox, cursor) = {
        let store = lock(store);
        (store.pending()?, store.cursor()?)
    };
    let pushed = outbox.changes.len() as u64;
    let response = transport.sync(&SyncRequest { cursor, changes: outbox.changes })?;
    let applied = lock(store).finish_sync(outbox.mark, &response)?;
    Ok((SyncResult { pushed, pulled: response.changes.len() as u64 }, applied))
}

/// Locks the store, recovering from a panicked holder: every store write is a transaction, so a panic mid-way
/// leaves nothing half-done.
pub fn lock(store: &Mutex<Store>) -> std::sync::MutexGuard<'_, Store> {
    store.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
pub(crate) mod memory;
#[cfg(test)]
mod tests;
