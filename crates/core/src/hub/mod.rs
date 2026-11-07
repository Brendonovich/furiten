//! Syncing with a self-hosted `recall-hub` over Iroh: the `recall/sync/1` and `recall/pair/1` wire format, pairing
//! tickets, endpoint keys and network settings, a device's [`HubLink`], and [`host`], the hub's side, which
//! `recall-hub` (`crates/hub`) and a hosting desktop both run.

pub mod host;
mod link;
mod ticket;
mod wire;

use std::io::Write;
use std::path::Path;

use iroh::endpoint::{Builder, PortmapperConfig, presets};
use iroh::{EndpointAddr, PublicKey, RelayMode, RelayUrl, SecretKey};

pub use link::{HubLink, pair};
pub use ticket::Ticket;
pub use wire::*;

use crate::store::HubAddr;
use crate::{Error, Result};

pub const SYNC_ALPN: &[u8] = b"recall/sync/1";
pub const PAIR_ALPN: &[u8] = b"recall/pair/1";

/// How an endpoint reaches the network.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Net {
    /// n0's public relays, and n0's DNS to publish and look up addresses.
    #[default]
    N0,
    /// A self-hosted `iroh-relay` and nothing of n0's.
    Relay(String),
    /// Loopback only: no relays, no address lookup. Peers are dialled by their direct addresses. For tests and
    /// demos on one machine.
    Local,
}

impl Net {
    /// `RECALL_NET=local` for [`Net::Local`], else `RECALL_RELAY=<url>` for [`Net::Relay`], else [`Net::N0`].
    pub fn from_env() -> Net {
        let var = |key| std::env::var(key).ok().filter(|v| !v.trim().is_empty());
        if var("RECALL_NET").is_some_and(|v| v == "local") {
            return Net::Local;
        }
        match var("RECALL_RELAY") {
            Some(url) => Net::Relay(url.trim().to_string()),
            None => Net::N0,
        }
    }

    /// An endpoint builder for this network with `key` as its identity.
    pub fn builder(&self, key: SecretKey) -> Result<Builder> {
        let builder = match self {
            Net::N0 => iroh::Endpoint::builder(presets::N0),
            Net::Relay(url) => {
                iroh::Endpoint::builder(presets::Minimal).relay_mode(RelayMode::custom([relay_url(url)?]))
            }
            Net::Local => iroh::Endpoint::builder(presets::Minimal)
                .relay_mode(RelayMode::Disabled)
                .portmapper_config(PortmapperConfig::Disabled)
                .clear_ip_transports()
                .bind_addr(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
                .map_err(|e| Error::new("network", e.to_string()))?,
        };
        Ok(builder.secret_key(key))
    }
}

fn relay_url(url: &str) -> Result<RelayUrl> {
    url.parse().map_err(|_| Error::invalid(format!("{url} isn't a relay URL")))
}

/// Reads the Ed25519 secret key at `path` (64 hex characters), creating it with mode 0600 if it doesn't exist.
pub fn load_or_create_key(path: &Path) -> Result<SecretKey> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::File::options();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    match options.open(path) {
        Ok(mut file) => {
            let key = SecretKey::generate();
            file.write_all(format!("{}\n", hex(&key.to_bytes())).as_bytes())?;
            file.sync_all()?;
            Ok(key)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let text = std::fs::read_to_string(path)?;
            let bytes: [u8; 32] = unhex(text.trim())
                .and_then(|b| b.try_into().ok())
                .ok_or_else(|| Error::new("bad_key", format!("{} isn't a key", path.display())))?;
            Ok(SecretKey::from_bytes(&bytes))
        }
        Err(e) => Err(e.into()),
    }
}

/// The endpoint address to dial for a stored [`HubAddr`].
pub fn endpoint_addr(addr: &HubAddr) -> Result<EndpointAddr> {
    let id: PublicKey = addr.id.parse().map_err(|_| Error::invalid(format!("{} isn't an endpoint id", addr.id)))?;
    let mut out = EndpointAddr::new(id);
    if let Some(url) = &addr.relay_url {
        out = out.with_relay_url(relay_url(url)?);
    }
    for ip in &addr.direct_addrs {
        out = out.with_ip_addr(*ip);
    }
    Ok(out)
}

/// The [`HubAddr`] form of an endpoint address.
pub fn hub_addr(addr: &EndpointAddr) -> HubAddr {
    HubAddr {
        id: addr.id.to_string(),
        relay_url: addr.relay_urls().next().map(|u| u.to_string()),
        direct_addrs: addr.ip_addrs().copied().collect(),
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect()
}

/// A name for this device, for the hub's device list: the host name, else "device".
pub fn default_device_name() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|name| name.trim().trim_end_matches(".local").to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "device".to_string())
}

#[cfg(test)]
mod tests;
