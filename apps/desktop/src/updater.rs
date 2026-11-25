//! Furiten's in-app updater, a client of the Anomaly release service (`anoma.ly/release`): the same update system
//! Hex uses, in place of the Sparkle feed it replaces. One worker polls the channel's manifest 10 seconds after
//! launch, then hourly or on "Check for updates", decides with the protocol's client rules (`updater/protocol.rs`,
//! the service's shared test vectors), and for a direct install downloads the target, checks its size, SHA-256, and
//! Developer ID signature, and stages it until the user restarts. Checks continue while an update is staged, so a
//! newer release replaces it, and Restart to update checks once more before it switches over. Versions are display
//! text and never compared: whenever the channel's current release is not the running build (its exact version, on
//! macOS), Furiten takes it. Only builds the release workflow made check: it compiles in the channel
//! (`FURITEN_RELEASE_CHANNEL`, `stable` for Furiten and `dev` for Furiten Dev), so local builds, source builds, and
//! installs whose updates an administrator turned off never check. Manifests are not signed: HTTPS, the download's SHA-256, and the bundle's code signature are the
//! integrity checks.
//!
//! The release workflow publishes the channel's manifest (`.github/workflows/release.yml`); the registry
//! entry that authorizes it lives in `anomalyco/release` (`registry/furiten.json`).

#[cfg_attr(target_os = "macos", path = "updater/macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "updater/none.rs")]
mod platform;
pub mod protocol;
mod proxy;
mod transport;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use eyre::{Result, WrapErr, eyre};
use protocol::{Decision, Install};

use platform::Staged;

/// This app's version, as the manifest's `release.version` and the bundle's `CFBundleShortVersionString` write it:
/// display text, never compared with an order.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const APP: &str = "furiten";
const ARTIFACT: &str = "desktop";
const ORIGIN: &str = "https://anoma.ly/release";
/// The channel the release workflow built this copy for: `stable` (Furiten) or `dev` (Furiten Dev). Copies built
/// anywhere else have none and never check. (The client rules for a channel switch are already in
/// `protocol::decide`.)
const RELEASE_CHANNEL: Option<&str> = option_env!("FURITEN_RELEASE_CHANNEL");
const STARTUP_DELAY: Duration = Duration::from_secs(10);
/// The service counts a running install from at least one check an hour.
const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// Furiten Dev checks more often, so a dev build reaches the people testing it soon after it's published.
const DEV_CHECK_INTERVAL: Duration = Duration::from_secs(10 * 60);
/// The first retry after a failed check, doubling with each failure.
const FAILURE_DELAY: Duration = Duration::from_secs(60);
/// Failed checks back off, but never past hourly.
const MAX_BACKOFF: Duration = CHECK_INTERVAL;
/// The manifest fetch of an ordinary check.
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(60);
/// The final check behind Restart to update: an unreachable service must not hold the restart for long.
const RESTART_MANIFEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Where an install that can't update itself points when the release carries no better link: the project, until
/// the landing page (`apps/site`) is deployed.
const DOWNLOAD_FALLBACK: &str = "https://github.com/anomalyco/furiten";

/// The channel this copy follows; `stable` for copies that never check.
pub fn channel() -> &'static str {
    RELEASE_CHANNEL.unwrap_or("stable")
}

fn check_interval() -> Duration {
    if channel() == "dev" { DEV_CHECK_INTERVAL } else { CHECK_INTERVAL }
}

/// How this install receives updates, which decides what the updater does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Method {
    /// Our installer's receipt (or, on macOS, a writable `Furiten.app`): Furiten downloads and stages updates
    /// itself on this channel.
    Direct { channel: String },
    /// A direct install Furiten cannot replace (macOS: a read-only or translocated location); updates link to the
    /// download.
    DownloadOnly,
    /// A package manager (`brew`, `deb-x64`, `nix`, …) updates it; Furiten shows its command once it serves a newer
    /// version. (None serve Furiten yet; the protocol's rules and this arm are ready for when one does.)
    #[allow(dead_code)]
    Package(&'static str),
    /// Built from a checkout, which updates it.
    Source,
    /// Anything else, such as a downloaded executable run in place (the platform yet to come constructs it).
    #[allow(dead_code)]
    Unknown,
}

