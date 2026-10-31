//! The SQLite store: decks, cards, reviews, a cached schedule per card, the outbox of unsynced changes (`dirty`)
//! and sync metadata.
//!
//! A store is either a device's or a hub's. A hub's store (see [`Store::open_hub`]) starts empty, answers sync
//! requests with [`Store::serve_sync`], and never clears `dirty`: there, a row's `dirty` is the `seq` of the change
//! that wrote its current value, since the outbox marker it comes from only ever increases.

mod queue;
mod schema;
mod syncing;

use std::path::Path;

use recall_srs::Scheduler;
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::model::{Card, CardView, Deck, DeckSummary, Id, Schedule, State, normalize_tags, ulid};
use crate::protocol::{CardQuery, CardUpdate, DeckUpdate, NewCard, NewDeck};
use crate::{Error, Result};

pub use syncing::{HubAddr, Outbox, SyncConfig};

pub struct Store {
    conn: Connection,
    scheduler: Scheduler,
    hub: bool,
    /// Fixed clock for tests.
    pub(crate) now: Option<i64>,
}

const DECK_COLS: &str = "d.id, d.name, d.parent_id, d.created_at, d.updated_at, d.deleted";
const CARD_VIEW: &str = "SELECT c.id, c.deck_id, c.content, c.tags, c.created_at, c.updated_at, c.deleted, \
     s.state, s.due, s.stability, s.difficulty, s.reps, s.lapses, s.last_review \
     FROM cards c JOIN schedules s ON s.card_id = c.id";
const DEFAULT_LIMIT: u32 = 50;

impl Store {
    /// Opens (creating and migrating if needed) the store at `path`. A new store gets the sample deck.
    pub fn open(path: &Path) -> Result<Store> {
        Store::init(open_file(path)?, false)
    }

    pub fn open_in_memory() -> Result<Store> {
        Store::init(Connection::open_in_memory()?, false)
    }

    /// Opens (creating if needed) a hub's store: no sample deck, and it serves sync requests. Refuses a device's
    /// store, and [`Store::open`] refuses a hub's.
    pub fn open_hub(path: &Path) -> Result<Store> {
        Store::init(open_file(path)?, true)
    }

    pub fn open_hub_in_memory() -> Result<Store> {
        Store::init(Connection::open_in_memory()?, true)
    }

