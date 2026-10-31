//! The store's side of sync: the outbox, merging incoming changes, and sync metadata.

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension, params};

use super::{Store, card_row, deck_row, put_card, put_deck, queue};
use crate::Result;
use crate::model::{Change, Review};
use crate::protocol::SyncResponse;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncConfig {
    pub server: String,
    pub token: String,
    pub user_id: String,
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
        Ok(match (get("server")?, get("token")?, get("user_id")?) {
            (Some(server), Some(token), Some(user_id)) => Some(SyncConfig { server, token, user_id }),
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
        tx.commit()?;
        Ok(())
    }

    pub fn clear_sync_config(&mut self) -> Result<()> {
        self.conn.execute("DELETE FROM meta WHERE key IN ('server', 'token', 'user_id')", [])?;
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
        let reviews = stmt.query_map([], |r| {
            Ok(Review {
                id: r.get(0)?,
                card_id: r.get(1)?,
                rating: r.get(2)?,
                reviewed_at: r.get(3)?,
                duration_ms: r.get(4)?,
            })
        })?;
        for review in reviews {
            changes.push(Change::Review(review?));
        }
        Ok(Outbox { changes, mark })
    }

    /// Merges changes from elsewhere: decks and cards last-writer-wins on `updated_at` (ties keep the stored
    /// value), reviews by union on id. Returns how many changed the store.
    pub fn apply(&mut self, changes: &[Change]) -> Result<u64> {
        let tx = self.conn.transaction()?;
        let applied = apply(&tx, &self.scheduler, changes)?;
        tx.commit()?;
        Ok(applied)
    }

    /// After a successful sync: clears the outbox up to `mark`, merges the response and stores its cursor.
    /// Returns how many incoming changes changed the store.
    pub fn finish_sync(&mut self, mark: i64, response: &SyncResponse) -> Result<u64> {
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

fn meta<T: rusqlite::types::FromSql>(conn: &Connection, key: &str) -> Result<Option<T>> {
    Ok(conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

fn set_meta(conn: &Connection, key: &str, value: impl rusqlite::ToSql) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}
