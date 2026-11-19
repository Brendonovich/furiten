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

/// `$FURITEN_HOME`, else `$FURITEN_TEMP_DB`'s throwaway library, else `~/Library/Application Support/Furiten` on
/// macOS and `$XDG_DATA_HOME/furiten` (default `~/.local/share/furiten`) elsewhere; a dev build uses `Furiten Dev`
/// and `furiten-dev`.
pub fn default_home() -> PathBuf {
    if let Some(home) = std::env::var_os("FURITEN_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(home);
    }
    if let Some(home) = temp_home() {
        return home;
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

/// The throwaway library `$FURITEN_TEMP_DB` asks for, in the system temp directory: seeded with sample decks for
/// trying the app out or testing, and started afresh each run. A core already serving it (a window, or the CLI
/// hosting one) keeps its library, so a command and the app it starts share one.
fn temp_home() -> Option<PathBuf> {
    std::env::var_os("FURITEN_TEMP_DB").filter(|v| !v.is_empty())?;
    static HOME: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    Some(
        HOME.get_or_init(|| {
            let folder = if is_dev_build() { "furiten-temp-dev" } else { "furiten-temp" };
            let home = std::env::temp_dir().join(folder);
            prepare_temp_home(&home);
            home
        })
        .clone(),
    )
}

/// Makes `home` the seeded throwaway library `$FURITEN_TEMP_DB` promises, unless a core is already serving it.
fn prepare_temp_home(home: &std::path::Path) {
    #[cfg(unix)]
    if client::Client::connect(home).is_ok() {
        return;
    }
    let _ = std::fs::remove_dir_all(home);
    if let Err(e) = std::fs::create_dir_all(home) {
        eprintln!("furiten: temp library {}: {e}", home.display());
        return;
    }
    match seed::seed_temp_home(home) {
        Ok(()) => eprintln!("furiten: temp library at {}", home.display()),
        Err(e) => eprintln!("furiten: temp library {}: {}", home.display(), e.message),
    }
}

/// Unix milliseconds now.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
