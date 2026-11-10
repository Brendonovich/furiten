//! Adding another app's decks: new decks, cards and their review history, in one transaction.

use std::collections::HashMap;

use furiten_srs::Rating;

use super::{Store, deck_name, next_dirty, put_card, put_deck, queue};
use crate::Result;
use crate::import::Library;
use crate::model::{Card, Deck, Id, Review, normalize_tags, ulid};
use crate::protocol::Imported;

impl Store {
    /// Adds `library` as new decks (under `parent_id`, or at the top level), all or nothing. Importing the same
    /// export twice makes two copies. Each card's schedule is folded from its imported reviews, so it comes due where
    /// it would have in the other app.
    pub fn import(&mut self, library: Library, parent_id: Option<&str>) -> Result<Imported> {
        if let Some(parent) = parent_id {
            self.deck(parent)?;
        }
        let now = self.now();
        let tx = self.conn.transaction()?;
        let dirty = next_dirty(&tx)?;
        let mut ids: HashMap<&str, Id> = HashMap::new();
        let mut decks = Vec::with_capacity(library.decks.len());
        let (mut cards, mut reviews, mut new_at) = (0, 0, now);
        for i in parents_first(&library) {
            let d = &library.decks[i];
            // A parent the export doesn't have (or a cycle) puts the deck at the top of the import.
            let parent = d.parent.as_deref().and_then(|p| ids.get(p).cloned()).or_else(|| parent_id.map(String::from));
            let name = deck_name(&d.name).unwrap_or_else(|_| "Untitled deck".to_string());
            let deck = Deck { id: ulid(), name, parent_id: parent, created_at: now, updated_at: now, deleted: false };
            put_deck(&tx, &deck, dirty)?;
            ids.insert(&d.key, deck.id.clone());
            for c in &d.cards {
                // Cards without a date of their own a ms apart, so they're learned in the export's order.
                let mut created_at = c.created_at.unwrap_or_else(|| {
                    new_at += 1;
                    new_at
                });
                if let Some(first) = c.reviews.iter().map(|r| r.reviewed_at).min() {
                    created_at = created_at.min(first);
                }
                let card = Card {
                    id: ulid(),
                    deck_id: deck.id.clone(),
                    content: c.content.clone(),
                    tags: normalize_tags(&c.tags),
                    created_at,
                    updated_at: now.max(created_at),
                    deleted: false,
                };
                put_card(&tx, &card, dirty)?;
                for r in c.reviews.iter().filter(|r| Rating::from_u8(r.rating).is_some()) {
                    let review = Review {
                        id: ulid(),
                        card_id: card.id.clone(),
                        rating: r.rating,
                        reviewed_at: r.reviewed_at,
                        duration_ms: r.duration_ms.filter(|d| *d >= 0),
                    };
                    queue::insert_review(&tx, &review, dirty)?;
                    reviews += 1;
                }
                queue::recompute(&tx, &self.scheduler, &card.id)?;
                cards += 1;
            }
            decks.push(deck);
        }
        tx.commit()?;
        Ok(Imported { decks, cards, reviews, skipped: library.skipped, attachments: library.attachments })
    }
}

/// The decks' indices with every parent before its children; decks in a cycle come last, in file order.
fn parents_first(library: &Library) -> Vec<usize> {
    let decks = &library.decks;
    let has = |key: &str| decks.iter().any(|d| d.key == key);
    let mut order = Vec::with_capacity(decks.len());
    let mut placed = vec![false; decks.len()];
    loop {
        let before = order.len();
        for (i, d) in decks.iter().enumerate() {
            let ready = d
                .parent
                .as_deref()
                .is_none_or(|p| !has(p) || decks.iter().enumerate().any(|(j, o)| placed[j] && o.key == p));
            if !placed[i] && ready {
                placed[i] = true;
                order.push(i);
            }
        }
        if order.len() == before {
            break;
        }
    }
    order.extend((0..decks.len()).filter(|&i| !placed[i]));
    order
}
