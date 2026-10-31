//! The store's side of sync: the outbox, merging incoming changes, sync metadata, and a hub's side of a sync.

use std::collections::{BTreeSet, HashSet};
use std::net::SocketAddr;

use rusqlite::{Connection, OptionalExtension, params};

use super::{Store, card_row, deck_row, get_card, get_deck, next_dirty, put_card, put_deck, queue};
use crate::model::{Card, Change, Review, normalize_tags};
use crate::protocol::{SyncRequest, SyncResponse};
use crate::{Error, Result};

/// Where a device syncs. For Recall Cloud, `server` is its URL and `token` the account's; for a self-hosted hub,
/// `server` is `hub:<endpoint id>`, `token` is empty, `user_id` is the hub's library id and `hub` says how to reach
/// it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncConfig {
    pub server: String,
    pub token: String,
    pub user_id: String,
    pub hub: Option<HubAddr>,
}

impl SyncConfig {
    pub fn hub(addr: HubAddr, library_id: &str) -> SyncConfig {
        SyncConfig {
            server: format!("hub:{}", addr.id),
            token: String::new(),
            user_id: library_id.to_string(),
            hub: Some(addr),
        }
    }
}

/// How to reach a hub: its endpoint id (hex), relay and direct addresses, as its pairing ticket gave them.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HubAddr {
    pub id: String,
    #[serde(default)]
    pub relay_url: Option<String>,
    #[serde(default)]
    pub direct_addrs: Vec<SocketAddr>,
}

/// Local changes not yet pushed. `mark` is passed back to [`Store::finish_sync`] so changes made while the request
/// was in flight stay in the outbox.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outbox {
    pub changes: Vec<Change>,
    pub mark: i64,
}

impl Store {
    pub fn device_id(&self) -> Result<String> {
        Ok(meta(&self.conn, "device_id")?.unwrap_or_default())
    }

    pub fn sync_config(&self) -> Result<Option<SyncConfig>> {
        let get = |key| meta::<String>(&self.conn, key);
        let hub = get("hub_addr")?.and_then(|json| serde_json::from_str(&json).ok());
        Ok(match (get("server")?, get("token")?, get("user_id")?) {
            (Some(server), Some(token), Some(user_id)) => Some(SyncConfig { server, token, user_id, hub }),
            _ => None,
        })
    }

