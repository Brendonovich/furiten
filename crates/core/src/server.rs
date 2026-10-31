//! The local core: owns the store and sync, and serves the JSON-lines protocol on `$RECALL_HOME/core.sock`.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
#[cfg(feature = "hub")]
use std::sync::Weak;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
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

/// Runs the core in the foreground until a client sends `shutdown`.
pub fn run(home: &Path) -> Result<()> {
    Server::bind(home)?.serve()
}

#[cfg(feature = "hub")]
use crate::hub::{self, HubLink, Net};

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

    /// [`Server::bind`] with explicit network settings for a hub link, instead of `RECALL_NET` / `RECALL_RELAY`.
    #[cfg(feature = "hub")]
    pub fn bind_with_net(home: &Path, net: Net) -> Result<Server> {
        Server::bind_inner(home, net)
    }

    fn bind_inner(home: &Path, #[cfg(feature = "hub")] net: Net) -> Result<Server> {
        std::fs::create_dir_all(home)?;
        let lock = File::options().create(true).truncate(false).write(true).open(home.join("core.lock"))?;
        if lock.try_lock().is_err() {
            return Err(Error::new("already_running", format!("a core is already running for {}", home.display())));
        }
        let socket = socket_path(home);
        if socket.exists() {
            std::fs::remove_file(&socket)?;
        }
        let store = Store::open(&home.join("recall.db"))?;
        let listener = UnixListener::bind(&socket)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        #[cfg_attr(not(feature = "hub"), allow(unused_variables))]
        let core = Arc::new_cyclic(|me| Core {
            #[cfg(feature = "hub")]
            me: me.clone(),
            home: home.to_path_buf(),
            store: Mutex::new(store),
            conns: Mutex::default(),
            timer: Mutex::new(None),
            wake: Condvar::new(),
            sync_lock: Mutex::new(()),
            sync_error: Mutex::new(None),
            shutdown: AtomicBool::new(false),
            #[cfg(feature = "hub")]
            net,
            #[cfg(feature = "hub")]
            link: Mutex::new(None),
        });
        core.refresh_link();
        Ok(Server { core, listener, _lock: lock })
    }

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
        // Last chance to push what's left; failures just wait for the next start.
        let pending = lock(&self.core.store).pending().is_ok_and(|o| !o.changes.is_empty());
        if pending && self.core.signed_in() {
            let _ = self.core.sync();
        }
        #[cfg(feature = "hub")]
        drop(self.core.link.lock().unwrap_or_else(|e| e.into_inner()).take());
        Ok(())
    }
}

struct Core {
    #[cfg(feature = "hub")]
    me: Weak<Core>,
    home: PathBuf,
    store: Mutex<Store>,
    conns: Mutex<Vec<Arc<Conn>>>,
    /// When the debounced sync after a write is due.
    timer: Mutex<Option<Instant>>,
    wake: Condvar,
    sync_lock: Mutex<()>,
    sync_error: Mutex<Option<String>>,
    shutdown: AtomicBool,
    #[cfg(feature = "hub")]
    net: Net,
    /// The live connection to a self-hosted hub, while signed in to one.
    #[cfg(feature = "hub")]
    link: Mutex<Option<Arc<HubLink>>>,
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
            let (reply, stop) = match serde_json::from_str::<Request>(&line) {
                Ok(req) => {
                    let stop = req.method == "shutdown";
                    let reply = match self.dispatch(&req.method, req.params) {
                        Ok(result) => json!({ "id": req.id, "result": result }),
                        Err(error) => json!({ "id": req.id, "error": error }),
                    };
                    (reply, stop)
                }
                Err(e) => (json!({ "id": null, "error": Error::new("bad_request", e.to_string()) }), false),
            };
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
            "decks.list" => to_value(self.store().decks()?),
            "cards.list" => to_value(self.store().cards(&parse(params)?)?),
            "cards.get" => to_value(self.store().card(&parse::<IdParams>(params)?.id)?),
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
        })
    }

    fn signed_in(&self) -> bool {
        self.store().sync_config().is_ok_and(|c| c.is_some())
    }

    fn broadcast(&self) {
        for conn in self.conns.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            conn.changed();
        }
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
            Some(_) => Err(Error::new("unsupported", "this build of recall can't sync with a hub")),
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

    fn login(&self, params: Login) -> Result<LoggedIn> {
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
        Err(Error::new("unsupported", "this build of recall can't sync with a hub"))
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
