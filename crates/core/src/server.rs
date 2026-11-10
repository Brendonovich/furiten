//! The local core: owns the store and sync, and serves the JSON-lines protocol on `$FURITEN_HOME/core.sock`. It runs
//! inside another process (the desktop app, or the CLI when there's no app), never as its own daemon. A mobile app
//! hosts it as [`Embedded`]: the same requests and replies, with no socket.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Weak;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::protocol::*;
use crate::store::SyncConfig;
use crate::sync::{Http, lock, sync_once, token_user_id};
use crate::{Error, Result, Store, VERSION};

const DEBOUNCE: Duration = Duration::from_secs(2);
const PERIOD: Duration = Duration::from_secs(60);
pub(crate) const CHANGED: &str = r#"{"event":"changed"}"#;

pub fn socket_path(home: &Path) -> PathBuf {
    home.join("core.sock")
}

#[cfg(feature = "hub")]
use crate::hub::{self, HubLink, Net};

#[cfg(feature = "hub")]
mod host;
#[cfg(feature = "web")]
mod web;

pub struct Server {
    core: Arc<Core>,
    listener: UnixListener,
    // Held for the life of the core: only one core per home.
    _lock: File,
}

impl Server {
    /// Takes the home's lock (refusing if another core holds it), removes a stale socket, binds a new one with mode
    /// 0600 and opens the store.
    pub fn bind(home: &Path) -> Result<Server> {
        Server::bind_inner(
            home,
            #[cfg(feature = "hub")]
            Net::from_env(),
        )
    }

    /// [`Server::bind`] with explicit network settings for a hub link, instead of `FURITEN_NET` / `FURITEN_RELAY`.
    #[cfg(feature = "hub")]
    pub fn bind_with_net(home: &Path, net: Net) -> Result<Server> {
        Server::bind_inner(home, net)
    }

    fn bind_inner(home: &Path, #[cfg(feature = "hub")] net: Net) -> Result<Server> {
        let lock = lock_home(home)?;
        let socket = socket_path(home);
        if socket.exists() {
            std::fs::remove_file(&socket)?;
        }
        let store = Store::open(&home.join("furiten.db"))?;
        let listener = UnixListener::bind(&socket)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        let core = Core::new(
            home,
            store,
            #[cfg(feature = "hub")]
            net,
        );
        Ok(Server { core, listener, _lock: lock })
    }

    /// Lets this core serve web access (`web.enable`), and the web app from `dir` when the host bundles it. Only the
    /// desktop app calls this: a core the CLI hosts for one command never listens.
    pub fn serve_web(&self, dir: Option<PathBuf>) {
        if self.core.web_host.set(dir).is_ok() {
            self.core.resume_web();
        }
    }

    /// Lets this core host devices (`host.enable`), and starts hosting if it was on when the app last ran. Only the
    /// desktop app calls this, as with [`Server::serve_web`].
    pub fn host_devices(&self) {
        if self.core.host_serves.set(()).is_ok() {
            #[cfg(feature = "hub")]
            self.core.restart_host();
        }
    }

    /// What `app.open` does: the desktop app shows its window. Without it `app.open` is `unsupported`.
    pub fn on_open(&self, open: impl Fn() + Send + Sync + 'static) {
        let _ = self.core.open.set(Box::new(open));
    }

    /// Stops [`Server::serve`] from another thread, as `shutdown` does.
    pub fn handle(&self) -> Handle {
        Handle { core: self.core.clone() }
    }

    /// Serves until a client sends `shutdown` or a [`Handle`] stops it, then syncs what's left.
    pub fn serve(self) -> Result<()> {
        let core = self.core.clone();
        let syncer = std::thread::spawn(move || core.sync_loop());
        for stream in self.listener.incoming() {
            if self.core.shutdown.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = stream else { continue };
            let core = self.core.clone();
            std::thread::spawn(move || core.connection(stream));
        }
        self.core.wake.notify_all();
        let _ = syncer.join();
        let _ = std::fs::remove_file(socket_path(&self.core.home));
        self.core.finish();
        Ok(())
    }
}

