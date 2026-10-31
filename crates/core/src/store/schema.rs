//! Migrations, tracked by `PRAGMA user_version`. Append to `MIGRATIONS`; never edit a shipped one.

use super::{Store, put_card, put_deck, queue};
use crate::{Result, model::ulid, seed};

const MIGRATIONS: &[&str] = &[
    // 1: initial schema.
    "CREATE TABLE meta (key TEXT PRIMARY KEY, value);
     CREATE TABLE decks (
       id TEXT PRIMARY KEY, name TEXT NOT NULL, parent_id TEXT,
       created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
       deleted INTEGER NOT NULL DEFAULT 0, dirty INTEGER NOT NULL DEFAULT 0
     );
     CREATE TABLE cards (
       id TEXT PRIMARY KEY, deck_id TEXT NOT NULL, content TEXT NOT NULL, tags TEXT NOT NULL DEFAULT '[]',
       created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
       deleted INTEGER NOT NULL DEFAULT 0, dirty INTEGER NOT NULL DEFAULT 0
     );
     CREATE INDEX cards_deck ON cards (deck_id, created_at);
     CREATE TABLE reviews (
       id TEXT PRIMARY KEY, card_id TEXT NOT NULL, rating INTEGER NOT NULL, reviewed_at INTEGER NOT NULL,
       duration_ms INTEGER, dirty INTEGER NOT NULL DEFAULT 0,
       prior_state TEXT -- derived: the card's state when it was reviewed
     );
     CREATE INDEX reviews_card ON reviews (card_id, reviewed_at, id);
     CREATE INDEX reviews_time ON reviews (reviewed_at);
     -- derived from reviews, see queue::recompute
     CREATE TABLE schedules (
       card_id TEXT PRIMARY KEY, state TEXT NOT NULL, due INTEGER NOT NULL,
       stability REAL NOT NULL, difficulty REAL NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL,
       last_review INTEGER, first_review INTEGER
     );
     CREATE INDEX schedules_due ON schedules (state, due);
     CREATE INDEX decks_dirty ON decks (dirty) WHERE dirty > 0;
     CREATE INDEX cards_dirty ON cards (dirty) WHERE dirty > 0;
     CREATE INDEX reviews_dirty ON reviews (dirty) WHERE dirty > 0;",
];

pub(super) fn migrate(store: &mut Store) -> Result<()> {
    let version = store.conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))? as usize;
    if version >= MIGRATIONS.len() {
        return Ok(());
    }
    let tx = store.conn.transaction()?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        tx.execute_batch(sql)?;
        if i == 0 {
            tx.execute("INSERT INTO meta (key, value) VALUES ('change_seq', 0), ('device_id', ?1)", [ulid()])?;
            let dirty = super::next_dirty(&tx)?;
            put_deck(&tx, &seed::deck(), dirty)?;
            for card in seed::cards() {
                put_card(&tx, &card, dirty)?;
                queue::recompute(&tx, &store.scheduler, &card.id)?;
            }
        }
    }
    tx.pragma_update(None, "user_version", MIGRATIONS.len() as i64)?;
    tx.commit()?;
    Ok(())
}
