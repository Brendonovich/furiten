//! `recall-hub`: one Recall library on hardware you own, synced to paired devices over Iroh. See
//! `docs/contract.md` ("Self-hosted hub") for the protocol and `README.md` for running one.

pub mod control;
pub mod data;
mod node;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use iroh::endpoint::{QuicTransportConfig, VarInt, WeakConnectionHandle};
use iroh::{Endpoint, EndpointId};
use recall_core::hub::{self as wire, Net, PAIR_ALPN, SYNC_ALPN, Ticket};
use recall_core::store::HubAddr;
use recall_core::{Error, Result, Store, now_ms};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use data::{Device, Devices};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How long a pairing ticket stays valid.
pub const TICKET_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug)]
pub struct Config {
    pub data: PathBuf,
    pub net: Net,
    pub ticket_ttl: Duration,
    /// Serve the control socket (`hub.sock`) that `recall-hub pair`, `status` and `devices` use.
    pub control: bool,
}

impl Config {
    pub fn new(data: &Path, net: Net) -> Config {
        Config { data: data.to_path_buf(), net, ticket_ttl: TICKET_TTL, control: true }
    }
}

/// A running hub. Dropping it without [`Hub::shutdown`] stops accepting connections.
pub struct Hub {
    shared: Arc<Shared>,
    endpoint: Endpoint,
    tasks: Vec<JoinHandle<()>>,
    _lock: std::fs::File,
}

pub(crate) struct Shared {
    config: Config,
    library_id: String,
    store: Mutex<Store>,
    gate: Mutex<Gate>,
    tickets: Mutex<Vec<Pending>>,
    /// `(writer, cursor)` after each sync that stored something.
    events: broadcast::Sender<(EndpointId, i64)>,
    last_sync: Mutex<HashMap<EndpointId, i64>>,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub version: String,
    pub data: String,
    pub running: bool,
    pub endpoint_id: String,
    pub library_id: String,
    /// `n0`, `local`, or the self-hosted relay's URL.
    pub net: String,
    /// How devices reach it right now; `None` when it isn't running.
    pub addr: Option<HubAddr>,
    pub cursor: i64,
    pub devices: usize,
    pub connected: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    #[serde(flatten)]
    pub device: Device,
    pub connected: bool,
    /// When it last synced since the hub started.
    pub last_sync: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairTicket {
    pub ticket: String,
    pub expires_at: i64,
}

impl Hub {
    /// Opens the data directory (taking its lock) and starts serving.
    pub async fn start(config: Config) -> Result<Hub> {
        let lock = data::lock(&config.data)?;
        let key = wire::load_or_create_key(&config.data.join(data::KEY))?;
        let store = Store::open_hub(&config.data.join(data::LIBRARY))?;
        let library_id = store.device_id()?;
        let devices = Devices::load(&config.data)?;
        let shared = Arc::new(Shared {
            config: config.clone(),
            library_id,
            store: Mutex::new(store),
            gate: Mutex::new(Gate { devices, conns: HashMap::new() }),
            tickets: Mutex::default(),
            events: broadcast::channel(256).0,
            last_sync: Mutex::default(),
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
        let mut tasks = vec![tokio::spawn(node::accept_loop(endpoint.clone(), shared.clone()))];
        if config.control {
            tasks.push(control::serve(&config.data, shared.clone(), endpoint.clone()).await?);
        }
        Ok(Hub { shared, endpoint, tasks, _lock: lock })
    }

    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    pub fn library_id(&self) -> &str {
        &self.shared.library_id
    }

    /// A new one-time pairing ticket.
    pub async fn pair(&self) -> Result<PairTicket> {
        self.shared.pair(&self.endpoint).await
    }

    pub fn status(&self) -> Result<Status> {
        self.shared.status(&self.endpoint)
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.shared.devices()
    }

    /// Removes a device (by id, id prefix or name) and closes its connections.
    pub fn remove_device(&self, query: &str) -> Result<Device> {
        self.shared.remove_device(query)
    }

    /// Stops serving and closes every connection.
    pub async fn shutdown(self) {
        for task in &self.tasks {
            task.abort();
        }
        self.endpoint.close().await;
        if self.shared.config.control {
            let _ = std::fs::remove_file(self.shared.config.data.join(data::SOCKET));
        }
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

impl Shared {
    fn gate(&self) -> MutexGuard<'_, Gate> {
        self.gate.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
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
        let ticket = Ticket { addr: wire::hub_addr(&endpoint.addr()), secret };
        Ok(PairTicket { ticket: ticket.to_string(), expires_at: now_ms() + self.config.ticket_ttl.as_millis() as i64 })
    }

    /// Spends a ticket's secret and adds the device.
    pub(crate) fn redeem(&self, request: &wire::PairRequest, device: EndpointId) -> Result<wire::Paired> {
        let name = request.name.trim();
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err(Error::invalid("a device name is 1 to 64 printable characters"));
        }
        let secret = recall_core::hub::Ticket::parse_secret(&request.secret);
        {
            let mut tickets = self.tickets.lock().unwrap_or_else(|e| e.into_inner());
            let ticket = secret.and_then(|s| tickets.iter_mut().find(|t| constant_time_eq(&t.secret, &s)));
            let Some(ticket) = ticket else {
                return Err(Error::new("bad_ticket", "that ticket isn't from this hub, or the hub restarted"));
            };
            if ticket.used {
                return Err(Error::new(
                    "ticket_used",
                    "that ticket was already used; make another with recall-hub pair",
                ));
            }
            if Instant::now() >= ticket.expires_at {
                return Err(Error::new("ticket_expired", "that ticket expired; make another with recall-hub pair"));
            }
            ticket.used = true;
        }
        let device = Device { id: device.to_string(), name: name.to_string(), added_at: now_ms() };
        self.gate().devices.add(device.clone())?;
        eprintln!("recall-hub: paired {} ({})", device.name, device.id);
        Ok(wire::Paired { library_id: self.library_id.clone() })
    }

    pub(crate) fn status(&self, endpoint: &Endpoint) -> Result<Status> {
        let (devices, connected) = {
            let mut gate = self.gate();
            (gate.devices.list().len(), gate.connected().len())
        };
        Ok(Status {
            version: VERSION.to_string(),
            data: self.config.data.display().to_string(),
            running: true,
            endpoint_id: endpoint.id().to_string(),
            library_id: self.library_id.clone(),
            net: net_name(&self.config.net),
            addr: Some(wire::hub_addr(&endpoint.addr())),
            cursor: self.store().seq()?,
            devices,
            connected,
        })
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
        eprintln!("recall-hub: removed {} ({}), closed {closed} connection(s)", device.name, device.id);
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
                conn.close(wire::close::NOT_PAIRED, b"device removed");
                closed += 1;
            }
            false
        });
        closed
    }
}

