//! The hub's Iroh side: the gate hook (device allowlist), the accept loop with its limits, `recall/sync/1` and
//! `recall/pair/1`.

use std::sync::Arc;
use std::time::Duration;

use iroh::endpoint::{
    AfterHandshakeOutcome, BeforeConnectOutcome, Connection, EndpointHooks, RecvStream, SendStream, Side,
};
use iroh::{Endpoint, EndpointAddr, EndpointId};
use recall_core::hub::{self as wire, MAX_SMALL_FRAME, MAX_SYNC_FRAME, PAIR_ALPN, SYNC_ALPN, close};
use recall_core::protocol::{SyncRequest, SyncResponse};
use recall_core::{Error, Result, now_ms};
use tokio::sync::{Semaphore, broadcast};
use tokio::task::JoinSet;

use crate::Shared;

/// Concurrent sync requests on one connection.
pub(crate) const MAX_STREAMS_PER_CONN: u32 = 4;
/// Connections still in their handshake. Unidentified peers can't use up the sync slots.
const MAX_HANDSHAKES: usize = 64;
/// Admitted sync connections across all devices, and per device.
const MAX_SYNC_CONNS: usize = 64;
const MAX_CONNS_PER_DEVICE: usize = 4;
/// Pairing connections at once.
const MAX_PAIRINGS: usize = 4;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const PAIR_TIMEOUT: Duration = Duration::from_secs(30);
const SYNC_TIMEOUT: Duration = Duration::from_secs(300);
const EVENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub(crate) struct GateHook(pub(crate) Arc<Shared>);

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared").field("library_id", &self.library_id).finish_non_exhaustive()
    }
}

impl EndpointHooks for GateHook {
    /// The hub never dials out.
    async fn before_connect(&self, _remote_addr: &EndpointAddr, _alpn: &[u8]) -> BeforeConnectOutcome {
        BeforeConnectOutcome::Reject
    }

    /// Sync connections only from paired devices, registered under the gate's lock so a removal closes them.
    fn after_handshake<'a>(&'a self, conn: &'a Connection) -> impl Future<Output = AfterHandshakeOutcome> + Send + 'a {
        let outcome = match (conn.side(), conn.alpn()) {
            (Side::Server, SYNC_ALPN) => {
                let peer = conn.remote_id();
                let mut gate = self.0.gate();
                if gate.devices.contains(&peer.to_string()) {
                    let list = gate.conns.entry(peer).or_default();
                    list.retain(|c| c.upgrade().is_some_and(|c| c.close_reason().is_none()));
                    list.push(conn.weak_handle());
                    AfterHandshakeOutcome::Accept
                } else {
                    reject(close::NOT_PAIRED, b"not a paired device")
                }
            }
            (Side::Server, PAIR_ALPN) => AfterHandshakeOutcome::Accept,
            _ => reject(close::PROTOCOL, b"unknown protocol"),
        };
        async move { outcome }
    }
}

fn reject(error_code: iroh::endpoint::VarInt, reason: &[u8]) -> AfterHandshakeOutcome {
    AfterHandshakeOutcome::Reject { error_code, reason: reason.to_vec() }
}

pub(crate) async fn accept_loop(endpoint: Endpoint, shared: Arc<Shared>) {
    let handshakes = Arc::new(Semaphore::new(MAX_HANDSHAKES));
    let syncs = Arc::new(Semaphore::new(MAX_SYNC_CONNS));
    let pairings = Arc::new(Semaphore::new(MAX_PAIRINGS));
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break };
                // Make the sender prove it receives at its address (QUIC Retry) before it holds a handshake slot,
                // so spoofed packets can't fill them.
                if !incoming.remote_addr_validated() {
                    if let Err(e) = incoming.retry() {
                        e.into_incoming().refuse();
                    }
                    continue;
                }
                let Ok(handshake) = handshakes.clone().try_acquire_owned() else {
                    incoming.refuse();
                    continue;
                };
                let (shared, syncs, pairings) = (shared.clone(), syncs.clone(), pairings.clone());
                tasks.spawn(async move {
                    // The gate hook runs inside this await: a device that isn't paired never gets past it.
                    let Ok(Ok(conn)) = tokio::time::timeout(HANDSHAKE_TIMEOUT, incoming).await else { return };
                    drop(handshake);
                    match conn.alpn() {
                        SYNC_ALPN => {
                            let Ok(_permit) = syncs.try_acquire_owned() else {
                                conn.close(close::BUSY, b"too many connections");
                                return;
                            };
                            if shared.gate().conns.get(&conn.remote_id()).map_or(0, Vec::len) > MAX_CONNS_PER_DEVICE {
                                conn.close(close::BUSY, b"too many connections from this device");
                                return;
                            }
                            serve_device(conn, shared).await;
                        }
                        PAIR_ALPN => {
                            let Ok(_permit) = pairings.try_acquire_owned() else {
                                conn.close(close::BUSY, b"too many pairings");
                                return;
                            };
                            let _ = tokio::time::timeout(PAIR_TIMEOUT, serve_pairing(&conn, &shared)).await;
                            conn.close(close::DONE, b"");
                        }
                        _ => conn.close(close::PROTOCOL, b"unknown protocol"),
                    }
                });
            }
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
        }
    }
}

