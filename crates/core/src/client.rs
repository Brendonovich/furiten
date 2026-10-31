//! A blocking client for the local core.
//!
//! ```no_run
//! # fn main() -> recall_core::Result<()> {
//! use recall_core::client::{Client, Event};
//!
//! let home = recall_core::default_home();
//! let mut client = Client::connect_or_spawn(&home)?; // starts `recall core` if it isn't running
//! let next = client.next_review(None)?;
//! if let Some(card) = next.card {
//!     println!("{}", card.front());
//!     client.answer(&card.card.id, 3, Some(4_000))?;
//! }
//!
//! // Events come on their own connection; read them on another thread and refetch on each one.
//! let events = client.events()?;
//! std::thread::spawn(move || {
//!     for event in events {
//!         assert_eq!(event, Event::Changed);
//!         // refetch
//!     }
//! });
//! # Ok(()) }
//! ```

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::model::{CardView, Deck, DeckSummary};
use crate::protocol::*;
use crate::server::socket_path;
use crate::{Error, Result};

const SPAWN_TIMEOUT: Duration = Duration::from_secs(10);

/// One connection to the core. Calls are sequential; use one `Client` per thread (they're cheap).
pub struct Client {
    home: PathBuf,
    reader: BufReader<UnixStream>,
    next_id: u64,
}

impl Client {
    /// Connects to a running core.
    pub fn connect(home: &Path) -> Result<Client> {
        let stream = UnixStream::connect(socket_path(home))
            .map_err(|e| Error::new("core_unavailable", format!("no core at {}: {e}", socket_path(home).display())))?;
        Ok(Client { home: home.to_path_buf(), reader: BufReader::new(stream), next_id: 0 })
    }

    /// Connects, starting `recall core` in the background first if it isn't running. The binary is `$RECALL_BIN`,
    /// else `recall` next to the current executable, else `recall` on `PATH`.
    pub fn connect_or_spawn(home: &Path) -> Result<Client> {
        if let Ok(client) = Client::connect(home) {
            return Ok(client);
        }
        spawn_core(home)?;
        let started = Instant::now();
        loop {
            match Client::connect(home) {
                Ok(client) => return Ok(client),
                Err(e) if started.elapsed() > SPAWN_TIMEOUT => {
                    return Err(Error::new(
                        "core_unavailable",
                        format!(
                            "started recall core but it didn't answer ({}); see {}",
                            e.message,
                            log_path(home).display()
                        ),
                    ));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(25)),
            }
        }
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Opens a second connection that yields the core's `changed` events.
    pub fn events(&self) -> Result<Events> {
        Events::connect(&self.home)
    }

    /// Sends any method with any params. The typed helpers below cover every method in the contract.
    pub fn call<R: DeserializeOwned>(&mut self, method: &str, params: impl Serialize) -> Result<R> {
        self.next_id += 1;
        let id = self.next_id;
        let line = json!({ "id": id, "method": method, "params": params }).to_string() + "\n";
        let gone = |e: std::io::Error| Error::new("core_unavailable", format!("lost the core: {e}"));
        self.reader.get_mut().write_all(line.as_bytes()).map_err(gone)?;
        let mut buf = String::new();
        loop {
            buf.clear();
            if self.reader.read_line(&mut buf).map_err(gone)? == 0 {
                return Err(Error::new("core_unavailable", "the core closed the connection"));
            }
            let mut reply: Value = serde_json::from_str(&buf).map_err(Error::internal)?;
            if reply.get("event").is_some() || reply.get("id") != Some(&json!(id)) {
                continue;
            }
            if let Some(error) = reply.get_mut("error") {
                return Err(serde_json::from_value(error.take()).map_err(Error::internal)?);
            }
            let result = reply.get_mut("result").map(Value::take).unwrap_or(Value::Null);
            return serde_json::from_value(result).map_err(Error::internal);
        }
    }

    pub fn status(&mut self) -> Result<Status> {
        self.call("status", json!({}))
    }

    pub fn decks(&mut self) -> Result<Vec<DeckSummary>> {
        self.call("decks.list", json!({}))
    }

    pub fn create_deck(&mut self, name: &str, parent_id: Option<&str>) -> Result<Deck> {
        self.call("decks.create", NewDeck { name: name.to_string(), parent_id: parent_id.map(str::to_string) })
    }

    pub fn update_deck(&mut self, update: &DeckUpdate) -> Result<Deck> {
        self.call("decks.update", update)
    }

    /// Soft-deletes the deck, its subdecks and their cards.
    pub fn delete_deck(&mut self, id: &str) -> Result<()> {
        self.call::<Ack>("decks.delete", IdParams { id: id.to_string() }).map(drop)
    }

    pub fn cards(&mut self, query: &CardQuery) -> Result<Vec<CardView>> {
        self.call("cards.list", query)
    }

    pub fn card(&mut self, id: &str) -> Result<CardView> {
        self.call("cards.get", IdParams { id: id.to_string() })
    }

    pub fn create_card(&mut self, card: &NewCard) -> Result<CardView> {
        self.call("cards.create", card)
    }

    /// Creates all the cards or none.
    pub fn create_cards(&mut self, cards: &[NewCard]) -> Result<Vec<CardView>> {
        self.call::<Cards>("cards.create", json!({ "cards": cards })).map(|c| c.cards)
    }

    pub fn update_card(&mut self, update: &CardUpdate) -> Result<CardView> {
        self.call("cards.update", update)
    }

    pub fn delete_card(&mut self, id: &str) -> Result<()> {
        self.call::<Ack>("cards.delete", IdParams { id: id.to_string() }).map(drop)
    }

    /// The next card to review (due first, then new) and what's left, optionally within a deck and its subdecks.
    pub fn next_review(&mut self, deck_id: Option<&str>) -> Result<NextReview> {
        self.call("review.next", DeckFilter { deck_id: deck_id.map(str::to_string) })
    }

    /// Grades a card: 1 again, 2 hard, 3 good, 4 easy. Returns it with its new schedule.
    pub fn answer(&mut self, card_id: &str, rating: u8, duration_ms: Option<i64>) -> Result<CardView> {
        self.call("review.answer", Answer { card_id: card_id.to_string(), rating, duration_ms })
    }

    pub fn stats(&mut self, deck_id: Option<&str>) -> Result<Stats> {
        self.call("stats", DeckFilter { deck_id: deck_id.map(str::to_string) })
    }

    /// Signs in to a sync server, creating an account when `token` is `None`.
    pub fn login(&mut self, server: &str, token: Option<&str>) -> Result<LoggedIn> {
        self.call("sync.login", Login { server: server.to_string(), token: token.map(str::to_string) })
    }

    pub fn sync_now(&mut self) -> Result<SyncResult> {
        self.call("sync.now", json!({}))
    }

    /// Stops the core. It replies before exiting.
    pub fn shutdown(&mut self) -> Result<()> {
        self.call::<Ack>("shutdown", json!({})).map(drop)
    }

    /// Finds a live deck by id, else by name (case-insensitive). With `create`, a missing name becomes a new
    /// top-level deck.
    pub fn resolve_deck(&mut self, name_or_id: &str, create: bool) -> Result<Deck> {
        let decks = self.decks()?;
        if let Some(d) = decks.iter().find(|d| d.deck.id == name_or_id) {
            return Ok(d.deck.clone());
        }
        let named: Vec<&DeckSummary> =
            decks.iter().filter(|d| d.deck.name.eq_ignore_ascii_case(name_or_id.trim())).collect();
        match named.as_slice() {
            [one] => Ok(one.deck.clone()),
            [] if create => self.create_deck(name_or_id, None),
            [] => Err(Error::not_found("deck", name_or_id)),
            _ => Err(Error::invalid(format!("more than one deck is named {name_or_id}; use its id"))),
        }
    }
}

/// Something the core pushes to every connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// Data changed (a write from any client, or sync): refetch what you show.
    Changed,
}

