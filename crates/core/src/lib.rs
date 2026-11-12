//! Furiten's core: the store, review queue and sync client, plus the socket server a host process (the desktop app,
//! or the CLI) runs it with and a blocking client for it. See `docs/contract.md` for the formats and protocol
//! implemented here.

pub mod cram;
#[cfg(feature = "hub")]
pub mod hub;
pub mod import;
pub mod model;
pub mod protocol;
mod seed;
pub mod sort;
pub mod store;
pub mod sync;

#[cfg(unix)]
pub mod client;
#[cfg(unix)]
pub mod server;

use std::fmt;
use std::path::PathBuf;

pub use model::{Card, CardView, Change, Deck, DeckSummary, Review, Schedule, State};
pub use store::Store;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Every failure in the core is a `{ code, message }` pair, which is also what the protocol sends back.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}

impl Error {
    pub fn new(code: &str, message: impl Into<String>) -> Error {
        Error { code: code.to_string(), message: message.into() }
    }

    pub fn not_found(what: &str, id: &str) -> Error {
        Error::new("not_found", format!("{what} {id} not found"))
    }

    pub fn invalid(message: impl Into<String>) -> Error {
        Error::new("invalid_params", message)
    }

    pub fn internal(message: impl fmt::Display) -> Error {
        Error::new("internal", message.to_string())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Error {
        Error::new("storage", err.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::new("io", err.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Error {
        Error::invalid(err.to_string())
    }
}

/// Whether this is a dev build (`FURITEN_CHANNEL=dev` at build time): `Furiten Dev.app`, with its own name, bundle ID,
/// data home and preferences, so it runs beside Furiten as its own library.
pub fn is_dev_build() -> bool {
    env!("FURITEN_DEV_BUILD") == "1"
}

/// `$FURITEN_HOME`, else `~/Library/Application Support/Furiten` on macOS and `$XDG_DATA_HOME/furiten` (default
/// `~/.local/share/furiten`) elsewhere; a dev build uses `Furiten Dev` and `furiten-dev`.
pub fn default_home() -> PathBuf {
    if let Some(home) = std::env::var_os("FURITEN_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(home);
    }
    let dev = is_dev_build();
    let user_home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        let folder = if dev { "Furiten Dev" } else { "Furiten" };
        return user_home.join("Library/Application Support").join(folder);
    }
    let folder = if dev { "furiten-dev" } else { "furiten" };
    match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(data) => PathBuf::from(data).join(folder),
        None => user_home.join(".local/share").join(folder),
    }
}

/// Unix milliseconds now.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
