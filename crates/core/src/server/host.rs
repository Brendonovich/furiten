//! Hosting: the desktop's core as its library's hub, so phones pair with it and sync over Iroh with stores of their
//! own (docs/contract.md, "Hosting"). The store serves sync itself ([`Store::set_hosting`]), and a
//! [`Host`] serves it to paired devices on the device's own endpoint key.
//!
//! [`Store::set_hosting`]: crate::Store::set_hosting

use std::time::Duration;

use tokio::runtime::Runtime;

use super::Core;
use crate::hub;
use crate::hub::host::{Devices, Host, HostConfig};
use crate::protocol::{Device, DeviceInfo, HostStatus, PairTicket};
use crate::{Error, Result};

/// A running [`Host`] with the runtime it runs on. Dropping it closes every device's connection.
pub(super) struct Running {
    runtime: Option<Runtime>,
    host: Host,
}

impl Running {
    pub(super) fn start(core: &Core) -> Result<Running> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("furiten-host")
            .enable_all()
            .build()?;
        let key = hub::load_or_create_key(&core.home.join("iroh.key"))?;
        let config = HostConfig::new(&core.home, core.net.clone());
        let me = core.me.clone();
        // A device's sync stored something, or one paired: tell this app's clients (not the devices; the host
        // already has).
        let host = runtime.block_on(Host::start(key, config, core.store.clone(), move || {
            if let Some(core) = me.upgrade() {
                core.broadcast_clients();
            }
        }))?;
        Ok(Running { runtime: Some(runtime), host })
    }

    pub(super) fn pair(&self) -> Result<PairTicket> {
        self.runtime.as_ref().expect("the runtime lives until drop").block_on(self.host.pair())
    }

    pub(super) fn changed(&self, cursor: i64) {
        self.host.changed(cursor);
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            let host = &self.host;
            runtime.block_on(async {
                let _ = tokio::time::timeout(Duration::from_secs(2), host.shutdown()).await;
            });
            runtime.shutdown_background();
        }
    }
}

impl Core {
    /// Starts or stops the host to match the store, if this core's host process hosts at all.
    pub(super) fn restart_host(&self) {
        let mut running = self.host.lock().unwrap_or_else(|e| e.into_inner());
        drop(running.take());
        let wanted = self.host_serves.get().is_some() && self.store().is_hosting();
        let started = wanted.then(|| Running::start(self));
        *self.host_error.lock().unwrap_or_else(|e| e.into_inner()) =
            started.as_ref().and_then(|s| s.as_ref().err()).map(|e| e.message.clone());
        *running = started.and_then(Result::ok);
    }

    pub(super) fn host_status(&self) -> Result<HostStatus> {
        let enabled = self.store().is_hosting();
        let running = self.host.lock().unwrap_or_else(|e| e.into_inner());
        let devices = match running.as_ref() {
            Some(running) => running.host.devices(),
            None => Devices::load(&self.home)?
                .list()
                .iter()
                .map(|d| DeviceInfo { device: d.clone(), connected: false, last_sync: None })
                .collect(),
        };
        let endpoint_id = match running.as_ref() {
            Some(running) => Some(running.host.endpoint_id().to_string()),
            None => {
                let key = self.home.join("iroh.key");
                key.exists().then(|| hub::load_or_create_key(&key).ok().map(|k| k.public().to_string())).flatten()
            }
        };
        let mut error = self.host_error.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if enabled && self.host_serves.get().is_none() {
            error = Some("only the desktop app hosts devices; open it to start".into());
        }
        Ok(HostStatus { enabled, running: running.is_some(), endpoint_id, devices, error })
    }

    /// Makes this library its own hub, kept on across restarts until `host.disable`.
    pub(super) fn host_enable(&self) -> Result<HostStatus> {
        self.store().set_hosting(true)?;
        if self.host.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
            self.restart_host();
        }
        self.broadcast_clients();
        let status = self.host_status()?;
        match &status.error {
            Some(e) if self.host_serves.get().is_some() => Err(Error::new("host_failed", e.clone())),
            _ => Ok(status),
        }
    }

    /// Stops hosting. Paired devices stay paired, and reconnect when it's turned back on.
    pub(super) fn host_disable(&self) -> Result<HostStatus> {
        self.store().set_hosting(false)?;
        self.restart_host();
        self.broadcast_clients();
        self.host_status()
    }

    /// A pairing ticket for a new device, turning hosting on first if it's off.
    pub(super) fn host_pair(&self) -> Result<PairTicket> {
        if !self.store().is_hosting() {
            self.host_enable()?;
        }
        let running = self.host.lock().unwrap_or_else(|e| e.into_inner());
        match running.as_ref() {
            Some(running) => running.pair(),
            None => Err(self
                .host_status()?
                .error
                .map_or_else(|| Error::new("host_failed", "hosting isn't running"), |e| Error::new("host_failed", e))),
        }
    }

    /// Unpairs a device and closes its connections. It keeps what it already has.
    pub(super) fn host_remove(&self, query: &str) -> Result<Device> {
        let running = self.host.lock().unwrap_or_else(|e| e.into_inner());
        let device = match running.as_ref() {
            Some(running) => running.host.remove_device(query)?,
            None => {
                let mut devices = Devices::load(&self.home)?;
                let device = devices.find(query)?;
                devices.remove(&device.id)?;
                device
            }
        };
        drop(running);
        self.broadcast_clients();
        Ok(device)
    }

    /// After a write here: every connected device should pull.
    pub(super) fn host_changed(&self) {
        let running = self.host.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(running) = running.as_ref() {
            let cursor = self.store().seq().unwrap_or(0);
            running.changed(cursor);
        }
    }
}
