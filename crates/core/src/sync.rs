//! Moving changes between a device's store and its hub. Merging lives in the store (a device's
//! [`Store::finish_sync`], a hub's [`Store::serve_sync`]); this module runs the client loop over any [`Transport`]:
//! [`Http`] for Furiten Cloud (`POST /v1/sync`, `POST /v1/accounts`), `crate::hub::HubLink` for a self-hosted hub
//! over Iroh, or a hub's store in the same process.

use std::sync::Mutex;
use std::time::Duration;

use crate::protocol::{Account, SyncRequest, SyncResponse, SyncResult};
use crate::store::SyncConfig;
use crate::{Error, Result, Store};

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

/// Talks to a Furiten server over HTTPS.
pub struct Http {
    agent: ureq::Agent,
    server: String,
    token: Option<String>,
}

impl Http {
    pub fn new(server: &str, token: Option<&str>) -> Http {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        Http { agent, server: server.trim_end_matches('/').to_string(), token: token.map(str::to_string) }
    }

    pub fn from_config(config: &SyncConfig) -> Http {
        Http::new(&config.server, Some(&config.token))
    }

    /// `POST /v1/accounts`: a new interim account.
    pub fn create_account(&self) -> Result<Account> {
        self.post("/v1/accounts", "{}")
    }

    fn post<T: serde::de::DeserializeOwned>(&self, path: &str, body: &str) -> Result<T> {
        let mut request = self.agent.post(format!("{}{path}", self.server)).content_type("application/json");
        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let mut response = request.send(body).map_err(|e| Error::new("network", e.to_string()))?;
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .with_config()
            .limit(512 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| Error::new("network", e.to_string()))?;
        if (200..300).contains(&status) {
            return serde_json::from_str(&text).map_err(|e| Error::new("bad_response", e.to_string()));
        }
        #[derive(serde::Deserialize)]
        struct Body {
            error: Error,
        }
        let error = serde_json::from_str::<Body>(&text).ok().map(|b| b.error);
        Err(match (status, error) {
            // Normalised so callers can tell a bad token from everything else.
            (401, error) => Error::new(
                "unauthorized",
                error.map_or_else(|| "the server rejected the token".to_string(), |e| e.message),
            ),
            (_, Some(error)) => error,
            (_, None) => Error::new("server", format!("server answered {status}")),
        })
    }
}

impl Transport for Http {
    fn sync(&self, request: &SyncRequest) -> Result<SyncResponse> {
        self.post("/v1/sync", &serde_json::to_string(request)?)
    }
}

/// The user id inside a token (`rc_<user_id>_<secret>`).
pub fn token_user_id(token: &str) -> Result<String> {
    let mut parts = token.trim().splitn(3, '_');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("rc"), Some(user), Some(secret)) if !user.is_empty() && !secret.is_empty() => Ok(user.to_string()),
        _ => Err(Error::invalid("a token looks like rc_<user_id>_<secret>")),
    }
}

#[cfg(test)]
pub(crate) mod memory;
#[cfg(test)]
mod tests;