/// A connection that only reads events. Iterating blocks until the next event and ends when the core goes away.
pub struct Events {
    reader: BufReader<UnixStream>,
}

impl Events {
    pub fn connect(home: &Path) -> Result<Events> {
        Ok(Events { reader: Client::connect(home)?.reader })
    }
}

impl Iterator for Events {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        let mut buf = String::new();
        loop {
            buf.clear();
            if self.reader.read_line(&mut buf).ok()? == 0 {
                return None;
            }
            let Ok(line) = serde_json::from_str::<Value>(&buf) else { continue };
            if line.get("event").and_then(Value::as_str) == Some("changed") {
                return Some(Event::Changed);
            }
        }
    }
}

fn log_path(home: &Path) -> PathBuf {
    home.join("core.log")
}

fn spawn_core(home: &Path) -> Result<()> {
    let bin = find_recall().ok_or_else(|| {
        Error::new("core_unavailable", "can't find the recall binary; put it on PATH or set RECALL_BIN")
    })?;
    std::fs::create_dir_all(home)?;
    let log = std::fs::OpenOptions::new().create(true).append(true).open(log_path(home))?;
    let mut child = Command::new(&bin)
        .arg("core")
        .env("RECALL_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        // Its own process group, so Ctrl-C in the terminal that started it doesn't stop it.
        .process_group(0)
        .spawn()
        .map_err(|e| Error::new("core_unavailable", format!("couldn't start {}: {e}", bin.display())))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

fn find_recall() -> Option<PathBuf> {
    let env = std::env::var_os("RECALL_BIN").map(PathBuf::from);
    let sibling = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("recall")));
    let on_path = std::env::var_os("PATH")
        .and_then(|path| std::env::split_paths(&path).map(|dir| dir.join("recall")).find(|p| p.is_file()));
    [env, sibling, on_path].into_iter().flatten().find(|p| p.is_file())
}