impl Method {
    fn distribution(&self) -> &str {
        match self {
            Self::Direct { .. } | Self::DownloadOnly => "direct",
            Self::Package(kind) => kind,
            Self::Source | Self::Unknown => "unknown",
        }
    }
}

/// What the update panel (`ui/update.rs`) shows, live from the worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// This copy never checks: a dev or source build, or updates turned off.
    Off,
    Checking,
    Current,
    /// The last check failed; the next one is already scheduled.
    Failed,
    Downloading,
    /// A release is staged; restarting installs it.
    Ready {
        version: String,
    },
    /// A release this install can't apply itself: where to get it.
    Available {
        version: String,
        url: String,
    },
}

struct Shared {
    state: Mutex<Status>,
    /// Set by "Check for updates" and `stop`; the worker waits on it between checks.
    wake: Mutex<Wake>,
    signal: Condvar,
    /// The newest staged update. Checks continue while one waits, and a different current release replaces it
    /// before the restart.
    staged: Mutex<Option<StagedUpdate>>,
    /// A staged update has been swapped in (Restart to update, or quit); there is nothing left to offer.
    applied: Mutex<bool>,
    restart: Mutex<Restart>,
}

impl Shared {
    fn new(state: Status) -> Self {
        Self {
            state: Mutex::new(state),
            wake: Mutex::new(Wake::default()),
            signal: Condvar::new(),
            staged: Mutex::new(None),
            applied: Mutex::new(false),
            restart: Mutex::new(Restart::Idle),
        }
    }
}

/// A staged update and the SHA-256 of the target it came from, which tells a later check whether the channel still
/// offers that release.
#[derive(Clone)]
struct StagedUpdate {
    staged: Staged,
    sha256: String,
    version: String,
}

#[derive(Debug, Default)]
enum Restart {
    #[default]
    Idle,
    /// Restart to update was pressed: the worker checks the channel once more, then switches to the newest staged
    /// update.
    Requested,
    /// The worker switched over (the app quits) or could not.
    Done(std::result::Result<(), String>),
}

#[derive(Default)]
struct Wake {
    check: bool,
    stop: bool,
}

/// The updater the app owns: a worker behind shared state, polled by the update panel and `App::tick`.
#[derive(Clone)]
pub struct Updater {
    shared: Arc<Shared>,
}

impl Updater {
    /// Starts the worker, once the first frame is up. Copies that never check (dev and source builds, updates
    /// turned off, platforms without an apply module) start none.
    pub fn start() -> Self {
        let method = platform::method();
        let disabled = if !cfg!(target_os = "macos")
            || RELEASE_CHANNEL.is_none()
            || method == Method::Source
            || updates_off_by_env()
            || managed_by_policy()
        {
            Some(Status::Off)
        } else {
            None
        };
        let checks = disabled.is_none();
        let shared = Arc::new(Shared::new(disabled.unwrap_or(Status::Checking)));
        if checks {
            let worker = shared.clone();
            if let Err(error) =
                std::thread::Builder::new().name("furiten-updater".into()).spawn(move || run(&worker, &method))
            {
                eprintln!("furiten-desktop: could not start the updater: {error}");
                *lock(&shared.state) = Status::Failed;
            }
        }
        Self { shared }
    }

    /// Checks now instead of at the next interval. A staged update stays offered while the check looks for a newer
    /// release.
    pub fn check_now(&self) {
        {
            let mut state = lock(&self.shared.state);
            match &*state {
                Status::Off | Status::Downloading => return,
                Status::Ready { .. } => {}
                _ => *state = Status::Checking,
            }
        }
        self.wake();
    }

    fn wake(&self) {
        lock(&self.shared.wake).check = true;
        self.shared.signal.notify_all();
    }

    /// Stops the worker at quit, so a check in flight doesn't hold the process.
    pub fn stop(&self) {
        lock(&self.shared.wake).stop = true;
        self.shared.signal.notify_all();
    }

    /// The panel's state, live.
    pub fn status(&self) -> Status {
        lock(&self.shared.state).clone()
    }

    /// Whether a release waits for the user (`App::tick` opens the panel for it once per run).
    pub fn offering(&self) -> bool {
        matches!(&*lock(&self.shared.state), Status::Ready { .. } | Status::Available { .. })
    }

