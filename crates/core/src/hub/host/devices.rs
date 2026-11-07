//! The device list a host admits, kept in `devices.json` (mode 0600) in its directory.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::Device;
use crate::{Error, Result};

pub const DEVICES: &str = "devices.json";

#[derive(Default, Serialize, Deserialize)]
struct File {
    devices: Vec<Device>,
}

#[derive(Debug)]
pub struct Devices {
    path: PathBuf,
    list: Vec<Device>,
}

impl Devices {
    /// The list in `dir`'s `devices.json`, empty if there isn't one.
    pub fn load(dir: &Path) -> Result<Devices> {
        let path = dir.join(DEVICES);
        let list = match std::fs::read_to_string(&path) {
            Ok(text) => {
                serde_json::from_str::<File>(&text)
                    .map_err(|e| Error::new("storage", format!("{} is damaged: {e}", path.display())))?
                    .devices
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(Devices { path, list })
    }

    pub fn list(&self) -> &[Device] {
        &self.list
    }

    pub fn contains(&self, id: &str) -> bool {
        self.list.iter().any(|d| d.id == id)
    }

    /// Adds a device, or renames it if it's already paired.
    pub fn add(&mut self, device: Device) -> Result<()> {
        let mut next = self.list.clone();
        match next.iter_mut().find(|d| d.id == device.id) {
            Some(existing) => existing.name = device.name,
            None => next.push(device),
        }
        self.save(next)
    }

    /// Finds a device by id, a unique id prefix, or a unique name (case-insensitive).
    pub fn find(&self, query: &str) -> Result<Device> {
        let query = query.trim();
        if let Some(d) = self.list.iter().find(|d| d.id == query) {
            return Ok(d.clone());
        }
        let by_name: Vec<&Device> = self.list.iter().filter(|d| d.name.eq_ignore_ascii_case(query)).collect();
        let by_prefix: Vec<&Device> =
            self.list.iter().filter(|d| query.len() >= 4 && d.id.starts_with(&query.to_lowercase())).collect();
        match (by_name.as_slice(), by_prefix.as_slice()) {
            ([one], _) | ([], [one]) => Ok((*one).clone()),
            ([], []) => Err(Error::not_found("device", query)),
            _ => Err(Error::invalid(format!("more than one device matches {query}; use its id"))),
        }
    }

    pub fn remove(&mut self, id: &str) -> Result<()> {
        let next = self.list.iter().filter(|d| d.id != id).cloned().collect();
        self.save(next)
    }

    /// Writes the list to a temporary file and renames it over the old one, then keeps it.
    fn save(&mut self, list: Vec<Device>) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let mut options = std::fs::File::options();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&tmp)?;
        let body = serde_json::to_string_pretty(&File { devices: list.clone() }).map_err(Error::internal)?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, &self.path)?;
        self.list = list;
        Ok(())
    }
}
