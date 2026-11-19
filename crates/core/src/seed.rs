//! The sample deck every new store starts with. Its IDs and times are fixed so two devices that both seed it
//! converge on one deck after sync instead of two copies. Also the sample library `$FURITEN_TEMP_DB` starts from.

use std::path::Path;

use crate::Result;
use crate::model::{Card, CardView, Deck, ulid_from};
use crate::protocol::{Answer, NewCard, NewDeck};
use crate::store::Store;

/// 2026-01-01T00:00:00Z.
const SEEDED_AT: i64 = 1_767_225_600_000;

const CARDS: [&str; 5] = [
    "What does a line with only `---` do in a card?\n---\nIt splits the card into sides. The first side is the \
     question; the rest are revealed when you flip it.",
    "How do you answer a card while reviewing?\n---\nPress Space (Enter in the terminal) to reveal the answer, then say whether \
     you remembered it: 1 forgot, 2 remembered.",
    "Why is a card you know well shown less often?\n---\nFuriten schedules with FSRS. Each answer updates how well \
     you know the card, so cards you keep remembering wait longer and ones you forget come back sooner.",
    "Can other tools use Furiten?\n---\nYes. The `furiten` command line does everything the app does (add `--json` \
     for scripts), and agents can connect through the MCP server.",
    "How do you make your own deck?\n---\nCreate a deck, then add cards: write the question, a line with `---`, \
     then the answer. From a terminal: `printf 'hola\\n---\\nhello' | furiten add --deck Spanish -`.",
];

pub fn deck() -> Deck {
    Deck {
        id: ulid_from(SEEDED_AT, 0),
        name: "Furiten basics".to_string(),
        parent_id: None,
        created_at: SEEDED_AT,
        updated_at: SEEDED_AT,
        deleted: false,
    }
}

pub fn cards() -> Vec<Card> {
    let deck_id = deck().id;
    CARDS
        .iter()
        .enumerate()
        .map(|(i, content)| Card {
            id: ulid_from(SEEDED_AT + i as i64, i as u128 + 1),
            deck_id: deck_id.clone(),
            content: content.to_string(),
            tags: vec!["furiten".to_string()],
            // One ms apart so they come up in order.
            created_at: SEEDED_AT + i as i64,
            updated_at: SEEDED_AT + i as i64,
            deleted: false,
        })
        .collect()
}

/// The decks `$FURITEN_TEMP_DB` starts with: Spanish, studied for a while, and Rust, just begun.
const SPANISH: [(&str, &[&str]); 6] = [
    ("**la biblioteca**\n---\nthe library", &["vocab", "places"]),
    ("**el desayuno**\n---\nbreakfast\n---\n*Desayuno a las ocho.* I have breakfast at eight.", &["vocab", "food"]),
    ("**madrugar**\n---\nto get up early", &["vocab", "verbs"]),
    (
        "When do you use *ser* rather than *estar*?\n---\n*Ser* for what something is: identity, origin, \
         profession, time.\n---\n*Estar* for how or where it is: location, mood, the result of a change.",
        &["grammar"],
    ),
    ("**aprovechar**\n---\nto make the most of something", &["vocab", "verbs"]),
    ("**tener ganas de**\n---\nto feel like doing something", &["vocab", "expressions"]),
];

const RUST: [(&str, &[&str]); 3] = [
    (
        "What does `?` do in a function returning `Result`?\n---\nReturns early with the error (converted with \
         `From`), otherwise unwraps the `Ok` value.",
        &["rust", "programming"],
    ),
    (
        "`Rc<T>` vs `Arc<T>`?\n---\n`Arc` uses atomic reference counts, so it is `Send + Sync`; `Rc` is cheaper but \
         single-threaded.",
        &["rust", "programming"],
    ),
    (
        "What is lifetime elision?\n---\nWith one input reference, its lifetime is given to every output reference.",
        &["rust", "programming"],
    ),
];

/// Seeds `home`'s store with the sample library `$FURITEN_TEMP_DB` starts from.
pub fn seed_temp_home(home: &Path) -> Result<()> {
    let mut store = Store::open(&home.join("furiten.db"))?;
    seed_temp(&mut store)
}

/// A couple of decks someone has used for a while: some reviews due, some waiting, some cards new. The reviews are
/// dated in the past, so the schedules and the sidebar's counts look lived in.
pub fn seed_temp(store: &mut Store) -> Result<()> {
    const DAY: i64 = 86_400_000;
    let now = crate::now_ms();
    let at = |days: i64| Some(now - days * DAY);
    // Made when the study began, so no review is older than the cards.
    store.set_now(at(30));
    let spanish = store.create_deck(NewDeck { name: "Spanish".into(), parent_id: None })?.id;
    let rust = store.create_deck(NewDeck { name: "Rust".into(), parent_id: None })?.id;
    let spanish = add(store, &spanish, &SPANISH)?;
    let rust = add(store, &rust, &RUST)?;

    // Ten days ago: most of the Spanish deck once, one card forgotten.
    store.set_now(at(10));
    for card in &spanish[..3] {
        answer(store, card, true)?;
    }
    answer(store, &spanish[4], false)?;
    // Three days ago: two of them again, and the forgotten one forgotten again.
    store.set_now(at(3));
    for card in &spanish[..2] {
        answer(store, card, true)?;
    }
    answer(store, &spanish[4], false)?;
    // Today: the Rust deck begun, so its reviews wait a few days, and one card is still new.
    store.set_now(Some(now));
    for card in &rust[..2] {
        answer(store, card, true)?;
    }
    store.set_now(None);
    Ok(())
}

fn add(store: &mut Store, deck: &str, cards: &[(&str, &[&str])]) -> Result<Vec<CardView>> {
    let new = cards
        .iter()
        .map(|(content, tags)| NewCard {
            deck_id: deck.to_string(),
            content: (*content).to_string(),
            tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
        })
        .collect();
    store.create_cards(new)
}

fn answer(store: &mut Store, card: &CardView, remembered: bool) -> Result<()> {
    store.answer(Answer { card_id: card.card.id.clone(), remembered, duration_ms: Some(4_000) })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_temp_library_starts_somewhere_someone_has_been() {
        let mut store = Store::open_in_memory().unwrap();
        seed_temp(&mut store).unwrap();
        let names: Vec<String> = store.decks().unwrap().into_iter().map(|d| d.deck.name).collect();
        assert!(names.contains(&"Spanish".to_string()) && names.contains(&"Rust".to_string()), "{names:?}");
        let next = store.next_review(None).unwrap();
        assert!(next.card.is_some(), "a card to review");
        assert!(next.remaining.due > 0, "some reviews are due: {:?}", next.remaining);
        assert!(next.remaining.new > 0, "and some cards are new: {:?}", next.remaining);
    }
}