/// A core hosted in this process with no socket, for a mobile app that links it in. Requests and replies are the
/// protocol's JSON lines, and `on_change` runs (on a core thread) wherever a socket client would get `changed`.
pub struct Embedded {
    core: Arc<Core>,
    syncer: Option<std::thread::JoinHandle<()>>,
    _lock: File,
}

impl Embedded {
    pub fn open(home: &Path, on_change: impl Fn() + Send + Sync + 'static) -> Result<Embedded> {
        Embedded::open_inner(
            home,
            #[cfg(feature = "hub")]
            Net::from_env(),
            on_change,
        )
    }

    /// [`Embedded::open`] with explicit network settings for a hub link, instead of `FURITEN_NET` / `FURITEN_RELAY`.
    #[cfg(feature = "hub")]
    pub fn open_with_net(home: &Path, net: Net, on_change: impl Fn() + Send + Sync + 'static) -> Result<Embedded> {
        Embedded::open_inner(home, net, on_change)
    }

    fn open_inner(
        home: &Path,
        #[cfg(feature = "hub")] net: Net,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Result<Embedded> {
        let lock = lock_home(home)?;
        let store = Store::open(&home.join("furiten.db"))?;
        let core = Core::new(
            home,
            store,
            #[cfg(feature = "hub")]
            net,
        );
        let _ = core.on_change.set(Box::new(on_change));
        let syncer = core.clone();
        let syncer = std::thread::Builder::new()
            .name("furiten-sync".into())
            .spawn(move || syncer.sync_loop())
            .expect("spawn the sync thread");
        Ok(Embedded { core, syncer: Some(syncer), _lock: lock })
    }

    /// Answers one request line (`{ id, method, params }`) with its reply line. `shutdown` is `unsupported`: the
    /// host closes an embedded core by dropping it.
    pub fn request(&self, line: &str) -> String {
        if serde_json::from_str::<Request>(line).is_ok_and(|req| req.method == "shutdown") {
            return json!({ "id": null, "error": Error::new("unsupported", "drop the embedded core to close it") })
                .to_string();
        }
        self.core.handle(line).0.to_string()
    }

    /// Syncs now if signed in, as when the app comes back to the foreground.
    pub fn sync_soon(&self) {
        self.core.sync_at(Instant::now());
    }
}

impl Drop for Embedded {
    fn drop(&mut self) {
        self.core.stop();
        if let Some(syncer) = self.syncer.take() {
            let _ = syncer.join();
        }
        self.core.finish();
    }
}

/// Creates the home and takes its lock, refusing if another core holds it.
fn lock_home(home: &Path) -> Result<File> {
    std::fs::create_dir_all(home)?;
    let lock = File::options().create(true).truncate(false).write(true).open(home.join("core.lock"))?;
    if lock.try_lock().is_err() {
        return Err(Error::new("already_running", format!("a core is already running for {}", home.display())));
    }
    Ok(lock)
}

impl Server {
    /// Serves on a thread of its own. `on_exit` runs on that thread once serving ends, whoever stopped it.
    pub fn spawn(self, on_exit: impl FnOnce() + Send + 'static) -> Hosted {
        let handle = self.handle();
        let thread = std::thread::Builder::new()
            .name("furiten-core".into())
            .spawn(move || {
                let result = self.serve();
                on_exit();
                result
            })
            .expect("spawn the core thread");
        Hosted { handle, thread }
    }
}

/// A core served from a thread of this process ([`Server::spawn`]).
pub struct Hosted {
    handle: Handle,
    thread: std::thread::JoinHandle<Result<()>>,
}

impl Hosted {
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Stops the core and waits for it to push what's left.
    pub fn stop(self) -> Result<()> {
        self.handle.stop();
        self.thread.join().unwrap_or_else(|_| Err(Error::internal("the core thread panicked")))
    }
}