    /// Restart to update: the worker checks the channel once more, so a release published since the update was
    /// staged replaces it first, then switches to the newest staged update.
    pub fn request_restart(&self) -> Result<()> {
        // A staged update that has already been swapped in (Restart to update, or quit) has nothing left to offer.
        if *lock(&self.shared.applied) {
            return Ok(());
        }
        let mut state = lock(&self.shared.state);
        let mut restart = lock(&self.shared.restart);
        // A second click while the restart is under way changes nothing.
        if matches!(&*restart, Restart::Requested | Restart::Done(Ok(()))) {
            return Ok(());
        }
        if !matches!(&*state, Status::Ready { .. }) {
            return Err(eyre!("no update is ready"));
        }
        *restart = Restart::Requested;
        *state = Status::Checking;
        drop(restart);
        drop(state);
        self.wake();
        Ok(())
    }

    /// The outcome of Restart to update, once: `Ok` when the update starts as soon as the app quits, which the
    /// caller must now do.
    pub fn take_restart(&self) -> Option<Result<()>> {
        let mut restart = lock(&self.shared.restart);
        if !matches!(&*restart, Restart::Done(_)) {
            return None;
        }
        match std::mem::take(&mut *restart) {
            Restart::Done(result) => Some(result.map_err(|error| eyre!("{}", error))),
            Restart::Idle | Restart::Requested => None,
        }
    }

    /// Quitting with an update ready installs it for the next launch.
    pub fn install_on_quit(&self) {
        if *lock(&self.shared.applied) {
            return;
        }
        if let Some(update) = lock(&self.shared.staged).as_ref() {
            platform::install_on_quit(&update.staged);
            *lock(&self.shared.applied) = true;
        }
    }
}

/// `FURITEN_DISABLE_UPDATES=1` turns this launch's updater off: no checks, no network, nothing offered. For copies
/// of Furiten that must stay quiet, such as benchmarks.
fn updates_off_by_env() -> bool {
    std::env::var_os("FURITEN_DISABLE_UPDATES").is_some_and(|value| value == "1")
}