/// One device's sync connection: a request per bidirectional stream, and `changed` events on a unidirectional
/// stream opened on the first one.
async fn serve_device(conn: Connection, shared: Arc<Shared>) {
    let peer = conn.remote_id();
    let mut events = shared.events.subscribe();
    let mut out: Option<SendStream> = None;
    let mut requests = JoinSet::new();
    loop {
        tokio::select! {
            stream = conn.accept_bi() => {
                let Ok((send, recv)) = stream else { break };
                if !shared.is_member(&peer) {
                    conn.close(close::NOT_PAIRED, b"not a paired device");
                    break;
                }
                requests.spawn(serve_request(shared.clone(), peer, send, recv));
            }
            event = events.recv() => {
                let cursor = match event {
                    Ok((writer, _)) if writer == peer => continue,
                    Ok((_, cursor)) => cursor,
                    // Missed some: one event with the latest cursor covers them.
                    Err(broadcast::error::RecvError::Lagged(_)) => shared.store().seq().unwrap_or(0),
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                let sent = tokio::time::timeout(EVENT_TIMEOUT, send_event(&conn, &mut out, cursor)).await;
                if !matches!(sent, Ok(Ok(()))) {
                    // A device that doesn't read its events gets a fresh stream next time.
                    if let Some(mut stream) = out.take() {
                        let _ = stream.reset(close::DONE);
                    }
                }
            }
            Some(_) = requests.join_next(), if !requests.is_empty() => {}
        }
    }
}

async fn send_event(conn: &Connection, out: &mut Option<SendStream>, cursor: i64) -> Result<()> {
    if out.is_none() {
        *out = Some(conn.open_uni().await.map_err(|e| Error::new("network", e.to_string()))?);
    }
    let stream = out.as_mut().expect("just opened");
    wire::write_frame(stream, &wire::Event::changed(cursor)).await
}

async fn serve_request(shared: Arc<Shared>, peer: EndpointId, mut send: SendStream, mut recv: RecvStream) {
    let result = tokio::time::timeout(SYNC_TIMEOUT, handle_sync(&shared, peer, &mut recv))
        .await
        .unwrap_or_else(|_| Err(Error::new("timeout", "the sync took too long")));
    let _ = match &result {
        Ok(response) => wire::write_frame(&mut send, response).await,
        Err(error) => wire::write_frame(&mut send, &wire::Failure { error }).await,
    };
    let _ = send.finish();
    // Keep the stream until the device has the whole answer.
    let _ = tokio::time::timeout(SYNC_TIMEOUT, send.stopped()).await;
}

async fn handle_sync(shared: &Arc<Shared>, peer: EndpointId, recv: &mut RecvStream) -> Result<SyncResponse> {
    let request: SyncRequest = wire::read_message(recv, MAX_SYNC_FRAME).await?;
    let store = shared.clone();
    let (response, accepted) =
        tokio::task::spawn_blocking(move || store.store().serve_sync(&request)).await.map_err(Error::internal)??;
    shared.last_sync.lock().unwrap_or_else(|e| e.into_inner()).insert(peer, now_ms());
    if accepted > 0 {
        let _ = shared.events.send((peer, response.cursor));
    }
    Ok(response)
}

async fn serve_pairing(conn: &Connection, shared: &Shared) -> Result<()> {
    let (mut send, mut recv) = conn.accept_bi().await.map_err(|e| Error::new("network", e.to_string()))?;
    let reply = match wire::read_message::<_, wire::PairRequest>(&mut recv, MAX_SMALL_FRAME).await {
        Ok(request) => shared.redeem(&request, conn.remote_id()),
        Err(e) => Err(e),
    };
    match &reply {
        Ok(paired) => wire::write_frame(&mut send, paired).await?,
        Err(error) => wire::write_frame(&mut send, &wire::Failure { error }).await?,
    }
    let _ = send.finish();
    // The device closes the connection once it has read the answer.
    conn.closed().await;
    Ok(())
}
