//! A device's side of a hub: pairing, and a [`HubLink`] that keeps one connection open, syncs over it and hears
//! the hub's `changed` events. Iroh is async; the core is threads, so a link owns a small Tokio runtime and offers
//! a blocking API.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointAddr, SecretKey};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use super::wire::{self, MAX_SMALL_FRAME, MAX_SYNC_FRAME, close, closed_error};
use super::{Net, PAIR_ALPN, SYNC_ALPN, Ticket, endpoint_addr};
use crate::protocol::{SyncRequest, SyncResponse};
use crate::store::HubAddr;
use crate::sync::Transport;
use crate::{Error, Result};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const SYNC_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// A live link to the device's hub. Dropping it closes the connection.
pub struct HubLink {
    runtime: Option<Runtime>,
    inner: Arc<Inner>,
    live: JoinHandle<()>,
}

struct Inner {
    endpoint: Endpoint,
    hub: EndpointAddr,
    conn: tokio::sync::Mutex<Option<Connection>>,
    connected: AtomicBool,
}

impl HubLink {
    /// Binds an endpoint with the device's key and starts keeping a connection to the hub. `on_changed` runs, on
    /// the link's own threads, each time the connection is (re)established and each time the hub reports another
    /// device's write: the caller should sync soon. It must not block.
    pub fn start(
        key: SecretKey,
        net: &Net,
        hub: &HubAddr,
        on_changed: impl Fn() + Send + Sync + 'static,
    ) -> Result<HubLink> {
        let runtime = runtime()?;
        let hub = endpoint_addr(hub)?;
        let endpoint = runtime.block_on(bind(net, key))?;
        let inner =
            Arc::new(Inner { endpoint, hub, conn: tokio::sync::Mutex::new(None), connected: AtomicBool::new(false) });
        let live = runtime.spawn(inner.clone().live(on_changed));
        Ok(HubLink { runtime: Some(runtime), inner, live })
    }

    /// Pairs with the hub in `ticket` (this link's or another) using the link's endpoint. Returns the library id.
    pub fn pair(&self, ticket: &Ticket, name: &str) -> Result<String> {
        self.runtime().block_on(pair_with(&self.inner.endpoint, ticket, name))
    }

    /// Whether the link holds an open connection to the hub right now.
    pub fn is_connected(&self) -> bool {
        self.inner.connected.load(Ordering::SeqCst)
    }

    fn runtime(&self) -> &Runtime {
        self.runtime.as_ref().expect("the runtime lives until drop")
    }
}

impl Transport for HubLink {
    fn sync(&self, request: &SyncRequest) -> Result<SyncResponse> {
        let inner = self.inner.clone();
        let request = request.clone();
        self.runtime().block_on(async move {
            tokio::time::timeout(SYNC_TIMEOUT, inner.sync(&request))
                .await
                .unwrap_or_else(|_| Err(Error::new("network", "the hub didn't answer in time")))
        })
    }
}

impl Drop for HubLink {
    fn drop(&mut self) {
        self.live.abort();
        if let Some(runtime) = self.runtime.take() {
            let endpoint = self.inner.endpoint.clone();
            runtime.block_on(async move {
                let _ = tokio::time::timeout(Duration::from_secs(2), endpoint.close()).await;
            });
            runtime.shutdown_background();
        }
    }
}

impl Inner {
    /// The open connection, or a new one.
    async fn connection(&self) -> Result<Connection> {
        let mut slot = self.conn.lock().await;
        if let Some(conn) = slot.as_ref().filter(|c| c.close_reason().is_none()) {
            return Ok(conn.clone());
        }
        let conn = connect(&self.endpoint, self.hub.clone(), SYNC_ALPN).await?;
        *slot = Some(conn.clone());
        Ok(conn)
    }