/// Whether an administrator turned updates off, read once per launch so a managed value applies at the next one.
fn managed_by_policy() -> bool {
    static MANAGED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *MANAGED.get_or_init(platform::updates_disabled_by_policy)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// Waits `delay` or until woken; true when the updater should stop.
fn wait(shared: &Shared, delay: Duration) -> bool {
    let mut wake = lock(&shared.wake);
    let deadline = std::time::Instant::now() + delay;
    while !wake.check && !wake.stop {
        let now = std::time::Instant::now();
        if now >= deadline {
            break;
        }
        wake = shared.signal.wait_timeout(wake, deadline - now).unwrap_or_else(|error| error.into_inner()).0;
    }
    wake.check = false;
    wake.stop
}

/// Whether this run is inside an app bundle, so the menu can offer updates at all.
pub fn bundled() -> bool {
    RELEASE_CHANNEL.is_some() && platform::installed_bundle().is_some()
}

/// Removes what an earlier update left beside the bundle: the replaced bundle once this (new) version runs, or an
/// unpacked update never applied.
pub fn finish_on_launch() {
    platform::finish_on_launch();
}

fn run(shared: &Shared, method: &Method) {
    let mut cache = transport::ManifestCache::default();
    let mut failures = 0;
    if wait(shared, STARTUP_DELAY) {
        return;
    }
    loop {
        let restarting = matches!(*lock(&shared.restart), Restart::Requested);
        let staged = lock(&shared.staged).clone();
        {
            // A staged update stays offered while the check runs.
            let mut state = lock(&shared.state);
            if !matches!(&*state, Status::Ready { .. }) {
                *state = Status::Checking;
            }
        }
        let result = check(shared, method, &mut cache, staged.as_ref(), restarting);
        let (next, delay) = match result {
            Ok(next) => {
                failures = 0;
                (next, check_interval())
            }
            Err(error) => {
                eprintln!("furiten-desktop: the update check failed: {error:#}");
                failures = (failures + 1).min(8);
                let next = lock(&shared.staged)
                    .as_ref()
                    .map(|staged| Status::Ready { version: staged.version.clone() })
                    .unwrap_or(Status::Failed);
                (next, (FAILURE_DELAY * 2u32.pow(failures)).min(MAX_BACKOFF))
            }
        };
        if restarting {
            if finish_restart(shared, next) {
                return;
            }
        } else {
            let mut state = lock(&shared.state);
            // Restart to update was pressed during this check: the next check, which starts at once, finishes it.
            if !matches!(*lock(&shared.restart), Restart::Requested) {
                *state = next;
            }
        }
        if wait(shared, delay) {
            return;
        }
    }
}

/// Switches to the newest staged update once Restart to update's final check has run, whatever its result. True
/// when the app now quits.
fn finish_restart(shared: &Shared, next: Status) -> bool {
    let mut state = lock(&shared.state);
    let staged = lock(&shared.staged).clone();
    let result = match &staged {
        Some(update) => platform::restart_into(&update.staged).map_err(|error| format!("{error:#}")),
        None => Err("no update is ready".into()),
    };
    let restarted = result.is_ok();
    if let Err(error) = &result {
        eprintln!("furiten-desktop: could not restart into the update: {error}");
    }
    if restarted {
        *lock(&shared.applied) = true;
    }
    // The panel shows Ready for the moment before the app quits — the new bundle's relauncher is already waiting —
    // and keeps showing it when the restart failed, so the button can be pressed again.
    *state = staged.as_ref().map(|update| Status::Ready { version: update.version.clone() }).unwrap_or(next);
    *lock(&shared.restart) = Restart::Done(result);
    restarted
}

fn check(
    shared: &Shared,
    method: &Method,
    cache: &mut transport::ManifestCache,
    staged: Option<&StagedUpdate>,
    restarting: bool,
) -> Result<Status> {
    let updates = updates_dir()?;
    let user_agent = transport::user_agent(channel(), VERSION);
    let manifest_url = manifest_url();
    let timeout = if restarting { RESTART_MANIFEST_TIMEOUT } else { MANIFEST_TIMEOUT };
    let manifest = cache.fetch(&manifest_url, &user_agent, timeout)?;
    let install = Install {
        version: VERSION.into(),
        target: platform::TARGET.into(),
        distribution: method.distribution().into(),
        rollout_bucket: rollout_bucket(&updates)?,
        // The macOS target is a zip of the bundle, which the running app cannot reproduce, so a Mac build is
        // identified by its version text.
        sha256: running_sha256(),
        channel: Some(channel().into()),
    };
    let decision = protocol::decide(APP, ARTIFACT, channel(), &manifest, &install);
    match decision {
        Decision::Reject { reason } => Err(eyre!("the update manifest was rejected: {reason}")),
        Decision::None { .. } | Decision::Install { .. }
            if staged.is_some_and(|staged| !replaces_staged(&decision, &staged.sha256)) =>
        {
            // The channel still offers the staged release (or nothing this install takes); it stays ready.
            Ok(Status::Ready { version: staged.map(|update| update.version.clone()).unwrap_or_default() })
        }
        Decision::None { .. } => {
            transport::prune(&updates, None);
            Ok(Status::Current)
        }
        Decision::Notify { version, notes_url, .. } => {
            Ok(Status::Available { version, url: notes_url.unwrap_or_else(|| DOWNLOAD_FALLBACK.into()) })
        }
        Decision::Install { version, target, .. } => {
            if *method == Method::DownloadOnly {
                // A bundle this user can't replace: the DMG people install from.
                let url = protocol::parse_manifest(&manifest)
                    .and_then(|manifest| {
                        manifest
                            .release
                            .target(platform::DOWNLOAD_TARGET)
                            .and_then(|target| protocol::resolve_url(&manifest_url, &target.url))
                            .or(manifest.release.notes_url)
                    })
                    .unwrap_or_else(|| DOWNLOAD_FALLBACK.into());
                return Ok(Status::Available { version, url });
            }
            let url = protocol::resolve_url(&manifest_url, &target.url)
                .ok_or_else(|| eyre!("the update URL {} is not usable", target.url))?;
            // A staged update stays offered while a newer one downloads in the background.
            if staged.is_none() || restarting {
                *lock(&shared.state) = Status::Downloading;
            }
            transport::prune(&updates, Some(&target.sha256));
            let file = transport::download(&url, &user_agent, &target, &updates)?;
            let staged =
                platform::stage(&file, &version).wrap_err_with(|| format!("could not stage Furiten {version}"))?;
            *lock(&shared.staged) = Some(StagedUpdate { staged, sha256: target.sha256, version: version.clone() });
            Ok(Status::Ready { version })
        }
    }
}

/// Whether `decision` replaces the staged update: only a different release this install takes by itself does. A
/// rollout that has not reached this install, an unconfirmed channel switch, or the same release keeps it.
fn replaces_staged(decision: &Decision, staged_sha256: &str) -> bool {
    matches!(
        decision,
        Decision::Install {
            reason: protocol::Reason::Update | protocol::Reason::Required,
            target,
            ..
        } if target.sha256 != staged_sha256
    )
}

/// `FURITEN_RELEASE_BASE_URL` (HTTPS only) points a build at another service origin, for local tests against a
/// fixture service. Nothing defaults to the service's dev stage, which only tests the service itself.
fn manifest_url() -> String {
    let origin = std::env::var("FURITEN_RELEASE_BASE_URL").ok().filter(|origin| origin.starts_with("https://"));
    format!("{}/{APP}/{ARTIFACT}/{}.json", origin.as_deref().unwrap_or(ORIGIN).trim_end_matches('/'), channel())
}

/// This build's target SHA-256, when the platform can tell (the target is the executable itself); hashed once per
/// launch.
fn running_sha256() -> Option<String> {
    static SHA256: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    SHA256.get_or_init(platform::running_sha256).clone()
}

/// Where downloads and the rollout bucket live, beside the library (`furiten_core::default_home`).
fn updates_dir() -> Result<PathBuf> {
    let updates = furiten_core::default_home().join("updates");
    std::fs::create_dir_all(&updates)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&updates, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(updates)
}

/// This install's rollout bucket, 0–99: a random 32-bit number drawn once from the OS's secure source and kept with
/// the install. It never leaves the device.
fn rollout_bucket(updates: &Path) -> Result<u64> {
    let path = updates.join("update-bucket");
    if let Some(number) = std::fs::read_to_string(&path).ok().and_then(|text| text.trim().parse::<u32>().ok()) {
        return Ok(u64::from(number) % 100);
    }
    let mut bytes = [0; 4];
    getrandom::fill(&mut bytes).map_err(|error| eyre!("no secure random source: {error}"))?;
    let number = u32::from_le_bytes(bytes);
    let partial = path.with_extension("partial");
    std::fs::write(&partial, format!("{number}\n"))?;
    std::fs::rename(&partial, &path)?;
    Ok(u64::from(number) % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("furiten-updater-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn the_rollout_bucket_is_drawn_once_and_kept() {
        let updates = temporary("bucket");
        let first = rollout_bucket(&updates).unwrap();
        assert!(first < 100);
        assert_eq!(rollout_bucket(&updates).unwrap(), first);
        std::fs::write(updates.join("update-bucket"), "4294967295\n").unwrap();
        assert_eq!(rollout_bucket(&updates).unwrap(), 95);
        let _ = std::fs::remove_dir_all(updates);
    }

    #[test]
    fn only_a_taken_release_replaces_a_staged_one() {
        let target = |sha: &str| protocol::Target {
            url: "/release/furiten/files/x/Furiten.zip".into(),
            size: 1,
            sha256: sha.into(),
        };
        let install = |sha: &str, reason| Decision::Install { reason, version: "0.2.0".into(), target: target(sha) };
        let staged = "a".repeat(64);
        assert!(replaces_staged(&install(&"b".repeat(64), protocol::Reason::Update), &staged));
        assert!(replaces_staged(&install(&"b".repeat(64), protocol::Reason::Required), &staged));
        assert!(!replaces_staged(&install(&staged, protocol::Reason::Update), &staged));
        assert!(!replaces_staged(&install(&"b".repeat(64), protocol::Reason::Switch), &staged));
        assert!(!replaces_staged(&Decision::None { reason: "up_to_date" }, &staged));
    }

    #[test]
    fn the_manifest_url_names_this_app_and_channel() {
        assert_eq!(manifest_url(), "https://anoma.ly/release/furiten/desktop/stable.json");
    }
}
