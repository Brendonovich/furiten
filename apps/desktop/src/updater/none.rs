//! The platform that has no updater yet: Windows and Linux builds are planned (PLAN.md), and their apply
//! modules (`windows.rs`, `linux.rs` in Hex's updater, receipts and version directories) land with them. Nothing
//! here runs: the orchestrator's `Updater::start` turns the updater off where `method()` can't say how an update
//! would even be applied.

use eyre::{Result, bail};

use super::Method;

/// The target this build would download. `unknown` fails the protocol's name rules without a real platform, which
/// is the point: no manifest can offer it anything.
pub const TARGET: &str = "unknown";
/// The target the download page for this platform serves.
pub const DOWNLOAD_TARGET: &str = "unknown";

/// The activated update: none can be.
#[derive(Clone, Debug)]
pub struct Staged;

pub fn installed_bundle() -> Option<std::path::PathBuf> {
    None
}

pub fn method() -> Method {
    Method::Unknown
}

pub fn running_sha256() -> Option<String> {
    None
}

/// Whether an administrator turned updates off, read once per launch so a managed value applies at the next one.
pub fn updates_disabled_by_policy() -> bool {
    false
}

pub fn finish_on_launch() {}

pub fn stage(_archive: &std::path::Path, _version: &str) -> Result<Staged> {
    bail!("this platform can't apply an update yet")
}

pub fn restart_into(_staged: &Staged) -> Result<()> {
    bail!("this platform can't apply an update yet")
}

pub fn install_on_quit(_staged: &Staged) {}
