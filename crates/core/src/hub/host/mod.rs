//! Being a hub: serving `recall/sync/1` and `recall/pair/1` to paired devices from a store that serves sync. A
//! `recall-hub` runs one over its own store; the desktop app runs one over its own library while it hosts phones
//! (see "Hosting" in docs/contract.md).

mod devices;
mod node;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use iroh::endpoint::{QuicTransportConfig, VarInt, WeakConnectionHandle};
use iroh::{Endpoint, EndpointId, SecretKey};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

pub use devices::{DEVICES, Devices};

pub use crate::protocol::{Device, DeviceInfo, PairTicket};

use super::{Net, PAIR_ALPN, PairRequest, Paired, SYNC_ALPN, Ticket, hub_addr};
use crate::store::HubAddr;
use crate::sync::lock;
use crate::{Error, Result, Store, now_ms};

/// How long a pairing ticket stays valid.
pub const TICKET_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug)]
pub struct HostConfig {
    pub net: Net,
    pub ticket_ttl: Duration,
    /// Where `devices.json` lives.
    pub dir: PathBuf,
}

impl HostConfig {
    pub fn new(dir: impl Into<PathBuf>, net: Net) -> HostConfig {
        HostConfig { net, ticket_ttl: TICKET_TTL, dir: dir.into() }
    }
}

/// A running host. Dropping it without [`Host::shutdown`] stops accepting connections.
pub struct Host {
    shared: Arc<Shared>,
    endpoint: Endpoint,
    accept: JoinHandle<()>,
}

pub(crate) struct Shared {
    config: HostConfig,
    library_id: String,
    store: Arc<Mutex<Store>>,
    gate: Mutex<Gate>,
    tickets: Mutex<Vec<Pending>>,
    /// `(writer, cursor)` after each write: a device's sync that stored something, or a local one.
    events: broadcast::Sender<(EndpointId, i64)>,
    last_sync: Mutex<HashMap<EndpointId, i64>>,
    /// Told after a device's sync stored something, and after a device paired.
    on_change: Box<dyn Fn() + Send + Sync>,
}

/// The device list and the sync connections it admitted, under one lock: a connection admitted concurrently with a
/// removal is either refused on admission or closed by the removal.
pub(crate) struct Gate {
    devices: Devices,
    conns: HashMap<EndpointId, Vec<WeakConnectionHandle>>,
}

struct Pending {
    secret: [u8; 16],
    expires_at: Instant,
    used: bool,
}

impl Host {
    /// Starts serving `store` (which must serve sync: a hub's, or a device's that hosts) with `key` as the
    /// endpoint's identity. `on_change` runs on the host's threads after a device's sync stored something and
    /// after a device paired; it must not block. Call from inside a Tokio runtime.
    pub async fn start(
        key: SecretKey,
        config: HostConfig,
        store: Arc<Mutex<Store>>,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Result<Host> {
        let library_id = {
            let store = lock(&store);
            if !store.serves_sync() {
                return Err(Error::new("wrong_store", "only a hub's store, or a hosting device's, serves sync"));
            }
            store.device_id()?
        };
        let devices = Devices::load(&config.dir)?;
        let shared = Arc::new(Shared {
            config: config.clone(),
            library_id,
            store,
            gate: Mutex::new(Gate { devices, conns: HashMap::new() }),
            tickets: Mutex::default(),
            events: broadcast::channel(256).0,
            last_sync: Mutex::default(),
            on_change: Box::new(on_change),
        });
        let transport = QuicTransportConfig::builder()
            .max_concurrent_bidi_streams(VarInt::from_u32(node::MAX_STREAMS_PER_CONN))
            .max_concurrent_uni_streams(VarInt::from_u32(0))
            .build();
        let endpoint = config
            .net
            .builder(key)?
            .alpns(vec![SYNC_ALPN.to_vec(), PAIR_ALPN.to_vec()])
            .transport_config(transport)
            .hooks(node::GateHook(shared.clone()))
            .bind()
            .await
            .map_err(|e| Error::new("network", format!("couldn't start Iroh: {e}")))?;
        let accept = tokio::spawn(node::accept_loop(endpoint.clone(), shared.clone()));
        Ok(Host { shared, endpoint, accept })
    }

    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    pub fn library_id(&self) -> &str {
        &self.shared.library_id
    }

    /// How devices reach the host right now.
    pub fn addr(&self) -> HubAddr {
        hub_addr(&self.endpoint.addr())
    }

    pub fn net(&self) -> &Net {
        &self.shared.config.net
    }

    /// A new one-time pairing ticket.
    pub async fn pair(&self) -> Result<PairTicket> {
        self.shared.pair(&self.endpoint).await
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.shared.devices()
    }

    /// How many devices hold a live connection.
    pub fn connected(&self) -> usize {
        self.shared.gate().connected().len()
    }

    /// Removes a device (by id, id prefix or name) and closes its connections.
    pub fn remove_device(&self, query: &str) -> Result<Device> {
        self.shared.remove_device(query)
    }

    /// Something wrote to the store other than a device's sync: every connected device should pull.
    pub fn changed(&self, cursor: i64) {
        let _ = self.shared.events.send((self.endpoint.id(), cursor));
    }

    /// Stops serving and closes every connection.
    pub async fn shutdown(&self) {
        self.accept.abort();
        self.endpoint.close().await;
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.accept.abort();
    }
}

impl Shared {
    fn gate(&self) -> MutexGuard<'_, Gate> {
        self.gate.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        lock(&self.store)
    }