/// Stops a serving core from outside it, so the process hosting it can quit cleanly.
#[derive(Clone)]
pub struct Handle {
    core: Arc<Core>,
}

impl Handle {
    pub fn stop(&self) {
        self.core.stop();
    }
}

struct Core {
    #[cfg_attr(not(any(feature = "hub", feature = "web")), allow(dead_code))]
    me: Weak<Core>,
    home: PathBuf,
    /// Shared with the host while hosting, which serves devices' syncs from it.
    store: Arc<Mutex<Store>>,
    conns: Mutex<Vec<Arc<Conn>>>,
    /// When the debounced sync after a write is due.
    timer: Mutex<Option<Instant>>,
    wake: Condvar,
    sync_lock: Mutex<()>,
    sync_error: Mutex<Option<String>>,
    shutdown: AtomicBool,
    /// Set by the desktop app hosting the core: shows its window.
    open: OnceLock<Box<dyn Fn() + Send + Sync>>,
    /// Set by an [`Embedded`] host: told of every change, as socket clients are.
    on_change: OnceLock<Box<dyn Fn() + Send + Sync>>,
    #[cfg(feature = "hub")]
    net: Net,
    /// The live connection to a self-hosted hub, while signed in to one.
    #[cfg(feature = "hub")]
    link: Mutex<Option<Arc<HubLink>>>,
    /// Set by a host process that hosts devices (the desktop app).
    host_serves: OnceLock<()>,
    /// The host while this library is its own hub, and why starting it last failed.
    #[cfg(feature = "hub")]
    host: Mutex<Option<host::Running>>,
    #[cfg(feature = "hub")]
    host_error: Mutex<Option<String>>,
    /// Set by a host that serves web access, to the bundled web app's directory if it has one.
    web_host: OnceLock<Option<PathBuf>>,
    /// The HTTP server while web access runs, and why starting it last failed.
    #[cfg(feature = "web")]
    web: Mutex<Option<web::Running>>,
    #[cfg(feature = "web")]
    web_error: Mutex<Option<String>>,
    /// `/v1/events` streams, told of every change.
    #[cfg(feature = "web")]
    events: Mutex<Vec<std::sync::mpsc::Sender<()>>>,
}

/// One client's outgoing lines. A writer thread drains it so a client that doesn't read never blocks the core;
/// pending `changed` events coalesce into one.
#[derive(Default)]
struct Conn {
    out: Mutex<Outgoing>,
    ready: Condvar,
}

#[derive(Default)]
struct Outgoing {
    lines: VecDeque<String>,
    changed: bool,
    closed: bool,
}

impl Conn {
    fn out(&self) -> MutexGuard<'_, Outgoing> {
        self.out.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn send(&self, line: String) {
        self.out().lines.push_back(line);
        self.ready.notify_all();
    }

    fn changed(&self) {
        self.out().changed = true;
        self.ready.notify_all();
    }

    fn close(&self) {
        self.out().closed = true;
        self.ready.notify_all();
    }

    fn write_loop(&self, mut stream: UnixStream) {
        loop {
            let line = {
                let mut out = self.out();
                loop {
                    if let Some(line) = out.lines.pop_front() {
                        break line;
                    }
                    if std::mem::take(&mut out.changed) {
                        break CHANGED.to_string();
                    }
                    if out.closed {
                        return;
                    }
                    out = self.ready.wait(out).unwrap_or_else(|e| e.into_inner());
                }
            };
            if stream.write_all(format!("{line}\n").as_bytes()).is_err() {
                self.close();
                return;
            }
            self.ready.notify_all();
        }
    }

    /// Waits until every queued line has been written (or the connection died).
    fn flush(&self) {
        let mut out = self.out();
        while !out.lines.is_empty() && !out.closed {
            out = self.ready.wait(out).unwrap_or_else(|e| e.into_inner());
        }
    }
}