fn net_name(net: &Net) -> String {
    match net {
        Net::N0 => "n0".to_string(),
        Net::Relay(url) => url.clone(),
        Net::Local => "local".to_string(),
    }
}

fn constant_time_eq(a: &[u8; 16], b: &[u8; 16]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Status and devices from the data directory, for when the hub isn't running.
pub fn offline_status(data: &Path) -> Result<(Status, Vec<DeviceInfo>)> {
    if !data.join(data::KEY).exists() {
        return Err(Error::new(
            "no_hub",
            format!("there's no hub in {}; start one with recall-hub serve", data.display()),
        ));
    }
    let key = wire::load_or_create_key(&data.join(data::KEY))?;
    let store = Store::open_hub(&data.join(data::LIBRARY))?;
    let devices = Devices::load(data)?;
    let status = Status {
        version: VERSION.to_string(),
        data: data.display().to_string(),
        running: false,
        endpoint_id: key.public().to_string(),
        library_id: store.device_id()?,
        net: String::new(),
        addr: None,
        cursor: store.seq()?,
        devices: devices.list().len(),
        connected: 0,
    };
    let list =
        devices.list().iter().map(|d| DeviceInfo { device: d.clone(), connected: false, last_sync: None }).collect();
    Ok((status, list))
}

/// Removes a device while the hub isn't running.
pub fn offline_remove_device(data: &Path, query: &str) -> Result<Device> {
    let _lock = data::lock(data)?;
    let mut devices = Devices::load(data)?;
    let device = devices.find(query)?;
    devices.remove(&device.id)?;
    Ok(device)
}
