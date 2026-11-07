use std::cmp::Ordering;

use super::*;
use crate::model::{Card, Schedule, State};

fn card(id: &str, content: &str, created_at: i64) -> CardView {
    let card = Card {
        id: id.into(),
        deck_id: "d".into(),
        content: content.into(),
        tags: vec![],
        created_at,
        updated_at: created_at,
        deleted: false,
    };
    let schedule = Schedule {
        state: State::New,
        due: created_at,
        stability: 0.0,
        difficulty: 0.0,
        reps: 0,
        lapses: 0,
        last_review: None,
    };
    CardView::new(card, schedule)
}

fn reviewed(mut c: CardView, last: i64, due: i64, reps: u32, lapses: u32) -> CardView {
    c.schedule = Schedule { state: State::Review, due, reps, lapses, last_review: Some(last), ..c.schedule };
    c
}

fn ids(mut cards: Vec<CardView>, by: CardSort, desc: bool) -> Vec<String> {
    sort(&mut cards, by, desc, 7);
    cards.into_iter().map(|c| c.card.id).collect()
}

#[test]
fn names_sort_naturally_without_markup() {
    let cards = vec![
        card("a", "Card 10", 1),
        card("b", "**card 9**\n---\nback", 2),
        card("c", "# Apple", 3),
        card("d", "- banana", 4),
        card("e", "card 09", 5),
    ];
    assert_eq!(ids(cards.clone(), CardSort::Name, false), ["c", "d", "b", "e", "a"]);
    assert_eq!(ids(cards, CardSort::Name, true), ["a", "e", "b", "d", "c"]);
    assert_eq!(title_key("\n> **La** `biblioteca`"), "la biblioteca");
}

#[test]
fn natural_order_compares_digit_runs_by_value() {
    assert_eq!(natural_cmp("a2", "a10"), Ordering::Less);
    assert_eq!(natural_cmp("a010", "a9"), Ordering::Greater);
    assert_eq!(natural_cmp("a", "a1"), Ordering::Less);
    assert_eq!(natural_cmp("b", "a9"), Ordering::Greater);
    assert_eq!(natural_cmp("x0", "x00"), Ordering::Equal);
}

#[test]
fn unreviewed_cards_stay_last_either_way() {
    let cards = vec![
        card("new", "n", 1),
        reviewed(card("weak", "w", 2), 100, 200, 4, 2),
        reviewed(card("strong", "s", 3), 50, 1_000, 4, 0),
    ];
    assert_eq!(ids(cards.clone(), CardSort::Retention, false), ["weak", "strong", "new"]);
    assert_eq!(ids(cards.clone(), CardSort::Retention, true), ["strong", "weak", "new"]);
    assert_eq!(ids(cards.clone(), CardSort::Interval, false), ["weak", "strong", "new"]);
    assert_eq!(ids(cards.clone(), CardSort::Reviewed, true), ["weak", "strong", "new"]);
    assert_eq!(ids(cards, CardSort::Due, false), ["new", "weak", "strong"]);
}

#[test]
fn random_is_fixed_by_the_seed() {
    let cards: Vec<CardView> = (0..20).map(|i| card(&format!("c{i:02}"), "x", i)).collect();
    let once = ids(cards.clone(), CardSort::Random, false);
    assert_eq!(once, ids(cards.clone(), CardSort::Random, false));
    let mut other = cards.clone();
    sort(&mut other, CardSort::Random, false, 8);
    assert_ne!(once, other.into_iter().map(|c| c.card.id).collect::<Vec<_>>());
    assert_ne!(once, ids(cards, CardSort::Created, false));
    // The same hash the Worker uses (server/src/Sort.ts).
    assert_eq!(fnv1a(b"7:c00"), 0x0037_724d);
}