#[derive(serde::Deserialize)]
struct Request {
    #[serde(default)]
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

impl Core {
    fn new(home: &Path, store: Store, #[cfg(feature = "hub")] net: Net) -> Arc<Core> {
        let core = Arc::new_cyclic(|me| Core {
            me: me.clone(),
            home: home.to_path_buf(),
            store: Arc::new(Mutex::new(store)),
            conns: Mutex::default(),
            timer: Mutex::new(None),
            wake: Condvar::new(),
            sync_lock: Mutex::new(()),
            sync_error: Mutex::new(None),
            shutdown: AtomicBool::new(false),
            open: OnceLock::new(),
            on_change: OnceLock::new(),
            #[cfg(feature = "hub")]
            net,
            #[cfg(feature = "hub")]
            link: Mutex::new(None),
            host_serves: OnceLock::new(),
            #[cfg(feature = "hub")]
            host: Mutex::new(None),
            #[cfg(feature = "hub")]
            host_error: Mutex::new(None),
            web_host: OnceLock::new(),
            #[cfg(feature = "web")]
            web: Mutex::new(None),
            #[cfg(feature = "web")]
            web_error: Mutex::new(None),
            #[cfg(feature = "web")]
            events: Mutex::default(),
        });
        core.refresh_link();
        core
    }

    /// Once serving has stopped: one last push of what's left (failures wait for the next start), then the hub
    /// link closes.
    fn finish(&self) {
        #[cfg(feature = "web")]
        drop(self.web.lock().unwrap_or_else(|e| e.into_inner()).take());
        #[cfg(feature = "hub")]
        drop(self.host.lock().unwrap_or_else(|e| e.into_inner()).take());
        let pending = lock(&self.store).pending().is_ok_and(|o| !o.changes.is_empty());
        if pending && self.signed_in() {
            let _ = self.sync();
        }
        #[cfg(feature = "hub")]
        drop(self.link.lock().unwrap_or_else(|e| e.into_inner()).take());
    }

    /// One request line's reply, and whether it asked the core to shut down.
    fn handle(&self, line: &str) -> (Value, bool) {
        match serde_json::from_str::<Request>(line) {
            Ok(req) => {
                let stop = req.method == "shutdown";
                let reply = match self.dispatch(&req.method, req.params) {
                    Ok(result) => json!({ "id": req.id, "result": result }),
                    Err(error) => json!({ "id": req.id, "error": error }),
                };
                (reply, stop)
            }
            Err(e) => (json!({ "id": null, "error": Error::new("bad_request", e.to_string()) }), false),
        }
    }

    fn connection(self: Arc<Core>, stream: UnixStream) {
        let Ok(writer) = stream.try_clone() else { return };
        let conn = Arc::new(Conn::default());
        self.conns.lock().unwrap_or_else(|e| e.into_inner()).push(conn.clone());
        let writer_conn = conn.clone();
        let writer = std::thread::spawn(move || writer_conn.write_loop(writer));
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let (reply, stop) = self.handle(&line);
            conn.send(reply.to_string());
            if stop {
                conn.flush();
                self.stop();
                break;
            }
        }
        conn.close();
        let _ = writer.join();
        self.conns.lock().unwrap_or_else(|e| e.into_inner()).retain(|c| !Arc::ptr_eq(c, &conn));
    }

    fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.wake.notify_all();
        // Wake the accept loop so it sees the flag.
        let _ = UnixStream::connect(socket_path(&self.home));
    }

    fn dispatch(&self, method: &str, params: Value) -> Result<Value> {
        let result = match method {
            "status" => to_value(self.status()?),
            "shutdown" => to_value(Ack { ok: true }),
            "app.open" => match self.open.get() {
                Some(open) => {
                    open();
                    to_value(Ack { ok: true })
                }
                None => return Err(Error::new("unsupported", "this core isn't running in the desktop app")),
            },
            "decks.list" => to_value(self.store().decks()?),
            "cards.list" => to_value(self.store().cards(&parse(params)?)?),
            "cards.get" => to_value(self.store().card(&parse::<IdParams>(params)?.id)?),
            "tags.list" => to_value(self.store().tags(parse::<DeckFilter>(params)?.deck_id.as_deref())?),
            "review.next" => to_value(self.store().next_review(parse::<DeckFilter>(params)?.deck_id.as_deref())?),
            "stats" => to_value(self.store().stats(parse::<DeckFilter>(params)?.deck_id.as_deref())?),
            "sync.login" => {
                let params: Login = parse(params)?;
                let result = match params.hub {
                    Some(ticket) => to_value(self.login_hub(&ticket, params.name.as_deref())?),
                    None => to_value(self.login(params)?),
                };
                self.broadcast();
                result
            }
            "sync.now" => to_value(self.sync()?),
            "sync.logout" => to_value(self.logout()?),
            "host.status" => to_value(self.host_status()?),
            "host.enable" => to_value(self.host_enable()?),
            "host.disable" => to_value(self.host_disable()?),
            "host.pair" => to_value(self.host_pair()?),
            "host.remove" => to_value(self.host_remove(&parse::<DeviceQuery>(params)?.device)?),
            "web.status" => to_value(self.web_status()),
            "web.enable" => to_value(self.web_enable(parse(params)?)?),
            "web.disable" => to_value(self.web_disable()?),
            "import.mochi" => {
                let imported = self.import_mochi(parse(params)?)?;
                self.broadcast();
                self.schedule_sync();
                to_value(imported)
            }
            _ => {
                let result = self.write(method, params)?;
                self.broadcast();
                self.schedule_sync();
                result
            }
        };
        Ok(result)
    }

    fn write(&self, method: &str, params: Value) -> Result<Value> {
        let mut store = self.store();
        Ok(match method {
            "decks.create" => to_value(store.create_deck(parse(params)?)?),
            "decks.update" => to_value(store.update_deck(parse(params)?)?),
            "decks.delete" => {
                store.delete_deck(&parse::<IdParams>(params)?.id)?;
                to_value(Ack { ok: true })
            }
            "cards.create" if params.get("cards").is_some() => {
                to_value(Cards { cards: store.create_cards(parse::<NewCards>(params)?.cards)? })
            }
            "cards.create" => to_value(store.create_card(parse(params)?)?),
            "cards.update" => to_value(store.update_card(parse(params)?)?),
            "cards.delete" => {
                store.delete_card(&parse::<IdParams>(params)?.id)?;
                to_value(Ack { ok: true })
            }
            "review.answer" => to_value(store.answer(parse(params)?)?),
            _ => return Err(Error::new("unknown_method", format!("unknown method {method}"))),
        })
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        lock(&self.store)
    }

    /// Reads the export before taking the store, so a big file doesn't hold up other clients.
    #[cfg(feature = "import")]
    fn import_mochi(&self, params: ImportFile) -> Result<Imported> {
        let path = Path::new(&params.path);
        if !path.is_absolute() {
            return Err(Error::invalid("path must be absolute"));
        }
        let library = crate::import::mochi::read(path)?;
        self.store().import(library, params.parent_id.as_deref())
    }

    #[cfg(not(feature = "import"))]
    fn import_mochi(&self, _params: ImportFile) -> Result<Imported> {
        Err(Error::new("unsupported", "this build of furiten can't import"))
    }

    fn status(&self) -> Result<Status> {
        let store = self.store();
        let config = store.sync_config()?;
        Ok(Status {
            version: VERSION.to_string(),
            home: self.home.display().to_string(),
            signed_in: config.is_some(),
            last_sync: store.last_sync()?,
            connected: config.as_ref().and_then(|c| c.hub.as_ref()).map(|_| self.hub_connected()),
            server: config.map(|c| c.server),
            sync_error: self.sync_error.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            hosting: store.is_hosting(),
        })
    }

    fn signed_in(&self) -> bool {
        self.store().sync_config().is_ok_and(|c| c.is_some())
    }

    /// After a write: tell this core's clients, and devices it hosts.
    fn broadcast(&self) {
        self.broadcast_clients();
        #[cfg(feature = "hub")]
        self.host_changed();
    }

    fn broadcast_clients(&self) {
        for conn in self.conns.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            conn.changed();
        }
        if let Some(on_change) = self.on_change.get() {
            on_change();
        }
        #[cfg(feature = "web")]
        self.events.lock().unwrap_or_else(|e| e.into_inner()).retain(|tx| tx.send(()).is_ok());
    }

    fn schedule_sync(&self) {
        self.sync_at(Instant::now() + DEBOUNCE);
    }

    fn sync_at(&self, at: Instant) {
        *self.timer.lock().unwrap_or_else(|e| e.into_inner()) = Some(at);
        self.wake.notify_all();
    }

    /// Syncs 2 s after the last write and every 60 s, while signed in.
    fn sync_loop(&self) {
        let mut last = Instant::now();
        loop {
            {
                let mut timer = self.timer.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    if self.shutdown.load(Ordering::SeqCst) {
                        return;
                    }
                    let periodic = last + PERIOD;
                    let at = timer.map_or(periodic, |t| t.min(periodic));
                    let now = Instant::now();
                    if now >= at {
                        *timer = None;
                        break;
                    }
                    timer = self.wake.wait_timeout(timer, at - now).unwrap_or_else(|e| e.into_inner()).0;
                }
            }
            if self.signed_in() {
                let _ = self.sync();
            }
            last = Instant::now();
        }
    }

    /// One sync round, recording its error for `status` and telling clients when anything changed.
    fn sync(&self) -> Result<SyncResult> {
        let _one_at_a_time = self.sync_lock.lock().unwrap_or_else(|e| e.into_inner());
        let config = self.store().sync_config()?.ok_or_else(|| Error::new("not_signed_in", "run sync.login first"))?;
        let result = match &config.hub {
            #[cfg(feature = "hub")]
            Some(_) => match self.hub_link() {
                Some(link) => sync_once(&self.store, &*link),
                None => Err(Error::new("network", "couldn't start the connection to the hub")),
            },
            #[cfg(not(feature = "hub"))]
            Some(_) => Err(Error::new("unsupported", "this build of furiten can't sync with a hub")),
            None => sync_once(&self.store, &Http::from_config(&config)),
        };
        let error = result.as_ref().err().map(|e| e.message.clone());
        let error_changed = {
            let mut current = self.sync_error.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::replace(&mut *current, error.clone()) != error
        };
        let applied = result.as_ref().map_or(0, |(_, applied)| *applied);
        if applied > 0 || error_changed {
            self.broadcast();
        }
        result.map(|(r, _)| r)
    }

    /// Refuses to sign in while this library hosts devices: it syncs with one hub, and here it is one.
    fn not_hosting(&self) -> Result<()> {
        if self.store().is_hosting() {
            return Err(Error::new(
                "conflict",
                "this library hosts other devices; turn hosting off (furiten host off) to sync it elsewhere",
            ));
        }
        Ok(())
    }

    /// Stops syncing. Everything stays here, and a later sign-in sends all of it.
    fn logout(&self) -> Result<Ack> {
        self.store().clear_sync_config()?;
        *self.sync_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.refresh_link();
        self.broadcast_clients();
        Ok(Ack { ok: true })
    }

    fn login(&self, params: Login) -> Result<LoggedIn> {
        self.not_hosting()?;
        let server = params.server.trim().trim_end_matches('/').to_string();
        if !(server.starts_with("https://") || server.starts_with("http://")) {
            return Err(Error::invalid("server must be an http(s) URL"));
        }
        let (user_id, token) = match params.token {
            Some(token) => (token_user_id(&token)?, token.trim().to_string()),
            None => {
                let account = Http::new(&server, None).create_account()?;
                (account.user_id, account.token)
            }
        };
        let previous = self.store().sync_config()?;
        self.store().set_sync_config(&SyncConfig {
            server,
            token: token.clone(),
            user_id: user_id.clone(),
            hub: None,
        })?;
        self.refresh_link();
        // A rejected token undoes the login; any other failure (offline, say) is retried in the background.
        if let Err(e) = self.sync()
            && e.code == "unauthorized"
        {
            let mut store = self.store();
            match previous {
                Some(previous) => store.set_sync_config(&previous)?,
                None => store.clear_sync_config()?,
            }
            drop(store);
            self.refresh_link();
            *self.sync_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
            return Err(e);
        }
        Ok(LoggedIn { user_id, token })
    }

    /// Pairs with a self-hosted hub and syncs with it from now on.
    #[cfg(feature = "hub")]
    fn login_hub(&self, ticket: &str, name: Option<&str>) -> Result<HubLoggedIn> {
        self.not_hosting()?;
        let ticket: hub::Ticket = ticket.parse()?;
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => name.to_string(),
            None => hub::default_device_name(),
        };
        // One endpoint per key at a time: pair over the running link's, if there is one.
        let link = self.link.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let library_id = match link {
            Some(link) => link.pair(&ticket, &name)?,
            None => hub::pair(hub::load_or_create_key(&self.home.join("iroh.key"))?, &self.net, &ticket, &name)?,
        };
        self.store().set_sync_config(&SyncConfig::hub(ticket.addr, &library_id))?;
        *self.sync_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.refresh_link();
        // Failures are retried in the background: the link keeps trying to reach the hub.
        let _ = self.sync();
        Ok(HubLoggedIn { library_id })
    }

    #[cfg(not(feature = "hub"))]
    fn login_hub(&self, _ticket: &str, _name: Option<&str>) -> Result<HubLoggedIn> {
        Err(Error::new("unsupported", "this build of furiten can't sync with a hub"))
    }

    /// Starts, restarts or stops the hub link to match the sync config.
    #[cfg(feature = "hub")]
    fn refresh_link(&self) {
        drop(self.link.lock().unwrap_or_else(|e| e.into_inner()).take());
        let _ = self.hub_link();
    }

    #[cfg(not(feature = "hub"))]
    fn refresh_link(&self) {}

    /// The hub link, started if the core is signed in to a hub and it isn't running.
    #[cfg(feature = "hub")]
    fn hub_link(&self) -> Option<Arc<HubLink>> {
        let addr = self.store().sync_config().ok()??.hub?;
        let mut link = self.link.lock().unwrap_or_else(|e| e.into_inner());
        if link.is_none() {
            let started = hub::load_or_create_key(&self.home.join("iroh.key")).and_then(|key| {
                let me = self.me.clone();
                // Another device wrote, or the link (re)connected: pull now.
                HubLink::start(key, &self.net, &addr, move || {
                    if let Some(core) = me.upgrade() {
                        core.sync_at(Instant::now());
                    }
                })
            });
            match started {
                Ok(started) => *link = Some(Arc::new(started)),
                Err(e) => *self.sync_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.message),
            }
        }
        link.clone()
    }

    /// Serves again if web access was on when this host last ran.
    fn resume_web(&self) {
        #[cfg(feature = "web")]
        if web::load(&self.home).is_some_and(|c| c.enabled) {
            self.restart_web();
        }
    }

    /// (Re)starts the HTTP server to match `web.json`, if this host serves web access.
    #[cfg(feature = "web")]
    fn restart_web(&self) {
        let mut running = self.web.lock().unwrap_or_else(|e| e.into_inner());
        // Free the port before binding it again.
        drop(running.take());
        let config = web::load(&self.home).filter(|c| c.enabled);
        let started = match (config, self.me.upgrade(), self.web_host.get()) {
            (Some(config), Some(core), Some(_)) => Some(web::start(&core, &config)),
            _ => None,
        };
        *self.web_error.lock().unwrap_or_else(|e| e.into_inner()) =
            started.as_ref().and_then(|s| s.as_ref().err()).map(|e| e.message.clone());
        *running = started.and_then(Result::ok);
    }

    #[cfg(feature = "web")]
    fn web_status(&self) -> WebStatus {
        let running = self.web.lock().unwrap_or_else(|e| e.into_inner()).is_some();
        let mut error = self.web_error.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let config = web::load(&self.home);
        if config.as_ref().is_some_and(|c| c.enabled) && self.web_host.get().is_none() {
            error = Some("only the desktop app serves web access; open it to start".into());
        }
        web::status(config.as_ref(), running, error)
    }

    /// Turns web access on (or changes where it listens) and keeps it on across restarts. The token is made once
    /// and kept, so signed-in browsers stay signed in.
    #[cfg(feature = "web")]
    fn web_enable(&self, params: WebEnable) -> Result<WebStatus> {
        let previous = web::load(&self.home);
        let config = web::Config {
            enabled: true,
            bind: params
                .bind
                .or_else(|| previous.as_ref().map(|c| c.bind.clone()))
                .unwrap_or_else(|| web::DEFAULT_BIND.into()),
            port: params.port.or(previous.as_ref().map(|c| c.port)).unwrap_or(web::DEFAULT_PORT),
            token: match previous {
                Some(c) => c.token,
                None => web::new_token()?,
            },
        };
        web::save(&self.home, &config)?;
        self.restart_web();
        let status = self.web_status();
        match &status.error {
            Some(e) if self.web_host.get().is_some() => Err(Error::new("web_failed", e.clone())),
            _ => Ok(status),
        }
    }

    #[cfg(feature = "web")]
    fn web_disable(&self) -> Result<WebStatus> {
        if let Some(mut config) = web::load(&self.home) {
            config.enabled = false;
            web::save(&self.home, &config)?;
        }
        self.restart_web();
        Ok(self.web_status())
    }

    #[cfg(not(feature = "web"))]
    fn web_status(&self) -> WebStatus {
        WebStatus::default()
    }

    #[cfg(not(feature = "web"))]
    fn web_enable(&self, _params: WebEnable) -> Result<WebStatus> {
        Err(Error::new("unsupported", "this build of furiten can't serve web access"))
    }

    #[cfg(not(feature = "web"))]
    fn web_disable(&self) -> Result<WebStatus> {
        Ok(WebStatus::default())
    }

    #[cfg(not(feature = "hub"))]
    fn host_status(&self) -> Result<HostStatus> {
        Ok(HostStatus { enabled: self.store().is_hosting(), ..HostStatus::default() })
    }

    #[cfg(not(feature = "hub"))]
    fn host_enable(&self) -> Result<HostStatus> {
        Err(Error::new("unsupported", "this build of furiten can't host devices"))
    }

    #[cfg(not(feature = "hub"))]
    fn host_disable(&self) -> Result<HostStatus> {
        self.store().set_hosting(false)?;
        self.host_status()
    }

    #[cfg(not(feature = "hub"))]
    fn host_pair(&self) -> Result<PairTicket> {
        Err(Error::new("unsupported", "this build of furiten can't host devices"))
    }

    #[cfg(not(feature = "hub"))]
    fn host_remove(&self, _query: &str) -> Result<Device> {
        Err(Error::new("unsupported", "this build of furiten can't host devices"))
    }

    fn hub_connected(&self) -> bool {
        #[cfg(feature = "hub")]
        if let Some(link) = self.link.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            return link.is_connected();
        }
        false
    }
}

fn parse<T: DeserializeOwned>(params: Value) -> Result<T> {
    let params = if params.is_null() { json!({}) } else { params };
    serde_json::from_value(params).map_err(|e| Error::invalid(e.to_string()))
}

fn to_value(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests;