    pub(crate) fn is_member(&self, id: &EndpointId) -> bool {
        self.gate().devices.contains(&id.to_string())
    }

    async fn pair(&self, endpoint: &Endpoint) -> Result<PairTicket> {
        if self.config.net != Net::Local {
            // The ticket should carry the relay; it's known once the endpoint is online.
            let _ = tokio::time::timeout(Duration::from_secs(10), endpoint.online()).await;
        }
        let mut secret = [0u8; 16];
        getrandom::getrandom(&mut secret).map_err(Error::internal)?;
        let now = Instant::now();
        let mut tickets = self.tickets.lock().unwrap_or_else(|e| e.into_inner());
        // Spent and expired tickets are kept an hour so a late device hears why its ticket failed.
        tickets.retain(|t| t.expires_at + Duration::from_secs(3600) > now);
        tickets.push(Pending { secret, expires_at: now + self.config.ticket_ttl, used: false });
        let ticket = Ticket { addr: hub_addr(&endpoint.addr()), secret };
        Ok(PairTicket { ticket: ticket.to_string(), expires_at: now_ms() + self.config.ticket_ttl.as_millis() as i64 })
    }

    /// Spends a ticket's secret and adds the device.
    pub(crate) fn redeem(&self, request: &PairRequest, device: EndpointId) -> Result<Paired> {
        let name = request.name.trim();
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err(Error::invalid("a device name is 1 to 64 printable characters"));
        }
        let secret = Ticket::parse_secret(&request.secret);
        {
            let mut tickets = self.tickets.lock().unwrap_or_else(|e| e.into_inner());
            let ticket = secret.and_then(|s| tickets.iter_mut().find(|t| constant_time_eq(&t.secret, &s)));
            let Some(ticket) = ticket else {
                return Err(Error::new("bad_ticket", "that ticket isn't from this hub, or the hub restarted"));
            };
            if ticket.used {
                return Err(Error::new("ticket_used", "that ticket was already used; make another"));
            }
            if Instant::now() >= ticket.expires_at {
                return Err(Error::new("ticket_expired", "that ticket expired; make another"));
            }
            ticket.used = true;
        }
        let device = Device { id: device.to_string(), name: name.to_string(), added_at: now_ms() };
        self.gate().devices.add(device.clone())?;
        eprintln!("recall: paired {} ({})", device.name, device.id);
        (self.on_change)();
        Ok(Paired { library_id: self.library_id.clone() })
    }

    pub(crate) fn devices(&self) -> Vec<DeviceInfo> {
        let mut gate = self.gate();
        let connected = gate.connected();
        let last_sync = self.last_sync.lock().unwrap_or_else(|e| e.into_inner());
        gate.devices
            .list()
            .iter()
            .map(|d| DeviceInfo {
                device: d.clone(),
                connected: connected.iter().any(|c| c.to_string() == d.id),
                last_sync: last_sync.iter().find(|(id, _)| id.to_string() == d.id).map(|(_, at)| *at),
            })
            .collect()
    }

    pub(crate) fn remove_device(&self, query: &str) -> Result<Device> {
        let mut gate = self.gate();
        let device = gate.devices.find(query)?;
        gate.devices.remove(&device.id)?;
        let closed = gate.close(&device.id);
        drop(gate);
        eprintln!("recall: removed {} ({}), closed {closed} connection(s)", device.name, device.id);
        Ok(device)
    }
}

impl Gate {
    /// Devices with a live sync connection.
    fn connected(&mut self) -> Vec<EndpointId> {
        self.conns.retain(|_, list| {
            list.retain(|c| c.upgrade().is_some_and(|c| c.close_reason().is_none()));
            !list.is_empty()
        });
        self.conns.keys().copied().collect()
    }

    fn close(&mut self, id: &str) -> usize {
        let mut closed = 0;
        self.conns.retain(|peer, list| {
            if peer.to_string() != id {
                return true;
            }
            for conn in list.iter().filter_map(|c| c.upgrade()) {
                conn.close(super::close::NOT_PAIRED, b"device removed");
                closed += 1;
            }
            false
        });
        closed
    }
}

fn constant_time_eq(a: &[u8; 16], b: &[u8; 16]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
