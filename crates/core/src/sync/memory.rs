//! An in-memory sync server with the contract's server-side merge, for offline tests.

use std::collections::HashMap;
use std::sync::Mutex;

use super::Transport;
use crate::Result;
use crate::model::Change;
use crate::protocol::{SyncRequest, SyncResponse};

#[derive(Default)]
pub struct MemoryServer {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    seq: i64,
    /// Latest accepted change per entity, with its seq.
    entities: HashMap<(&'static str, String), (i64, Change)>,
    requests: usize,
}

impl MemoryServer {
    pub fn requests(&self) -> usize {
        self.state.lock().unwrap().requests
    }

    pub fn len(&self) -> usize {
        self.state.lock().unwrap().entities.len()
    }
}

fn updated_at(change: &Change) -> Option<i64> {
    match change {
        Change::Deck(d) => Some(d.updated_at),
        Change::Card(c) => Some(c.updated_at),
        Change::Review(_) => None,
    }
}

impl Transport for MemoryServer {
    fn sync(&self, request: &SyncRequest) -> Result<SyncResponse> {
        let mut state = self.state.lock().unwrap();
        state.requests += 1;
        // Echo back everything stored since the cursor, except versions this request itself sent.
        let mut sent = HashMap::new();
        for change in &request.changes {
            let (kind, id) = change.key();
            let key = (kind, id.to_string());
            sent.insert(key.clone(), updated_at(change));
            let accept = match (state.entities.get(&key), updated_at(change)) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some((_, stored)), Some(at)) => at > updated_at(stored).unwrap_or(i64::MIN),
            };
            if accept {
                state.seq += 1;
                let seq = state.seq;
                state.entities.insert(key, (seq, change.clone()));
            }
        }
        let cursor = request.cursor.unwrap_or(0);
        let mut changes: Vec<(i64, Change)> = state
            .entities
            .iter()
            .filter(|(key, (seq, stored))| *seq > cursor && sent.get(*key) != Some(&updated_at(stored)))
            .map(|(_, (seq, change))| (*seq, change.clone()))
            .collect();
        changes.sort_by_key(|(seq, _)| *seq);
        Ok(SyncResponse { cursor: state.seq, changes: changes.into_iter().map(|(_, c)| c).collect() })
    }
}
