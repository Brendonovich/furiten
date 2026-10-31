//! The one place the app talks to the core. Views and actions call `Core` and read the contract's types; nothing
//! else knows whether the answers come from the `recall core` daemon or from fixtures.

mod fixture;
mod types;

use recall_core::client::Client;

pub use types::*;

pub type Result<T> = std::result::Result<T, String>;

/// How many cards a deck's list asks for.
const LIST_LIMIT: u32 = 500;

const PENDING: &str = "Still connecting to the Recall core";

pub enum Core {
    /// The `recall core` daemon over its Unix socket.
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

fn err(e: recall_core::Error) -> String {
    e.message
}

impl Core {
    /// Connects to the local core at `$RECALL_HOME`, starting `recall core` if it isn't running.
    pub fn connect() -> Result<Core> {
        Client::connect_or_spawn(&recall_core::default_home()).map(Core::Live).map_err(err)
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

    /// The cards of a deck (or every deck), filtered to those whose Markdown contains `q` when it isn't empty.
    pub fn cards(&mut self, deck_id: Option<&str>, q: &str) -> Result<Vec<CardView>> {
        match self {
            Core::Live(c) => c
                .cards(&CardQuery {
                    deck_id: deck_id.map(String::from),
                    q: Some(q.to_string()).filter(|q| !q.is_empty()),
                    limit: Some(LIST_LIMIT),
                    ..CardQuery::default()
                })
                .map_err(err),
            Core::Fixture(f) => Ok(f.cards(deck_id, q)),
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
            Core::Live(c) => c.answer(&answer.card_id, answer.rating, answer.duration_ms).map_err(err),
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
