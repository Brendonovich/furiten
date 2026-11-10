//! `furiten-hub`: one Furiten library on hardware you own, synced to paired devices over Iroh. See
//! `docs/contract.md` ("Self-hosted hub") for the protocol and `README.md` for running one.

pub mod control;
pub mod data;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use furiten_core::hub::host::{Host, HostConfig};
use furiten_core::hub::{self as wire, Net};
use furiten_core::store::HubAddr;
use furiten_core::sync::lock;
use furiten_core::{Error, Result, Store};
use iroh::EndpointId;
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

pub use furiten_core::hub::host::TICKET_TTL;
pub use furiten_core::protocol::{Device, DeviceInfo, PairTicket};

use data::Devices;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct Config {
    pub data: PathBuf,
    pub net: Net,
    pub ticket_ttl: Duration,
    /// Serve the control socket (`hub.sock`) that `furiten-hub pair`, `status` and `devices` use.
    pub control: bool,
}

impl Config {
    pub fn new(data: &Path, net: Net) -> Config {
        Config { data: data.to_path_buf(), net, ticket_ttl: TICKET_TTL, control: true }
    }
}

/// A running hub: a [`Host`] over the data directory's store. Dropping it without [`Hub::shutdown`] stops
/// accepting connections.
pub struct Hub {
    config: Config,
    host: Arc<Host>,
    store: Arc<Mutex<Store>>,
    control: Option<JoinHandle<()>>,
    _lock: std::fs::File,
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

impl Hub {
    /// Opens the data directory (taking its lock) and starts serving.
    pub async fn start(config: Config) -> Result<Hub> {
        let lock = data::lock(&config.data)?;
        let key = wire::load_or_create_key(&config.data.join(data::KEY))?;
        let store = Arc::new(Mutex::new(Store::open_hub(&config.data.join(data::LIBRARY))?));
        let host_config =
            HostConfig { net: config.net.clone(), ticket_ttl: config.ticket_ttl, dir: config.data.clone() };
        let host = Arc::new(Host::start(key, host_config, store.clone(), || {}).await?);
        let control = match config.control {
            true => Some(control::serve(&config.data, host.clone(), store.clone()).await?),
            false => None,
        };
        Ok(Hub { config, host, store, control, _lock: lock })
    }

    pub fn endpoint_id(&self) -> EndpointId {
        self.host.endpoint_id()
    }

    pub fn library_id(&self) -> &str {
        self.host.library_id()
    }

    /// A new one-time pairing ticket.
    pub async fn pair(&self) -> Result<PairTicket> {
        self.host.pair().await
    }

    pub fn status(&self) -> Result<Status> {
        status(&self.host, &self.store, &self.config.data)
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.host.devices()
    }

    /// Removes a device (by id, id prefix or name) and closes its connections.
    pub fn remove_device(&self, query: &str) -> Result<Device> {
        self.host.remove_device(query)
    }

    /// Stops serving and closes every connection.
    pub async fn shutdown(mut self) {
        if let Some(control) = self.control.take() {
            control.abort();
        }
        self.host.shutdown().await;
        if self.config.control {
            let _ = std::fs::remove_file(self.config.data.join(data::SOCKET));
        }
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        if let Some(control) = self.control.take() {
            control.abort();
        }
    }
}

pub(crate) fn status(host: &Host, store: &Mutex<Store>, data: &Path) -> Result<Status> {
    Ok(Status {
        version: VERSION.to_string(),
        data: data.display().to_string(),
        running: true,
        endpoint_id: host.endpoint_id().to_string(),
        library_id: host.library_id().to_string(),
        net: net_name(host.net()),
        addr: Some(host.addr()),
        cursor: lock(store).seq()?,
        devices: host.devices().len(),
        connected: host.connected(),
    })
}

fn net_name(net: &Net) -> String {
    match net {
        Net::N0 => "n0".to_string(),
        Net::Relay(url) => url.clone(),
        Net::Local => "local".to_string(),
    }
}

/// Status and devices from the data directory, for when the hub isn't running.
pub fn offline_status(data: &Path) -> Result<(Status, Vec<DeviceInfo>)> {
    if !data.join(data::KEY).exists() {
        return Err(Error::new(
            "no_hub",
            format!("there's no hub in {}; start one with furiten-hub serve", data.display()),
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
