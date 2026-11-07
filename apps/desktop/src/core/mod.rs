//! The one place the app talks to the core. Views and actions call `Core` and read the contract's types; nothing
//! else knows whether the answers come from the core or from fixtures.
//!
//! The app hosts the core: it serves the socket the CLI (and anything else) talks to, on a thread of its own, and the
//! app is a client of it like any other.

mod fixture;
mod types;

use recall_core::client::Client;
use recall_core::server::{Hosted, Server};

pub use types::*;

pub type Result<T> = std::result::Result<T, String>;

/// How many cards a deck's list asks for.
const LIST_LIMIT: u32 = 500;

const PENDING: &str = "Still connecting to the Recall core";

pub enum Core {
    /// The core over its Unix socket: the one this app hosts, or rarely another process's.
    Live(Client),
    /// Sample data in memory (`--fixtures`, scenes and tests).
    Fixture(fixture::Fixture),
    /// Not connected yet: the window is up and the connection is being made on another thread. Every call fails.
    Pending,
}

/// Which fixture store to start from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fixtures {
    FirstRun,
    LivedIn,
    CaughtUp,
}

/// Who already serves the core for `$RECALL_HOME`, checked before a window opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Running {
    Nothing,
    /// Another Recall app, which has just been asked to show its window.
    App,
    /// A process with no window to show: the CLI hosting the core because it found no app.
    Windowless,
}

/// Finds a core already serving `$RECALL_HOME`, and if another Recall app hosts it, asks it to show its window.
pub fn running() -> Running {
    let Ok(mut client) = Client::connect(&recall_core::default_home()) else { return Running::Nothing };
    match client.open_app() {
        Ok(()) => Running::App,
        Err(_) => Running::Windowless,
    }
}

/// The built web app this app serves with web access: `$RECALL_WEB_DIR`, else `web` in the app bundle's resources,
/// else (in a checkout) `apps/web/dist`. Without one, web access serves just the API.
fn web_dir() -> Option<std::path::PathBuf> {
    let has_app = |d: &std::path::Path| d.join("index.html").is_file();
    if let Some(dir) = std::env::var_os("RECALL_WEB_DIR").map(std::path::PathBuf::from) {
        return Some(dir).filter(|d| has_app(d));
    }
    let bundled = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.parent()?.join("Resources/web")));
    let checkout = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/dist");
    bundled.into_iter().chain([checkout]).find(|d| has_app(d))
}

fn err(e: recall_core::Error) -> String {
    e.message
}

impl Core {
    /// Hosts the core for `$RECALL_HOME` in this process and connects to it. `open` runs when a client asks for the
    /// window (`app.open`), `exited` once the core stops (a client sent `shutdown`, say). When another process
    /// already hosts it, connects to that one and hosts nothing.
    pub fn host(
        open: impl Fn() + Send + Sync + 'static,
        exited: impl FnOnce() + Send + 'static,
    ) -> Result<(Core, Option<Hosted>)> {
        let home = recall_core::default_home();
        let hosted = match Server::bind(&home) {
            Ok(server) => {
                server.on_open(open);
                server.serve_web(web_dir());
                server.host_devices();
                Some(server.spawn(exited))
            }
            Err(e) if e.code == "already_running" => None,
            Err(e) => return Err(err(e)),
        };
        match Client::connect(&home) {
            Ok(client) => Ok((Core::Live(client), hosted)),
            Err(e) => {
                if let Some(hosted) = hosted {
                    let _ = hosted.stop();
                }
                Err(err(e))
            }
        }
    }

    pub fn fixtures(which: Fixtures) -> Core {
        Core::Fixture(match which {
            Fixtures::FirstRun => fixture::Fixture::first_run(),
            Fixtures::LivedIn => fixture::Fixture::lived_in(),
            Fixtures::CaughtUp => fixture::Fixture::caught_up(),
        })
    }

