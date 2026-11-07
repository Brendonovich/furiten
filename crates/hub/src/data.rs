//! The hub's data directory. Its device list is `recall_core::hub::host::Devices`.

use std::path::{Path, PathBuf};

use recall_core::{Error, Result};

pub const KEY: &str = "iroh.key";
pub const LIBRARY: &str = "library.db";
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

pub use recall_core::hub::host::{DEVICES, Devices};
pub use recall_core::protocol::Device;
