//! Schedules (derived by folding reviews), the review queue, answers and stats.

use std::collections::HashMap;

use recall_srs::{Rating, Scheduler};
use rusqlite::{Connection, OptionalExtension, named_params, params};

use super::{Store, next_dirty, subtree};
use crate::model::{CardView, Review, State, ulid};
use crate::protocol::{Answer, NextReview, Remaining, Stats};
use crate::{Error, Result};

const NEW_PER_DECK_PER_DAY: i64 = 20;
const DAY: i64 = 86_400_000;

/// New cards that may be shown today: per deck, the oldest `20 - introduced today` new cards. `:scope` is a JSON
/// array of deck ids, or NULL for every deck.
const NEW_QUEUE: &str = "WITH intro AS (
       SELECT c.deck_id, COUNT(*) AS n FROM cards c JOIN schedules s ON s.card_id = c.id
       WHERE c.deleted = 0 AND s.first_review >= :day GROUP BY c.deck_id
     ), ranked AS (
       SELECT c.id, c.deck_id, c.created_at,
         ROW_NUMBER() OVER (PARTITION BY c.deck_id ORDER BY c.created_at, c.id) AS rn
       FROM cards c JOIN schedules s ON s.card_id = c.id
       WHERE c.deleted = 0 AND s.state = 'new'
         AND (:scope IS NULL OR c.deck_id IN (SELECT value FROM json_each(:scope)))
     )
     SELECT r.id, r.deck_id, r.created_at FROM ranked r LEFT JOIN intro i ON i.deck_id = r.deck_id
     WHERE r.rn <= :cap - COALESCE(i.n, 0)";

/// Reviewed cards that are due: `:scope` as above.
const DUE: &str = "SELECT c.id, c.deck_id, s.due FROM cards c JOIN schedules s ON s.card_id = c.id
     WHERE c.deleted = 0 AND s.state != 'new' AND s.due <= :now
       AND (:scope IS NULL OR c.deck_id IN (SELECT value FROM json_each(:scope)))";

impl Store {
    pub fn next_review(&self, deck_id: Option<&str>) -> Result<NextReview> {
        let now = self.now();
        let scope = self.scope(deck_id)?;
        let day = day_start(&self.conn, now)?;
        let due_params = named_params! { ":now": now, ":scope": scope };
        let new_params = named_params! { ":day": day, ":scope": scope, ":cap": NEW_PER_DECK_PER_DAY };
        let count = |sql: &str, params: &[(&str, &dyn rusqlite::ToSql)]| -> Result<u64> {
            Ok(self.conn.query_row(&format!("SELECT COUNT(*) FROM ({sql})"), params, uint)?)
        };
        let first = |sql: String, params: &[(&str, &dyn rusqlite::ToSql)]| -> Result<Option<String>> {
            Ok(self.conn.query_row(&sql, params, |r| r.get(0)).optional()?)
        };
        let remaining = Remaining { due: count(DUE, due_params)?, new: count(NEW_QUEUE, new_params)? };
        let id = match first(format!("SELECT id FROM ({DUE}) ORDER BY due, id LIMIT 1"), due_params)? {
            Some(id) => Some(id),
            None => first(format!("SELECT id FROM ({NEW_QUEUE}) ORDER BY created_at, id LIMIT 1"), new_params)?,
        };
        let card = id.map(|id| self.card(&id)).transpose()?;
        Ok(NextReview { card, remaining })
    }

    pub fn answer(&mut self, answer: Answer) -> Result<CardView> {
        if Rating::from_u8(answer.rating).is_none() {
            return Err(Error::invalid("rating must be 1 (again), 2 (hard), 3 (good) or 4 (easy)"));
        }
        if answer.duration_ms.is_some_and(|d| d < 0) {
            return Err(Error::invalid("duration_ms can't be negative"));
        }
        self.card(&answer.card_id)?;
        let review = Review {
            id: ulid(),
            card_id: answer.card_id,
            rating: answer.rating,
            reviewed_at: self.now(),
            duration_ms: answer.duration_ms,
        };
        let tx = self.conn.transaction()?;
        insert_review(&tx, &review, next_dirty(&tx)?)?;
        recompute(&tx, &self.scheduler, &review.card_id)?;
        tx.commit()?;
        self.card(&review.card_id)
    }