    fn init(conn: Connection, hub: bool) -> Result<Store> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let mut store = Store { conn, scheduler: Scheduler::default(), hub, now: None };
        schema::migrate(&mut store, hub)?;
        let is_hub = syncing::meta::<bool>(&store.conn, "hub")?.unwrap_or(false);
        if is_hub != hub {
            let what = if is_hub { "a hub's store, not a device's" } else { "a device's store, not a hub's" };
            return Err(Error::new("wrong_store", format!("this is {what}")));
        }
        Ok(store)
    }

    pub fn is_hub(&self) -> bool {
        self.hub
    }

    pub(crate) fn now(&self) -> i64 {
        self.now.unwrap_or_else(crate::now_ms)
    }

    /// Fixes the clock (`None` restores the system clock). For tests, including other crates'.
    #[doc(hidden)]
    pub fn set_now(&mut self, now: Option<i64>) {
        self.now = now;
    }

    pub fn decks(&self) -> Result<Vec<DeckSummary>> {
        let now = self.now();
        let decks: Vec<Deck> = self
            .conn
            .prepare(&format!(
                "SELECT {DECK_COLS} FROM decks d WHERE d.deleted = 0 ORDER BY d.name COLLATE NOCASE, d.id"
            ))?
            .query_map([], deck_row)?
            .collect::<rusqlite::Result<_>>()?;
        let counts = queue::deck_counts(&self.conn, now)?;
        let own = |id: &str| counts.get(id).copied().unwrap_or_default();
        // Each deck's counts include its subdecks.
        let mut totals: Vec<[u64; 3]> = decks.iter().map(|d| own(&d.id)).collect();
        for (i, deck) in decks.iter().enumerate() {
            let mut seen = vec![i];
            let mut parent = deck.parent_id.as_deref();
            while let Some(p) = parent.and_then(|p| decks.iter().position(|d| d.id == p)) {
                if seen.contains(&p) {
                    break;
                }
                seen.push(p);
                let c = own(&deck.id);
                (0..3).for_each(|k| totals[p][k] += c[k]);
                parent = decks[p].parent_id.as_deref();
            }
        }
        Ok(decks
            .into_iter()
            .zip(totals)
            .map(|(deck, [cards, due, new])| DeckSummary { deck, cards, due, new })
            .collect())
    }

    pub fn deck(&self, id: &str) -> Result<Deck> {
        get_deck(&self.conn, id)?.filter(|d| !d.deleted).ok_or_else(|| Error::not_found("deck", id))
    }

    pub fn create_deck(&mut self, new: NewDeck) -> Result<Deck> {
        let name = deck_name(&new.name)?;
        if let Some(parent) = &new.parent_id {
            self.deck(parent)?;
        }
        let now = self.now();
        let deck =
            Deck { id: ulid(), name, parent_id: new.parent_id, created_at: now, updated_at: now, deleted: false };
        let tx = self.conn.transaction()?;
        put_deck(&tx, &deck, next_dirty(&tx)?)?;
        tx.commit()?;
        Ok(deck)
    }

    pub fn update_deck(&mut self, update: DeckUpdate) -> Result<Deck> {
        let mut deck = self.deck(&update.id)?;
        if let Some(name) = &update.name {
            deck.name = deck_name(name)?;
        }
        if let Some(parent_id) = update.parent_id {
            if let Some(parent) = &parent_id {
                self.deck(parent)?;
                if subtree(&self.conn, &deck.id)?.contains(parent) {
                    return Err(Error::invalid("a deck can't be moved inside itself"));
                }
            }
            deck.parent_id = parent_id;
        }
        deck.updated_at = bump(deck.updated_at, self.now());
        let tx = self.conn.transaction()?;
        put_deck(&tx, &deck, next_dirty(&tx)?)?;
        tx.commit()?;
        Ok(deck)
    }

    /// Soft-deletes the deck, its subdecks and their cards.
    pub fn delete_deck(&mut self, id: &str) -> Result<()> {
        self.deck(id)?;
        let now = self.now();
        let ids = serde_json::to_string(&subtree(&self.conn, id)?)?;
        let tx = self.conn.transaction()?;
        let dirty = next_dirty(&tx)?;
        for table in ["decks", "cards"] {
            let column = if table == "decks" { "id" } else { "deck_id" };
            tx.execute(
                &format!(
                    "UPDATE {table} SET deleted = 1, updated_at = MAX(?1, updated_at + 1), dirty = ?2 \
                     WHERE deleted = 0 AND {column} IN (SELECT value FROM json_each(?3))"
                ),
                params![now, dirty, ids],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn cards(&self, query: &CardQuery) -> Result<Vec<CardView>> {
        let scope = match &query.deck_id {
            Some(id) => {
                self.deck(id)?;
                Some(serde_json::to_string(&subtree(&self.conn, id)?)?)
            }
            None => None,
        };
        let sql = format!(
            "{CARD_VIEW} WHERE c.deleted = 0 \
             AND (?1 IS NULL OR c.deck_id IN (SELECT value FROM json_each(?1))) \
             AND (?2 IS NULL OR instr(lower(c.content), lower(?2)) > 0) \
             AND (?3 IS NULL OR EXISTS (SELECT 1 FROM json_each(c.tags) WHERE value = ?3)) \
             AND (?4 IS NULL OR s.due <= ?4) \
             ORDER BY c.created_at, c.id LIMIT ?5 OFFSET ?6"
        );
        let tag = query.tag.as_deref().map(crate::model::normalize_tag);
        let due_before = query.due.unwrap_or(false).then(|| self.now());
        let q = query.q.as_deref().filter(|q| !q.is_empty());
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
        let offset = query.offset.unwrap_or(0);
        let cards = self
            .conn
            .prepare(&sql)?
            .query_map(params![scope, q, tag, due_before, limit, offset], card_view_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(cards)
    }

    pub fn card(&self, id: &str) -> Result<CardView> {
        card_view(&self.conn, id)?.filter(|c| !c.card.deleted).ok_or_else(|| Error::not_found("card", id))
    }

    pub fn create_card(&mut self, new: NewCard) -> Result<CardView> {
        Ok(self.create_cards(vec![new])?.remove(0))
    }

    /// Creates all the cards or none.
    pub fn create_cards(&mut self, new: Vec<NewCard>) -> Result<Vec<CardView>> {
        for card in &new {
            self.deck(&card.deck_id)?;
        }
        let now = self.now();
        let tx = self.conn.transaction()?;
        let dirty = next_dirty(&tx)?;
        let mut ids = Vec::with_capacity(new.len());
        for (i, card) in new.into_iter().enumerate() {
            // A ms apart so a batch keeps its order in the new-card queue.
            let at = now + i as i64;
            let card = Card {
                id: ulid(),
                deck_id: card.deck_id,
                content: card.content,
                tags: normalize_tags(&card.tags),
                created_at: at,
                updated_at: at,
                deleted: false,
            };
            put_card(&tx, &card, dirty)?;
            queue::recompute(&tx, &self.scheduler, &card.id)?;
            ids.push(card.id);
        }
        tx.commit()?;
        ids.iter().map(|id| self.card(id)).collect()
    }

    pub fn update_card(&mut self, update: CardUpdate) -> Result<CardView> {
        let mut card = self.card(&update.id)?.card;
        if let Some(content) = update.content {
            card.content = content;
        }
        if let Some(tags) = update.tags {
            card.tags = normalize_tags(&tags);
        }
        if let Some(deck_id) = update.deck_id {
            self.deck(&deck_id)?;
            card.deck_id = deck_id;
        }
        card.updated_at = bump(card.updated_at, self.now());
        let tx = self.conn.transaction()?;
        put_card(&tx, &card, next_dirty(&tx)?)?;
        tx.commit()?;
        self.card(&card.id)
    }

    pub fn delete_card(&mut self, id: &str) -> Result<()> {
        let mut card = self.card(id)?.card;
        card.deleted = true;
        card.updated_at = bump(card.updated_at, self.now());
        let tx = self.conn.transaction()?;
        put_card(&tx, &card, next_dirty(&tx)?)?;
        tx.commit()?;
        Ok(())
    }
}

fn open_file(path: &Path) -> Result<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    Ok(Connection::open(path)?)
}

/// A local edit's `updated_at`: now, but always later than the version it replaces so it wins last-writer-wins
/// even against a clock that runs behind.
fn bump(previous: i64, now: i64) -> i64 {
    now.max(previous + 1)
}

fn deck_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::invalid("deck name can't be empty"));
    }
    Ok(name.to_string())
}

