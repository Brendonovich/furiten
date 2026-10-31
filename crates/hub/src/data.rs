//! The hub's data directory and its device list.

use std::io::Write;
use std::path::{Path, PathBuf};

use recall_core::{Error, Result};
use serde::{Deserialize, Serialize};

pub const KEY: &str = "iroh.key";
pub const LIBRARY: &str = "library.db";
pub const DEVICES: &str = "devices.json";
pub const SOCKET: &str = "hub.sock";
pub const LOCK: &str = "hub.lock";

/// `$XDG_DATA_HOME/recall-hub` (default `~/.local/share/recall-hub`), or `~/Library/Application Support/recall-hub`
/// on macOS.
pub fn default_data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        return home.join("Library/Application Support/recall-hub");
    }
    match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(data) => PathBuf::from(data).join("recall-hub"),
        None => home.join(".local/share/recall-hub"),
    }
}

/// Creates the data directory (mode 0700) and takes its lock, so only one hub (or offline `devices rm`) uses it.
pub fn lock(data: &Path) -> Result<std::fs::File> {
    std::fs::create_dir_all(data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(data, std::fs::Permissions::from_mode(0o700))?;
    }
    let lock = std::fs::File::options().create(true).truncate(false).write(true).open(data.join(LOCK))?;
    if lock.try_lock().is_err() {
        return Err(Error::new("already_running", format!("a hub is already running for {}", data.display())));
    }
    Ok(lock)
}

/// A paired device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// Its endpoint id (hex).
    pub id: String,
    pub name: String,
    pub added_at: i64,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    devices: Vec<Device>,
}

/// The device list, kept in `devices.json` (mode 0600).
#[derive(Debug)]
pub struct Devices {
    path: PathBuf,
    list: Vec<Device>,
}

impl Devices {
    pub fn load(data: &Path) -> Result<Devices> {
        let path = data.join(DEVICES);
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