    pub fn stats(&self, deck_id: Option<&str>) -> Result<Stats> {
        let now = self.now();
        let scope = self.scope(deck_id)?;
        let day = day_start(&self.conn, now)?;
        let in_scope = "c.deleted = 0 AND (:scope IS NULL OR c.deck_id IN (SELECT value FROM json_each(:scope)))";
        let cards = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM cards c WHERE {in_scope}"),
            named_params! { ":scope": scope },
            uint,
        )?;
        let due = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM ({DUE})"),
            named_params! { ":now": now, ":scope": scope },
            uint,
        )?;
        let new = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM ({NEW_QUEUE})"),
            named_params! { ":day": day, ":scope": scope, ":cap": NEW_PER_DECK_PER_DAY },
            uint,
        )?;
        let (reviews_today, retention_30d) = self.conn.query_row(
            &format!(
                "SELECT COALESCE(SUM(r.reviewed_at >= :day), 0),
                   AVG(CASE WHEN r.prior_state = 'review' AND r.reviewed_at >= :month THEN r.rating > 1 END)
                 FROM reviews r JOIN cards c ON c.id = r.card_id
                 WHERE {in_scope} AND r.reviewed_at >= MIN(:day, :month)"
            ),
            named_params! { ":day": day, ":month": now - 30 * DAY, ":scope": scope },
            |r| Ok((uint(r)?, r.get(1)?)),
        )?;
        Ok(Stats { cards, due, new, reviews_today, retention_30d })
    }

    /// The JSON array of deck ids a `deck_id` filter covers (the deck and its subdecks), or `None` for all.
    fn scope(&self, deck_id: Option<&str>) -> Result<Option<String>> {
        deck_id
            .map(|id| {
                self.deck(id)?;
                Ok(serde_json::to_string(&subtree(&self.conn, id)?)?)
            })
            .transpose()
    }
}

type Named<'a> = Vec<(&'a str, &'a dyn rusqlite::ToSql)>;

/// Per deck (own cards only): `[cards, due, new available today]`.
pub(super) fn deck_counts(conn: &Connection, now: i64) -> Result<HashMap<String, [u64; 3]>> {
    let day = day_start(conn, now)?;
    let mut counts: HashMap<String, [u64; 3]> = HashMap::new();
    let none: Option<String> = None;
    let queries: [(String, Named); 3] = [
        ("SELECT deck_id, COUNT(*) FROM cards WHERE deleted = 0 GROUP BY deck_id".to_string(), vec![]),
        (
            format!("SELECT deck_id, COUNT(*) FROM ({DUE}) GROUP BY deck_id"),
            vec![(":now", &now as &dyn rusqlite::ToSql), (":scope", &none)],
        ),
        (
            format!("SELECT deck_id, COUNT(*) FROM ({NEW_QUEUE}) GROUP BY deck_id"),
            vec![(":day", &day as &dyn rusqlite::ToSql), (":scope", &none), (":cap", &NEW_PER_DECK_PER_DAY)],
        ),
    ];
    for (k, (sql, params)) in queries.iter().enumerate() {
        let mut stmt = conn.prepare(sql)?;
        let mut rows = stmt.query(params.as_slice())?;
        while let Some(row) = rows.next()? {
            counts.entry(row.get(0)?).or_default()[k] = row.get::<_, i64>(1)? as u64;
        }
    }
    Ok(counts)
}

fn uint(row: &rusqlite::Row) -> rusqlite::Result<u64> {
    Ok(row.get::<_, i64>(0)?.max(0) as u64)
}

/// Start of the local day containing `now`, in Unix ms. SQLite knows the system time zone, so no extra crate.
pub(super) fn day_start(conn: &Connection, now: i64) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT CAST(strftime('%s', ?1 / 1000, 'unixepoch', 'localtime', 'start of day', 'utc') AS INTEGER) * 1000",
        [now],
        |r| r.get(0),
    )?)
}

/// Inserts a review unless one with its id exists. Returns whether it was new.
pub(super) fn insert_review(conn: &Connection, review: &Review, dirty: i64) -> Result<bool> {
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO reviews (id, card_id, rating, reviewed_at, duration_ms, dirty) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![review.id, review.card_id, review.rating, review.reviewed_at, review.duration_ms, dirty],
    )?;
    Ok(inserted == 1)
}

/// Rebuilds a card's cached schedule by folding all its reviews in `(reviewed_at, id)` order, and records each
/// review's prior state (for retention stats). Does nothing for a card the store doesn't have yet.
pub(super) fn recompute(conn: &Connection, scheduler: &Scheduler, card_id: &str) -> Result<()> {
    let Some(created_at) =
        conn.query_row("SELECT created_at FROM cards WHERE id = ?1", [card_id], |r| r.get::<_, i64>(0)).optional()?
    else {
        return Ok(());
    };
    let reviews: Vec<(String, u8, i64, Option<String>)> = conn
        .prepare(
            "SELECT id, rating, reviewed_at, prior_state FROM reviews WHERE card_id = ?1 ORDER BY reviewed_at, id",
        )?
        .query_map([card_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut schedule = recall_srs::Schedule::new(created_at);
    let mut first_review = None;
    for (id, rating, at, prior) in reviews {
        // Ratings outside 1-4 can only come from a bad sync peer; they don't count.
        let Some(rating) = Rating::from_u8(rating) else { continue };
        let state = State::from(schedule.state).as_str();
        if prior.as_deref() != Some(state) {
            conn.execute("UPDATE reviews SET prior_state = ?1 WHERE id = ?2", params![state, id])?;
        }
        first_review.get_or_insert(at);
        schedule = scheduler.review(&schedule, rating, at);
    }
    conn.execute(
        "INSERT OR REPLACE INTO schedules \
         (card_id, state, due, stability, difficulty, reps, lapses, last_review, first_review) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            card_id,
            State::from(schedule.state).as_str(),
            schedule.due,
            schedule.stability,
            schedule.difficulty,
            schedule.reps,
            schedule.lapses,
            schedule.last_review,
            first_review
        ],
    )?;
    Ok(())
}