    pub fn is_fixture(&self) -> bool {
        matches!(self, Core::Fixture(_))
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, Core::Pending)
    }

    /// Calls `changed` from another thread whenever the core pushes `{ event: "changed" }`.
    pub fn watch(&self, changed: impl Fn() + Send + 'static) {
        match self {
            Core::Live(client) => match client.events() {
                Ok(events) => {
                    std::thread::Builder::new()
                        .name("recall-events".into())
                        .spawn(move || events.for_each(|_| changed()))
                        .expect("spawn the events thread");
                }
                Err(e) => eprintln!("recall: not watching for changes: {}", e.message),
            },
            // Fixtures only change through this app, which refreshes after its own writes.
            Core::Fixture(_) | Core::Pending => drop(changed),
        }
    }

    pub fn status(&mut self) -> Result<Status> {
        match self {
            Core::Live(c) => c.status().map_err(err),
            Core::Fixture(f) => Ok(f.status()),
            Core::Pending => Err(PENDING.into()),
        }
    }

    pub fn decks(&mut self) -> Result<Vec<DeckSummary>> {
        match self {
            Core::Live(c) => c.decks().map_err(err),
            Core::Fixture(f) => Ok(f.decks()),
            Core::Pending => Err(PENDING.into()),
        }
    }

    pub fn create_deck(&mut self, name: &str) -> Result<Deck> {
        match self {
            Core::Live(c) => c.create_deck(name, None).map_err(err),
            Core::Fixture(f) => f.create_deck(name),
            Core::Pending => Err(PENDING.into()),
        }
    }

    /// The cards of a deck (or every deck), filtered to those whose Markdown contains `q` when it isn't empty, in
    /// `order`.
    pub fn cards(&mut self, deck_id: Option<&str>, q: &str, order: Order) -> Result<Vec<CardView>> {
        match self {
            Core::Live(c) => c
                .cards(&CardQuery {
                    deck_id: deck_id.map(String::from),
                    q: Some(q.to_string()).filter(|q| !q.is_empty()),
                    limit: Some(LIST_LIMIT),
                    sort: Some(order.by),
                    desc: order.desc.then_some(true),
                    seed: (order.by == CardSort::Random).then_some(order.seed),
                    ..CardQuery::default()
                })
                .map_err(err),
            Core::Fixture(f) => Ok(f.cards(deck_id, q, order)),
            Core::Pending => Err(PENDING.into()),
        }
    }

    pub fn card(&mut self, id: &str) -> Result<CardView> {
        match self {
            Core::Live(c) => c.card(id).map_err(err),
            Core::Fixture(f) => f.card(id),
            Core::Pending => Err(PENDING.into()),
        }
    }

    /// Every tag in use, those used in `deck_id` (or its subdecks) first, each part the most recently used first.
    pub fn tags(&mut self, deck_id: &str) -> Result<Vec<TagSummary>> {
        match self {
            Core::Live(c) => c.tags(Some(deck_id)).map_err(err),
            Core::Fixture(f) => Ok(f.tags(deck_id)),
            Core::Pending => Err(PENDING.into()),
        }
    }

    pub fn create_card(&mut self, new: NewCard) -> Result<CardView> {
        match self {
            Core::Live(c) => c.create_card(&new).map_err(err),
            Core::Fixture(f) => f.create_card(new),
            Core::Pending => Err(PENDING.into()),
        }
    }

    pub fn update_card(&mut self, update: CardUpdate) -> Result<CardView> {
        match self {
            Core::Live(c) => c.update_card(&update).map_err(err),
            Core::Fixture(f) => f.update_card(update),
            Core::Pending => Err(PENDING.into()),
        }
    }

    pub fn delete_card(&mut self, id: &str) -> Result<()> {
        match self {
            Core::Live(c) => c.delete_card(id).map_err(err),
            Core::Fixture(f) => f.delete_card(id),
            Core::Pending => Err(PENDING.into()),
        }
    }

    pub fn next_review(&mut self, deck_id: Option<&str>) -> Result<NextReview> {
        match self {
            Core::Live(c) => c.next_review(deck_id).map_err(err),
            Core::Fixture(f) => Ok(f.next_review(deck_id)),
            Core::Pending => Err(PENDING.into()),
        }
    }

    pub fn answer(&mut self, answer: Answer) -> Result<CardView> {
        match self {
            Core::Live(c) => c.answer(&answer.card_id, answer.remembered, answer.duration_ms).map_err(err),
            Core::Fixture(f) => f.answer(answer),
            Core::Pending => Err(PENDING.into()),
        }
    }

    /// Signs in to a sync server (creating an account), returning the user id.
    pub fn login(&mut self, server: &str) -> Result<String> {
        match self {
            Core::Live(c) => c.login(server, None).map(|l| l.user_id).map_err(err),
            Core::Fixture(_) => Err(format!("Couldn’t sign in to {server}: the app is running on sample data")),
            Core::Pending => Err(PENDING.into()),
        }
    }

    /// Imports a Mochi export as new top-level decks. The core reads the file.
    pub fn import_mochi(&mut self, path: &std::path::Path) -> Result<Imported> {
        match self {
            Core::Live(c) => c.import_mochi(path, None).map_err(err),
            Core::Fixture(_) => Err("Importing isn’t available on sample data".into()),
            Core::Pending => Err(PENDING.into()),
        }
    }

    pub fn web_status(&mut self) -> Result<WebStatus> {
        match self {
            Core::Live(c) => c.web_status().map_err(err),
            Core::Fixture(_) | Core::Pending => Ok(WebStatus::default()),
        }
    }

    /// Turns web access on or off.
    pub fn set_web(&mut self, on: bool) -> Result<WebStatus> {
        match self {
            Core::Live(c) if on => c.web_enable(&WebEnable::default()).map_err(err),
            Core::Live(c) => c.web_disable().map_err(err),
            Core::Fixture(_) => Err("Web access isn’t available on sample data".into()),
            Core::Pending => Err(PENDING.into()),
        }
    }

    pub fn host_status(&mut self) -> Result<HostStatus> {
        match self {
            Core::Live(c) => c.host_status().map_err(err),
            Core::Fixture(_) | Core::Pending => Ok(HostStatus::default()),
        }
    }

    /// Turns hosting phones on or off.
    pub fn set_host(&mut self, on: bool) -> Result<HostStatus> {
        match self {
            Core::Live(c) if on => c.host_enable().map_err(err),
            Core::Live(c) => c.host_disable().map_err(err),
            Core::Fixture(_) => Err("Pairing a phone isn’t available on sample data".into()),
            Core::Pending => Err(PENDING.into()),
        }
    }

    /// A one-time ticket for a phone to scan, turning hosting on.
    pub fn host_pair(&mut self) -> Result<PairTicket> {
        match self {
            Core::Live(c) => c.host_pair().map_err(err),
            Core::Fixture(_) => Err("Pairing a phone isn’t available on sample data".into()),
            Core::Pending => Err(PENDING.into()),
        }
    }

    pub fn host_remove(&mut self, device: &str) -> Result<()> {
        match self {
            Core::Live(c) => c.host_remove(device).map(drop).map_err(err),
            Core::Fixture(_) => Err("Pairing a phone isn’t available on sample data".into()),
            Core::Pending => Err(PENDING.into()),
        }
    }

    pub fn sync_now(&mut self) -> Result<()> {
        match self {
            Core::Live(c) => c.sync_now().map(drop).map_err(err),
            Core::Fixture(_) => Err("Sync isn’t available on sample data".into()),
            Core::Pending => Err(PENDING.into()),
        }
    }
}

#[cfg(test)]
mod tests;