/// The next outbox marker. Rows with `dirty > 0` haven't been pushed; a sync clears rows up to the marker it read.
fn next_dirty(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("UPDATE meta SET value = value + 1 WHERE key = 'change_seq' RETURNING value", [], |r| r.get(0))?)
}

/// The deck and its live descendants.
fn subtree(conn: &Connection, id: &str) -> Result<Vec<Id>> {
    let ids = conn
        .prepare(
            "WITH RECURSIVE t(id) AS (SELECT ?1 UNION SELECT d.id FROM decks d JOIN t ON d.parent_id = t.id \
             WHERE d.deleted = 0) SELECT id FROM t",
        )?
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

fn get_card(conn: &Connection, id: &str) -> Result<Option<Card>> {
    Ok(conn
        .query_row(
            "SELECT id, deck_id, content, tags, created_at, updated_at, deleted FROM cards WHERE id = ?1",
            [id],
            card_row,
        )
        .optional()?)
}

fn get_deck(conn: &Connection, id: &str) -> Result<Option<Deck>> {
    Ok(conn.query_row(&format!("SELECT {DECK_COLS} FROM decks d WHERE d.id = ?1"), [id], deck_row).optional()?)
}

fn card_view(conn: &Connection, id: &str) -> Result<Option<CardView>> {
    Ok(conn.query_row(&format!("{CARD_VIEW} WHERE c.id = ?1"), [id], card_view_row).optional()?)
}

fn put_deck(conn: &Connection, deck: &Deck, dirty: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO decks (id, name, parent_id, created_at, updated_at, deleted, dirty) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT (id) DO UPDATE SET name = excluded.name, \
         parent_id = excluded.parent_id, created_at = excluded.created_at, updated_at = excluded.updated_at, \
         deleted = excluded.deleted, dirty = excluded.dirty",
        params![deck.id, deck.name, deck.parent_id, deck.created_at, deck.updated_at, deck.deleted, dirty],
    )?;
    Ok(())
}

fn put_card(conn: &Connection, card: &Card, dirty: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO cards (id, deck_id, content, tags, created_at, updated_at, deleted, dirty) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT (id) DO UPDATE SET deck_id = excluded.deck_id, \
         content = excluded.content, tags = excluded.tags, created_at = excluded.created_at, \
         updated_at = excluded.updated_at, deleted = excluded.deleted, dirty = excluded.dirty",
        params![
            card.id,
            card.deck_id,
            card.content,
            serde_json::to_string(&card.tags)?,
            card.created_at,
            card.updated_at,
            card.deleted,
            dirty
        ],
    )?;
    Ok(())
}

fn deck_row(row: &Row) -> rusqlite::Result<Deck> {
    Ok(Deck {
        id: row.get(0)?,
        name: row.get(1)?,
        parent_id: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        deleted: row.get(5)?,
    })
}

fn card_row(row: &Row) -> rusqlite::Result<Card> {
    let tags: String = row.get(3)?;
    Ok(Card {
        id: row.get(0)?,
        deck_id: row.get(1)?,
        content: row.get(2)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        deleted: row.get(6)?,
    })
}

fn card_view_row(row: &Row) -> rusqlite::Result<CardView> {
    let state: String = row.get(7)?;
    let schedule = Schedule {
        state: State::parse(&state).unwrap_or(State::New),
        due: row.get(8)?,
        stability: row.get(9)?,
        difficulty: row.get(10)?,
        reps: row.get(11)?,
        lapses: row.get(12)?,
        last_review: row.get(13)?,
    };
    Ok(CardView::new(card_row(row)?, schedule))
}

#[cfg(test)]
mod tests;