    /// Signs in. Switching to a different server or account puts everything back in the outbox and resets the
    /// cursor, so the new account gets all local data.
    pub fn set_sync_config(&mut self, config: &SyncConfig) -> Result<()> {
        let previous = self.sync_config()?;
        let same_account = previous.as_ref().is_some_and(|p| p.server == config.server && p.user_id == config.user_id);
        let tx = self.conn.transaction()?;
        if !same_account {
            let dirty = super::next_dirty(&tx)?;
            for table in ["decks", "cards", "reviews"] {
                tx.execute(&format!("UPDATE {table} SET dirty = ?1"), [dirty])?;
            }
            for key in ["cursor", "last_sync"] {
                tx.execute("DELETE FROM meta WHERE key = ?1", [key])?;
            }
        }
        set_meta(&tx, "server", &config.server)?;
        set_meta(&tx, "token", &config.token)?;
        set_meta(&tx, "user_id", &config.user_id)?;
        match &config.hub {
            Some(hub) => set_meta(&tx, "hub_addr", serde_json::to_string(hub)?)?,
            None => {
                tx.execute("DELETE FROM meta WHERE key = 'hub_addr'", [])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn clear_sync_config(&mut self) -> Result<()> {
        self.conn.execute("DELETE FROM meta WHERE key IN ('server', 'token', 'user_id', 'hub_addr')", [])?;
        Ok(())
    }

    pub fn cursor(&self) -> Result<Option<i64>> {
        meta(&self.conn, "cursor")
    }

    pub fn last_sync(&self) -> Result<Option<i64>> {
        meta(&self.conn, "last_sync")
    }

    pub fn pending(&self) -> Result<Outbox> {
        let mark: i64 = meta(&self.conn, "change_seq")?.unwrap_or(0);
        let mut changes = Vec::new();
        let mut stmt = self.conn.prepare(
            "SELECT id, name, parent_id, created_at, updated_at, deleted FROM decks d WHERE dirty > 0 ORDER BY id",
        )?;
        for deck in stmt.query_map([], deck_row)? {
            changes.push(Change::Deck(deck?));
        }
        let mut stmt = self.conn.prepare(
            "SELECT id, deck_id, content, tags, created_at, updated_at, deleted FROM cards WHERE dirty > 0 ORDER BY id",
        )?;
        for card in stmt.query_map([], card_row)? {
            changes.push(Change::Card(card?));
        }
        let mut stmt = self.conn.prepare(
            "SELECT id, card_id, rating, reviewed_at, duration_ms FROM reviews WHERE dirty > 0 ORDER BY reviewed_at, id",
        )?;
        let reviews = stmt.query_map([], review_row)?;
        for review in reviews {
            changes.push(Change::Review(review?));
        }
        Ok(Outbox { changes, mark })
    }

    /// Merges changes from elsewhere: decks and cards last-writer-wins on `updated_at` (ties keep the stored
    /// value), reviews by union on id. Returns how many changed the store.
    pub fn apply(&mut self, changes: &[Change]) -> Result<u64> {
        self.device_only()?;
        let tx = self.conn.transaction()?;
        let applied = apply(&tx, &self.scheduler, changes)?;
        tx.commit()?;
        Ok(applied)
    }

    /// After a successful sync: clears the outbox up to `mark`, merges the response and stores its cursor.
    /// Returns how many incoming changes changed the store.
    pub fn finish_sync(&mut self, mark: i64, response: &SyncResponse) -> Result<u64> {
        self.device_only()?;
        let now = self.now();
        let tx = self.conn.transaction()?;
        for table in ["decks", "cards", "reviews"] {
            tx.execute(&format!("UPDATE {table} SET dirty = 0 WHERE dirty > 0 AND dirty <= ?1"), [mark])?;
        }
        let applied = apply(&tx, &self.scheduler, &response.changes)?;
        set_meta(&tx, "cursor", response.cursor)?;
        set_meta(&tx, "last_sync", now)?;
        tx.commit()?;
        Ok(applied)
    }
}

impl Store {
    /// A hub's side of a sync: merges the request's changes and answers with what the device hasn't seen. Decks
    /// and cards merge last-writer-wins on `updated_at` (ties keep the stored value), reviews by union on `id`, and
    /// every accepted change gets the next `seq`. The response holds every stored change with `seq > cursor` except
    /// values this request itself sent, plus the stored value of any deck or card whose incoming version lost.
    /// Also returns how many changes were accepted.
    pub fn serve_sync(&mut self, request: &SyncRequest) -> Result<(SyncResponse, u64)> {
        if !self.hub {
            return Err(Error::new("wrong_store", "only a hub's store serves sync requests"));
        }
        let tx = self.conn.transaction()?;
        let mut sent = HashSet::new();
        let mut lost = HashSet::new();
        let mut touched = BTreeSet::new();
        let mut accepted = 0;
        for change in &request.changes {
            let (kind, id) = change.key();
            let key = (kind, id.to_string());
            // `Some(stored == incoming)` when the stored value wins.
            let kept = match change {
                Change::Deck(deck) => match get_deck(&tx, &deck.id)? {
                    Some(stored) if deck.updated_at <= stored.updated_at => Some(stored == *deck),
                    _ => {
                        put_deck(&tx, deck, next_dirty(&tx)?)?;
                        None
                    }
                },
                Change::Card(card) => {
                    let card = Card { tags: normalize_tags(&card.tags), ..card.clone() };
                    match get_card(&tx, &card.id)? {
                        Some(stored) if card.updated_at <= stored.updated_at => Some(stored == card),
                        _ => {
                            put_card(&tx, &card, next_dirty(&tx)?)?;
                            touched.insert(card.id);
                            None
                        }
                    }
                }
                Change::Review(review) => {
                    let exists = tx
                        .query_row("SELECT 1 FROM reviews WHERE id = ?1", [&review.id], |_| Ok(()))
                        .optional()?
                        .is_some();
                    if exists {
                        Some(true)
                    } else {
                        queue::insert_review(&tx, review, next_dirty(&tx)?)?;
                        touched.insert(review.card_id.clone());
                        None
                    }
                }
            };
            match kept {
                None => {
                    accepted += 1;
                    lost.remove(&key);
                    sent.insert(key);
                }
                Some(true) => {
                    sent.insert(key);
                }
                Some(false) => {
                    lost.insert(key);
                }
            }
        }
        for card_id in touched {
            queue::recompute(&tx, &self.scheduler, &card_id)?;
        }

        let cursor = request.cursor.unwrap_or(0);
        let lost_ids = |kind: &str| -> Result<String> {
            let ids: Vec<&str> = lost.iter().filter(|(k, _)| *k == kind).map(|(_, id)| id.as_str()).collect();
            Ok(serde_json::to_string(&ids)?)
        };
        let pick = |kind: &'static str, id: &str| {
            !sent.contains(&(kind, id.to_string())) || lost.contains(&(kind, id.to_string()))
        };
        let since = "WHERE dirty > ?1 OR id IN (SELECT value FROM json_each(?2))";
        let mut changes: Vec<(i64, Change)> = Vec::new();
        {
            let mut stmt = tx.prepare(&format!(
                "SELECT id, name, parent_id, created_at, updated_at, deleted, dirty FROM decks d {since}"
            ))?;
            for row in stmt.query_map(params![cursor, lost_ids("deck")?], |r| Ok((deck_row(r)?, r.get(6)?)))? {
                let (deck, seq) = row?;
                if pick("deck", &deck.id) {
                    changes.push((seq, Change::Deck(deck)));
                }
            }
            let mut stmt = tx.prepare(&format!(
                "SELECT id, deck_id, content, tags, created_at, updated_at, deleted, dirty FROM cards {since}"
            ))?;
            for row in stmt.query_map(params![cursor, lost_ids("card")?], |r| Ok((card_row(r)?, r.get(7)?)))? {
                let (card, seq) = row?;
                if pick("card", &card.id) {
                    changes.push((seq, Change::Card(card)));
                }
            }
            let mut stmt = tx.prepare(&format!(
                "SELECT id, card_id, rating, reviewed_at, duration_ms, dirty FROM reviews {since}"
            ))?;
            for row in stmt.query_map(params![cursor, "[]"], |r| Ok((review_row(r)?, r.get(5)?)))? {
                let (review, seq) = row?;
                if pick("review", &review.id) {
                    changes.push((seq, Change::Review(review)));
                }
            }
        }
        changes.sort_by_key(|(seq, _)| *seq);
        let cursor = meta(&tx, "change_seq")?.unwrap_or(0);
        tx.commit()?;
        Ok((SyncResponse { cursor, changes: changes.into_iter().map(|(_, c)| c).collect() }, accepted))
    }

    /// A hub's latest `seq`.
    pub fn seq(&self) -> Result<i64> {
        Ok(meta(&self.conn, "change_seq")?.unwrap_or(0))
    }

    fn device_only(&self) -> Result<()> {
        if self.hub {
            return Err(Error::new("wrong_store", "a hub's store only changes through serve_sync and local writes"));
        }
        Ok(())
    }
}

fn review_row(r: &rusqlite::Row) -> rusqlite::Result<Review> {
    Ok(Review { id: r.get(0)?, card_id: r.get(1)?, rating: r.get(2)?, reviewed_at: r.get(3)?, duration_ms: r.get(4)? })
}

fn apply(conn: &Connection, scheduler: &recall_srs::Scheduler, changes: &[Change]) -> Result<u64> {
    let mut applied = 0;
    let mut touched = BTreeSet::new();
    for change in changes {
        let newer = |table: &str, id: &str, updated_at: i64| -> Result<bool> {
            let stored: Option<i64> = conn
                .query_row(&format!("SELECT updated_at FROM {table} WHERE id = ?1"), [id], |r| r.get(0))
                .optional()?;
            Ok(stored.is_none_or(|s| updated_at > s))
        };
        match change {
            Change::Deck(deck) => {
                if newer("decks", &deck.id, deck.updated_at)? {
                    put_deck(conn, deck, 0)?;
                    applied += 1;
                }
            }
            Change::Card(card) => {
                if newer("cards", &card.id, card.updated_at)? {
                    put_card(conn, card, 0)?;
                    touched.insert(card.id.clone());
                    applied += 1;
                }
            }
            Change::Review(review) => {
                if queue::insert_review(conn, review, 0)? {
                    touched.insert(review.card_id.clone());
                    applied += 1;
                }
            }
        }
    }
    for card_id in touched {
        queue::recompute(conn, scheduler, &card_id)?;
    }
    Ok(applied)
}

pub(super) fn meta<T: rusqlite::types::FromSql>(conn: &Connection, key: &str) -> Result<Option<T>> {
    Ok(conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

fn set_meta(conn: &Connection, key: &str, value: impl rusqlite::ToSql) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}