    async fn sync(&self, request: &SyncRequest) -> Result<SyncResponse> {
        // A connection that looked open may have died (the hub restarted, the network changed): one retry on a
        // fresh one.
        for attempt in 0..2 {
            let conn = self.connection().await?;
            match exchange(&conn, request, MAX_SYNC_FRAME).await {
                Ok(response) => return response,
                Err(e) => {
                    let error = conn.close_reason().map_or(e, |reason| closed_error(&reason));
                    if attempt == 1 || error.code != "network" {
                        return Err(error);
                    }
                    conn.close(close::DONE, b"retrying");
                }
            }
        }
        unreachable!()
    }

    /// Keeps a connection open and listens for events, reconnecting with backoff.
    async fn live(self: Arc<Self>, on_changed: impl Fn() + Send + Sync + 'static) {
        let mut backoff = Duration::from_secs(1);
        loop {
            if let Ok(conn) = self.connection().await {
                self.connected.store(true, Ordering::SeqCst);
                backoff = Duration::from_secs(1);
                on_changed();
                while let Ok(mut events) = conn.accept_uni().await {
                    while let Ok(Some(frame)) = wire::read_frame(&mut events, MAX_SMALL_FRAME).await {
                        if serde_json::from_slice::<wire::Event>(&frame).is_ok_and(|e| e.event == "changed") {
                            on_changed();
                        }
                    }
                }
                self.connected.store(false, Ordering::SeqCst);
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }
}

/// Sends one request on a new bidirectional stream and reads the answer. The outer `Result` is the transport,
/// the inner one the hub's answer.
async fn exchange<T: serde::de::DeserializeOwned>(
    conn: &Connection,
    request: &impl serde::Serialize,
    max: usize,
) -> Result<Result<T>> {
    let (mut send, mut recv) = conn.open_bi().await.map_err(|e| Error::new("network", e.to_string()))?;
    wire::write_frame(&mut send, request).await?;
    send.finish().map_err(|e| Error::new("network", e.to_string()))?;
    match wire::read_reply(&mut recv, max).await {
        Err(e) if e.code == "network" => Err(e),
        reply => Ok(reply),
    }
}

/// Pairs with the hub in `ticket` as `name`, on a short-lived endpoint with the device's key. Returns the hub's
/// library id. Use [`HubLink::pair`] instead while a link with the same key is running.
pub fn pair(key: SecretKey, net: &Net, ticket: &Ticket, name: &str) -> Result<String> {
    let runtime = runtime()?;
    let result = runtime.block_on(async {
        let endpoint = bind(net, key).await?;
        let result = pair_with(&endpoint, ticket, name).await;
        let _ = tokio::time::timeout(Duration::from_secs(2), endpoint.close()).await;
        result
    });
    runtime.shutdown_background();
    result
}

async fn pair_with(endpoint: &Endpoint, ticket: &Ticket, name: &str) -> Result<String> {
    let conn = connect(endpoint, endpoint_addr(&ticket.addr)?, PAIR_ALPN).await?;
    let request = wire::PairRequest { secret: ticket.secret_hex(), name: name.to_string() };
    let reply = exchange::<wire::Paired>(&conn, &request, MAX_SMALL_FRAME).await;
    conn.close(close::DONE, b"paired");
    let reply = reply.map_err(|e| conn.close_reason().map_or(e, |r| closed_error(&r)))?;
    reply.map(|paired| paired.library_id)
}

async fn bind(net: &Net, key: SecretKey) -> Result<Endpoint> {
    net.builder(key)?.bind().await.map_err(|e| Error::new("network", format!("couldn't start Iroh: {e}")))
}

async fn connect(endpoint: &Endpoint, addr: EndpointAddr, alpn: &[u8]) -> Result<Connection> {
    match tokio::time::timeout(CONNECT_TIMEOUT, endpoint.connect(addr, alpn)).await {
        Ok(Ok(conn)) => Ok(conn),
        Ok(Err(e)) => Err(Error::new("network", format!("couldn't reach the hub: {e}"))),
        Err(_) => Err(Error::new("network", "couldn't reach the hub in time")),
    }
}

fn runtime() -> Result<Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("furiten-iroh")
        .enable_all()
        .build()?)
}
